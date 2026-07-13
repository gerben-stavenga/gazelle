//! Minimal example: parse and sum a list of numbers separated by '+'.
//!
//! ```text
//! cargo run --example hello
//! ```

use gazelle_macros::gazelle;

gazelle! {
    grammar sum {
        start expr;
        terminals {
            NUM: _ = "[0-9]+",
            PLUS = r"\+"
        }

        expr = expr PLUS NUM => add
             | NUM => num;
    }
}

struct Eval;

impl gazelle::ErrorType for Eval {
    type Error = core::convert::Infallible;
}

impl sum::Types for Eval {
    type Num = i64;
    type Expr = i64;
}

impl gazelle::Action<sum::Expr<Self>> for Eval {
    fn build(&mut self, node: sum::Expr<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            sum::Expr::Add(left, right) => left + right,
            sum::Expr::Num(n) => n,
        })
    }
}

fn parse(input: &str) -> Result<i64, String> {
    use gazelle::lexer::Scanner;

    let mut src = Scanner::new(input);
    let mut parser = sum::Parser::<Eval>::new();
    let mut actions = Eval;

    loop {
        src.skip_whitespace();
        if src.at_end() {
            break;
        }
        let (lexed, span) = sum::next_token(&mut src).ok_or_else(|| {
            format!(
                "unexpected char: {:?}",
                input.as_bytes()[src.offset()] as char
            )
        })?;
        let tok = match lexed {
            sum::Lexed::Token(t) => t,
            sum::Lexed::Raw(sum::RawToken::Num) => {
                let n: i64 = input[span].parse().map_err(|e| format!("{e}"))?;
                sum::Terminal::Num(n)
            }
        };
        parser = parser.push(tok, &mut actions).map_err(
            |gazelle::ParseError::Syntax { terminal, recovery }| {
                recovery.format_error(terminal, None, None)
            },
        )?;
    }

    parser
        .finish(&mut actions)
        .map_err(|gazelle::ParseError::Syntax { terminal, recovery }| {
            recovery.format_error(terminal, None, None)
        })
}

fn main() {
    let input = "1 + 2 + 3";
    match parse(input) {
        Ok(result) => println!("{input} = {result}"),
        Err(e) => eprintln!("error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single() {
        assert_eq!(parse("42").unwrap(), 42);
    }

    #[test]
    fn test_sum() {
        assert_eq!(parse("1 + 2 + 3").unwrap(), 6);
    }

    #[test]
    fn test_error() {
        assert!(parse("1 +").is_err());
    }
}
