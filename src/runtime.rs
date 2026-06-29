use crate::grammar::SymbolId;

// ============================================================================
// Core types — no alloc needed
// ============================================================================

/// Defines the error type for parser actions.
///
/// Implement this once on your action type, then use `Self::Error` in all
/// `Action::build` return types.
///
/// ```ignore
/// struct Eval;
/// impl gazelle::ErrorType for Eval {
///     type Error = core::convert::Infallible;
/// }
/// ```
pub trait ErrorType {
    type Error;
}

/// Marker trait for generated AST node types.
///
/// Implemented by codegen for each non-terminal enum. Maps the node to its
/// output type (determined by the action type `A` baked into the node's
/// generic parameter).
///
/// The output type determines how reduction works:
/// - `Output = N` (identity): CST mode, node passes through unchanged
/// - `Output = Ignore`: node is discarded
/// - `Output = Box<N>`: node is auto-boxed for recursive types
/// - Any other type: custom reduction via `Action` impl
pub trait AstNode {
    type Output;
}

/// Convert a grammar node to an output value.
///
/// Blanket implementations cover identity, `Ignore`, and `Box<N>`.
/// Bounded on `AstNode` so that `Ignore` and `Box<N>` (which don't implement
/// `AstNode`) can't cause overlap with the identity impl.
#[doc(hidden)]
pub trait FromAstNode<N: AstNode> {
    fn from(node: N) -> Self;
}

/// Blanket: identity — node passes through unchanged (CST mode).
impl<N: AstNode> FromAstNode<N> for N {
    fn from(node: N) -> N {
        node
    }
}

/// Marker type for discarding a node during reduction.
///
/// Set `type Foo = Ignore` on your `Types` impl to discard nodes of that type.
/// The blanket `Action` impl handles the rest.
#[derive(Debug, Clone, Copy)]
pub struct Ignore;

/// Blanket: ignore — node is discarded.
impl<N: AstNode> FromAstNode<N> for Ignore {
    fn from(_: N) -> Self {
        Ignore
    }
}

/// Blanket: auto-box — node is wrapped in `Box`.
impl<N: AstNode> FromAstNode<N> for alloc::boxed::Box<N> {
    fn from(node: N) -> alloc::boxed::Box<N> {
        alloc::boxed::Box::new(node)
    }
}

/// Reduce a grammar node to its output value.
///
/// A blanket implementation covers any output that implements `FromAstNode<N>`
/// (identity, `Ignore`, `Box<N>`). Custom reductions override this for specific node types.
pub trait Action<N: AstNode>: ErrorType {
    fn build(&mut self, node: N) -> Result<N::Output, Self::Error>;
}

/// Blanket: if `Output: FromAstNode<N>`, build is automatic.
impl<N: AstNode, A: ErrorType> Action<N> for A
where
    N::Output: FromAstNode<N>,
{
    fn build(&mut self, node: N) -> Result<N::Output, Self::Error> {
        Ok(FromAstNode::from(node))
    }
}

/// An operation instruction in the parse table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParserOp {
    /// Shift the token and go to the given state.
    Shift(usize),
    /// Reduce using the given rule index. Reduce(0) means accept.
    Reduce(usize),
    /// Shift/reduce conflict resolved by precedence at runtime.
    ShiftOrReduce {
        shift_state: usize,
        reduce_rule: usize,
    },
    /// Error (no valid action).
    Error,
}

/// Encoded operation entry for compact parse tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct OpEntry(pub(crate) u32);

impl OpEntry {
    pub fn shift(state: usize) -> Self {
        debug_assert!(state > 0, "Shift(0) is reserved for Error");
        debug_assert!(state < 0x80000000, "Shift state too large");
        OpEntry(state as u32)
    }

    pub fn reduce(rule: usize) -> Self {
        debug_assert!(rule < 0x1000, "Reduce rule too large (max 4095)");
        OpEntry(!(rule as u32))
    }

    pub fn shift_or_reduce(shift_state: usize, reduce_rule: usize) -> Self {
        debug_assert!(shift_state > 0, "Shift(0) is reserved for Error");
        debug_assert!(shift_state < 0x80000, "Shift state too large (max 19 bits)");
        debug_assert!(reduce_rule < 0x1000, "Reduce rule too large (max 4095)");
        OpEntry(!((reduce_rule as u32) | ((shift_state as u32) << 12)))
    }

    pub fn decode(&self) -> ParserOp {
        let v = self.0 as i32;
        if v > 0 {
            ParserOp::Shift(v as usize)
        } else if v == 0 {
            ParserOp::Error
        } else {
            let payload = !self.0;
            let r = (payload & 0xFFF) as usize;
            let s = ((payload >> 12) & 0x7FFFF) as usize;
            if s == 0 {
                ParserOp::Reduce(r)
            } else {
                ParserOp::ShiftOrReduce {
                    shift_state: s,
                    reduce_rule: r,
                }
            }
        }
    }
}

/// This is the runtime representation used by the parser. It borrows slices
/// from either static data (generated code) or a [`CompiledTable`](crate::table::CompiledTable).
///
/// Bison-style split base: action_base[state] and goto_base[non_terminal]
/// share the same data/check arrays. Goto is transposed (rows=NTs, cols=states).
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct ParseTable<'a> {
    data: &'a [u32],
    check: &'a [u32],
    action_base: &'a [i32],
    goto_base: &'a [i32],
    rules: &'a [(u32, u8)],
    pub(crate) num_terminals: u32,
    default_reduce: &'a [u32],
    default_goto: &'a [u32],
}

