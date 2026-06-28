//! Procedural macros for Gazelle parser generator.
//!
//! This crate provides the `gazelle!` macro that allows defining grammars
//! in Rust with type-safe parsers generated at compile time.
//!
//! # Example
//!
//! ```
//! use gazelle_macros::gazelle;
//! use gazelle::Precedence;
//!
//! gazelle! {
//!     grammar expr {
//!         start expr;
//!         terminals {
//!             NUM: _,
//!             LPAREN, RPAREN,
//!             prec OP: _
//!         }
//!         expr = NUM => num
//!              | expr OP expr => binop
//!              | LPAREN expr RPAREN => paren;
//!     }
//! }
//!
//! struct Eval;
//! impl gazelle::ErrorType for Eval {
//!     type Error = core::convert::Infallible;
//! }
//! impl expr::Types for Eval {
//!     type Num = f64;
//!     type Op = char;
//!     type Expr = f64;
//! }
//! impl gazelle::Action<expr::Expr<Eval>> for Eval {
//!     fn build(&mut self, node: expr::Expr<Eval>) -> Result<f64, Self::Error> {
//!         Ok(match node {
//!             expr::Expr::Num(n) => n,
//!             expr::Expr::Binop(l, op, r) => match op {
//!                 '+' => l + r, '-' => l - r, '*' => l * r, '/' => l / r, _ => 0.0,
//!             },
//!             expr::Expr::Paren(e) => e,
//!         })
//!     }
//! }
//!
//! let mut parser = expr::Parser::<Eval>::new();
//! let mut eval = Eval;
//! parser.push(expr::Terminal::Num(1.0), &mut eval).unwrap();
//! parser.push(expr::Terminal::Op('+', Precedence::Left(1)), &mut eval).unwrap();
//! parser.push(expr::Terminal::Num(2.0), &mut eval).unwrap();
//! parser.push(expr::Terminal::Op('*', Precedence::Left(2)), &mut eval).unwrap();
//! parser.push(expr::Terminal::Num(3.0), &mut eval).unwrap();
//! let result = parser.finish(&mut eval).map_err(|(_, e)| e).unwrap();
//! assert_eq!(result, 7.0);  // 1 + (2 * 3)
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenTree;

use gazelle::meta::{AstBuilder, Terminal};

/// Define a grammar and generate a type-safe parser.
///
/// See the crate-level documentation for usage examples.
#[proc_macro]
pub fn gazelle(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();

    match parse_and_generate(input2) {
        Ok(tokens) => tokens.into(),
        Err(msg) => {
            let err = format!("compile_error!({:?});", msg);
            err.parse().unwrap()
        }
    }
}

fn parse_and_generate(input: proc_macro2::TokenStream) -> Result<proc_macro2::TokenStream, String> {
    let (derives, ast_defaults, visibility, name, source) = lex_token_stream(input)?;

    let apply_derives = |ctx: &mut gazelle::codegen::CodegenContext| {
        for d in &derives {
            ctx.derives.insert(d.clone());
        }
        ctx.ast_defaults = ast_defaults;
    };

    let grammar_def = match source {
        GrammarSource::Inline(tokens) => {
            if tokens.is_empty() {
                return Err("Empty grammar".to_string());
            }
            gazelle::meta::parse_tokens_typed(tokens)?
        }
        GrammarSource::File(path) => {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
                .map_err(|_| "CARGO_MANIFEST_DIR not set".to_string())?;
            let full_path = std::path::Path::new(&manifest_dir).join(&path);
            let content = std::fs::read_to_string(&full_path)
                .map_err(|e| format!("Failed to read {}: {}", full_path.display(), e))?;
            let grammar_def = gazelle::parse_grammar(&content)?;

            // Emit include_bytes! so cargo tracks the file for recompilation
            let mut ctx = gazelle::codegen::CodegenContext::from_grammar(
                &grammar_def,
                &name,
                &visibility,
                true,
            )?;
            apply_derives(&mut ctx);
            let mut tokens = gazelle::codegen::generate_tokens(&ctx)?;
            let abs = full_path
                .canonicalize()
                .map_err(|e| format!("Failed to canonicalize {}: {}", full_path.display(), e))?;
            let abs_str = abs.to_str().ok_or("Non-UTF8 path")?;
            let include: proc_macro2::TokenStream =
                format!("const _: &[u8] = include_bytes!({:?});", abs_str)
                    .parse()
                    .map_err(|e| format!("Failed to generate include_bytes: {}", e))?;
            tokens.extend(include);
            return Ok(tokens);
        }
    };

    let mut ctx =
        gazelle::codegen::CodegenContext::from_grammar(&grammar_def, &name, &visibility, true)?;
    apply_derives(&mut ctx);
    gazelle::codegen::generate_tokens(&ctx)
}

