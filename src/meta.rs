//! Meta-grammar: parse grammar definitions using Gazelle itself.
//!
//! This module provides the parser for Gazelle grammar syntax.
//! The parser is generated from `grammars/meta.gzl` using the CLI.
//!
//! To regenerate `meta_generated.rs`:
//! ```bash
//! cargo build --release
//! ./target/release/gazelle --rust grammars/meta.gzl > src/meta_generated.rs
//! ```

#![allow(dead_code)]

use alloc::string::{String, ToString};
use alloc::{format, vec::Vec};
use core::fmt;
use core::ops::Range;

use crate as gazelle;
use crate::grammar;
use crate::lexer::Scanner;

// ============================================================================
// Generated parser
// ============================================================================

include!("meta_generated.rs");

// ============================================================================
// AST builder implementing Actions
// ============================================================================

#[doc(hidden)]
pub struct AstBuilder;

impl crate::ErrorType for AstBuilder {
    type Error = core::convert::Infallible;
}

impl Types for AstBuilder {
    type Ident = String;
    type Num = String;
    type Regex = String;
    type Modifier = String;
    type GrammarDef = grammar::Grammar;
    type ExpectDecl = ExpectDecl<Self>;
    type TerminalItem = grammar::TerminalDef;
    type TypeAnnot = crate::Ignore;
    type RegexAnnot = String;
    type Rule = grammar::Rule;
    type Alt = grammar::Alt;
    type Variant = String;
    type Term = grammar::Term;
}

impl gazelle::Action<Variant<Self>> for AstBuilder {
    fn build(&mut self, node: Variant<Self>) -> Result<String, Self::Error> {
        let Variant::Variant(name) = node;
        Ok(name)
    }
}

impl gazelle::Action<GrammarDef<Self>> for AstBuilder {
    fn build(&mut self, node: GrammarDef<Self>) -> Result<grammar::Grammar, Self::Error> {
        let GrammarDef::GrammarDef(start, expects, terminals, rules) = node;
        let mut expect_rr = 0;
        let mut expect_sr = 0;
        for e in expects {
            let ExpectDecl::ExpectDecl(count, kind) = e;
            let count: usize = count.parse().unwrap_or(0);
            match kind.as_str() {
                "rr" => expect_rr = count,
                "sr" => expect_sr = count,
                _ => {}
            }
        }
        Ok(grammar::Grammar {
            start,
            expect_rr,
            expect_sr,
            terminals,
            rules,
        })
    }
}

impl gazelle::Action<RegexAnnot<Self>> for AstBuilder {
    fn build(&mut self, node: RegexAnnot<Self>) -> Result<String, Self::Error> {
        let RegexAnnot::RegexAnnot(regex) = node;
        Ok(regex)
    }
}

impl gazelle::Action<TerminalItem<Self>> for AstBuilder {
    fn build(&mut self, node: TerminalItem<Self>) -> Result<grammar::TerminalDef, Self::Error> {
        let TerminalItem::TerminalItem(modifier, name, has_type, regex_pattern) = node;
        let kind = match modifier.as_deref() {
            Some("prec") => grammar::TerminalKind::Prec,
            Some("shift") => grammar::TerminalKind::Shift,
            Some("reduce") => grammar::TerminalKind::Reduce,
            Some("conflict") => grammar::TerminalKind::Conflict,
            _ => grammar::TerminalKind::Plain,
        };
        Ok(grammar::TerminalDef {
            name,
            has_type: has_type.is_some(),
            kind,
            pattern: regex_pattern,
        })
    }
}

impl gazelle::Action<Rule<Self>> for AstBuilder {
    fn build(&mut self, node: Rule<Self>) -> Result<grammar::Rule, Self::Error> {
        let Rule::Rule(name, alts) = node;
        Ok(grammar::Rule { name, alts })
    }
}

