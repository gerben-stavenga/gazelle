//! `#[ast_defaults]`: non-terminals default to their generated AST enums.
//!
//! ```text
//! cargo +nightly run --example ast_defaults
//! ```
//!
//! With `#[ast_defaults]`, the generated `Types` trait gives each non-terminal
//! an associated-type default pointing at its generated AST enum
//! (`type Term = Term<Self>;`). An impl then only has to specify the terminals
//! and the non-terminals it wants to handle differently.
//!
//! Caveat (no auto-boxing yet): a *directly recursive* non-terminal — `expr`
//! references `expr` — would make `Expr<Self>` infinitely sized, so it must be
//! overridden with an indirection (`Box<Expr<Self>>`). Non-recursive ones
//! (`term`) just inherit the default.
#![feature(associated_type_defaults)]

use gazelle_macros::gazelle;

gazelle! {
    #[derive(Debug)]
    #[ast_defaults]
    grammar expr {
        start expr;
        terminals {
            NUM: _ = "[0-9]+",
            PLUS = r"\+",
            STAR = r"\*"
        }

        expr = expr PLUS term => add
             | term => from_term;
        term = NUM STAR NUM => mul
             | NUM => num;
    }
}

struct Build;

impl gazelle::ErrorType for Build {
    type Error = core::convert::Infallible;
}

impl expr::Types for Build {
    type Num = i64;
    // `expr` is directly recursive → break the layout cycle with Box.
    type Expr = Box<expr::Expr<Self>>;
    // `term` is not recursive → inherits the default `type Term = Term<Self>;`.
}

// No `Action` impls needed: gazelle's blanket `Action` builds each node into
// its associated type automatically — identity for the defaulted `Term`, and
// `FromAstNode for Box<N>` for the boxed `Expr`.

fn parse(input: &str) -> Result<Box<expr::Expr<Build>>, String> {
    use gazelle::lexer::Scanner;

    let mut src = Scanner::new(input);
    let mut parser = expr::Parser::<Build>::new();
    let mut actions = Build;

    loop {
        src.skip_whitespace();
        if src.at_end() {
            break;
        }
        let (lexed, span) = expr::next_token(&mut src).ok_or_else(|| {
            format!(
                "unexpected char: {:?}",
                input.as_bytes()[src.offset()] as char
            )
        })?;
        let tok = match lexed {
            expr::Lexed::Token(t) => t,
            expr::Lexed::Raw(expr::RawToken::Num) => {
                let n: i64 = input[span].parse().map_err(|e| format!("{e}"))?;
                expr::Terminal::Num(n)
            }
        };
        parser
            .push(tok, &mut actions)
            .map_err(|gazelle::ParseError::Syntax { terminal }| {
                parser.format_error(terminal, None, None)
            })?;
    }

    parser
        .finish(&mut actions)
        .map_err(|(p, gazelle::ParseError::Syntax { terminal })| {
            p.format_error(terminal, None, None)
        })
}

fn main() {
    let input = "1 + 2 * 3 + 4";
    match parse(input) {
        Ok(tree) => println!("{input} =>\n{tree:#?}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