impl<'a> ParseTable<'a> {
    /// Create a parse table from borrowed slices.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        data: &'a [u32],
        check: &'a [u32],
        action_base: &'a [i32],
        goto_base: &'a [i32],
        rules: &'a [(u32, u8)],
        num_terminals: u32,
        default_reduce: &'a [u32],
        default_goto: &'a [u32],
    ) -> Self {
        ParseTable {
            data,
            check,
            action_base,
            goto_base,
            rules,
            num_terminals,
            default_reduce,
            default_goto,
        }
    }

    /// Displacement table lookup: data[base[row] + col] if check matches.
    fn lookup(&self, base: &[i32], row: usize, col: u32) -> Option<u32> {
        let idx = (base[row] + col as i32) as usize;
        if idx < self.check.len() && self.check[idx] == col {
            Some(self.data[idx])
        } else {
            None
        }
    }

    /// Get the action for a state and terminal. O(1) lookup.
    pub(crate) fn action(&self, state: usize, terminal: SymbolId) -> ParserOp {
        if let Some(v) = self.lookup(self.action_base, state, terminal.0) {
            OpEntry(v).decode()
        } else {
            let rule = self.default_reduce[state];
            if rule > 0 {
                ParserOp::Reduce(rule as usize)
            } else {
                ParserOp::Error
            }
        }
    }

    /// Get the goto state for a state and non-terminal. O(1) lookup.
    /// Transposed: row = non-terminal index, col = state.
    pub(crate) fn goto(&self, state: usize, non_terminal: SymbolId) -> Option<usize> {
        let nt_idx = (non_terminal.0 - self.num_terminals) as usize;
        if let Some(v) = self.lookup(self.goto_base, nt_idx, state as u32) {
            Some(v as usize)
        } else {
            let default = self.default_goto[nt_idx];
            if default < u32::MAX {
                Some(default as usize)
            } else {
                None
            }
        }
    }

    /// Get rule info: (lhs symbol ID, rhs length).
    pub(crate) fn rule_info(&self, rule: usize) -> (SymbolId, usize) {
        let (lhs, len) = self.rules[rule];
        (SymbolId(lhs), len as usize)
    }

    /// Get all rules as (lhs_id, rhs_len) pairs.
    pub(crate) fn rules(&self) -> &[(u32, u8)] {
        self.rules
    }
}

/// Trait for providing error context (symbol names, state/rule info).
///
/// Implemented by [`CompiledTable`](crate::CompiledTable), [`ErrorInfo`](crate::ErrorInfo),
/// and the generated parser's `error_info()` static.
pub trait ErrorContext {
    /// Get the name for a symbol ID.
    fn symbol_name(&self, id: SymbolId) -> &str;
    /// Get the accessing symbol for a state (the symbol shifted/reduced to enter it).
    fn state_symbol(&self, state: usize) -> SymbolId;
    /// Get active LR items for a state. Each pair is `(rule_index, dot_position)`:
    /// the dot position is how far into the rule's RHS the parser has progressed.
    fn state_items(&self, state: usize) -> &[(u16, u8)];
    /// Get the RHS symbol IDs for a rule (each ID indexes into `symbol_name`).
    fn rule_rhs(&self, rule: usize) -> &[u32];
}

/// Precedence information carried by a token at parse time.
///
/// Used with `prec` terminals to resolve operator precedence at runtime.
/// Higher levels bind tighter. Associativity determines behavior at equal levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Precedence {
    /// Left-associative with the given level (e.g., `+`, `-`).
    Left(u8),
    /// Right-associative with the given level (e.g., `=`, `**`).
    Right(u8),
}

impl Precedence {
    /// Get the precedence level.
    pub fn level(&self) -> u8 {
        match self {
            Precedence::Left(l) | Precedence::Right(l) => *l,
        }
    }
}

/// How a token resolves shift/reduce conflicts at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resolution {
    /// Compare precedence levels; associativity breaks ties.
    Prec(Precedence),
    /// Explicitly shift (e.g., pair `else` with nearest `if`).
    Shift,
    /// Explicitly reduce.
    Reduce,
}

/// Parse error: either a syntax error (unexpected terminal) or a user action error.
///
/// The default type parameter `E = Infallible` means `ParseError` alone represents
/// syntax-only errors (e.g. from the raw `Parser` API). Generated typed parsers
/// return `ParseError<A::Error>` which can also carry action errors.
///
/// The parser remains in a valid state after a syntax error, so you can call
/// `parser.format_error()` to get a detailed error message.
#[derive(Debug, Clone)]
pub enum ParseError<E = core::convert::Infallible> {
    /// Parser encountered an unexpected terminal.
    Syntax { terminal: SymbolId },
    /// A user action (reduction) returned an error.
    Action(E),
}

impl ParseError<core::convert::Infallible> {
    /// Cast a syntax-only error (`ParseError<Infallible>`) to any `ParseError<F>`.
    pub fn cast<F>(self) -> ParseError<F> {
        match self {
            ParseError::Syntax { terminal } => ParseError::Syntax { terminal },
            ParseError::Action(e) => match e {},
        }
    }
}

/// A token with terminal symbol ID and optional conflict resolution.
///
/// Create with [`Token::new`] for simple tokens, [`Token::with_prec`]
/// for precedence terminals, or [`Token::with_resolution`] for conflict terminals.
#[derive(Debug, Clone, Copy)]
pub struct Token {
    /// The terminal symbol ID.
    pub terminal: SymbolId,
    /// How this token resolves shift/reduce conflicts, if at all.
    pub resolution: Option<Resolution>,
}

impl Token {
    /// Create a token without resolution info.
    pub fn new(terminal: SymbolId) -> Self {
        Self {
            terminal,
            resolution: None,
        }
    }

    /// Create a token with precedence (for `prec` terminals).
    pub fn with_prec(terminal: SymbolId, prec: Precedence) -> Self {
        Self {
            terminal,
            resolution: Some(Resolution::Prec(prec)),
        }
    }

    /// Create a token with explicit resolution (for `conflict` terminals).
    pub fn with_resolution(terminal: SymbolId, resolution: Resolution) -> Self {
        Self {
            terminal,
            resolution: Some(resolution),
        }
    }
}

// ============================================================================
// Alloc-dependent types
// ============================================================================

use alloc::collections::BTreeSet;
use alloc::collections::BinaryHeap;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::{format, vec, vec::Vec};
use core::cmp::Reverse;

/// Convert `__foo_star` → `foo*`, `__foo_plus` → `foo+`, `__foo_opt` → `foo?`,
/// `__item_sep_comma` → `item % comma`.
fn format_sym(s: &str) -> String {
    if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_star")) {
        format!("{}*", base)
    } else if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_plus")) {
        format!("{}+", base)
    } else if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_opt")) {
        format!("{}?", base)
    } else if let Some(rest) = s.strip_prefix("__") {
        if let Some(idx) = rest.find("_sep_") {
            let base = &rest[..idx];
            let sep = &rest[idx + 5..];
            return format!("{} % {}", base, sep);
        }
        s.to_string()
    } else {
        s.to_string()
    }
}

type RecoveryState<'a> = (SimState<'a>, usize, Option<(usize, Repair)>);