enum GrammarSource {
    Inline(Vec<Terminal<AstBuilder>>),
    File(String),
}

/// Lex a proc_macro2::TokenStream into Terminals.
/// Returns (derives, visibility_string, name, source).
///
/// Expected formats:
///   `[#[derive(Debug, Clone)]] [pub] grammar Name { grammar_content... }`   — inline
///   `[#[derive(Debug, Clone)]] [pub] grammar Name = "path/to/file.gzl"`     — file include
fn lex_token_stream(
    input: proc_macro2::TokenStream,
) -> Result<(Vec<String>, bool, String, String, GrammarSource), String> {
    let mut iter = input.into_iter().peekable();

    // Check for attributes: `#[derive(...)]` and `#[ast_defaults]` (any order).
    let mut derives = Vec::new();
    let mut ast_defaults = false;
    while matches!(iter.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
        iter.next(); // consume '#'
        match iter.next() {
            Some(TokenTree::Group(g))
                if matches!(g.delimiter(), proc_macro2::Delimiter::Bracket) =>
            {
                let mut attr_iter = g.stream().into_iter().peekable();
                match attr_iter.next() {
                    Some(TokenTree::Ident(id)) if id == "derive" => match attr_iter.next() {
                        Some(TokenTree::Group(g2))
                            if matches!(g2.delimiter(), proc_macro2::Delimiter::Parenthesis) =>
                        {
                            for tt in g2.stream() {
                                if let TokenTree::Ident(id) = tt {
                                    derives.push(id.to_string());
                                }
                                // skip commas
                            }
                        }
                        other => {
                            return Err(format!("Expected `(...)` after `derive`, got {:?}", other));
                        }
                    },
                    Some(TokenTree::Ident(id)) if id == "ast_defaults" => {
                        ast_defaults = true;
                    }
                    other => {
                        return Err(format!(
                            "Expected `derive` or `ast_defaults` in attribute, got {:?}",
                            other
                        ));
                    }
                }
            }
            other => return Err(format!("Expected `[...]` after `#`, got {:?}", other)),
        }
    }

    // Check for visibility (pub, pub(crate), etc.)
    let visibility = if matches!(iter.peek(), Some(TokenTree::Ident(id)) if *id == "pub") {
        iter.next(); // consume "pub"

        // Check for (crate) or (super) etc.
        if matches!(iter.peek(), Some(TokenTree::Group(g)) if matches!(g.delimiter(), proc_macro2::Delimiter::Parenthesis))
        {
            let group = iter.next().unwrap();
            format!("pub{} ", group)
        } else {
            "pub ".to_string()
        }
    } else {
        String::new()
    };

    // Expect `grammar` keyword
    match iter.next() {
        Some(TokenTree::Ident(id)) if id == "grammar" => {}
        other => return Err(format!("Expected `grammar` keyword, got {:?}", other)),
    }

    // Extract grammar name
    let name = match iter.next() {
        Some(TokenTree::Ident(id)) => id.to_string(),
        other => {
            return Err(format!(
                "Expected grammar name after `grammar`, got {:?}",
                other
            ));
        }
    };

    // File include: `grammar Name = "path.gzl"`
    if matches!(iter.peek(), Some(TokenTree::Punct(p)) if p.as_char() == '=') {
        iter.next(); // consume '='
        match iter.next() {
            Some(TokenTree::Literal(lit)) => {
                let s = lit.to_string();
                // Strip surrounding quotes
                if s.starts_with('"') && s.ends_with('"') {
                    let path = s[1..s.len() - 1].to_string();
                    return Ok((
                        derives,
                        ast_defaults,
                        visibility,
                        name,
                        GrammarSource::File(path),
                    ));
                }
                return Err(format!("Expected string literal after `=`, got {}", s));
            }
            other => return Err(format!("Expected file path after `=`, got {:?}", other)),
        }
    }

    // Inline: `grammar Name { ... }`
    let content = match iter.next() {
        Some(TokenTree::Group(g)) if matches!(g.delimiter(), proc_macro2::Delimiter::Brace) => {
            g.stream()
        }
        other => {
            return Err(format!(
                "Expected {{ or = after grammar name, got {:?}",
                other
            ));
        }
    };

    let mut tokens = Vec::new();
    let mut inner_iter = content.into_iter().peekable();
    lex_tokens(&mut inner_iter, &mut tokens)?;

    Ok((
        derives,
        ast_defaults,
        visibility,
        name,
        GrammarSource::Inline(tokens),
    ))
}

