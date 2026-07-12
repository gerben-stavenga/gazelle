//! Demonstrates auto-generated lexer from regex patterns on terminals.
//!
//! Terminals with `= /pattern/` (or `= "pattern"` in macro syntax) get an
//! auto-generated `RawToken` enum and `next_token` function. Terminals without
//! patterns remain manual.

use gazelle_macros::gazelle;

gazelle! {
    grammar calc {
        start expr;
        terminals {
            NUM: _ = "[0-9]+",
            PLUS = r"\+",
            STAR = r"\*",
            LPAREN = r"\(",
            RPAREN = r"\)"
        }

        expr = expr PLUS term => add | term => term;
        term = term STAR factor => mul | factor => factor;
        factor = NUM => num | LPAREN expr RPAREN => paren;
    }
}

struct Eval;

impl gazelle::ErrorType for Eval {
    type Error = core::convert::Infallible;
}

impl calc::Types for Eval {
    type Num = i64;
    type Expr = i64;
    type Term = i64;
    type Factor = i64;
}

impl gazelle::Action<calc::Expr<Self>> for Eval {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            calc::Expr::Add(l, r) => l + r,
            calc::Expr::Term(t) => t,
        })
    }
}

impl gazelle::Action<calc::Term<Self>> for Eval {
    fn build(&mut self, node: calc::Term<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            calc::Term::Mul(l, r) => l * r,
            calc::Term::Factor(f) => f,
        })
    }
}

impl gazelle::Action<calc::Factor<Self>> for Eval {
    fn build(&mut self, node: calc::Factor<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            calc::Factor::Num(n) => n,
            calc::Factor::Paren(e) => e,
        })
    }
}

fn eval(input: &str) -> Result<i64, String> {
    let mut scanner = gazelle::lexer::Scanner::new(input);
    let mut parser = calc::Parser::<Eval>::new();
    let mut actions = Eval;

    loop {
        scanner.skip_whitespace();
        if scanner.at_end() {
            break;
        }
        let tok = if let Some((lexed, span)) = calc::next_token(&mut scanner) {
            match lexed {
                calc::Lexed::Token(t) => t,
                calc::Lexed::Raw(calc::RawToken::Num) => {
                    calc::Terminal::Num(input[span].parse::<i64>().unwrap())
                }
            }
        } else {
            return Err(format!(
                "unexpected character at offset {}",
                scanner.offset()
            ));
        };
        parser = parser
            .push(tok, &mut actions)
            .map_err(|e| format!("{:?}", e))?;
    }

    parser.finish(&mut actions).map_err(|e| format!("{:?}", e))
}

fn main() {
    let input = "2 + 3 * (4 + 5)";
    match eval(input) {
        Ok(result) => println!("{} = {}", input, result),
        Err(e) => eprintln!("error: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_lexer_eval() {
        assert_eq!(eval("2 + 3").unwrap(), 5);
        assert_eq!(eval("2 + 3 * 4").unwrap(), 14);
        assert_eq!(eval("(2 + 3) * 4").unwrap(), 20);
        assert_eq!(eval("1 + 2 + 3").unwrap(), 6);
    }

    #[test]
    fn test_next_token() {
        let input = "42 + 7";
        let mut scanner = gazelle::lexer::Scanner::new(input);

        scanner.skip_whitespace();
        let (lexed, span) = calc::next_token::<Eval, _>(&mut scanner).unwrap();
        assert!(matches!(lexed, calc::Lexed::Raw(calc::RawToken::Num)));
        assert_eq!(&input[span], "42");

        scanner.skip_whitespace();
        let (lexed, _) = calc::next_token::<Eval, _>(&mut scanner).unwrap();
        assert!(matches!(lexed, calc::Lexed::Token(calc::Terminal::Plus)));

        scanner.skip_whitespace();
        let (lexed, span) = calc::next_token::<Eval, _>(&mut scanner).unwrap();
        assert!(matches!(lexed, calc::Lexed::Raw(calc::RawToken::Num)));
        assert_eq!(&input[span], "7");
    }
}
