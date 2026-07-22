//! Grammar types - both public AST and internal representation types.

/// An interned symbol ID for O(1) lookups.
/// Layout:
/// - IDs 0..num_terminals: terminals (EOF is always terminal 0)
/// - IDs num_terminals.. onwards: non-terminals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub(crate) u32);

impl SymbolId {
    /// The EOF symbol ID (always 0).
    pub const EOF: SymbolId = SymbolId(0);

    /// Create a SymbolId from a raw u32.
    #[doc(hidden)]
    pub const fn new(id: u32) -> Self {
        SymbolId(id)
    }

    /// Return the dense index used by generated grammar metadata.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

// ============================================================================
// Public AST types for grammar definitions (require alloc)
// ============================================================================

use alloc::string::String;
use alloc::vec::Vec;

/// A grammar definition, typically produced by [`parse_grammar`](crate::parse_grammar)
/// or built programmatically with fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grammar {
    /// Name of the start symbol.
    pub start: String,
    /// Expected number of reduce/reduce conflicts.
    pub expect_rr: usize,
    /// Expected number of shift/reduce conflicts.
    pub expect_sr: usize,
    /// Terminal definitions.
    pub terminals: Vec<TerminalDef>,
    /// Grammar rules (productions).
    pub rules: Vec<Rule>,
}

/// How a terminal's shift/reduce conflicts are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    /// Normal terminal — conflicts are reported as errors.
    Plain,
    /// `prec` — resolved at runtime by comparing `Precedence` levels.
    Prec,
    /// `shift` — conflicts are resolved statically in favor of shift.
    Shift,
    /// `reduce` — conflicts are resolved statically in favor of reduce.
    Reduce,
    /// `conflict` — resolved at runtime by the lexer passing
    /// `Resolution::Shift` or `Resolution::Reduce`.
    Conflict,
}

/// A terminal definition in the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDef {
    /// Terminal name (e.g., "NUM", "PLUS").
    pub name: String,
    /// Whether this terminal carries a typed payload.
    pub has_type: bool,
    /// How shift/reduce conflicts on this terminal are resolved.
    pub kind: TerminalKind,
    /// Optional regex pattern for automatic lexer generation.
    pub pattern: Option<String>,
}

/// A rule (production) in the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Non-terminal name (left-hand side).
    pub name: String,
    /// Alternatives (right-hand sides).
    pub alts: Vec<Alt>,
}

/// An alternative (right-hand side) of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alt {
    /// Terms in this alternative.
    pub terms: Vec<Term>,
    /// Action name (e.g., `=> binop`).
    pub name: String,
}

/// A term in a grammar rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Term {
    /// Plain symbol reference.
    Symbol(String),
    /// `?` - optional (zero or one).
    Optional(String),
    /// `*` - zero or more. `name` is the optional `as Name` knob: when set, the
    /// sequence is a named non-terminal with associated type `Name` (the user's
    /// container) instead of an anonymous `Vec`.
    ZeroOrMore {
        symbol: String,
        name: Option<String>,
    },
    /// `+` - one or more. `name` as in [`Term::ZeroOrMore`].
    OneOrMore {
        symbol: String,
        name: Option<String>,
    },
    /// `%` - one or more separated by the given symbol. `name` as above.
    SeparatedBy {
        symbol: String,
        sep: String,
        name: Option<String>,
    },
    /// `_` - empty production marker.
    Empty,
}