impl gazelle::Action<Alt<Self>> for AstBuilder {
    fn build(&mut self, node: Alt<Self>) -> Result<grammar::Alt, Self::Error> {
        let Alt::Alt(terms, name) = node;
        Ok(grammar::Alt { terms, name })
    }
}

impl gazelle::Action<Term<Self>> for AstBuilder {
    fn build(&mut self, node: Term<Self>) -> Result<grammar::Term, Self::Error> {
        Ok(match node {
            Term::SymSep(symbol, sep) => grammar::Term::SeparatedBy {
                symbol,
                sep,
                name: None,
            },
            Term::SymSepAs(symbol, sep, list) => grammar::Term::SeparatedBy {
                symbol,
                sep,
                name: Some(list),
            },
            Term::SymOpt(name) => grammar::Term::Optional(name),
            Term::SymStar(symbol) => grammar::Term::ZeroOrMore { symbol, name: None },
            Term::SymStarAs(symbol, list) => grammar::Term::ZeroOrMore {
                symbol,
                name: Some(list),
            },
            Term::SymPlus(symbol) => grammar::Term::OneOrMore { symbol, name: None },
            Term::SymPlusAs(symbol, list) => grammar::Term::OneOrMore {
                symbol,
                name: Some(list),
            },
            Term::SymPlain(name) => grammar::Term::Symbol(name),
            Term::SymEmpty => grammar::Term::Empty,
        })
    }
}

// ============================================================================
// Lexer
// ============================================================================

/// A source-located error produced while parsing textual Gazelle grammar syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarDiagnostic {
    /// Human-readable description and parser context.
    pub message: String,
    /// Byte range in the grammar source. Empty at end of input.
    pub span: Range<usize>,
    /// One-based source line.
    pub line: usize,
    /// One-based source column.
    pub column: usize,
    /// Contents of the primary source line.
    pub line_text: String,
    /// Width of the primary marker in source characters.
    pub marker_width: usize,
}

impl GrammarDiagnostic {
    fn new(input: &str, span: Range<usize>, message: impl Into<String>) -> Self {
        let start = span.start.min(input.len());
        let end = span.end.min(input.len()).max(start);
        let line_start = input[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = input[start..].find('\n').map_or(input.len(), |i| start + i);
        let line = input[..line_start].bytes().filter(|&b| b == b'\n').count() + 1;
        let column = input[line_start..start].chars().count() + 1;
        let marker_width = input[start..end.min(line_end)].chars().count().max(1);
        Self {
            message: message.into(),
            span: start..end,
            line,
            column,
            line_text: input[line_start..line_end].to_string(),
            marker_width,
        }
    }
}

impl fmt::Display for GrammarDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}\n  |\n{} | {}\n  | {}{}",
            self.line,
            self.column,
            self.message,
            self.line,
            self.line_text,
            " ".repeat(self.column.saturating_sub(1)),
            "^".repeat(self.marker_width),
        )
    }
}

struct LocatedToken {
    terminal: Terminal<AstBuilder>,
    span: Range<usize>,
}

