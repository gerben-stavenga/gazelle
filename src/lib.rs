//! A typed LR(1) parser generator for Rust with runtime operator precedence
//! and a push-based API for natural lexer feedback.
//!
//! # Quick start
//!
//! Define a grammar with [`gazelle_macros::gazelle!`]. A `prec` terminal carries
//! [`Precedence`] at parse time, so one rule handles all operator levels:
//!
//! ```rust
//! use gazelle_macros::gazelle;
//!
//! gazelle! {
//!     grammar calc {
//!         start expr;
//!         terminals { NUM: _, prec OP: _ }
//!         expr = expr OP expr => binop | NUM => num;
//!     }
//! }
//!
//! struct Eval;
//!
//! impl gazelle::ErrorType for Eval {
//!     type Error = core::convert::Infallible;
//! }
//!
//! impl calc::Types for Eval {
//!     type Num = i64;
//!     type Op = char;
//!     type Expr = i64;
//! }
//!
//! impl gazelle::Action<calc::Expr<Self>> for Eval {
//!     fn build(&mut self, node: calc::Expr<Self>) -> Result<i64, Self::Error> {
//!         Ok(match node {
//!             calc::Expr::Binop(l, op, r) => match op {
//!                 '+' => l + r, '-' => l - r, '*' => l * r, '/' => l / r,
//!                 _ => unreachable!(),
//!             },
//!             calc::Expr::Num(n) => n,
//!         })
//!     }
//! }
//! ```
//!
//! Then push tokens with precedence and collect the result:
//!
//! ```rust,ignore
//! use gazelle::Precedence;
//!
//! let mut parser = calc::Parser::<Eval>::new();
//! let mut actions = Eval;
//! // Precedence is supplied per-token — the grammar stays flat:
//! parser.push(calc::Terminal::Num(1), &mut actions)?;
//! parser.push(calc::Terminal::Op('+', Precedence::Left(1)), &mut actions)?;
//! parser.push(calc::Terminal::Num(2), &mut actions)?;
//! parser.push(calc::Terminal::Op('*', Precedence::Left(2)), &mut actions)?;
//! parser.push(calc::Terminal::Num(3), &mut actions)?;
//! let result = parser.finish(&mut actions).map_err(|(p, gazelle::ParseError::Syntax { terminal })| p.format_error(terminal, None, None))?;
//! assert_eq!(result, 7); // 1 + (2 * 3)
//! ```
//!
//! See `examples/expr_eval.rs` for a complete runnable version.
//!
//! # Key features
//!
//! - **Runtime operator precedence**: `prec` terminals carry [`Precedence`] at
//!   parse time, so one grammar rule handles any number of operator levels —
//!   including user-defined operators.
//! - **Push-based parsing**: you drive the loop, so the lexer can inspect
//!   parser state between tokens (solves C's typedef problem).
//! - **CST/AST continuum**: set associated types to the generated enum for a
//!   full CST, to a custom type for an AST, or to [`Ignore`] to discard.
//! - **Library API**: build [`CompiledTable`]s programmatically for dynamic
//!   grammars, analyzers, or conflict debuggers.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// -- Core modules (always available) --
pub mod grammar;
pub mod lexer;
pub mod runtime;
pub mod table;

// -- Construction modules --
pub mod automaton;
mod lr;
#[cfg(not(feature = "bootstrap"))]
#[doc(hidden)]
pub mod meta;
#[cfg(not(feature = "bootstrap_regex"))]
pub mod regex;

#[doc(hidden)]
#[cfg(feature = "codegen")]
pub mod codegen;

// Core grammar types (AST)
pub use grammar::{Alt, Grammar, Rule, SymbolId, Term, TerminalDef};

// Parse table types
pub use table::{CompiledTable, Conflict, ErrorInfo};

// Runtime parser types
pub use runtime::{
    Action, AstNode, Cst, CstParser, ErrorContext, ErrorType, FromAstNode, Ignore, ParseError,
    ParseTable, Parser, Precedence, RecoveryInfo, Repair, Resolution, Token,
};

// Lexer DFA
pub use lexer::{LexerDfa, OwnedLexerDfa};

// Meta-grammar parser
#[cfg(not(feature = "bootstrap"))]
pub use meta::parse_grammar;
