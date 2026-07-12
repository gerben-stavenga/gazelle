//! Composable lexer building blocks with position tracking.
//!
//! [`Scanner`] wraps any char iterator and tracks byte offsets and line starts.
//! Its methods are building blocks — compose them to build a lexer for your grammar's terminals.
//!
//! ```
//! use gazelle::lexer::Scanner;
//!
//! let input = "foo + 123";
//! let mut src = Scanner::new(input);
//!
//! src.skip_whitespace();
//! if let Some(span) = src.read_ident() {
//!     let text = &input[span];  // "foo"
//!     // Line/col computed on demand: src.line_col(span.start)
//! }
//! ```

use core::ops::Range;

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::{format, vec, vec::Vec};

// ============================================================================
// Scanner - Composable lexer building blocks with position tracking
// ============================================================================

/// Error from lexer operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// The error message describing what went wrong.
    pub message: String,
    /// Byte offset in the source where the error occurred.
    pub offset: usize,
}

impl LexError {
    /// Format error with line/column from a Scanner.
    pub fn format<I: Iterator<Item = char>>(&self, src: &Scanner<I>) -> String {
        let (line, col) = src.line_col(self.offset);
        format!("{}:{}: {}", line, col, self.message)
    }
}

impl core::fmt::Display for LexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "offset {}: {}", self.offset, self.message)
    }
}

impl core::error::Error for LexError {}

/// Position-tracking character scanner for building lexers.
///
/// Wraps any `Iterator<Item = char>` and tracks byte offset and line starts.
/// Line/column are computed on demand from offset.
///
/// # Example
///
/// ```
/// use gazelle::lexer::Scanner;
///
/// let input = "hello 123";
/// let mut src = Scanner::new(input);
///
/// src.skip_whitespace();
/// if let Some(span) = src.read_ident() {
///     let ident = &input[span];
///     assert_eq!(ident, "hello");
///     // Line/col on demand: src.line_col(span.start)
/// }
/// ```
pub struct Scanner<I: Iterator<Item = char>> {
    chars: I,
    /// Lookahead buffer for peeking without consuming.
    lookahead: VecDeque<char>,
    /// Current byte offset.
    offset: usize,
    /// Byte offsets where each line starts. line_starts[0] = 0 (line 1 starts at offset 0).
    line_starts: Vec<usize>,
}

impl<'a> Scanner<core::str::Chars<'a>> {
    /// Create a new Scanner from a string slice.
    pub fn new(input: &'a str) -> Self {
        Self::from_chars(input.chars())
    }
}

impl<I: Iterator<Item = char>> Scanner<I> {
    /// Create a new Scanner from any char iterator.
    pub fn from_chars(iter: I) -> Self {
        Self {
            chars: iter,
            lookahead: VecDeque::new(),
            offset: 0,
            line_starts: vec![0], // Line 1 starts at offset 0
        }
    }

