//! Code generation for grammar parsers.
//!
//! This module generates Rust source code for type-safe LR parsers.

#[cfg(not(feature = "bootstrap_regex"))]
mod lexer;
mod parser;
mod reduction;
mod table;
mod terminal;

use alloc::string::{String, ToString};
use alloc::{format, vec, vec::Vec};

use proc_macro2::TokenStream;
use quote::quote;

use crate::grammar::Grammar;
use crate::lr::{GrammarInternal, to_grammar_internal};

/// A terminal with a regex pattern for automatic lexer generation.
#[derive(Debug, Clone)]
pub struct TerminalPattern {
    /// Terminal name (e.g., "NUM", "PLUS").
    pub name: String,
    /// Regex pattern string.
    pub pattern: String,
    /// Whether this terminal carries a typed payload.
    pub has_type: bool,
    /// Whether this terminal carries a runtime resolution field (prec or conflict).
    pub is_prec: bool,
}

/// Context for code generation.
///
/// Contains all information needed to generate parser code:
/// - The grammar structure (with symbols, rules, type info)
/// - Naming information
#[derive(Debug, Clone)]
pub struct CodegenContext {
    /// The grammar with symbols, rules, and type info.
    pub(crate) grammar: GrammarInternal,
    /// Visibility for generated code (e.g., "pub", "pub(crate)", "").
    pub visibility: String,
    /// Grammar name (for naming generated types).
    pub name: String,

    /// If true, use absolute paths (`::gazelle::`). If false, use relative
    /// paths (`gazelle::`) which requires `use ... as gazelle;` in scope.
    pub use_absolute_path: bool,

    /// Start symbol name.
    pub start_symbol: String,

    /// Expected reduce/reduce conflicts (0 = none expected, error if different).
    pub expect_rr: usize,
    /// Expected shift/reduce conflicts (0 = none expected, error if different).
    pub expect_sr: usize,

    /// Patterned terminals for automatic lexer generation.
    /// Each entry: (name, pattern, has_type, is_prec).
    pub terminal_patterns: Vec<TerminalPattern>,

    /// Which trait impls to generate and add as bounds on associated types.
    /// Supported: "Debug", "Clone", "PartialEq", "Eq", "Hash", "Serialize", "Deserialize".
    pub derives: alloc::collections::BTreeSet<String>,

    /// If true, emit associated-type defaults pointing at the generated AST
    /// enums (`type Foo = Foo<Self>;`) for non-terminals that have a generated
    /// enum. Requires the consuming crate to enable
    /// `#![feature(associated_type_defaults)]` (nightly). Opt-in via the
    /// `#[ast_defaults]` attribute on the `gazelle!` invocation.
    pub ast_defaults: bool,
}

impl CodegenContext {
    /// Build a CodegenContext from a GrammarDef.
    pub fn from_grammar(
        grammar_def: &Grammar,
        name: &str,
        visibility: &str,
        use_absolute_path: bool,
    ) -> Result<Self, String> {
        let grammar = to_grammar_internal(grammar_def)?;

        let terminal_patterns: Vec<TerminalPattern> = grammar_def
            .terminals
            .iter()
            .filter_map(|t| {
                t.pattern.as_ref().map(|p| TerminalPattern {
                    name: t.name.clone(),
                    pattern: p.clone(),
                    has_type: t.has_type,
                    is_prec: matches!(
                        t.kind,
                        crate::grammar::TerminalKind::Prec | crate::grammar::TerminalKind::Conflict
                    ),
                })
            })
            .collect();

        Ok(CodegenContext {
            grammar,
            visibility: visibility.to_string(),
            name: name.to_string(),
            use_absolute_path,
            start_symbol: grammar_def.start.clone(),
            expect_rr: grammar_def.expect_rr,
            expect_sr: grammar_def.expect_sr,
            terminal_patterns,
            derives: alloc::collections::BTreeSet::new(),
            ast_defaults: false,
        })
    }