/// Compute which symbols are nullable (can derive epsilon).
fn compute_nullable(table: &ParseTable, ctx: &impl ErrorContext) -> Vec<bool> {
    let rules = table.rules();
    let num_terminals = table.num_terminals as usize;

    // Find max symbol ID by scanning rules
    let mut max_sym = num_terminals;
    for (rule_idx, &(lhs, _)) in rules.iter().enumerate() {
        let lhs = lhs as usize;
        if lhs >= max_sym {
            max_sym = lhs + 1;
        }
        for &sym in ctx.rule_rhs(rule_idx) {
            let id = sym as usize;
            if id >= max_sym {
                max_sym = id + 1;
            }
        }
    }

    let mut nullable: Vec<bool> = vec![false; max_sym];

    // Fixed-point iteration
    let mut changed = true;
    while changed {
        changed = false;

        for (rule_idx, &(lhs, _)) in rules.iter().enumerate() {
            let lhs = lhs as usize;
            let rhs = ctx.rule_rhs(rule_idx);

            // If RHS is empty or all nullable, LHS is nullable
            let all_nullable = rhs.iter().all(|&sym| nullable[sym as usize]);
            if all_nullable && !nullable[lhs] {
                nullable[lhs] = true;
                changed = true;
            }
        }
    }

    nullable
}

/// Collect expected symbols from a sequence, keeping non-nullable nonterminal names.
/// Nullable nonterminals are expanded to their first non-nullable start symbols.
fn expected_from_sequence(
    sequence: &[u32],
    table: &ParseTable,
    ctx: &impl ErrorContext,
    nullable: &[bool],
    num_terminals: usize,
) -> BTreeSet<usize> {
    let mut result = BTreeSet::new();
    for &sym in sequence {
        let sym_id = sym as usize;
        if sym_id < num_terminals || !nullable.get(sym_id).copied().unwrap_or(false) {
            // Terminal or non-nullable nonterminal: add directly
            result.insert(sym_id);
            break;
        }
        // Nullable nonterminal: expand to its first non-nullable start symbols
        expand_nullable(
            sym_id,
            table,
            ctx,
            nullable,
            num_terminals,
            &mut result,
            &mut BTreeSet::new(),
        );
        // Continue to next symbol since this one can be empty
    }
    result
}

/// Expand a nullable nonterminal to its first non-nullable start symbols.
fn expand_nullable(
    sym: usize,
    table: &ParseTable,
    ctx: &impl ErrorContext,
    nullable: &[bool],
    num_terminals: usize,
    result: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<usize>,
) {
    if !visited.insert(sym) {
        return;
    }
    for (rule_idx, &(lhs, _)) in table.rules().iter().enumerate() {
        if lhs as usize != sym {
            continue;
        }
        for &s in ctx.rule_rhs(rule_idx) {
            let s_id = s as usize;
            if s_id < num_terminals || !nullable.get(s_id).copied().unwrap_or(false) {
                result.insert(s_id);
                break;
            }
            expand_nullable(s_id, table, ctx, nullable, num_terminals, result, visited);
        }
    }
}

/// Check if a sequence is nullable.
fn is_sequence_nullable(sequence: &[u32], nullable: &[bool]) -> bool {
    sequence
        .iter()
        .all(|&sym| nullable.get(sym as usize).copied().unwrap_or(false))
}

/// Stack entry for the parser.

#[derive(Debug, Clone, Copy)]
struct StackEntry {
    state: usize,
    prec: Option<Precedence>,
    /// Start token index for this subtree (for span tracking).
    token_idx: usize,
}

/// Stack that preserves entries on logical truncation for checkpoint/restore.
/// Physical entries are never removed — truncation only decrements the logical length.
/// This allows restoring the stack to a previous state after spurious reductions.

#[derive(Clone)]
struct LrStack {
    entries: Vec<StackEntry>,
    len: usize,
}

impl LrStack {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            len: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn push(&mut self, entry: StackEntry) {
        if self.len < self.entries.len() {
            self.entries[self.len] = entry;
        } else {
            self.entries.push(entry);
        }
        self.len += 1;
    }

    fn truncate(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.len);
        self.len = new_len;
    }

    fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.entries.len());
        self.len = new_len;
    }

    fn last(&self) -> Option<&StackEntry> {
        if self.len > 0 {
            Some(&self.entries[self.len - 1])
        } else {
            None
        }
    }

    fn to_vec(&self) -> Vec<StackEntry> {
        self.entries[..self.len].to_vec()
    }
}

impl core::ops::Index<usize> for LrStack {
    type Output = StackEntry;
    fn index(&self, idx: usize) -> &StackEntry {
        debug_assert!(idx < self.len);
        &self.entries[idx]
    }
}

/// Push-based LR parser. Call [`maybe_reduce`](Self::maybe_reduce) in a loop,
/// then [`shift`](Self::shift) each token. Rule 0 signals acceptance.

#[derive(Clone)]
pub struct Parser<'a> {
    table: ParseTable<'a>,
    /// Current state (top of stack, kept in "register").
    state: StackEntry,
    /// Previous states (rest of stack).
    stack: LrStack,
    /// Count of tokens shifted (for span tracking).
    token_count: usize,
    // Checkpoint for error reporting — the state before the current reduction
    // sequence, restored if an error is detected after spurious reductions.
    checkpoint_state: StackEntry,
    checkpoint_len: usize,
    overwrites: Vec<(usize, StackEntry)>,
}

impl<'a> Parser<'a> {
    /// Create a new parser with the given parse table.
    pub fn new(table: ParseTable<'a>) -> Self {
        let initial = StackEntry {
            state: 0,
            prec: None,
            token_idx: 0,
        };
        Self {
            table,
            state: initial,
            stack: LrStack::new(),
            token_count: 0,
            checkpoint_state: initial,
            checkpoint_len: 0,
            overwrites: Vec::new(),
        }
    }

    /// Check if a reduction should happen for the given lookahead.
    ///
    /// Returns `Ok(Some((rule, len, start_idx)))` if a reduction should occur.
    /// The `start_idx` together with `token_count()` forms the half-open range `[start_idx, token_count())`.
    /// Returns `Ok(None)` if should shift or if accepted.
    /// Returns `Err(ParseError)` on parse error.
    pub fn maybe_reduce(
        &mut self,
        lookahead: Option<Token>,
    ) -> Result<Option<(usize, usize, usize)>, ParseError> {
        let terminal = lookahead.map(|t| t.terminal).unwrap_or(SymbolId::EOF);
        let resolution = lookahead.and_then(|t| t.resolution);

        match self.table.action(self.state.state, terminal) {
            ParserOp::Reduce(rule) => {
                if rule == 0 {
                    Ok(Some((0, 0, 0))) // Accept
                } else {
                    let (len, start_idx) = self.do_reduce(rule);
                    Ok(Some((rule, len, start_idx)))
                }
            }
            ParserOp::Shift(_) => Ok(None),
            ParserOp::ShiftOrReduce { reduce_rule, .. } => {
                let should_reduce = match (self.state.prec, resolution) {
                    (_, Some(Resolution::Reduce)) => true,
                    (_, Some(Resolution::Shift)) => false,
                    (Some(sp), Some(Resolution::Prec(tp))) => {
                        if tp.level() > sp.level() {
                            false
                        } else if tp.level() < sp.level() {
                            true
                        } else {
                            matches!(sp, Precedence::Left(_))
                        }
                    }
                    _ => false,
                };

                if should_reduce {
                    let (len, start_idx) = self.do_reduce(reduce_rule);
                    Ok(Some((reduce_rule, len, start_idx)))
                } else {
                    Ok(None)
                }
            }
            ParserOp::Error => Err(ParseError::Syntax { terminal }),
        }
    }