fn unescape_string(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    // Pass through other escapes (like \+, \*, etc.) as-is for regex
                    out.push('\\');
                    out.push(other);
                }
                None => return Err("unexpected end of string after backslash".into()),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn lex_tokens(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
    tokens: &mut Vec<Terminal<AstBuilder>>,
) -> Result<(), String> {
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Ident(id) => {
                let s = id.to_string();
                match s.as_str() {
                    "start" => tokens.push(Terminal::KwStart),
                    "terminals" => tokens.push(Terminal::KwTerminals),
                    "prec" | "shift" | "reduce" | "conflict" => {
                        tokens.push(Terminal::Modifier(s));
                    }
                    "expect" => tokens.push(Terminal::KwExpect),
                    "as" => tokens.push(Terminal::KwAs),
                    "_" => tokens.push(Terminal::Underscore),
                    _ => tokens.push(Terminal::Ident(s)),
                }
            }
            TokenTree::Punct(p) => {
                let c = p.as_char();
                match c {
                    '{' => tokens.push(Terminal::Lbrace),
                    '}' => tokens.push(Terminal::Rbrace),
                    ',' => tokens.push(Terminal::Comma),
                    '|' => tokens.push(Terminal::Pipe),
                    ';' => tokens.push(Terminal::Semi),
                    '?' => tokens.push(Terminal::Question),
                    '*' => tokens.push(Terminal::Star),
                    '+' => tokens.push(Terminal::Plus),
                    '%' => tokens.push(Terminal::Percent),
                    ':' => {
                        tokens.push(Terminal::Colon);
                    }
                    '=' => {
                        // Check for => (fat arrow)
                        if p.spacing() == proc_macro2::Spacing::Joint
                            && let Some(TokenTree::Punct(p2)) = iter.peek()
                            && p2.as_char() == '>'
                        {
                            iter.next();
                            tokens.push(Terminal::FatArrow);
                            continue;
                        }
                        tokens.push(Terminal::Eq);
                    }
                    _ => return Err(format!("Unexpected punctuation: {}", c)),
                }
            }
            TokenTree::Group(g) => match g.delimiter() {
                proc_macro2::Delimiter::Brace => {
                    tokens.push(Terminal::Lbrace);
                    let mut inner_iter = g.stream().into_iter().peekable();
                    lex_tokens(&mut inner_iter, tokens)?;
                    tokens.push(Terminal::Rbrace);
                }
                proc_macro2::Delimiter::Parenthesis => {
                    tokens.push(Terminal::Lparen);
                    let mut inner_iter = g.stream().into_iter().peekable();
                    lex_tokens(&mut inner_iter, tokens)?;
                    tokens.push(Terminal::Rparen);
                }
                _ => return Err(format!("Unexpected group delimiter: {:?}", g.delimiter())),
            },
            TokenTree::Literal(lit) => {
                let s = lit.to_string();
                // Check if it's a number (integer literal)
                if s.chars().all(|c| c.is_ascii_digit()) {
                    tokens.push(Terminal::Num(s));
                } else if s.starts_with('"') {
                    // Regular string literal — strip quotes and unescape
                    let inner = &s[1..s.len() - 1];
                    let value = unescape_string(inner)
                        .map_err(|e| format!("Invalid string literal: {}", e))?;
                    tokens.push(Terminal::Regex(value));
                } else if s.starts_with("r\"") || s.starts_with("r#") {
                    // Raw string literal — strip r, hashes, and quotes
                    let after_r = &s[1..];
                    let hashes = after_r.bytes().take_while(|&b| b == b'#').count();
                    // Skip hashes + opening quote, remove closing quote + hashes
                    let inner = &after_r[hashes + 1..after_r.len() - hashes - 1];
                    tokens.push(Terminal::Regex(inner.to_string()));
                } else {
                    return Err(format!("Unexpected literal in grammar: {}", s));
                }
            }
        }
    }
    Ok(())
}