    /// Get the gazelle path prefix as a string.
    pub fn gazelle_crate_path(&self) -> &'static str {
        if self.use_absolute_path {
            "::gazelle"
        } else {
            "gazelle"
        }
    }

    /// Get the gazelle path prefix as tokens.
    pub fn gazelle_crate_path_tokens(&self) -> TokenStream {
        if self.use_absolute_path {
            quote! { ::gazelle }
        } else {
            quote! { gazelle }
        }
    }

    /// Check if a derive is enabled.
    pub fn has_derive(&self, name: &str) -> bool {
        self.derives.contains(name)
    }

    /// Get a symbol's type by name.
    pub fn get_type(&self, name: &str) -> Option<&String> {
        let sym = self.grammar.symbols.get(name)?;
        self.grammar.types.get(&sym.id())?.as_ref()
    }
}

/// Generate bare parser items (no module wrapper).
pub fn generate_items(ctx: &CodegenContext) -> Result<TokenStream, String> {
    let (compiled, info) = table::build_table(ctx)?;

    let table_statics = table::generate_table_statics(ctx, &compiled, &info);
    let terminal_code = terminal::generate(ctx, &info);
    let parser_code = parser::generate(ctx, &info)?;

    #[cfg(not(feature = "bootstrap_regex"))]
    let lexer_code = match lexer::generate(ctx) {
        Some(Ok(tokens)) => tokens,
        Some(Err(e)) => return Err(e),
        None => TokenStream::new(),
    };
    #[cfg(feature = "bootstrap_regex")]
    let lexer_code = TokenStream::new();

    Ok(quote! {
        #table_statics

        #terminal_code

        #parser_code

        #lexer_code
    })
}

/// Convert a grammar to Bison/yacc format (.y).
pub fn to_yacc(grammar_def: &Grammar) -> Result<String, String> {
    let grammar = to_grammar_internal(grammar_def)?;
    let symbols = &grammar.symbols;
    let mut out = String::new();

    // %token declarations (skip EOF at index 0)
    out.push_str("%token");
    for i in 1..symbols.num_terminals() {
        out.push(' ');
        out.push_str(symbols.name(crate::SymbolId::new(i)));
    }
    out.push('\n');

    // %start
    out.push_str(&format!("\n%start {}\n", grammar_def.start));
    out.push_str("\n%%\n");

    // Group rules by lhs (skip augmented start rule at index 0)
    let mut rule_groups: Vec<(crate::SymbolId, Vec<&crate::lr::Rule>)> = Vec::new();
    for rule in grammar.rules.iter().skip(1) {
        let lhs_id = rule.lhs.id();
        if let Some(group) = rule_groups.last_mut().filter(|(id, _)| *id == lhs_id) {
            group.1.push(rule);
        } else {
            rule_groups.push((lhs_id, vec![rule]));
        }
    }

    for (lhs_id, alts) in &rule_groups {
        out.push_str(&format!("\n{}\n    :", symbols.name(*lhs_id)));
        for (i, rule) in alts.iter().enumerate() {
            if i > 0 {
                out.push_str("\n    |");
            }
            if rule.rhs.is_empty() {
                out.push_str(" /* empty */");
            } else {
                for sym in &rule.rhs {
                    out.push(' ');
                    out.push_str(symbols.name(sym.id()));
                }
            }
        }
        out.push_str("\n    ;\n");
    }

    Ok(out)
}

/// Generate all code wrapped in a module.
pub fn generate_tokens(ctx: &CodegenContext) -> Result<TokenStream, String> {
    use quote::format_ident;

    let items = generate_items(ctx)?;
    let mod_name = format_ident!("{}", ctx.name);
    let vis: TokenStream = ctx.visibility.parse().unwrap_or_default();

    Ok(quote! {
        #vis mod #mod_name {
            use super::*;

            #items
        }
    })
}