    /// Shift a token onto the stack.
    ///
    /// Call this only after `maybe_reduce` returns `Ok(None)`, meaning no
    /// more reductions apply for the current lookahead.
    pub fn shift(&mut self, token: Token) {
        let next_state = match self.table.action(self.state.state, token.terminal) {
            ParserOp::Shift(s) => s,
            ParserOp::ShiftOrReduce { shift_state, .. } => shift_state,
            _ => panic!("shift called when action is not shift"),
        };

        let token_prec = match token.resolution {
            Some(Resolution::Prec(p)) => Some(p),
            _ => None,
        };
        let prec = token_prec.or(self.state.prec);
        self.stack.push(self.state);
        self.state = StackEntry {
            state: next_state,
            prec,
            token_idx: self.token_count,
        };
        self.token_count += 1;
        self.save_checkpoint();
    }

    fn do_reduce(&mut self, rule: usize) -> (usize, usize) {
        let (lhs, len) = self.table.rule_info(rule);

        // Compute start token index for this reduction
        let start_idx = match len {
            0 => self.token_count,     // epsilon: empty range at current position
            1 => self.state.token_idx, // single symbol in register
            _ => self.stack[self.stack.len() - len + 1].token_idx, // first symbol in stack
        };

        if len == 0 {
            // Epsilon: anchor is current state, push it, then set new state
            if let Some(next_state) = self.table.goto(self.state.state, lhs) {
                // Save entry that will be overwritten if within checkpoint range
                if self.stack.len() < self.checkpoint_len {
                    self.overwrites
                        .push((self.stack.len(), self.stack.entries[self.stack.len()]));
                }
                self.stack.push(self.state);
                self.state = StackEntry {
                    state: next_state,
                    prec: None,
                    token_idx: start_idx,
                };
            }
        } else {
            // Truncate (entries preserved in buffer for checkpoint restore).
            self.stack.truncate(self.stack.len() - (len - 1));
            let anchor = *self.stack.last().unwrap();
            // For single-symbol (len=1): preserve the symbol's own prec (e.g., PLUS → op)
            // For multi-symbol (len>1): use anchor's prec (the "waiting" context)
            // This handles both binary (expr OP expr) and unary (OP expr) correctly.
            let captured_prec = if len == 1 {
                self.state.prec
            } else {
                anchor.prec
            };
            if let Some(next_state) = self.table.goto(anchor.state, lhs) {
                self.state = StackEntry {
                    state: next_state,
                    prec: captured_prec,
                    token_idx: start_idx,
                };
            }
        }

        (len, start_idx)
    }

    fn save_checkpoint(&mut self) {
        self.checkpoint_state = self.state;
        self.checkpoint_len = self.stack.len();
        self.overwrites.clear();
    }

    /// Restore parser state to before the current reduction sequence.
    ///
    /// When using a runtime grammar, call this after `maybe_reduce` returns
    /// a reduction you want to handle yourself, to undo the speculative
    /// stack modifications before continuing.
    pub fn restore_checkpoint(&mut self) {
        for &(idx, entry) in self.overwrites.iter().rev() {
            self.stack.entries[idx] = entry;
        }
        self.stack.set_len(self.checkpoint_len);
        self.state = self.checkpoint_state;
        self.overwrites.clear();
    }

    /// Get the current LR automaton state (an opaque index into the parse table).
    pub fn state(&self) -> usize {
        self.state.state
    }

    /// Get the count of tokens shifted so far.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Get the LR automaton state at a given stack depth (0 = bottom of stack).
    pub fn state_at(&self, depth: usize) -> usize {
        let idx = depth + 1;
        if idx < self.stack.len() {
            self.stack[idx].state
        } else {
            self.state.state
        }
    }

    /// Format a parse error into a detailed message.
    ///
    /// Call this after `maybe_reduce` returns an error.
    ///
    /// - `display_names`: optional map from grammar names to user-friendly names
    ///   (e.g., `"SEMI"` → `"';'"`).
    /// - `tokens`: optional token texts by index (must include the error token at
    ///   index [`token_count()`](Self::token_count)).
    ///
    /// # Adding source locations
    ///
    /// The parser is push-based, so the lexer knows the source position when
    /// each token is pushed. Capture the location before pushing and use it
    /// when formatting the error:
    ///
    /// ```rust,ignore
    /// loop {
    ///     let (line, col) = src.line_col();
    ///     let tok = lex_one_token(&mut src);
    ///     if let Err(ParseError::Syntax { terminal }) = parser.push(tok, &mut actions) {
    ///         let msg = parser.format_error(terminal, ctx, None, None);
    ///         return Err(format!("{line}:{col}: {msg}"));
    ///     }
    /// }
    /// ```
    pub fn format_error(
        &self,
        terminal: SymbolId,
        ctx: &impl ErrorContext,
        display_names: Option<&[(&str, &str)]>,
        tokens: Option<&[&str]>,
    ) -> String {
        let display_names = display_names.unwrap_or(&[]);
        let tokens = tokens.unwrap_or(&[]);
        // Build full stack for error analysis
        let mut full_stack: Vec<StackEntry> = self.stack.to_vec();
        full_stack.push(self.state);
        let error_token_idx = self.token_count;

        let display = |id: SymbolId| -> &str {
            let name = ctx.symbol_name(id);
            display_names
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| *v)
                .unwrap_or(name)
        };

        // Helper: compute stack spans
        let stack_spans = || -> Vec<(usize, usize, usize)> {
            let mut spans = Vec::with_capacity(full_stack.len());
            for i in 0..full_stack.len() {
                let start = full_stack[i].token_idx;
                let end = if i + 1 < full_stack.len() {
                    full_stack[i + 1].token_idx
                } else {
                    error_token_idx
                };
                spans.push((start, end, full_stack[i].state));
            }
            spans
        };

        let nullable = compute_nullable(&self.table, ctx);
        let num_terminals = self.table.num_terminals as usize;