    /// Current byte offset.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Compute line and column (1-indexed) from a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        // Binary search for the line containing this offset
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let line_start = self.line_starts[line - 1];
        let col = offset - line_start + 1;
        (line, col)
    }

    /// Peek at the next character without consuming it.
    pub fn peek(&mut self) -> Option<char> {
        if self.lookahead.is_empty()
            && let Some(c) = self.chars.next()
        {
            self.lookahead.push_back(c);
        }
        self.lookahead.front().copied()
    }

    /// Peek at the nth character ahead (0 = next char).
    pub fn peek_n(&mut self, n: usize) -> Option<char> {
        while self.lookahead.len() <= n {
            let c = self.chars.next()?;
            self.lookahead.push_back(c);
        }
        self.lookahead.get(n).copied()
    }

    /// Consume and return the next character.
    pub fn advance(&mut self) -> Option<char> {
        let c = if let Some(c) = self.lookahead.pop_front() {
            c
        } else {
            self.chars.next()?
        };

        self.offset += c.len_utf8();
        if c == '\n' {
            self.line_starts.push(self.offset);
        }

        Some(c)
    }

    /// Check if we've reached the end of input.
    pub fn at_end(&mut self) -> bool {
        self.peek().is_none()
    }

    // ========================================================================
    // Skipping methods
    // ========================================================================

    /// Skip whitespace characters (space, tab, newline, carriage return).
    pub fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip characters while the predicate returns true.
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let mut src = Scanner::new("   hello");
    /// src.skip_while(|c| c == ' ');
    /// assert_eq!(src.peek(), Some('h'));
    /// ```
    pub fn skip_while(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip a line comment starting with the given prefix.
    /// Returns true if a comment was skipped.
    pub fn skip_line_comment(&mut self, prefix: &str) -> bool {
        if !self.starts_with(prefix) {
            return false;
        }
        // Consume the prefix
        for _ in 0..prefix.chars().count() {
            self.advance();
        }
        // Skip to end of line
        while let Some(c) = self.advance() {
            if c == '\n' {
                break;
            }
        }
        true
    }

    /// Skip a block comment with the given open/close delimiters.
    /// Returns true if a comment was skipped.
    pub fn skip_block_comment(&mut self, open: &str, close: &str) -> bool {
        if !self.starts_with(open) {
            return false;
        }
        // Consume the opening delimiter
        for _ in 0..open.chars().count() {
            self.advance();
        }
        // Find the closing delimiter
        let close_chars: Vec<char> = close.chars().collect();
        loop {
            if self.at_end() {
                break; // Unterminated comment
            }
            // Check for close delimiter
            let mut matched = true;
            for (i, &expected) in close_chars.iter().enumerate() {
                if self.peek_n(i) != Some(expected) {
                    matched = false;
                    break;
                }
            }
            if matched {
                for _ in 0..close_chars.len() {
                    self.advance();
                }
                break;
            }
            self.advance();
        }
        true
    }

    // ========================================================================
    // Reading methods - return Range<usize> on success
    // ========================================================================

    /// Read an identifier (letter/underscore followed by alphanumerics/underscores).
    /// Returns the span of the identifier, or None if not at an identifier.
    pub fn read_ident(&mut self) -> Option<Range<usize>> {
        let c = self.peek()?;
        if !c.is_alphabetic() && c != '_' {
            return None;
        }
        let start = self.offset;
        self.advance();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        Some(start..self.offset)
    }

    /// Read an identifier with custom start/continue predicates.
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = "$foo123";
    /// let mut src = Scanner::new(input);
    /// let span = src.read_ident_where(|c| c == '$' || c.is_alphabetic(), |c| c.is_alphanumeric()).unwrap();
    /// assert_eq!(&input[span], "$foo123");
    /// ```
    pub fn read_ident_where(
        &mut self,
        is_start: impl Fn(char) -> bool,
        is_continue: impl Fn(char) -> bool,
    ) -> Option<Range<usize>> {
        let c = self.peek()?;
        if !is_start(c) {
            return None;
        }
        let start = self.offset;
        self.advance();
        while let Some(c) = self.peek() {
            if is_continue(c) {
                self.advance();
            } else {
                break;
            }
        }
        Some(start..self.offset)
    }

    /// Read decimal digits (0-9 and optional underscores).
    /// Returns None if not starting with a digit.
    pub fn read_digits(&mut self) -> Option<Range<usize>> {
        let c = self.peek()?;
        if !c.is_ascii_digit() {
            return None;
        }
        let start = self.offset;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        Some(start..self.offset)
    }

    /// Read hex digits (0-9, a-f, A-F, and optional underscores).
    /// Returns None if not starting with a hex digit.
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = "ff00";
    /// let mut src = Scanner::new(input);
    /// let span = src.read_hex_digits().unwrap();
    /// assert_eq!(&input[span], "ff00");
    /// ```
    pub fn read_hex_digits(&mut self) -> Option<Range<usize>> {
        let c = self.peek()?;
        if !c.is_ascii_hexdigit() {
            return None;
        }
        let start = self.offset;
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        Some(start..self.offset)
    }

    /// Read until any of the given characters (or EOF).
    /// Does not consume the stopping character.
    /// Returns span of content read (may be empty if immediately at a stop char).
    pub fn read_until_any(&mut self, chars: &[char]) -> Range<usize> {
        let start = self.offset;
        while let Some(c) = self.peek() {
            if chars.contains(&c) {
                break;
            }
            self.advance();
        }
        start..self.offset
    }

    /// Read a quoted string, skipping escape sequences without interpreting them.
    /// Returns span of raw content (excluding quotes).
    /// Use this when you just need to find the string boundary.
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = r#""hello \"world\"""#;
    /// let mut src = Scanner::new(input);
    /// let span = src.read_string_raw('"').unwrap();
    /// assert_eq!(&input[span], r#"hello \"world\""#);
    /// ```
    pub fn read_string_raw(&mut self, quote: char) -> Result<Range<usize>, LexError> {
        if self.peek() != Some(quote) {
            return Err(self.error(format!("expected '{}'", quote)));
        }
        self.advance(); // opening quote
        let start = self.offset;

        loop {
            self.read_until_any(&[quote, '\\']);
            match self.peek() {
                Some(c) if c == quote => {
                    let end = self.offset;
                    self.advance(); // closing quote
                    return Ok(start..end);
                }
                Some('\\') => {
                    self.advance(); // consume backslash
                    self.advance(); // skip next char
                }
                None => {
                    return Err(self.error("unterminated string"));
                }
                Some(_) => unreachable!(),
            }
        }
    }

    /// Read a C-style string with standard escape sequences.
    /// Consumes opening and closing quotes, returns (span of raw content, interpreted value).
    /// Handles: `\n` `\t` `\r` `\\` `\'` `\"` `\0` `\xNN`
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = r#""hello\nworld""#;
    /// let mut src = Scanner::new(input);
    /// let (span, value) = src.read_c_string('"', input).unwrap();
    /// assert_eq!(value, "hello\nworld");
    /// ```
    pub fn read_c_string(
        &mut self,
        quote: char,
        input: &str,
    ) -> Result<(Range<usize>, String), LexError> {
        if self.peek() != Some(quote) {
            return Err(self.error(format!("expected '{}'", quote)));
        }
        self.advance(); // opening quote
        let start = self.offset;
        let mut value = String::new();

        loop {
            let span = self.read_until_any(&[quote, '\\']);
            value.push_str(&input[span]);

            match self.peek() {
                Some(c) if c == quote => {
                    let end = self.offset;
                    self.advance(); // closing quote
                    return Ok((start..end, value));
                }
                Some('\\') => {
                    let esc_offset = self.offset;
                    self.advance(); // consume backslash
                    match self.peek() {
                        Some('n') => {
                            value.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            value.push('\t');
                            self.advance();
                        }
                        Some('r') => {
                            value.push('\r');
                            self.advance();
                        }
                        Some('\\') => {
                            value.push('\\');
                            self.advance();
                        }
                        Some('\'') => {
                            value.push('\'');
                            self.advance();
                        }
                        Some('"') => {
                            value.push('"');
                            self.advance();
                        }
                        Some('0') => {
                            value.push('\0');
                            self.advance();
                        }
                        Some('x') => {
                            self.advance(); // consume 'x'
                            let h1 = self.peek().and_then(|c| c.to_digit(16));
                            let h2 = self.peek_n(1).and_then(|c| c.to_digit(16));
                            match (h1, h2) {
                                (Some(a), Some(b)) => {
                                    self.advance();
                                    self.advance();
                                    value.push(char::from((a * 16 + b) as u8));
                                }
                                _ => {
                                    return Err(LexError {
                                        message: "invalid \\xNN escape".into(),
                                        offset: esc_offset,
                                    });
                                }
                            }
                        }
                        Some(c) => {
                            return Err(LexError {
                                message: format!("invalid escape sequence: \\{}", c),
                                offset: esc_offset,
                            });
                        }
                        None => {
                            return Err(self.error("unterminated escape sequence"));
                        }
                    }
                }
                None => {
                    return Err(self.error("unterminated string"));
                }
                Some(_) => unreachable!("read_until_any should stop at quote or backslash"),
            }
        }
    }

    /// Read a Rust-style raw string: `r"..."` or `r#"..."#`
    ///
    /// Caller has consumed `r` (and any `#` characters) and passes the number
    /// of `#` consumed (0 for `r"..."`). Returns span of content (excluding
    /// quotes and hashes).
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = r###"r#"hello "world""#"###;
    /// let mut src = Scanner::new(input);
    /// src.advance(); // consume 'r'
    /// src.advance(); // consume '#'
    /// let span = src.read_rust_raw_string(1).unwrap();
    /// assert_eq!(&input[span], r#"hello "world""#);
    /// ```
    pub fn read_rust_raw_string(&mut self, hashes: usize) -> Result<Range<usize>, LexError> {
        if self.peek() != Some('"') {
            return Err(self.error("expected '\"' after r"));
        }
        self.advance(); // opening quote
        let start = self.offset;

        loop {
            self.read_until_any(&['"']);

            if self.at_end() {
                return Err(self.error("unterminated raw string"));
            }

            let potential_end = self.offset;
            self.advance(); // consume "

            // Count following #s
            let mut hash_count = 0;
            while self.peek() == Some('#') && hash_count < hashes {
                self.advance();
                hash_count += 1;
            }

            if hash_count == hashes {
                return Ok(start..potential_end);
            }
            // Otherwise the " and #s were part of content, continue
        }
    }

    /// Read a C++11 raw string: `R"delim(...)delim"`
    ///
    /// Caller has consumed `R`, this consumes the rest.
    /// Returns span of content (excluding delimiters).
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = r#"R"(hello)""#;
    /// let mut src = Scanner::new(input);
    /// src.advance(); // consume 'R'
    /// let span = src.read_cpp_raw_string(input).unwrap();
    /// assert_eq!(&input[span], "hello");
    /// ```
    pub fn read_cpp_raw_string(&mut self, input: &str) -> Result<Range<usize>, LexError> {
        if self.peek() != Some('"') {
            return Err(self.error("expected '\"' after R"));
        }
        self.advance();

        // Read delimiter until '('
        let delim_start = self.offset;
        while self.peek() != Some('(') && !self.at_end() {
            self.advance();
        }
        if self.at_end() {
            return Err(self.error("expected '(' in raw string"));
        }
        let delimiter = &input[delim_start..self.offset];
        self.advance(); // consume (

        let content_start = self.offset;

        // Look for )delimiter"
        let closing = format!("){}\"", delimiter);

        loop {
            self.read_until_any(&[')']);

            if self.at_end() {
                return Err(self.error("unterminated raw string"));
            }

            let potential_end = self.offset;

            if self.starts_with(&closing) {
                // Consume the closing
                for _ in closing.chars() {
                    self.advance();
                }
                return Ok(content_start..potential_end);
            }

            self.advance(); // this ) wasn't the end, continue
        }
    }

    /// Read characters while the predicate returns true.
    /// Returns span of matched characters (may be empty).
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let input = "aaabbb";
    /// let mut src = Scanner::new(input);
    /// let span = src.read_while(|c| c == 'a');
    /// assert_eq!(&input[span], "aaa");
    /// ```
    pub fn read_while(&mut self, pred: impl Fn(char) -> bool) -> Range<usize> {
        let start = self.offset;
        while let Some(c) = self.peek() {
            if pred(c) {
                self.advance();
            } else {
                break;
            }
        }
        start..self.offset
    }

    /// Try to consume an exact string. Returns span if matched, None otherwise.
    /// Only consumes if the entire string matches.
    pub fn read_exact(&mut self, s: &str) -> Option<Range<usize>> {
        if !self.starts_with(s) {
            return None;
        }
        let start = self.offset;
        for _ in s.chars() {
            self.advance();
        }
        Some(start..self.offset)
    }

    /// Try to match one of the given strings, checking in order.
    /// Returns the index of the matched string and its span on success.
    ///
    /// Use this for maximal munch: put longer options first.
    /// The index can be used to look up corresponding data in a parallel array.
    ///
    /// # Example
    /// ```
    /// use gazelle::lexer::Scanner;
    ///
    /// let input = "<<= foo";
    /// let mut src = Scanner::new(input);
    ///
    /// const OPS: &[&str] = &["<<=", "<<", "<=", "<"];
    ///
    /// // Longest first for maximal munch
    /// if let Some((idx, span)) = src.read_one_of(OPS) {
    ///     assert_eq!(idx, 0);  // matched "<<=", first in list
    ///     assert_eq!(&input[span], "<<=");
    /// }
    /// ```
    pub fn read_one_of(&mut self, options: &[&str]) -> Option<(usize, Range<usize>)> {
        for (i, &option) in options.iter().enumerate() {
            if let Some(span) = self.read_exact(option) {
                return Some((i, span));
            }
        }
        None
    }

    /// Check if the remaining input starts with the given string.
    /// Does not consume any input.
    ///
    /// ```
    /// # use gazelle::lexer::Scanner;
    /// let mut src = Scanner::new("// comment");
    /// assert!(src.starts_with("//"));
    /// assert!(!src.starts_with("/*"));
    /// ```
    pub fn starts_with(&mut self, s: &str) -> bool {
        for (i, expected) in s.chars().enumerate() {
            if self.peek_n(i) != Some(expected) {
                return false;
            }
        }
        true
    }

    /// Create an error at the current offset.
    pub fn error(&self, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            offset: self.offset,
        }
    }
}