/// Lex grammar syntax using the composable Scanner API.
fn lex_grammar(input: &str) -> Result<Vec<LocatedToken>, GrammarDiagnostic> {
    let mut src = Scanner::new(input);
    let mut tokens = Vec::new();

    loop {
        // Skip whitespace and comments
        src.skip_whitespace();
        while src.skip_line_comment("//") || src.skip_block_comment("/*", "*/") {
            src.skip_whitespace();
        }

        if src.at_end() {
            break;
        }

        // Identifier or keyword
        if let Some(span) = src.read_ident() {
            let s = &input[span.clone()];
            let tok = match s {
                "start" => Terminal::KwStart,
                "terminals" => Terminal::KwTerminals,
                "prec" | "shift" | "reduce" | "conflict" => Terminal::Modifier(s.to_string()),
                "expect" => Terminal::KwExpect,
                "as" => Terminal::KwAs,

                "_" => Terminal::Underscore,
                _ => Terminal::Ident(s.to_string()),
            };
            tokens.push(LocatedToken {
                terminal: tok,
                span,
            });
            continue;
        }

        // Number
        if let Some(span) = src.read_digits() {
            let s = &input[span.clone()];
            tokens.push(LocatedToken {
                terminal: Terminal::Num(s.to_string()),
                span,
            });
            continue;
        }

        // Single-char operators and punctuation
        if let Some(c) = src.peek() {
            let token_start = src.offset();
            let tok = match c {
                '=' => {
                    src.advance();
                    if src.peek() == Some('>') {
                        src.advance();
                        Terminal::FatArrow
                    } else {
                        Terminal::Eq
                    }
                }
                '/' => {
                    src.advance(); // consume opening /
                    let start = src.offset();
                    loop {
                        match src.peek() {
                            None => {
                                return Err(GrammarDiagnostic::new(
                                    input,
                                    token_start..src.offset(),
                                    "unterminated regex pattern",
                                ));
                            }
                            Some('\\') => {
                                src.advance(); // consume backslash
                                src.advance(); // consume escaped char
                            }
                            Some('/') => break,
                            Some(_) => {
                                src.advance();
                            }
                        }
                    }
                    let end = src.offset();
                    src.advance(); // consume closing /
                    let pattern = input[start..end].to_string();
                    Terminal::Regex(pattern)
                }
                '|' => {
                    src.advance();
                    Terminal::Pipe
                }
                ':' => {
                    src.advance();
                    Terminal::Colon
                }
                '?' => {
                    src.advance();
                    Terminal::Question
                }
                '*' => {
                    src.advance();
                    Terminal::Star
                }
                '+' => {
                    src.advance();
                    Terminal::Plus
                }
                '%' => {
                    src.advance();
                    Terminal::Percent
                }
                ';' => {
                    src.advance();
                    Terminal::Semi
                }
                '{' => {
                    src.advance();
                    Terminal::Lbrace
                }
                '}' => {
                    src.advance();
                    Terminal::Rbrace
                }
                ',' => {
                    src.advance();
                    Terminal::Comma
                }
                '(' => {
                    src.advance();
                    Terminal::Lparen
                }
                ')' => {
                    src.advance();
                    Terminal::Rparen
                }
                _ => {
                    return Err(GrammarDiagnostic::new(
                        input,
                        token_start..token_start + c.len_utf8(),
                        format!("unexpected character: {:?}", c),
                    ));
                }
            };
            tokens.push(LocatedToken {
                terminal: tok,
                span: token_start..src.offset(),
            });
            continue;
        }
    }

    Ok(tokens)
}

// ============================================================================
// Parsing API
// ============================================================================

/// Parse tokens into typed AST.
pub fn parse_tokens_typed<I>(tokens: I) -> Result<grammar::Grammar, String>
where
    I: IntoIterator<Item = Terminal<AstBuilder>>,
{
    let mut parser = Parser::<AstBuilder>::new();
    let mut actions = AstBuilder;

    for tok in tokens {
        if let Err(crate::ParseError::Syntax { terminal }) = parser.push(tok, &mut actions) {
            return Err(parser.format_error(terminal, None, None));
        }
    }

    parser
        .finish(&mut actions)
        .map_err(|(p, crate::ParseError::Syntax { terminal })| p.format_error(terminal, None, None))
}

/// Parse a grammar string into a Grammar AST.
pub fn parse_grammar(input: &str) -> Result<grammar::Grammar, String> {
    parse_grammar_diagnostic(input).map_err(|diagnostic| diagnostic.to_string())
}