        // Collect relevant items and compute expected symbols
        let mut relevant_items = Vec::new();
        self.collect_relevant_items(
            ctx,
            self.state.state,
            self.stack.len() + 1,
            &mut relevant_items,
        );
        let expected_syms = self.compute_expected(ctx, &relevant_items, &nullable, num_terminals);

        // Convert to display names
        let mut expected: Vec<_> = expected_syms
            .iter()
            .map(|&sym| format_sym(display(SymbolId(sym as u32))))
            .collect();
        expected.sort();

        // Show actual token text if available, otherwise display name
        let found_name = tokens
            .get(error_token_idx)
            .copied()
            .unwrap_or_else(|| display(terminal));

        let mut msg = format!("unexpected '{}'", found_name);
        if !expected.is_empty() {
            msg.push_str(&format!(", expected: {}", expected.join(", ")));
        }

        // Show parsed stack with token spans
        if !tokens.is_empty() && error_token_idx <= tokens.len() {
            let spans = stack_spans();
            // Skip state 0 (initial), show recent entries
            let relevant: Vec<_> = spans
                .into_iter()
                .skip(1) // skip initial state
                .filter(|(start, end, _)| end > start) // skip empty spans
                .collect();

            if !relevant.is_empty() {
                // Build two lines: tokens and underlines with names
                let mut token_line = String::new();
                let mut label_line = String::new();

                for (start, end, state) in relevant.iter().rev().take(4).rev() {
                    let sym = ctx.state_symbol(*state);
                    let name = format_sym(display(sym));

                    // Get token text for this span
                    let span_text = if end - start == 1 {
                        tokens[*start].to_string()
                    } else if end - start <= 3 {
                        tokens[*start..*end].join(" ")
                    } else {
                        format!("{} ... {}", tokens[*start], tokens[end - 1])
                    };

                    let width = span_text.chars().count().max(name.len());

                    if !token_line.is_empty() {
                        token_line.push_str("  ");
                        label_line.push_str("  ");
                    }
                    token_line.push_str(&format!("{:^width$}", span_text, width = width));
                    label_line.push_str(&format!("{:^width$}", name, width = width));
                }

                msg.push_str(&format!("\n  {}\n  {}", token_line, label_line));
            }
        } else if full_stack.len() > 1 {
            // Fallback: show grammar symbols from stack
            let path: Vec<_> = full_stack[1..]
                .iter()
                .map(|e| display(ctx.state_symbol(e.state)))
                .collect();
            msg.push_str(&format!("\n  after: {}", path.join(" ")));
        }

        // Show informative items that explain what's expected
        let display_items = &relevant_items;
        let mut seen = BTreeSet::new();