// ============================================================================
// LexerDfa - Compiled multi-pattern DFA for regex-based lexing
// ============================================================================

/// Compiled multi-pattern lexer DFA with flat transition table for fast matching.
///
/// This type borrows its data from static arrays or an [`OwnedLexerDfa`].
/// For building a DFA from regex patterns, use
/// [`regex::build_lexer_dfa`](crate::regex::build_lexer_dfa) which returns
/// an [`OwnedLexerDfa`].
pub struct LexerDfa<'a> {
    /// Flat transition table: `transitions[state * num_classes + class] = next_state`.
    /// State 0 is the dead state (no transitions lead anywhere useful).
    /// State 1 is the start state.
    transitions: &'a [u16],
    num_classes: usize,
    class_map: &'a [u8; 256],
    /// `accept[state]` = terminal_id if accepting, `u16::MAX` if not.
    accept: &'a [u16],
}

impl<'a> LexerDfa<'a> {
    /// Create a `LexerDfa` from borrowed slices (typically static data).
    pub const fn new(
        transitions: &'a [u16],
        num_classes: usize,
        class_map: &'a [u8; 256],
        accept: &'a [u16],
    ) -> Self {
        LexerDfa {
            transitions,
            num_classes,
            class_map,
            accept,
        }
    }

    /// Access the transition table.
    #[doc(hidden)]
    pub fn transitions(&self) -> &[u16] {
        self.transitions
    }

