//! Grammar-level description of a syntax error.
//!
//! A [`SyntaxDiagnostic`] carries symbol and rule identities, not presentation
//! text. The parser produces it ([`Parser::diagnose`](crate::runtime::Parser::diagnose));
//! applications translate, serialize, or adapt it. The default English
//! rendering lives in [`crate::render`] and is only one consumer among many.

use alloc::vec::Vec;

use crate::grammar::SymbolId;

/// Structured description of a syntax error in grammar terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    /// Terminal that could not be accepted.
    pub unexpected: SymbolId,
    /// Source-token index at which the error was detected.
    pub position: usize,
    /// Terminals accepted from the error state.
    pub expected: Vec<SymbolId>,
    /// Recently recognized grammar symbols and their token ranges.
    pub stack: Vec<DiagnosticStackEntry>,
    /// Active productions that best explain the parser's position.
    pub contexts: Vec<RuleContext>,
}

/// A recognized grammar symbol on the parser stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticStackEntry {
    pub symbol: SymbolId,
    pub start: usize,
    pub end: usize,
}

/// An LR item represented as a grammar production and dot position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleContext {
    pub rule: usize,
    pub lhs: SymbolId,
    pub rhs: Vec<SymbolId>,
    pub dot: usize,
}
