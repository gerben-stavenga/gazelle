//! Regex parser and Thompson NFA construction.
//!
//! Patterns operate on Unicode codepoints; the [`Nfa`] matches UTF-8 byte
//! sequences (alphabet 0-255). Each codepoint in a pattern becomes a byte-chain
//! in the NFA. Character classes and `.` work at the codepoint level.
//!
//! The parser is generated from `grammars/regex.gzl` using the CLI.
//!
//! To regenerate `regex_generated.rs`:
//! ```bash
//! cargo run -- --rust grammars/regex.gzl > src/regex_generated.rs
//! ```

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::{format, vec, vec::Vec};

use crate as gazelle;
use crate::automaton::Nfa;

// ============================================================================
// Generated parser
// ============================================================================

include!("regex_generated.rs");

/// Error from regex parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegexError {
    pub message: String,
    pub offset: usize,
}

impl core::fmt::Display for RegexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "regex error at {}: {}", self.offset, self.message)
    }
}

impl core::error::Error for RegexError {}

// ============================================================================
// Unicode char range utilities
// ============================================================================

/// Sort and merge overlapping/adjacent inclusive char ranges.
fn normalize_ranges(ranges: &mut Vec<(char, char)>) {
    ranges.sort_by_key(|&(lo, _)| lo);
    let mut i = 0;
    while i + 1 < ranges.len() {
        let (_, hi) = ranges[i];
        let (next_lo, next_hi) = ranges[i + 1];
        // Merge if overlapping or adjacent (hi + 1 >= next_lo)
        if hi >= next_lo || hi as u32 + 1 >= next_lo as u32 {
            ranges[i].1 = core::cmp::max(hi, next_hi);
            ranges.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Complement char ranges within valid Unicode (excluding surrogates).
fn complement_ranges(ranges: &[(char, char)]) -> Vec<(char, char)> {
    let mut sorted: Vec<(char, char)> = ranges.to_vec();
    normalize_ranges(&mut sorted);
    let mut result = Vec::new();
    let mut cursor = 0u32; // start of next gap
    for &(lo, hi) in &sorted {
        let lo = lo as u32;
        let hi = hi as u32;
        // Add gap before this range (skipping surrogates)
        add_range_gap(&mut result, cursor, lo.saturating_sub(1));
        cursor = hi + 1;
    }
    // Add gap after last range to end of Unicode
    add_range_gap(&mut result, cursor, 0x10FFFF);
    result
}

/// Add a gap range [from, to] to result, skipping the surrogate range D800-DFFF.
fn add_range_gap(result: &mut Vec<(char, char)>, from: u32, to: u32) {
    if from > to {
        return;
    }
    if to < 0xD800 || from > 0xDFFF {
        // Entirely outside surrogates
        if let (Some(lo), Some(hi)) = (char::from_u32(from), char::from_u32(to)) {
            result.push((lo, hi));
        }
    } else {
        // Straddles surrogate range — split
        if from < 0xD800 {
            if let (Some(lo), Some(hi)) = (char::from_u32(from), char::from_u32(0xD7FF)) {
                result.push((lo, hi));
            }
        }
        if to > 0xDFFF {
            if let (Some(lo), Some(hi)) = (char::from_u32(0xE000), char::from_u32(to)) {
                result.push((lo, hi));
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Shorthand {
    Digit,
    Word,
    Space,
    NotDigit,
    NotWord,
    NotSpace,
}

impl Shorthand {
    /// Positive char ranges (always returns the non-negated set).
    fn char_ranges(&self) -> Vec<(char, char)> {
        match self {
            Shorthand::Digit | Shorthand::NotDigit => vec![('0', '9')],
            Shorthand::Word | Shorthand::NotWord => {
                vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')]
            }
            Shorthand::Space | Shorthand::NotSpace => {
                vec![('\t', '\n'), ('\x0B', '\x0C'), ('\r', '\r'), (' ', ' ')]
            }
        }
    }

    fn is_negated(&self) -> bool {
        matches!(
            self,
            Shorthand::NotDigit | Shorthand::NotWord | Shorthand::NotSpace
        )
    }

    /// Resolved ranges: positive set or its complement.
    fn resolved_ranges(&self) -> Vec<(char, char)> {
        let r = self.char_ranges();
        if self.is_negated() {
            complement_ranges(&r)
        } else {
            r
        }
    }
}

// ============================================================================
// NFA builder implementing Actions
// ============================================================================

/// An NFA fragment: a start state and an end state (not yet connected).
#[derive(Debug)]
struct Frag {
    start: usize,
    end: usize,
}

struct NfaBuilder {
    nfa: Nfa,
}

impl NfaBuilder {
    /// Build an NFA fragment matching a single char's UTF-8 byte sequence.
    fn char_frag(&mut self, ch: char) -> Frag {
        let start = self.nfa.add_state();
        let end = self.nfa.add_state();
        let mut buf = [0u8; 4];
        let bytes = ch.encode_utf8(&mut buf).as_bytes();
        let mut cur = start;
        for (i, &b) in bytes.iter().enumerate() {
            let next = if i + 1 == bytes.len() {
                end
            } else {
                self.nfa.add_state()
            };
            self.nfa.add_transition(cur, b as u32, next);
            cur = next;
        }
        Frag { start, end }
    }

    /// Build an NFA fragment matching any codepoint in the given ranges.
    fn char_ranges_frag(&mut self, ranges: &[(char, char)]) -> Frag {
        let start = self.nfa.add_state();
        let end = self.nfa.add_state();
        for &(lo, hi) in ranges {
            self.add_utf8_range(lo, hi, start, end);
        }
        Frag { start, end }
    }

    /// Encoding-length boundaries for UTF-8.
    const UTF8_BOUNDARIES: [(u32, u32); 5] = [
        (0x0000, 0x007F),    // 1-byte
        (0x0080, 0x07FF),    // 2-byte
        (0x0800, 0xD7FF),    // 3-byte (before surrogates)
        (0xE000, 0xFFFF),    // 3-byte (after surrogates)
        (0x10000, 0x10FFFF), // 4-byte
    ];

    /// Add NFA transitions for all codepoints in [lo, hi] from start to end.
    fn add_utf8_range(&mut self, lo: char, hi: char, start: usize, end: usize) {
        let lo = lo as u32;
        let hi = hi as u32;
        for &(bnd_lo, bnd_hi) in &Self::UTF8_BOUNDARIES {
            let sub_lo = core::cmp::max(lo, bnd_lo);
            let sub_hi = core::cmp::min(hi, bnd_hi);
            if sub_lo > sub_hi {
                continue;
            }
            // Safe: sub_lo/sub_hi are within valid Unicode (surrogates excluded by boundaries)
            let ch_lo = char::from_u32(sub_lo).unwrap();
            let ch_hi = char::from_u32(sub_hi).unwrap();
            let mut lo_bytes = [0u8; 4];
            let mut hi_bytes = [0u8; 4];
            let lo_len = ch_lo.encode_utf8(&mut lo_bytes).len();
            let hi_len = ch_hi.encode_utf8(&mut hi_bytes).len();
            debug_assert_eq!(lo_len, hi_len);
            self.add_utf8_byte_range(&lo_bytes[..lo_len], &hi_bytes[..hi_len], start, end);
        }
    }

    /// Recursively add NFA transitions for a byte-level range [lo_bytes, hi_bytes].
    /// Both slices have the same length. Adds paths from `start` to `end`.
    fn add_utf8_byte_range(&mut self, lo_bytes: &[u8], hi_bytes: &[u8], start: usize, end: usize) {
        debug_assert_eq!(lo_bytes.len(), hi_bytes.len());
        let n = lo_bytes.len();

        if n == 1 {
            for b in lo_bytes[0]..=hi_bytes[0] {
                self.nfa.add_transition(start, b as u32, end);
            }
            return;
        }

        if lo_bytes[0] == hi_bytes[0] {
            // Same first byte — recurse on tail
            let mid = self.nfa.add_state();
            self.nfa.add_transition(start, lo_bytes[0] as u32, mid);
            self.add_utf8_byte_range(&lo_bytes[1..], &hi_bytes[1..], mid, end);
            return;
        }

        // Split into: low partial, full middle, high partial
        let lo_tail_is_min = lo_bytes[1..].iter().all(|&b| b == 0x80);
        let hi_tail_is_max = hi_bytes[1..].iter().all(|&b| b == 0xBF);

        let mut mid_lo = lo_bytes[0];
        let mut mid_hi = hi_bytes[0];

        // Low partial: lo_bytes[0] with lo_bytes[1..] up to [0xBF, ...]
        if !lo_tail_is_min {
            let s = self.nfa.add_state();
            self.nfa.add_transition(start, lo_bytes[0] as u32, s);
            let max_tail: Vec<u8> = vec![0xBF; n - 1];
            self.add_utf8_byte_range(&lo_bytes[1..], &max_tail, s, end);
            mid_lo = lo_bytes[0] + 1;
        }

        // High partial: hi_bytes[0] with [0x80, ...] up to hi_bytes[1..]
        if !hi_tail_is_max {
            let s = self.nfa.add_state();
            self.nfa.add_transition(start, hi_bytes[0] as u32, s);
            let min_tail: Vec<u8> = vec![0x80; n - 1];
            self.add_utf8_byte_range(&min_tail, &hi_bytes[1..], s, end);
            mid_hi = hi_bytes[0] - 1;
        }

        // Full middle: bytes mid_lo..=mid_hi, all continuation bytes 0x80-0xBF
        if mid_lo <= mid_hi {
            let s = self.nfa.add_state();
            for b in mid_lo..=mid_hi {
                self.nfa.add_transition(start, b as u32, s);
            }
            self.add_utf8_full_cont(n - 1, s, end);
        }
    }

    /// Chain of states accepting full continuation byte range (0x80-0xBF) for `remaining` bytes.
    fn add_utf8_full_cont(&mut self, remaining: usize, start: usize, end: usize) {
        if remaining == 1 {
            for b in 0x80u32..=0xBF {
                self.nfa.add_transition(start, b, end);
            }
            return;
        }
        let mid = self.nfa.add_state();
        for b in 0x80u32..=0xBF {
            self.nfa.add_transition(start, b, mid);
        }
        self.add_utf8_full_cont(remaining - 1, mid, end);
    }
}

impl crate::ErrorType for NfaBuilder {
    type Error = RegexError;
}

impl Types for NfaBuilder {
    type Char = char;
    type Shorthand = Shorthand;
    type Regex = Frag;
    type Concat = Frag;
    type Repetition = Frag;
    type Atom = Frag;
    type CharClass = Frag;
    type ClassItem = Vec<(char, char)>;
    type ClassChar = char;
}

impl gazelle::Action<Regex<Self>> for NfaBuilder {
    fn build(&mut self, node: Regex<Self>) -> Result<Frag, Self::Error> {
        let Regex::Regex(alts) = node;
        let mut iter = alts.into_iter();
        let mut frag = iter.next().unwrap();
        for alt in iter {
            let start = self.nfa.add_state();
            let end = self.nfa.add_state();
            self.nfa.add_epsilon(start, frag.start);
            self.nfa.add_epsilon(start, alt.start);
            self.nfa.add_epsilon(frag.end, end);
            self.nfa.add_epsilon(alt.end, end);
            frag = Frag { start, end };
        }
        Ok(frag)
    }
}

impl gazelle::Action<Concat<Self>> for NfaBuilder {
    fn build(&mut self, node: Concat<Self>) -> Result<Frag, Self::Error> {
        let Concat::Concat(parts) = node;
        let mut iter = parts.into_iter();
        let mut frag = iter.next().unwrap();
        for part in iter {
            self.nfa.add_epsilon(frag.end, part.start);
            frag = Frag {
                start: frag.start,
                end: part.end,
            };
        }
        Ok(frag)
    }
}

impl gazelle::Action<Repetition<Self>> for NfaBuilder {
    fn build(&mut self, node: Repetition<Self>) -> Result<Frag, Self::Error> {
        Ok(match node {
            Repetition::Star(inner) => {
                let start = self.nfa.add_state();
                let end = self.nfa.add_state();
                self.nfa.add_epsilon(start, inner.start);
                self.nfa.add_epsilon(start, end);
                self.nfa.add_epsilon(inner.end, inner.start);
                self.nfa.add_epsilon(inner.end, end);
                Frag { start, end }
            }
            Repetition::Plus(inner) => {
                let start = self.nfa.add_state();
                let end = self.nfa.add_state();
                self.nfa.add_epsilon(start, inner.start);
                self.nfa.add_epsilon(inner.end, inner.start);
                self.nfa.add_epsilon(inner.end, end);
                Frag { start, end }
            }
            Repetition::Opt(inner) => {
                let start = self.nfa.add_state();
                let end = self.nfa.add_state();
                self.nfa.add_epsilon(start, inner.start);
                self.nfa.add_epsilon(start, end);
                self.nfa.add_epsilon(inner.end, end);
                Frag { start, end }
            }
            Repetition::Atom(a) => a,
        })
    }
}

impl gazelle::Action<Atom<Self>> for NfaBuilder {
    fn build(&mut self, node: Atom<Self>) -> Result<Frag, Self::Error> {
        Ok(match node {
            Atom::Char(ch) => self.char_frag(ch),
            Atom::Dash => self.char_frag('-'),
            Atom::Caret => self.char_frag('^'),
            Atom::Rbracket => self.char_frag(']'),
            Atom::Dot => self.char_ranges_frag(&complement_ranges(&[('\n', '\n')])),
            Atom::Shorthand(s) => self.char_ranges_frag(&s.resolved_ranges()),
            Atom::Group(r) => r,
            Atom::Class(c) => c,
        })
    }
}

impl gazelle::Action<CharClass<Self>> for NfaBuilder {
    fn build(&mut self, node: CharClass<Self>) -> Result<Frag, Self::Error> {
        let CharClass::Class(negated, items) = node;
        let mut ranges: Vec<(char, char)> = items.into_iter().flatten().collect();
        normalize_ranges(&mut ranges);
        if negated.is_some() {
            ranges = complement_ranges(&ranges);
        }
        Ok(self.char_ranges_frag(&ranges))
    }
}

impl gazelle::Action<ClassItem<Self>> for NfaBuilder {
    fn build(&mut self, node: ClassItem<Self>) -> Result<Vec<(char, char)>, Self::Error> {
        Ok(match node {
            ClassItem::Range(lo, hi) => {
                if lo > hi {
                    return Err(RegexError {
                        message: format!("invalid range {}-{}", lo, hi),
                        offset: 0,
                    });
                }
                vec![(lo, hi)]
            }
            ClassItem::Char(ch) => vec![(ch, ch)],
            ClassItem::Shorthand(s) => s.resolved_ranges(),
        })
    }
}

impl gazelle::Action<ClassChar<Self>> for NfaBuilder {
    fn build(&mut self, node: ClassChar<Self>) -> Result<char, Self::Error> {
        Ok(match node {
            ClassChar::Char(ch) => ch,
            ClassChar::Dot => '.',
            ClassChar::Star => '*',
            ClassChar::Plus => '+',
            ClassChar::Question => '?',
            ClassChar::Pipe => '|',
            ClassChar::Lparen => '(',
            ClassChar::Rparen => ')',
            ClassChar::Caret => '^',
            ClassChar::Dash => '-',
        })
    }
}

// ============================================================================
// Stateless lexer
// ============================================================================

fn lex_regex(input: &[u8]) -> Result<Vec<Terminal<NfaBuilder>>, RegexError> {
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        let b = input[pos];
        let tok = match b {
            b'*' => {
                pos += 1;
                Terminal::Star
            }
            b'+' => {
                pos += 1;
                Terminal::Plus
            }
            b'?' => {
                pos += 1;
                Terminal::Question
            }
            b'.' => {
                pos += 1;
                Terminal::Dot
            }
            b'|' => {
                pos += 1;
                Terminal::Pipe
            }
            b'(' => {
                pos += 1;
                Terminal::Lparen
            }
            b')' => {
                pos += 1;
                Terminal::Rparen
            }
            b'[' => {
                pos += 1;
                Terminal::Lbracket
            }
            b']' => {
                pos += 1;
                Terminal::Rbracket
            }
            b'^' => {
                pos += 1;
                Terminal::Caret
            }
            b'-' => {
                pos += 1;
                Terminal::Dash
            }
            b'\\' => {
                pos += 1;
                let c = *input.get(pos).ok_or_else(|| RegexError {
                    message: "unexpected end after '\\'".into(),
                    offset: pos,
                })?;
                pos += 1;
                match c {
                    b'd' => Terminal::Shorthand(Shorthand::Digit),
                    b'D' => Terminal::Shorthand(Shorthand::NotDigit),
                    b'w' => Terminal::Shorthand(Shorthand::Word),
                    b'W' => Terminal::Shorthand(Shorthand::NotWord),
                    b's' => Terminal::Shorthand(Shorthand::Space),
                    b'S' => Terminal::Shorthand(Shorthand::NotSpace),
                    b'n' => Terminal::Char('\n'),
                    b't' => Terminal::Char('\t'),
                    b'r' => Terminal::Char('\r'),
                    b'x' => {
                        let h1 = *input.get(pos).ok_or_else(|| RegexError {
                            message: "expected hex digit".into(),
                            offset: pos,
                        })?;
                        let h2 = *input.get(pos + 1).ok_or_else(|| RegexError {
                            message: "expected hex digit".into(),
                            offset: pos + 1,
                        })?;
                        let v = hex_val(h1).ok_or_else(|| RegexError {
                            message: "invalid hex digit".into(),
                            offset: pos,
                        })? * 16
                            + hex_val(h2).ok_or_else(|| RegexError {
                                message: "invalid hex digit".into(),
                                offset: pos + 1,
                            })?;
                        pos += 2;
                        Terminal::Char(char::from(v))
                    }
                    b'\\' | b'|' | b'(' | b')' | b'[' | b']' | b'*' | b'+' | b'?' | b'.' | b'^'
                    | b'$' | b'{' | b'}' | b'-' => Terminal::Char(c as char),
                    _ => {
                        return Err(RegexError {
                            message: format!("unknown escape '\\{}'", c as char),
                            offset: pos - 1,
                        });
                    }
                }
            }
            _ => {
                // Decode full UTF-8 codepoint and emit as single CHAR token
                let s = core::str::from_utf8(&input[pos..]).map_err(|_| RegexError {
                    message: "invalid UTF-8".into(),
                    offset: pos,
                })?;
                let ch = s.chars().next().unwrap();
                pos += ch.len_utf8();
                Terminal::Char(ch)
            }
        };
        tokens.push(tok);
    }

    Ok(tokens)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Parse a regex pattern and produce a byte-level NFA.
///
/// State 0 is the start state. The returned NFA's accept state is the last state added.
/// The caller determines which state is accepting (typically `nfa.num_states() - 1`
/// after construction, or tracked via the returned accept state).
///
/// Supported syntax:
/// - Literals: `a`, `\n`, `\t`, `\\`, `\xNN`
/// - Concatenation: `ab`
/// - Alternation: `a|b`
/// - Repetition: `*`, `+`, `?`
/// - Grouping: `(a|b)*`
/// - Character classes: `[a-z]`, `[^0-9]`, `[a-zA-Z_]`
/// - Dot: `.` (any byte except `\n`)
/// - Shorthand classes: `\d`, `\w`, `\s` and negations `\D`, `\W`, `\S`
pub fn regex_to_nfa(pattern: &str) -> Result<(Nfa, usize), RegexError> {
    let tokens = lex_regex(pattern.as_bytes())?;

    let mut builder = NfaBuilder { nfa: Nfa::new() };
    let state0 = builder.nfa.add_state();
    debug_assert_eq!(state0, 0);

    let mut parser = Parser::<NfaBuilder>::new();
    for tok in tokens {
        parser = parser.push(tok, &mut builder).map_err(|e| match e {
            crate::ParseError::Syntax { terminal, .. } => RegexError {
                message: format!("unexpected terminal {:?}", terminal),
                offset: 0,
            },
            crate::ParseError::Action { error, .. } => error,
        })?;
    }
    let frag = parser.finish(&mut builder).map_err(|e| match e {
        crate::ParseError::Syntax { terminal, .. } => RegexError {
            message: format!("unexpected terminal {:?}", terminal),
            offset: 0,
        },
        crate::ParseError::Action { error, .. } => error,
    })?;

    builder.nfa.add_epsilon(0, frag.start);
    Ok((builder.nfa, frag.end))
}

/// Build a [`LexerDfa`] from a set of `(terminal_id, regex)` patterns.
///
/// Lower terminal_id = higher priority for equal-length matches.
/// The returned DFA implements longest-match semantics via
/// [`LexerDfa::read_token`].
///
/// ```
/// use gazelle::lexer::{LexerDfa, Scanner};
/// use gazelle::regex::build_lexer_dfa;
///
/// let dfa = build_lexer_dfa(&[
///     (0, "[a-zA-Z_][a-zA-Z0-9_]*"),  // identifier
///     (1, "[0-9]+"),                     // number
///     (2, r"[+\-*/]"),                   // operator
/// ]).unwrap();
///
/// let mut s = Scanner::new("foo123 +");
/// assert_eq!(dfa.read_token(&mut s), Some((0, 0..6)));
/// ```
pub fn build_lexer_dfa(
    patterns: &[(u16, &str)],
) -> Result<crate::lexer::OwnedLexerDfa, RegexError> {
    use crate::automaton;

    // Build individual NFAs, then combine
    let mut nfas: Vec<(u16, Nfa, usize)> = Vec::new();
    for &(tid, pattern) in patterns {
        let (nfa, accept) = regex_to_nfa(pattern)?;
        nfas.push((tid, nfa, accept));
    }

    let mut combined = Nfa::new();
    let combined_start = combined.add_state();
    debug_assert_eq!(combined_start, 0);

    let mut nfa_accept_states: Vec<(usize, u16)> = Vec::new();

    for (tid, nfa, accept) in &nfas {
        let offset = combined.num_states();
        for _ in 0..nfa.num_states() {
            combined.add_state();
        }
        for (state, transitions) in nfa.transitions().iter().enumerate() {
            for &(sym, target) in transitions {
                combined.add_transition(state + offset, sym, target + offset);
            }
        }
        for (state, epsilons) in nfa.epsilons().iter().enumerate() {
            for &target in epsilons {
                combined.add_epsilon(state + offset, target + offset);
            }
        }
        combined.add_epsilon(0, offset);
        nfa_accept_states.push((accept + offset, *tid));
    }

    let (raw_dfa, raw_nfa_sets) = automaton::subset_construction(&combined);

    // Determine accept for each DFA state (min tid wins)
    let nfa_accept_set: BTreeMap<usize, u16> = nfa_accept_states.into_iter().collect();

    let mut dfa_accept: Vec<u16> = Vec::with_capacity(raw_dfa.num_states());
    for nfa_set in &raw_nfa_sets {
        let mut best = u16::MAX;
        for &nfa_state in nfa_set {
            if let Some(&tid) = nfa_accept_set.get(&nfa_state) {
                best = best.min(tid);
            }
        }
        dfa_accept.push(best);
    }

    // Hopcroft minimize with initial partition by accept terminal
    let mut partition_ids: BTreeMap<u16, usize> = BTreeMap::new();
    let mut next_partition = 0usize;
    let initial_partition: Vec<usize> = dfa_accept
        .iter()
        .map(|&tid| {
            *partition_ids.entry(tid).or_insert_with(|| {
                let p = next_partition;
                next_partition += 1;
                p
            })
        })
        .collect();

    let (min_dfa, state_map) = automaton::hopcroft_minimize(&raw_dfa, &initial_partition);

    // Map accept through minimization
    let mut min_accept = vec![u16::MAX; min_dfa.num_states()];
    for (old_state, &tid) in dfa_accept.iter().enumerate() {
        let new_state = state_map[old_state];
        if tid < min_accept[new_state] {
            min_accept[new_state] = tid;
        }
    }

    // Symbol classes (byte-level: 256 symbols)
    let (class_map_vec, num_classes) = automaton::symbol_classes(&min_dfa, 256);
    let _ = num_classes;

    let mut class_map = [0u8; 256];
    for (i, &c) in class_map_vec.iter().enumerate() {
        class_map[i] = c as u8;
    }

    // Collect sparse accept pairs
    let accept: Vec<(usize, u16)> = min_accept
        .iter()
        .enumerate()
        .filter(|(_, tid)| **tid != u16::MAX)
        .map(|(s, tid)| (s, *tid))
        .collect();

    Ok(crate::lexer::OwnedLexerDfa::from_dfa(
        &min_dfa, &accept, class_map,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automaton;

    fn matches(pattern: &str, input: &[u8]) -> bool {
        let (nfa, accept) = regex_to_nfa(pattern).unwrap();
        let (dfa, nfa_sets) = automaton::subset_construction(&nfa);
        // Walk DFA
        let mut state = 0usize;
        for &b in input {
            let mut next = None;
            for &(sym, target) in &dfa.transitions[state] {
                if sym == b as u32 {
                    next = Some(target);
                    break;
                }
            }
            match next {
                Some(s) => state = s,
                None => return false,
            }
        }
        // Check if current DFA state contains the NFA accept state
        nfa_sets[state].contains(&accept)
    }

    #[test]
    fn test_literal() {
        assert!(matches("abc", b"abc"));
        assert!(!matches("abc", b"ab"));
        assert!(!matches("abc", b"abcd"));
    }

    #[test]
    fn test_alternation() {
        assert!(matches("a|b", b"a"));
        assert!(matches("a|b", b"b"));
        assert!(!matches("a|b", b"c"));
        assert!(!matches("a|b", b"ab"));
    }

    #[test]
    fn test_star() {
        assert!(matches("a*", b""));
        assert!(matches("a*", b"a"));
        assert!(matches("a*", b"aaa"));
        assert!(!matches("a*", b"b"));
    }

    #[test]
    fn test_plus() {
        assert!(!matches("a+", b""));
        assert!(matches("a+", b"a"));
        assert!(matches("a+", b"aaa"));
    }

    #[test]
    fn test_question() {
        assert!(matches("a?", b""));
        assert!(matches("a?", b"a"));
        assert!(!matches("a?", b"aa"));
    }

    #[test]
    fn test_grouping() {
        assert!(matches("(ab)+", b"ab"));
        assert!(matches("(ab)+", b"abab"));
        assert!(!matches("(ab)+", b""));
        assert!(!matches("(ab)+", b"a"));
    }

    #[test]
    fn test_char_class() {
        assert!(matches("[abc]", b"a"));
        assert!(matches("[abc]", b"b"));
        assert!(matches("[abc]", b"c"));
        assert!(!matches("[abc]", b"d"));
    }

    #[test]
    fn test_char_class_range() {
        assert!(matches("[a-z]", b"a"));
        assert!(matches("[a-z]", b"m"));
        assert!(matches("[a-z]", b"z"));
        assert!(!matches("[a-z]", b"A"));
        assert!(!matches("[a-z]", b"0"));
    }

    #[test]
    fn test_char_class_negated() {
        assert!(!matches("[^a-z]", b"a"));
        assert!(matches("[^a-z]", b"0"));
        assert!(matches("[^a-z]", b"A"));
    }

    #[test]
    fn test_dot() {
        assert!(matches(".", b"a"));
        assert!(matches(".", b"0"));
        assert!(!matches(".", b"\n"));
        assert!(!matches(".", b""));
        // UTF-8 multibyte codepoints
        assert!(matches(".", "é".as_bytes())); // 2-byte
        assert!(matches(".", "€".as_bytes())); // 3-byte
        assert!(matches(".", "𝄞".as_bytes())); // 4-byte (U+1D11E)
        assert!(!matches(".", b"\xc3")); // lone lead byte
        assert!(!matches(".", b"\x80")); // lone continuation byte
    }

    #[test]
    fn test_escape() {
        assert!(matches(r"\n", b"\n"));
        assert!(matches(r"\t", b"\t"));
        assert!(matches(r"\\", b"\\"));
        assert!(matches(r"\x41", b"A"));
    }

    #[test]
    fn test_complex_pattern() {
        // Identifier: [a-zA-Z_][a-zA-Z0-9_]*
        assert!(matches("[a-zA-Z_][a-zA-Z0-9_]*", b"foo"));
        assert!(matches("[a-zA-Z_][a-zA-Z0-9_]*", b"_bar"));
        assert!(matches("[a-zA-Z_][a-zA-Z0-9_]*", b"x1"));
        assert!(!matches("[a-zA-Z_][a-zA-Z0-9_]*", b"1x"));
        assert!(!matches("[a-zA-Z_][a-zA-Z0-9_]*", b""));
    }

    #[test]
    fn test_shorthand_digit() {
        assert!(matches(r"\d+", b"123"));
        assert!(!matches(r"\d+", b"abc"));
        assert!(!matches(r"\d+", b""));
    }

    #[test]
    fn test_shorthand_word() {
        assert!(matches(r"\w+", b"hello_123"));
        assert!(!matches(r"\w+", b""));
    }

    #[test]
    fn test_shorthand_space() {
        assert!(matches(r"\s+", b" \t\n"));
        assert!(!matches(r"\s+", b"a"));
    }

    #[test]
    fn test_escaped_metachar() {
        assert!(matches(r"\.", b"."));
        assert!(!matches(r"\.", b"a"));
        assert!(matches(r"\*", b"*"));
        assert!(matches(r"\+", b"+"));
    }

    #[test]
    fn test_utf8_literal() {
        assert!(matches("café", "café".as_bytes()));
        assert!(!matches("café", "cafe".as_bytes()));
        assert!(matches("é", "é".as_bytes()));
        assert!(!matches("é", "e".as_bytes()));
    }

    #[test]
    fn test_utf8_char_class() {
        assert!(matches("[é]", "é".as_bytes()));
        assert!(!matches("[é]", "e".as_bytes()));
        assert!(matches("[aé]", "a".as_bytes()));
        assert!(matches("[aé]", "é".as_bytes()));
        assert!(!matches("[aé]", "b".as_bytes()));
    }

    #[test]
    fn test_utf8_char_class_range() {
        // a (U+0061) through é (U+00E9)
        assert!(matches("[a-é]", "a".as_bytes()));
        assert!(matches("[a-é]", "z".as_bytes()));
        assert!(matches("[a-é]", "é".as_bytes()));
        assert!(matches("[a-é]", "à".as_bytes())); // U+00E0, within range
        assert!(!matches("[a-é]", "ë".as_bytes())); // U+00EB, past range
    }

    #[test]
    fn test_utf8_negated_class() {
        assert!(!matches("[^é]", "é".as_bytes()));
        assert!(matches("[^é]", "a".as_bytes()));
        assert!(matches("[^é]", "€".as_bytes())); // 3-byte char
        assert!(matches("[^é]", "𝄞".as_bytes())); // 4-byte char
    }

    #[test]
    fn test_utf8_negated_shorthand() {
        // \W matches non-word chars
        assert!(matches(r"\W", ".".as_bytes()));
        assert!(matches(r"\W", "é".as_bytes())); // non-ASCII, not in \w
        assert!(!matches(r"\W", "a".as_bytes()));
        assert!(!matches(r"\W", "0".as_bytes()));
    }

    #[test]
    fn test_range_utils() {
        let mut r = vec![('d', 'f'), ('a', 'c')];
        normalize_ranges(&mut r);
        assert_eq!(r, vec![('a', 'f')]);

        let mut r = vec![('a', 'c'), ('e', 'g')];
        normalize_ranges(&mut r);
        assert_eq!(r, vec![('a', 'c'), ('e', 'g')]);

        // Adjacent ranges merge
        let mut r = vec![('a', 'c'), ('d', 'f')];
        normalize_ranges(&mut r);
        assert_eq!(r, vec![('a', 'f')]);

        // Complement of single char
        let c = complement_ranges(&[('a', 'a')]);
        assert_eq!(c[0], ('\0', '`')); // before 'a'
        assert_eq!(c[1], ('b', '\u{D7FF}')); // after 'a' up to surrogate gap
    }

    // Note: empty alternatives like (|a) are not supported by the grammar.
    // The grammar requires at least one repetition per concat branch.
}
