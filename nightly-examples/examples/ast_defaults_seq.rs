//! `#[ast_defaults]` + `* as Args`: a named sequence's knob defaults to `Vec`.
//!
//! ```text
//! cargo +nightly run --example ast_defaults_seq
//! ```
//!
//! The `Types` impl only spells out the terminal payload. Everything else is a
//! default: `Item` → `Item<Self>`, `Prog` → `Prog<Self>`, and the named
//! sequence `Items` → `Vec<Self::Item>` — so a named list costs nothing extra
//! on nightly, yet remains a swap point (`type Items = SmallVec<…>`) when wanted.
#![feature(associated_type_defaults)]

use gazelle_macros::gazelle;

gazelle! {
    #[derive(Debug)]
    #[ast_defaults]
    grammar prog {
        start prog;
        terminals {
            NUM: _ = "[0-9]+",
            SEMI = ";"
        }

        prog = item* as Items => prog;
        item = NUM SEMI => item;
    }
}

struct Build;

impl gazelle::ErrorType for Build {
    type Error = core::convert::Infallible;
}

impl prog::Types for Build {
    type Num = i64;
    // Item, Prog, and the named sequence Items all inherit defaults:
    //   type Item  = Item<Self>;
    //   type Items = Vec<Self::Item>;   // the named-list Vec default
    //   type Prog  = Prog<Self>;
}

fn parse(input: &str) -> Result<prog::Prog<Build>, String> {
    use gazelle::lexer::Scanner;

    let mut src = Scanner::new(input);
    let mut parser = prog::Parser::<Build>::new();
    let mut actions = Build;

    loop {
        src.skip_whitespace();
        if src.at_end() {
            break;
        }
        let (lexed, span) = prog::next_token(&mut src)
            .ok_or_else(|| format!("unexpected char at {}", src.offset()))?;
        let tok = match lexed {
            prog::Lexed::Token(t) => t,
            prog::Lexed::Raw(prog::RawToken::Num) => {
                prog::Terminal::Num(input[span].parse().map_err(|e| format!("{e}"))?)
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
        .map_err(|(p, gazelle::ParseError::Syntax { terminal })| p.format_error(terminal, None, None))
}

fn main() {
    let input = "1; 2; 3;";
    match parse(input) {
        Ok(tree) => println!("{input} =>\n{tree:#?}"),
        Err(e) => eprintln!("error: {e}"),
    }
}