    /// Access the number of equivalence classes.
    #[doc(hidden)]
    pub fn num_classes(&self) -> usize {
        self.num_classes
    }

    /// Access the byte-to-class map.
    #[doc(hidden)]
    pub fn class_map(&self) -> &[u8; 256] {
        self.class_map
    }

    /// Access the accept table.
    #[doc(hidden)]
    pub fn accept(&self) -> &[u16] {
        self.accept
    }

    fn step(&self, state: u16, byte: u8) -> u16 {
        let class = self.class_map[byte as usize] as usize;
        self.transitions[state as usize * self.num_classes + class]
    }
}

impl<'a> LexerDfa<'a> {
    /// Read the longest matching token from the scanner.
    /// Returns `(terminal_id, span)` or `None` if no pattern matches.
    /// On match, the scanner is advanced past the matched characters.
    /// On no match, the scanner is unchanged.
    pub fn read_token<I: Iterator<Item = char>>(
        &self,
        scanner: &mut Scanner<I>,
    ) -> Option<(u16, Range<usize>)> {
        let mut state = 1u16; // start state
        let mut last_accept: Option<(u16, usize)> = None;
        let start = scanner.offset();
        let mut chars_consumed = 0usize;
        let mut accept_chars = 0usize;

        if self.accept[state as usize] != u16::MAX {
            last_accept = Some((self.accept[state as usize], 0));
        }

        loop {
            let ch = scanner.peek_n(chars_consumed);
            let Some(ch) = ch else { break };

            // Step through each UTF-8 byte of this char
            let mut buf = [0u8; 4];
            let bytes = ch.encode_utf8(&mut buf).as_bytes();
            let mut dead = false;
            for &byte in bytes {
                state = self.step(state, byte);
                if state == 0 {
                    dead = true;
                    break;
                }
            }
            if dead {
                break;
            }

            chars_consumed += 1;
            if self.accept[state as usize] != u16::MAX {
                last_accept = Some((self.accept[state as usize], chars_consumed));
                accept_chars = chars_consumed;
            }
        }

        let (tid, _) = last_accept?;
        // Advance scanner past the accepted chars
        for _ in 0..accept_chars {
            scanner.advance();
        }
        Some((tid, start..scanner.offset()))
    }
}