/// Parse a textual grammar, preserving structured source information on error.
pub fn parse_grammar_diagnostic(input: &str) -> Result<grammar::Grammar, GrammarDiagnostic> {
    let tokens = lex_grammar(input)?;
    if tokens.is_empty() {
        return Err(GrammarDiagnostic::new(input, 0..0, "empty grammar"));
    }

    let mut parser = Parser::<AstBuilder>::new();
    let mut actions = AstBuilder;
    let token_texts_owned: Vec<String> = tokens
        .iter()
        .map(|token| input[token.span.clone()].to_string())
        .collect();
    let token_texts: Vec<&str> = token_texts_owned.iter().map(String::as_str).collect();

    for token in tokens {
        if let Err(crate::ParseError::Syntax { terminal }) =
            parser.push(token.terminal, &mut actions)
        {
            let message = parser.format_error(terminal, None, Some(&token_texts));
            return Err(GrammarDiagnostic::new(input, token.span, message));
        }
    }

    parser
        .finish(&mut actions)
        .map_err(|(parser, crate::ParseError::Syntax { terminal })| {
            let message = parser.format_error(terminal, None, Some(&token_texts));
            GrammarDiagnostic::new(input, input.len()..input.len(), message)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lr::to_grammar_internal;
    use alloc::vec;

    #[test]
    fn test_lex() {
        let tokens = lex_grammar("start s; terminals { A } s: S = A;").unwrap();
        assert!(matches!(
            &tokens[0].terminal,
            Terminal::<AstBuilder>::KwStart
        ));
        assert!(matches!(&tokens[1].terminal, Terminal::<AstBuilder>::Ident(s) if s == "s"));
        assert_eq!(tokens[0].span, 0..5);
    }

    #[test]
    fn test_located_syntax_diagnostic() {
        let input = "start expr;\nterminals { NUM }\nexpr = NUM => ;\n";
        let diagnostic = parse_grammar_diagnostic(input).unwrap_err();

        assert_eq!(diagnostic.line, 3);
        assert_eq!(diagnostic.column, 15);
        assert_eq!(diagnostic.span, 44..45);
        assert_eq!(diagnostic.line_text, "expr = NUM => ;");
        assert!(diagnostic.message.contains("unexpected ';'"));
        assert!(diagnostic.to_string().contains("3 | expr = NUM => ;"));
    }

    #[test]
    fn test_located_lexer_diagnostic() {
        let input = "start s;\n@";
        let diagnostic = parse_grammar_diagnostic(input).unwrap_err();

        assert_eq!(diagnostic.line, 2);
        assert_eq!(diagnostic.column, 1);
        assert_eq!(diagnostic.span, 9..10);
        assert_eq!(diagnostic.line_text, "@");
        assert_eq!(diagnostic.message, "unexpected character: '@'");
    }

    #[test]
    fn test_diagnostic_marker_uses_character_width() {
        let input = "start s;\n💥";
        let diagnostic = parse_grammar_diagnostic(input).unwrap_err();

        assert_eq!(diagnostic.span, 9..13);
        assert_eq!(diagnostic.marker_width, 1);
        assert!(diagnostic.to_string().ends_with("| ^"));
    }

    #[test]
    fn test_located_eof_diagnostic() {
        let input = "start s;\nterminals { A }\ns = A => a";
        let diagnostic = parse_grammar_diagnostic(input).unwrap_err();

        assert_eq!(diagnostic.span, input.len()..input.len());
        assert_eq!(diagnostic.line, 3);
        assert_eq!(diagnostic.column, 11);
        assert!(diagnostic.message.contains("unexpected '$'"));
    }

    #[test]
    fn test_parse_simple() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { PLUS, NUM }
            expr = expr PLUS term => add | term => term;
            term = NUM => num;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.start, "expr");
        assert_eq!(grammar.terminals.len(), 2);
        assert_eq!(grammar.rules.len(), 2);
    }

    #[test]
    fn test_parse_expr_grammar() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { PLUS, STAR, NUM, LPAREN, RPAREN }
            expr = expr PLUS term => add | term => term;
            term = term STAR factor => mul | factor => factor;
            factor = NUM => num | LPAREN expr RPAREN => paren;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules.len(), 3);
        assert_eq!(grammar.rules[0].alts.len(), 2);
        assert_eq!(grammar.rules[1].alts.len(), 2);
        assert_eq!(grammar.rules[2].alts.len(), 2);
    }

    #[test]
    fn test_parse_error_message() {
        let result = parse_grammar(
            r#"
            start foo;
            terminals { A }
            foo = A A A => triple;
        "#,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_prec_terminal() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { prec OP, NUM }
            expr = expr OP expr => binop | NUM => num;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.terminals.len(), 2);
        assert_eq!(grammar.terminals[0].kind, grammar::TerminalKind::Prec);
        assert_eq!(grammar.terminals[1].kind, grammar::TerminalKind::Plain);
    }

    #[test]
    fn test_terminal_kinds() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { prec OP, shift ELSE, reduce SEMI, conflict TOK, NUM }
            expr = expr OP expr => binop
                 | expr ELSE expr => cond
                 | expr SEMI => semi
                 | expr TOK expr => tok
                 | NUM => num;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.terminals.len(), 5);
        assert_eq!(grammar.terminals[0].kind, grammar::TerminalKind::Prec);
        assert_eq!(grammar.terminals[1].kind, grammar::TerminalKind::Shift);
        assert_eq!(grammar.terminals[2].kind, grammar::TerminalKind::Reduce);
        assert_eq!(grammar.terminals[3].kind, grammar::TerminalKind::Conflict);
        assert_eq!(grammar.terminals[4].kind, grammar::TerminalKind::Plain);
    }

    #[test]
    fn test_roundtrip() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { a }
            s = a => a;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();
        // 2 rules: __start -> s, s -> a
        assert_eq!(internal.rules.len(), 2);
    }

    #[test]
    fn test_terminals_with_types() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { NUM: _, IDENT: _, PLUS }
            expr = NUM => num | IDENT => ident | expr PLUS expr => add;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.terminals.len(), 3);
        assert_eq!(grammar.terminals[0].name, "NUM");
        assert!(grammar.terminals[0].has_type);
        assert_eq!(grammar.terminals[1].name, "IDENT");
        assert!(grammar.terminals[1].has_type);
        assert_eq!(grammar.terminals[2].name, "PLUS");
        assert!(!grammar.terminals[2].has_type);
    }

    #[test]
    fn test_rule_without_action() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { NUM }
            expr = NUM => num;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules[0].alts[0].name, "num");
    }

    #[test]
    fn test_named_reductions() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { PLUS, NUM }
            expr = expr PLUS expr => binop | NUM => literal;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules[0].alts[0].name, "binop");
        assert_eq!(grammar.rules[0].alts[1].name, "literal");
    }

    #[test]
    fn test_modifier_parsing() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A, B, C }
            s = A? B* C+ => s;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules[0].alts[0].terms.len(), 3);
        assert_eq!(
            grammar.rules[0].alts[0].terms[0],
            grammar::Term::Optional("A".to_string())
        );
        assert_eq!(
            grammar.rules[0].alts[0].terms[1],
            grammar::Term::ZeroOrMore {
                symbol: "B".to_string(),
                name: None
            }
        );
        assert_eq!(
            grammar.rules[0].alts[0].terms[2],
            grammar::Term::OneOrMore {
                symbol: "C".to_string(),
                name: None
            }
        );
    }

    #[test]
    fn test_named_empty_production() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A }
            s = A => a | _ => empty;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules[0].alts.len(), 2);
        assert_eq!(grammar.rules[0].alts[1].terms.len(), 1);
        assert_eq!(grammar.rules[0].alts[1].terms[0], grammar::Term::Empty);
        assert_eq!(grammar.rules[0].alts[1].name, "empty");
    }

    #[test]
    fn test_modifier_desugaring() {
        use crate::lr::AltAction;

        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A: _ }
            s = A? => s;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();

        // Check synthetic non-terminal has correct type
        let opt_id = internal.symbols.get_id("__a_opt").unwrap();
        assert_eq!(internal.types[&opt_id], Some("Option<A>".to_string()));

        // Find synthetic rules for __a_opt
        let opt_sym = internal.symbols.get("__a_opt").unwrap();
        let opt_rules: Vec<_> = internal.rules.iter().filter(|r| r.lhs == opt_sym).collect();
        assert_eq!(opt_rules.len(), 2);
        assert_eq!(opt_rules[0].action, AltAction::OptSome);
        assert_eq!(opt_rules[1].action, AltAction::OptNone);

        // The user rule should reference the synthetic non-terminal
        let s_sym = internal.symbols.get("s").unwrap();
        let s_rules: Vec<_> = internal.rules.iter().filter(|r| r.lhs == s_sym).collect();
        assert_eq!(s_rules.len(), 1);
        assert_eq!(s_rules[0].rhs, vec![opt_sym]);
    }

    #[test]
    fn test_expect_declarations() {
        let grammar = parse_grammar(
            r#"
            start s;
            expect 2 sr;
            expect 1 rr;
            terminals { A }
            s = A => a;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.expect_sr, 2);
        assert_eq!(grammar.expect_rr, 1);
    }

    #[test]
    fn test_no_trailing_comma() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A, B, C }
            s = A => a;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.terminals.len(), 3);
    }

    #[test]
    fn test_terminals_with_regex_patterns() {
        let grammar = parse_grammar(
            r#"
            start expr;
            terminals { NUM: _ = /[0-9]+/, PLUS = /\+/, STRING: _ }
            expr = NUM => num | expr PLUS expr => add;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.terminals.len(), 3);
        assert_eq!(grammar.terminals[0].name, "NUM");
        assert!(grammar.terminals[0].has_type);
        assert_eq!(grammar.terminals[0].pattern.as_deref(), Some("[0-9]+"));
        assert_eq!(grammar.terminals[1].name, "PLUS");
        assert!(!grammar.terminals[1].has_type);
        assert_eq!(grammar.terminals[1].pattern.as_deref(), Some("\\+"));
        assert_eq!(grammar.terminals[2].name, "STRING");
        assert!(grammar.terminals[2].has_type);
        assert!(grammar.terminals[2].pattern.is_none());
    }

    #[test]
    fn test_unknown_symbol_error() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A }
            s = A B => s;
        "#,
        )
        .unwrap();

        let result = to_grammar_internal(&grammar);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown symbol: B"));
    }

    #[test]
    fn test_untyped_modifier_star() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A }
            s = A* => s;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();
        let star_id = internal.symbols.get_id("__a_star").unwrap();
        assert_eq!(internal.types[&star_id], Some("Vec<()>".to_string()));
    }

    #[test]
    fn test_untyped_nonterminal_modifier_optional() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A }
            s = foo? => s;
            foo = A => a;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();
        let opt_id = internal.symbols.get_id("__foo_opt").unwrap();
        assert_eq!(internal.types[&opt_id], Some("Option<Foo>".to_string()));
    }

    #[test]
    fn test_untyped_nonterminal_modifier_star() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A }
            s = foo* => s;
            foo = A => a;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();
        let star_id = internal.symbols.get_id("__foo_star").unwrap();
        assert_eq!(internal.types[&star_id], Some("Vec<Foo>".to_string()));
    }

    #[test]
    fn test_separator_modifier_parsing() {
        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A, COMMA }
            s = (A % COMMA) => s;
        "#,
        )
        .unwrap();

        assert_eq!(grammar.rules[0].alts[0].terms.len(), 1);
        assert_eq!(
            grammar.rules[0].alts[0].terms[0],
            grammar::Term::SeparatedBy {
                symbol: "A".to_string(),
                sep: "COMMA".to_string(),
                name: None
            }
        );
    }

    #[test]
    fn test_separator_modifier_desugaring() {
        use crate::lr::AltAction;

        let grammar = parse_grammar(
            r#"
            start s;
            terminals { A: _, COMMA }
            s = (A % COMMA) => s;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();

        // Check synthetic type
        let sep_id = internal.symbols.get_id("__a_sep_comma").unwrap();
        assert_eq!(internal.types[&sep_id], Some("Vec<A>".to_string()));

        // Find synthetic rules
        let sep_sym = internal.symbols.get("__a_sep_comma").unwrap();
        let sep_rules: Vec<_> = internal.rules.iter().filter(|r| r.lhs == sep_sym).collect();
        assert_eq!(sep_rules.len(), 2);

        // First: __a_sep_comma -> __a_sep_comma COMMA A (VecAppend)
        let a_sym = internal.symbols.get("A").unwrap();
        let comma_sym = internal.symbols.get("COMMA").unwrap();
        assert_eq!(sep_rules[0].rhs, vec![sep_sym, comma_sym, a_sym]);
        assert_eq!(sep_rules[0].action, AltAction::VecAppend);

        // Second: __a_sep_comma -> A (VecSingle)
        assert_eq!(sep_rules[1].rhs, vec![a_sym]);
        assert_eq!(sep_rules[1].action, AltAction::VecSingle);

        // The user rule should reference the synthetic non-terminal
        let s_sym = internal.symbols.get("s").unwrap();
        let s_rules: Vec<_> = internal.rules.iter().filter(|r| r.lhs == s_sym).collect();
        assert_eq!(s_rules.len(), 1);
        assert_eq!(s_rules[0].rhs, vec![sep_sym]);
    }

    #[test]
    fn test_separator_end_to_end() {
        let grammar = parse_grammar(
            r#"
            start items;
            terminals { ITEM, COMMA }
            items = (ITEM % COMMA) => items;
        "#,
        )
        .unwrap();

        let internal = to_grammar_internal(&grammar).unwrap();
        use crate::table::CompiledTable;
        let compiled = CompiledTable::build_from_internal(&internal);
        assert!(!compiled.has_conflicts());

        // Parse: ITEM
        let item_id = compiled.symbol_id("ITEM").unwrap();
        let comma_id = compiled.symbol_id("COMMA").unwrap();
        {
            use crate::runtime::{Parser, Token};
            let mut parser = Parser::new(compiled.table());
            let token = Token::new(item_id);
            assert!(parser.maybe_reduce(Some(token)).unwrap().is_none());
            parser.shift(token);
            // Reduce to accept
            while let Some((rule, _, _)) = parser.maybe_reduce(None).unwrap() {
                if rule == 0 {
                    break;
                }
            }
        }

        // Parse: ITEM COMMA ITEM
        {
            use crate::runtime::{Parser, Token};
            let mut parser = Parser::new(compiled.table());
            let tokens = vec![
                Token::new(item_id),
                Token::new(comma_id),
                Token::new(item_id),
            ];
            for tok in tokens {
                while let Some((rule, _, _)) = parser.maybe_reduce(Some(tok)).unwrap() {
                    if rule == 0 {
                        break;
                    }
                }
                parser.shift(tok);
            }
            // Finish
            while let Some((rule, _, _)) = parser.maybe_reduce(None).unwrap() {
                if rule == 0 {
                    break;
                }
            }
        }

        // Parse: ITEM COMMA ITEM COMMA ITEM
        {
            use crate::runtime::{Parser, Token};
            let mut parser = Parser::new(compiled.table());
            let tokens = vec![
                Token::new(item_id),
                Token::new(comma_id),
                Token::new(item_id),
                Token::new(comma_id),
                Token::new(item_id),
            ];
            for tok in tokens {
                while let Some((rule, _, _)) = parser.maybe_reduce(Some(tok)).unwrap() {
                    if rule == 0 {
                        break;
                    }
                }
                parser.shift(tok);
            }
            while let Some((rule, _, _)) = parser.maybe_reduce(None).unwrap() {
                if rule == 0 {
                    break;
                }
            }
        }
    }
}