        for &(rule, dot) in display_items {
            let rhs = ctx.rule_rhs(rule);
            let lhs = self.table.rule_info(rule).0;
            if ctx.symbol_name(lhs) == "__start" {
                continue;
            }
            let lhs_name = format_sym(display(lhs));

            let before: Vec<_> = rhs[..dot]
                .iter()
                .map(|&id| format_sym(display(SymbolId(id))))
                .collect();
            let after: Vec<_> = rhs[dot..]
                .iter()
                .map(|&id| format_sym(display(SymbolId(id))))
                .collect();
            let line = format!(
                "\n  in {}: {} \u{2022} {}",
                lhs_name,
                before.join(" "),
                after.join(" ")
            );
            if seen.insert(line.clone()) {
                msg.push_str(&line);
            }
        }
        msg
    }

    /// Collect relevant items from the current state.
    /// Skips dot=0 closure items and __start.
    /// Items with progress (0 < dot < len) are included directly.
    /// Complete items (dot=len) trace back to the caller item.
    fn collect_relevant_items(
        &self,
        ctx: &impl ErrorContext,
        state: usize,
        stack_len: usize,
        result: &mut Vec<(usize, usize)>,
    ) {
        let mut visited = Vec::new();
        self.collect_relevant_items_inner(ctx, state, stack_len, result, &mut visited);
    }

    fn collect_relevant_items_inner(
        &self,
        ctx: &impl ErrorContext,
        state: usize,
        stack_len: usize,
        result: &mut Vec<(usize, usize)>,
        visited: &mut Vec<(usize, usize)>,
    ) {
        if visited.contains(&(state, stack_len)) {
            return;
        }
        visited.push((state, stack_len));

        for &(rule, dot) in ctx.state_items(state) {
            let rule = rule as usize;
            let dot = dot as usize;
            let rhs = ctx.rule_rhs(rule);
            let lhs = self.table.rule_info(rule).0;

            if ctx.symbol_name(lhs) == "__start" {
                result.push((rule, dot));
                continue;
            }

            if dot == 0 {
                continue;
            }

            if dot < rhs.len() {
                result.push((rule, dot));
            } else {
                // Complete: goto caller state on lhs and recurse
                let consumed = rhs.len();
                if stack_len > consumed {
                    let caller_state = self.state_at_idx(stack_len - consumed - 1);
                    if let Some(goto_state) = self.table.goto(caller_state, lhs) {
                        self.collect_relevant_items_inner(
                            ctx,
                            goto_state,
                            stack_len - consumed + 1,
                            result,
                            visited,
                        );
                    }
                }
            }
        }
    }

    /// Compute expected symbols from relevant items.
    fn compute_expected(
        &self,
        ctx: &impl ErrorContext,
        items: &[(usize, usize)],
        nullable: &[bool],
        num_terminals: usize,
    ) -> BTreeSet<usize> {
        let mut expected = BTreeSet::new();
        let stack_len = self.stack.len() + 1;

        for &(rule, dot) in items {
            let rhs = ctx.rule_rhs(rule);
            let lhs = self.table.rule_info(rule).0;
            let suffix = &rhs[dot..];

            expected.extend(expected_from_sequence(
                suffix,
                &self.table,
                ctx,
                nullable,
                num_terminals,
            ));

            if is_sequence_nullable(suffix, nullable) && stack_len > dot {
                expected.extend(self.compute_follow_from_context(
                    ctx,
                    lhs,
                    stack_len - dot,
                    nullable,
                    num_terminals,
                    &mut BTreeSet::new(),
                ));
            }
        }

        expected
    }

    /// Get state at a given stack index (0 = bottom, stack.len() = current state).
    fn state_at_idx(&self, idx: usize) -> usize {
        if idx < self.stack.len() {
            self.stack[idx].state
        } else {
            self.state.state
        }
    }

    /// Compute what follows a nonterminal using the stack as calling context.
    fn compute_follow_from_context(
        &self,
        ctx: &impl ErrorContext,
        nonterminal: SymbolId,
        caller_idx: usize,
        nullable: &[bool],
        num_terminals: usize,
        visited: &mut BTreeSet<(usize, u32)>,
    ) -> BTreeSet<usize> {
        // Rule 0 is __start → S, nothing follows __start
        if nonterminal == self.table.rule_info(0).0 {
            let mut result = BTreeSet::new();
            result.insert(0); // EOF
            return result;
        }

        if caller_idx == 0 {
            let mut result = BTreeSet::new();
            result.insert(0); // EOF
            return result;
        }

        let caller_state = self.state_at_idx(caller_idx - 1);

        // Use caller_idx in visited key to allow same state at different stack depths
        if !visited.insert((caller_idx, nonterminal.0)) {
            return BTreeSet::new();
        }

        let mut expected = BTreeSet::new();

        // Find items [B → γ • A δ] where A is our nonterminal
        for &(rule, dot) in ctx.state_items(caller_state) {
            let rule = rule as usize;
            let dot = dot as usize;
            let rhs = ctx.rule_rhs(rule);
            if dot < rhs.len() && rhs[dot] == nonterminal.0 {
                let suffix = &rhs[dot + 1..];
                let lhs = self.table.rule_info(rule).0;
                let consumed = dot;

                if suffix.is_empty() {
                    // Nothing after A, follow what follows B
                    if caller_idx > consumed {
                        expected.extend(self.compute_follow_from_context(
                            ctx,
                            lhs,
                            caller_idx - consumed,
                            nullable,
                            num_terminals,
                            visited,
                        ));
                    } else {
                        expected.insert(0);
                    }
                } else {
                    expected.extend(expected_from_sequence(
                        suffix,
                        &self.table,
                        ctx,
                        nullable,
                        num_terminals,
                    ));

                    if is_sequence_nullable(suffix, nullable) {
                        if caller_idx > consumed {
                            expected.extend(self.compute_follow_from_context(
                                ctx,
                                lhs,
                                caller_idx - consumed,
                                nullable,
                                num_terminals,
                                visited,
                            ));
                        } else {
                            expected.insert(0);
                        }
                    }
                }
            }
        }

        expected
    }

    /// Try to shift a token, performing any necessary reductions first.
    /// Returns a cloned parser in the new state, or None if the token causes an error.
    pub(crate) fn try_shift(&self, token: Token) -> Option<Parser<'a>> {
        let mut sim = self.clone();
        let mut iters = 0;
        loop {
            iters += 1;
            if iters > 1000 {
                return None;
            }
            match sim.maybe_reduce(Some(token)) {
                Ok(None) => {
                    sim.shift(token);
                    return Some(sim);
                }
                Ok(Some((0, _, _))) => return Some(sim), // accept
                Ok(Some(_)) => continue,                 // reduction, loop
                Err(_) => return None,
            }
        }
    }

    /// Recover from parse errors by finding minimum-cost repairs.
    ///
    /// Takes the remaining token buffer (starting from the error token).
    /// Returns a list of errors found, each with the repairs applied.
    /// An empty result means no viable recovery was found within the search
    /// budget. On success the parser state is advanced past the repaired
    /// region and parsing can continue.
    pub fn recover(&mut self, buffer: &[Token]) -> Vec<RecoveryInfo> {
        // Fast-forward past leading tokens that shift cleanly
        let mut start = 0;
        while start < buffer.len() {
            if let Some(advanced) = self.try_shift(buffer[start]) {
                *self = advanced;
                start += 1;
            } else {
                break;
            }
        }

        // Single Dijkstra search from error point through rest of buffer.
        // Uses SimState with Rc linked-list stack for O(1) cloning.
        // Priority: (cost, tokens_remaining) — at equal cost, prefer further along.
        let buf_len = buffer.len();
        let mut pq: BinaryHeap<Reverse<(usize, usize, usize)>> = BinaryHeap::new();
        // States: (sim, buf_pos, parent_info)
        let mut states: Vec<RecoveryState<'a>> = Vec::new();
        let mut visited: BTreeSet<(usize, usize, usize)> = BTreeSet::new();

        states.push((SimState::from_parser(self), start, None));
        pq.push(Reverse((0, buf_len - start, 0)));

        while let Some(Reverse((cost, _, state_idx))) = pq.pop() {
            if states.len() > 5000 {
                break;
            }

            let sim = states[state_idx].0.clone();
            let pos = states[state_idx].1;
            let remaining = buf_len - pos;

            let key = (sim.state, sim.depth, pos);
            if !visited.insert(key) {
                continue;
            }

            // Check if we've consumed all tokens and can accept
            if pos >= buf_len {
                let mut candidate = sim.clone();
                if candidate.try_accept() {
                    let edits = Self::reconstruct_edits(&states, state_idx);
                    return Self::edits_to_errors(&edits, start);
                }
            }

            // Shift current real token (cost 0)
            if pos < buf_len
                && let Some(sim2) = sim.try_shift(buffer[pos])
            {
                let idx = states.len();
                states.push((sim2, pos + 1, Some((state_idx, Repair::Shift))));
                pq.push(Reverse((cost, remaining - 1, idx)));
            }

            // Insert any terminal (cost +1)
            let num_terms = self.table.num_terminals;
            for t in 1..num_terms {
                let token = Token::new(SymbolId(t));
                if let Some(sim2) = sim.try_shift(token) {
                    let idx = states.len();
                    states.push((sim2, pos, Some((state_idx, Repair::Insert(SymbolId(t))))));
                    pq.push(Reverse((cost + 1, remaining, idx)));
                }
            }

            // Delete current token (cost +1)
            if pos < buf_len {
                let idx = states.len();
                states.push((
                    sim,
                    pos + 1,
                    Some((state_idx, Repair::Delete(buffer[pos].terminal))),
                ));
                pq.push(Reverse((cost + 1, remaining - 1, idx)));
            }
        }

        // Search exhausted without finding acceptance
        vec![]
    }

    /// Reconstruct the edit sequence by following parent pointers.
    fn reconstruct_edits(states: &[RecoveryState<'a>], mut idx: usize) -> Vec<Repair> {
        let mut edits = Vec::new();
        while let Some((parent, ref edit)) = states[idx].2 {
            edits.push(edit.clone());
            idx = parent;
        }
        edits.reverse();
        edits
    }

    /// Split a flat edit sequence into grouped RecoveryInfo entries.
    fn edits_to_errors(edits: &[Repair], start: usize) -> Vec<RecoveryInfo> {
        let mut errors = Vec::new();
        let mut pos = start;
        let mut current_repairs: Vec<Repair> = Vec::new();
        let mut error_pos = pos;

        for edit in edits {
            match edit {
                Repair::Shift => {
                    if !current_repairs.is_empty() {
                        errors.push(RecoveryInfo {
                            position: error_pos,
                            repairs: core::mem::take(&mut current_repairs),
                        });
                    }
                    pos += 1;
                    error_pos = pos;
                }
                Repair::Insert(t) => {
                    if current_repairs.is_empty() {
                        error_pos = pos;
                    }
                    current_repairs.push(Repair::Insert(*t));
                }
                Repair::Delete(t) => {
                    if current_repairs.is_empty() {
                        error_pos = pos;
                    }
                    current_repairs.push(Repair::Delete(*t));
                    pos += 1;
                }
            }
        }
        if !current_repairs.is_empty() {
            errors.push(RecoveryInfo {
                position: error_pos,
                repairs: current_repairs,
            });
        }
        errors
    }
}