// ============================================================================
// OwnedLexerDfa - Heap-allocated multi-pattern DFA
// ============================================================================

use crate::automaton::Dfa;

/// Owned (heap-allocated) multi-pattern lexer DFA.
///
/// Build from a [`Dfa`] with [`OwnedLexerDfa::from_dfa`], or from regex patterns
/// with [`regex::build_lexer_dfa`](crate::regex::build_lexer_dfa).
///
/// Call [`as_ref()`](OwnedLexerDfa::as_ref) to get a borrowed [`LexerDfa`] for
/// reading tokens.
pub struct OwnedLexerDfa {
    transitions: Vec<u16>,
    num_classes: usize,
    class_map: [u8; 256],
    accept: Vec<u16>,
}

impl OwnedLexerDfa {
    /// Build an `OwnedLexerDfa` from a [`Dfa`], accepting states, and a byte-to-class map.
    ///
    /// - `dfa`: the DFA whose transitions use class ids (not raw bytes).
    ///   State 0 is the start state.
    /// - `accept`: `(state, terminal_id)` pairs for accepting states.
    ///   When multiple patterns accept at the same length, lower terminal_id wins.
    /// - `class_map`: maps each byte value (0..255) to a class id used in the DFA.
    pub fn from_dfa(dfa: &Dfa, accept: &[(usize, u16)], class_map: [u8; 256]) -> OwnedLexerDfa {
        let num_classes = *class_map.iter().max().unwrap() as usize + 1;

        // Insert dead state 0, shift all DFA states by 1
        let num_states = dfa.num_states() + 1;
        let mut transitions = vec![0u16; num_states * num_classes];

        for (old_state, trans) in dfa.transitions.iter().enumerate() {
            let new_state = old_state + 1;
            for &(sym, target) in trans {
                let class = class_map[sym as usize] as usize;
                transitions[new_state * num_classes + class] = (target + 1) as u16;
            }
        }

        let mut accept_table = vec![u16::MAX; num_states];
        for &(state, tid) in accept {
            let shifted = state + 1;
            if tid < accept_table[shifted] {
                accept_table[shifted] = tid;
            }
        }

        OwnedLexerDfa {
            transitions,
            num_classes,
            class_map,
            accept: accept_table,
        }
    }

    /// Get a borrowed [`LexerDfa`] reference for reading tokens.
    pub fn as_ref(&self) -> LexerDfa<'_> {
        LexerDfa {
            transitions: &self.transitions,
            num_classes: self.num_classes,
            class_map: &self.class_map,
            accept: &self.accept,
        }
    }

    /// Read the longest matching token from the scanner.
    /// Convenience method that delegates to [`LexerDfa::read_token`].
    pub fn read_token<I: Iterator<Item = char>>(
        &self,
        scanner: &mut Scanner<I>,
    ) -> Option<(u16, Range<usize>)> {
        self.as_ref().read_token(scanner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Scanner tests
    // ========================================================================

    #[test]
    fn test_source_line_col() {
        let input = "ab\ncd\nef";
        let mut src = Scanner::new(input);

        // Consume all chars to build line table
        while src.advance().is_some() {}

        // Test line/col computation
        assert_eq!(src.line_col(0), (1, 1)); // 'a'
        assert_eq!(src.line_col(1), (1, 2)); // 'b'
        assert_eq!(src.line_col(2), (1, 3)); // '\n'
        assert_eq!(src.line_col(3), (2, 1)); // 'c'
        assert_eq!(src.line_col(4), (2, 2)); // 'd'
        assert_eq!(src.line_col(5), (2, 3)); // '\n'
        assert_eq!(src.line_col(6), (3, 1)); // 'e'
    }

    #[test]
    fn test_source_peek() {
        let input = "abc";
        let mut src = Scanner::new(input);

        assert_eq!(src.peek(), Some('a'));
        assert_eq!(src.peek(), Some('a')); // peek doesn't consume
        assert_eq!(src.peek_n(0), Some('a'));
        assert_eq!(src.peek_n(1), Some('b'));
        assert_eq!(src.peek_n(2), Some('c'));
        assert_eq!(src.peek_n(3), None);

        assert_eq!(src.advance(), Some('a'));
        assert_eq!(src.peek(), Some('b'));
        assert_eq!(src.peek_n(1), Some('c'));
    }

    #[test]
    fn test_source_skip_whitespace() {
        let input = "  \t\n  hello";
        let mut src = Scanner::new(input);

        src.skip_whitespace();
        assert_eq!(src.peek(), Some('h'));
        assert_eq!(src.offset(), 6);
    }

    #[test]
    fn test_source_skip_line_comment() {
        let input = "// comment\nhello";
        let mut src = Scanner::new(input);

        assert!(src.skip_line_comment("//"));
        assert_eq!(src.peek(), Some('h'));
    }

    #[test]
    fn test_source_skip_block_comment() {
        let input = "/* block */hello";
        let mut src = Scanner::new(input);

        assert!(src.skip_block_comment("/*", "*/"));
        assert_eq!(src.peek(), Some('h'));
    }

    #[test]
    fn test_source_read_ident() {
        let input = "foo_bar123 + rest";
        let mut src = Scanner::new(input);

        let span = src.read_ident().unwrap();
        assert_eq!(&input[span], "foo_bar123");
        assert_eq!(src.peek(), Some(' '));
    }

    #[test]
    fn test_source_read_ident_where() {
        let input = "foo-bar-baz + rest";
        let mut src = Scanner::new(input);

        // Lisp-style identifiers with hyphens
        let span = src
            .read_ident_where(|c| c.is_alphabetic(), |c| c.is_alphanumeric() || c == '-')
            .unwrap();
        assert_eq!(&input[span], "foo-bar-baz");
    }

    #[test]
    fn test_source_read_digits() {
        let input = "12345 rest";
        let mut src = Scanner::new(input);

        let span = src.read_digits().unwrap();
        assert_eq!(&input[span], "12345");
    }

    #[test]
    fn test_source_read_digits_with_underscores() {
        let input = "1_000_000 rest";
        let mut src = Scanner::new(input);

        let span = src.read_digits().unwrap();
        assert_eq!(&input[span], "1_000_000");
    }

    #[test]
    fn test_source_read_hex_digits() {
        let input = "DEAD_BEEF rest";
        let mut src = Scanner::new(input);

        let span = src.read_hex_digits().unwrap();
        assert_eq!(&input[span], "DEAD_BEEF");
    }

    #[test]
    fn test_source_read_until_any() {
        let input = "hello, world";
        let mut src = Scanner::new(input);

        let span = src.read_until_any(&[',', '!']);
        assert_eq!(&input[span], "hello");
        assert_eq!(src.peek(), Some(','));
    }

    #[test]
    fn test_source_read_c_string() {
        let input = r#""hello world" rest"#;
        let mut src = Scanner::new(input);

        let (span, value) = src.read_c_string('"', input).unwrap();
        assert_eq!(&input[span], "hello world");
        assert_eq!(value, "hello world");
        assert_eq!(src.peek(), Some(' '));
    }

    #[test]
    fn test_source_read_c_string_escapes() {
        let input = r#""hello\nworld\t!" rest"#;
        let mut src = Scanner::new(input);

        let (span, value) = src.read_c_string('"', input).unwrap();
        assert_eq!(&input[span], r#"hello\nworld\t!"#);
        assert_eq!(value, "hello\nworld\t!");
    }

    #[test]
    fn test_source_read_c_string_hex_escape() {
        let input = r#""\x41\x42\x43" rest"#;
        let mut src = Scanner::new(input);

        let (_span, value) = src.read_c_string('"', input).unwrap();
        assert_eq!(value, "ABC");
    }

    #[test]
    fn test_source_read_rust_raw_string() {
        let input = r#""hello world" rest"#;
        let mut src = Scanner::new(input);

        // Simulate: caller consumed 'r', hashes=0
        let span = src.read_rust_raw_string(0).unwrap();
        assert_eq!(&input[span], "hello world");
    }

    #[test]
    fn test_source_read_rust_raw_string_with_hashes() {
        // Input: "hello"world"# (what remains after caller consumed r#)
        let input = r##""hello"world"#"##;
        let mut src = Scanner::new(input);

        // Simulate: caller consumed 'r#', hashes=1
        let span = src.read_rust_raw_string(1).unwrap();
        assert_eq!(&input[span], r#"hello"world"#);
    }

    #[test]
    fn test_source_read_rust_raw_string_multiple_hashes() {
        // Input: "a"#b"## (what remains after caller consumed r##)
        let input = r###""a"#b"##"###;
        let mut src = Scanner::new(input);

        // Simulate: caller consumed 'r##', hashes=2
        let span = src.read_rust_raw_string(2).unwrap();
        assert_eq!(&input[span], r##"a"#b"##);
    }

    #[test]
    fn test_source_read_cpp_raw_string() {
        let input = r#""(hello world)" rest"#;
        let mut src = Scanner::new(input);

        // Simulate: caller consumed 'R'
        let span = src.read_cpp_raw_string(input).unwrap();
        assert_eq!(&input[span], "hello world");
    }

    #[test]
    fn test_source_read_cpp_raw_string_with_delimiter() {
        let input = r#""delim(hello)world)delim" rest"#;
        let mut src = Scanner::new(input);

        // Simulate: caller consumed 'R'
        let span = src.read_cpp_raw_string(input).unwrap();
        assert_eq!(&input[span], "hello)world");
    }

    #[test]
    fn test_source_read_exact() {
        let input = "<<= rest";
        let mut src = Scanner::new(input);

        assert!(src.read_exact("<<=").is_some());
        assert_eq!(src.peek(), Some(' '));
    }

    #[test]
    fn test_source_read_exact_no_match() {
        let input = "<< rest";
        let mut src = Scanner::new(input);

        assert!(src.read_exact("<<=").is_none());
        // Nothing consumed
        assert_eq!(src.peek(), Some('<'));
        assert_eq!(src.offset(), 0);
    }

    #[test]
    fn test_source_read_one_of() {
        // Maximal munch: longer options first
        const OPS: &[&str] = &["<<=", "<<", "<=", "<"];

        let input = "<<= rest";
        let mut src = Scanner::new(input);
        let (idx, span) = src.read_one_of(OPS).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(OPS[idx], "<<=");
        assert_eq!(&input[span], "<<=");

        let input = "<< rest";
        let mut src = Scanner::new(input);
        let (idx, span) = src.read_one_of(OPS).unwrap();
        assert_eq!(idx, 1);
        assert_eq!(OPS[idx], "<<");
        assert_eq!(&input[span], "<<");

        let input = "<= rest";
        let mut src = Scanner::new(input);
        let (idx, span) = src.read_one_of(OPS).unwrap();
        assert_eq!(idx, 2);
        assert_eq!(OPS[idx], "<=");
        assert_eq!(&input[span], "<=");

        let input = "< rest";
        let mut src = Scanner::new(input);
        let (idx, span) = src.read_one_of(OPS).unwrap();
        assert_eq!(idx, 3);
        assert_eq!(OPS[idx], "<");
        assert_eq!(&input[span], "<");

        // No match
        let input = "> rest";
        let mut src = Scanner::new(input);
        assert!(src.read_one_of(OPS).is_none());
        assert_eq!(src.offset(), 0); // Nothing consumed
    }

    #[test]
    fn test_source_starts_with() {
        let input = "hello world";
        let mut src = Scanner::new(input);

        assert!(src.starts_with("hello"));
        assert!(src.starts_with("hel"));
        assert!(!src.starts_with("world"));
        // Didn't consume anything
        assert_eq!(src.offset(), 0);
    }

    #[test]
    fn test_source_read_while() {
        let input = "aaabbbccc";
        let mut src = Scanner::new(input);

        let span = src.read_while(|c| c == 'a');
        assert_eq!(&input[span], "aaa");
        assert_eq!(src.peek(), Some('b'));
    }

    #[test]
    fn test_source_complete_lexer() {
        // Example of composing Scanner methods into a simple lexer
        let input = "foo + 123";
        let mut src = Scanner::new(input);
        let mut tokens = Vec::new();

        loop {
            src.skip_whitespace();
            if src.at_end() {
                break;
            }

            if let Some(span) = src.read_ident() {
                tokens.push(("ident", &input[span]));
            } else if let Some(span) = src.read_digits() {
                tokens.push(("number", &input[span]));
            } else if src.read_exact("+").is_some() {
                tokens.push(("op", "+"));
            } else {
                panic!("unexpected char at {}", src.offset());
            }
        }

        assert_eq!(
            tokens,
            vec![("ident", "foo"), ("op", "+"), ("number", "123"),]
        );
    }

    // ========================================================================
    // LexerDfa tests
    // ========================================================================

    fn read(dfa: &super::OwnedLexerDfa, input: &str) -> Option<(u16, Range<usize>)> {
        let mut scanner = Scanner::new(input);
        dfa.read_token(&mut scanner)
    }

    fn dfa(patterns: &[(u16, &str)]) -> super::OwnedLexerDfa {
        crate::regex::build_lexer_dfa(patterns).unwrap()
    }

    #[test]
    fn test_lexer_dfa_single_pattern() {
        let dfa = dfa(&[(0, "[a-z]+")]);

        assert_eq!(read(&dfa, "hello world"), Some((0, 0..5)));
        assert_eq!(read(&dfa, "x"), Some((0, 0..1)));
        assert_eq!(read(&dfa, "123"), None);
    }

    #[test]
    fn test_lexer_dfa_longest_match() {
        let dfa = dfa(&[(0, "[a-zA-Z_][a-zA-Z0-9_]*"), (1, "[0-9]+")]);

        assert_eq!(read(&dfa, "foo123 rest"), Some((0, 0..6)));
        assert_eq!(read(&dfa, "42 rest"), Some((1, 0..2)));
        assert_eq!(read(&dfa, " oops"), None);
    }

    #[test]
    fn test_lexer_dfa_priority() {
        let dfa = dfa(&[(0, "if"), (1, "[a-z]+")]);

        assert_eq!(read(&dfa, "if "), Some((0, 0..2)));
        assert_eq!(read(&dfa, "ifx "), Some((1, 0..3)));
        assert_eq!(read(&dfa, "hello"), Some((1, 0..5)));
    }

    #[test]
    fn test_lexer_dfa_operators() {
        let dfa = dfa(&[(0, r"\+"), (1, r"\-"), (2, r"\*"), (3, "/")]);

        assert_eq!(read(&dfa, "+"), Some((0, 0..1)));
        assert_eq!(read(&dfa, "-"), Some((1, 0..1)));
        assert_eq!(read(&dfa, "*"), Some((2, 0..1)));
        assert_eq!(read(&dfa, "/"), Some((3, 0..1)));
        assert_eq!(read(&dfa, "x"), None);
    }

    #[test]
    fn test_lexer_dfa_multi_char_operators() {
        let dfa = dfa(&[(0, "=="), (1, "="), (2, "!=")]);

        assert_eq!(read(&dfa, "== x"), Some((0, 0..2)));
        assert_eq!(read(&dfa, "= x"), Some((1, 0..1)));
        assert_eq!(read(&dfa, "!= x"), Some((2, 0..2)));
    }

    #[test]
    fn test_lexer_dfa_no_match() {
        let dfa = dfa(&[(0, "[a-z]+")]);

        assert_eq!(read(&dfa, ""), None);
        assert_eq!(read(&dfa, "123"), None);
    }

    #[test]
    fn test_lexer_dfa_full_tokenizer() {
        let dfa = dfa(&[
            (0, "[a-zA-Z_][a-zA-Z0-9_]*"),
            (1, "[0-9]+"),
            (2, r"[+\-*/=]"),
            (3, r"\("),
            (4, r"\)"),
        ]);

        let input = "foo + bar123 * (42 - x)";
        let mut scanner = Scanner::new(input);
        let mut tokens = Vec::new();

        loop {
            scanner.skip_whitespace();
            if scanner.at_end() {
                break;
            }

            let (tid, span) = dfa.read_token(&mut scanner).expect("unexpected char");
            tokens.push((tid, &input[span]));
        }

        assert_eq!(
            tokens,
            vec![
                (0, "foo"),
                (2, "+"),
                (0, "bar123"),
                (2, "*"),
                (3, "("),
                (1, "42"),
                (2, "-"),
                (0, "x"),
                (4, ")"),
            ]
        );
    }
}