/// Lightweight parser simulation state for error recovery search.
/// Uses an Rc linked-list stack so cloning is O(1).

#[derive(Clone)]
struct SimState<'a> {
    table: ParseTable<'a>,
    state: usize,
    prec: Option<Precedence>,
    token_idx: usize,
    stack: Option<Rc<SimStackNode>>,
    depth: usize,
}

struct SimStackNode {
    state: usize,
    prec: Option<Precedence>,
    token_idx: usize,
    parent: Option<Rc<SimStackNode>>,
}

impl<'a> SimState<'a> {
    fn from_parser(parser: &Parser<'a>) -> Self {
        let mut node: Option<Rc<SimStackNode>> = None;
        for i in 0..parser.stack.len() {
            node = Some(Rc::new(SimStackNode {
                state: parser.stack[i].state,
                prec: parser.stack[i].prec,
                token_idx: parser.stack[i].token_idx,
                parent: node,
            }));
        }
        SimState {
            table: parser.table,
            state: parser.state.state,
            prec: parser.state.prec,
            token_idx: parser.state.token_idx,
            stack: node,
            depth: parser.stack.len(),
        }
    }

    fn try_shift(&self, token: Token) -> Option<SimState<'a>> {
        let mut sim = self.clone();
        let mut iters = 0;
        loop {
            iters += 1;
            if iters > 1000 {
                return None;
            }
            match sim.maybe_reduce(Some(token)) {
                Ok(true) => return Some(sim), // accept
                Ok(false) => {
                    sim.shift(token);
                    return Some(sim);
                }
                Err(true) => continue,     // reduced, keep going
                Err(false) => return None, // error
            }
        }
    }

    /// Try EOF reductions until acceptance or failure.
    fn try_accept(&mut self) -> bool {
        let mut iters = 0;
        loop {
            iters += 1;
            if iters > 1000 {
                return false;
            }
            match self.maybe_reduce(None) {
                Ok(true) => return true,    // accept
                Ok(false) => return false,  // shift — can't shift EOF
                Err(true) => continue,      // reduced
                Err(false) => return false, // error
            }
        }
    }

    /// Check action for lookahead. Returns:
    /// - Ok(true): accept (rule 0)
    /// - Ok(false): should shift
    /// - Err(true): reduced (call again)
    /// - Err(false): parse error
    fn maybe_reduce(&mut self, lookahead: Option<Token>) -> Result<bool, bool> {
        let terminal = lookahead.map(|t| t.terminal).unwrap_or(SymbolId::EOF);
        let resolution = lookahead.and_then(|t| t.resolution);

        match self.table.action(self.state, terminal) {
            ParserOp::Reduce(rule) => {
                if rule == 0 {
                    return Ok(true);
                }
                self.do_reduce(rule);
                Err(true)
            }
            ParserOp::Shift(_) => Ok(false),
            ParserOp::ShiftOrReduce { reduce_rule, .. } => {
                let should_reduce = match (self.prec, resolution) {
                    (_, Some(Resolution::Reduce)) => true,
                    (_, Some(Resolution::Shift)) => false,
                    (Some(sp), Some(Resolution::Prec(tp))) => {
                        if tp.level() > sp.level() {
                            false
                        } else if tp.level() < sp.level() {
                            true
                        } else {
                            matches!(sp, Precedence::Left(_))
                        }
                    }
                    _ => false,
                };
                if should_reduce {
                    self.do_reduce(reduce_rule);
                    Err(true)
                } else {
                    Ok(false)
                }
            }
            ParserOp::Error => Err(false),
        }
    }

    fn shift(&mut self, token: Token) {
        let next_state = match self.table.action(self.state, token.terminal) {
            ParserOp::Shift(s) => s,
            ParserOp::ShiftOrReduce { shift_state, .. } => shift_state,
            _ => panic!("shift called when action is not shift"),
        };
        let token_prec = match token.resolution {
            Some(Resolution::Prec(p)) => Some(p),
            _ => None,
        };
        let prec = token_prec.or(self.prec);
        self.stack = Some(Rc::new(SimStackNode {
            state: self.state,
            prec: self.prec,
            token_idx: self.token_idx,
            parent: self.stack.take(),
        }));
        self.depth += 1;
        self.state = next_state;
        self.prec = prec;
    }

    fn do_reduce(&mut self, rule: usize) {
        let (lhs, len) = self.table.rule_info(rule);
        if len == 0 {
            let goto_state = match self.table.goto(self.state, lhs) {
                Some(s) => s,
                None => return,
            };
            self.stack = Some(Rc::new(SimStackNode {
                state: self.state,
                prec: self.prec,
                token_idx: self.token_idx,
                parent: self.stack.take(),
            }));
            self.depth += 1;
            self.state = goto_state;
            self.prec = None;
        } else {
            // Walk len-1 parent pointers to find anchor
            let mut anchor = self.stack.as_ref().unwrap().clone();
            for _ in 0..len - 1 {
                let parent = anchor.parent.as_ref().unwrap().clone();
                anchor = parent;
            }
            let captured_prec = if len == 1 { self.prec } else { anchor.prec };
            // Start token: for multi-symbol, walk to the deepest consumed node
            let start_token = if len == 1 {
                self.token_idx
            } else {
                // The first symbol consumed is len-1 nodes down from stack top
                let mut node = self.stack.as_ref().unwrap().clone();
                for _ in 0..len - 2 {
                    node = node.parent.as_ref().unwrap().clone();
                }
                node.token_idx
            };
            let goto_state = match self.table.goto(anchor.state, lhs) {
                Some(s) => s,
                None => return,
            };
            // Stack becomes anchor (it stays)
            self.stack = Some(anchor);
            self.depth -= len - 1;
            self.state = goto_state;
            self.prec = captured_prec;
            self.token_idx = start_token;
        }
    }
}

/// Information about one error recovery point.

#[derive(Debug, Clone)]
pub struct RecoveryInfo {
    /// Token index where the error was detected.
    pub position: usize,
    /// The repairs applied to recover.
    pub repairs: Vec<Repair>,
}

/// A single repair action during error recovery.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// Insert a terminal (by symbol ID).
    Insert(SymbolId),
    /// Delete a token (by symbol ID).
    Delete(SymbolId),
    /// Shift the current token (free cost — not an edit).
    Shift,
}

/// A concrete parse tree built by [`CstParser`].
///
/// Nodes store rule indices, not names. Use [`CompiledTable`](crate::table::CompiledTable)
/// to resolve names for display.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cst {
    /// A terminal leaf.
    Leaf {
        /// The terminal's symbol ID.
        symbol: SymbolId,
        /// Token index (from [`Parser::token_count`]).
        token_index: usize,
    },
    /// An interior node from reducing a grammar rule.
    Node {
        /// The rule index that produced this node.
        rule: usize,
        /// Child nodes.
        children: Vec<Cst>,
    },
}

/// A parser that builds a [`Cst`] automatically.
///
/// Mirrors the `push`/`finish` pattern of generated parsers.
pub struct CstParser<'a> {
    parser: Parser<'a>,
    stack: Vec<Cst>,
}

impl<'a> CstParser<'a> {
    /// Create a new tree parser with the given parse table.
    pub fn new(table: ParseTable<'a>) -> Self {
        CstParser {
            parser: Parser::new(table),
            stack: Vec::new(),
        }
    }

    /// Push a token, performing any pending reductions.
    pub fn push(&mut self, token: Token) -> Result<(), ParseError> {
        loop {
            match self.parser.maybe_reduce(Some(token)) {
                Ok(Some((rule, len, _))) if rule > 0 => {
                    let children = self.stack.drain(self.stack.len() - len..).collect();
                    self.stack.push(Cst::Node { rule, children });
                }
                Ok(_) => break,
                Err(e) => {
                    self.stack.clear();
                    self.parser.restore_checkpoint();
                    return Err(e);
                }
            }
        }
        let token_idx = self.parser.token_count();
        self.stack.push(Cst::Leaf {
            symbol: token.terminal,
            token_index: token_idx,
        });
        self.parser.shift(token);
        Ok(())
    }

    /// Finish parsing and return the parse tree.
    #[allow(clippy::result_large_err)]
    pub fn finish(mut self) -> Result<Cst, (Self, ParseError)> {
        loop {
            match self.parser.maybe_reduce(None) {
                Ok(Some((0, _, _))) => {
                    return Ok(self.stack.pop().expect("empty stack after accept"));
                }
                Ok(Some((rule, len, _))) => {
                    let children = self.stack.drain(self.stack.len() - len..).collect();
                    self.stack.push(Cst::Node { rule, children });
                }
                Ok(None) => unreachable!(),
                Err(e) => {
                    self.stack.clear();
                    self.parser.restore_checkpoint();
                    return Err((self, e));
                }
            }
        }
    }

    /// Format a parse error into a detailed message.
    pub fn format_error(
        &self,
        terminal: SymbolId,
        ctx: &impl ErrorContext,
        display_names: Option<&[(&str, &str)]>,
        tokens: Option<&[&str]>,
    ) -> String {
        self.parser
            .format_error(terminal, ctx, display_names, tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::SymbolId;
    use crate::table::CompiledTable;

    #[test]
    fn test_action_entry_encoding() {
        let shift = OpEntry::shift(42);
        assert_eq!(shift.decode(), ParserOp::Shift(42));

        let reduce = OpEntry::reduce(7);
        assert_eq!(reduce.decode(), ParserOp::Reduce(7));

        // Accept is Reduce(0)
        let accept = OpEntry::reduce(0);
        assert_eq!(accept.decode(), ParserOp::Reduce(0));

        let error = OpEntry(0);
        assert_eq!(error.decode(), ParserOp::Error);

        let sor = OpEntry::shift_or_reduce(10, 5);
        match sor.decode() {
            ParserOp::ShiftOrReduce {
                shift_state,
                reduce_rule,
            } => {
                assert_eq!(shift_state, 10);
                assert_eq!(reduce_rule, 5);
            }
            other => panic!("Expected ShiftOrReduce, got {:?}", other),
        }
    }
    use crate::lr::to_grammar_internal;
    use crate::meta::parse_grammar;

    #[test]
    fn test_parse_single_token() {
        let grammar = to_grammar_internal(
            &parse_grammar(
                r#"
            start s; terminals { a } s = a => a;
        "#,
            )
            .unwrap(),
        )
        .unwrap();

        let compiled = CompiledTable::build_from_internal(&grammar);
        let mut parser = Parser::new(compiled.table());

        let a_id = compiled.symbol_id("a").unwrap();
        let token = Token::new(a_id);

        // Should not reduce before shifting
        assert!(matches!(parser.maybe_reduce(Some(token)), Ok(None)));

        // Shift the token
        parser.shift(token);

        // Now reduce with EOF lookahead
        let result = parser.maybe_reduce(None);
        assert!(matches!(result, Ok(Some((1, 1, 0))))); // rule 1, len 1, start_idx 0

        // Should be accepted now (rule 0)
        let result = parser.maybe_reduce(None);
        assert!(matches!(result, Ok(Some((0, 0, 0)))));
    }

    #[test]
    fn test_parse_error() {
        let grammar = to_grammar_internal(
            &parse_grammar(
                r#"
            start s; terminals { a } s = a => a;
        "#,
            )
            .unwrap(),
        )
        .unwrap();

        let compiled = CompiledTable::build_from_internal(&grammar);
        let mut parser = Parser::new(compiled.table());

        let wrong_id = SymbolId(99);
        let token = Token::new(wrong_id);

        let result = parser.maybe_reduce(Some(token));
        assert!(result.is_err());
    }

    #[test]
    fn test_format_error() {
        let grammar = to_grammar_internal(
            &parse_grammar(
                r#"
            start s; terminals { a, b } s = a => a;
        "#,
            )
            .unwrap(),
        )
        .unwrap();

        let compiled = CompiledTable::build_from_internal(&grammar);
        let mut parser = Parser::new(compiled.table());

        // Try to parse 'b' when only 'a' is expected
        let b_id = compiled.symbol_id("b").unwrap();
        let token = Token::new(b_id);

        let ParseError::Syntax { terminal } = parser.maybe_reduce(Some(token)).unwrap_err();
        let msg = parser.format_error(terminal, &compiled, None, None);

        assert!(msg.contains("unexpected"), "msg: {}", msg);
        assert!(msg.contains("'b'"), "msg: {}", msg);
        assert!(msg.contains("s"), "msg: {}", msg);
    }
}
