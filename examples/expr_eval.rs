//! Expression evaluator to test dynamic precedence parsing.
//!
//! Uses the precedence-carrying non-terminal pattern: MINUS is a `prec`
//! terminal used both as binary subtract and unary negate. The grammar
//! handles the ambiguity, and precedence flows through the `binop`
//! non-terminal via single-symbol reduction.

use gazelle::Precedence;
use gazelle_macros::gazelle;

gazelle! {
    grammar expr {
        start expr;
        terminals {
            NUM: _ = "[0-9]+",
            LPAREN = r"\(",
            RPAREN = r"\)",
            prec MINUS = "-",
            prec OP: _ = r"\+|\*|/|%|\|\||&&|==|!=|<=|>=|<|>"
        }

        expr = term => term
             | expr binop expr => binop;

        binop = OP => op
              | MINUS => minus;

        term = NUM => num
             | LPAREN expr RPAREN => paren
             | MINUS term => neg;
    }
}

struct Eval;

impl gazelle::ErrorType for Eval {
    type Error = core::convert::Infallible;
}

impl expr::Types for Eval {
    type Num = i64;
    type Op = char;
    type Binop = char;
    type Term = i64;
    type Expr = i64;
}

impl gazelle::Action<expr::Binop<Self>> for Eval {
    fn build(&mut self, node: expr::Binop<Self>) -> Result<char, Self::Error> {
        Ok(match node {
            expr::Binop::Op(c) => c,
            expr::Binop::Minus => '-',
        })
    }
}

impl gazelle::Action<expr::Term<Self>> for Eval {
    fn build(&mut self, node: expr::Term<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            expr::Term::Num(n) => n,
            expr::Term::Paren(e) => e,
            expr::Term::Neg(e) => -e,
        })
    }
}

impl gazelle::Action<expr::Expr<Self>> for Eval {
    fn build(&mut self, node: expr::Expr<Self>) -> Result<i64, Self::Error> {
        Ok(match node {
            expr::Expr::Term(t) => t,
            expr::Expr::Binop(l, op, r) => match op {
                '|' => (l != 0 || r != 0) as i64,
                '&' => (l != 0 && r != 0) as i64,
                '=' => (l == r) as i64,
                '!' => (l != r) as i64,
                '<' => (l < r) as i64,
                '>' => (l > r) as i64,
                'L' => (l <= r) as i64,
                'G' => (l >= r) as i64,
                '+' => l + r,
                '-' => l - r,
                '*' => l * r,
                '/' => l / r,
                '%' => l % r,
                _ => panic!("unknown op: {}", op),
            },
        })
    }
}

fn op(s: &str) -> (char, Precedence) {
    match s {
        "+" => ('+', Precedence::Left(6)),
        "*" => ('*', Precedence::Left(7)),
        "/" => ('/', Precedence::Left(7)),
        "%" => ('%', Precedence::Left(7)),
        "||" => ('|', Precedence::Left(2)),
        "&&" => ('&', Precedence::Left(3)),
        "==" => ('=', Precedence::Left(4)),
        "!=" => ('!', Precedence::Left(4)),
        "<=" => ('L', Precedence::Left(5)),
        ">=" => ('G', Precedence::Left(5)),
        "<" => ('<', Precedence::Left(5)),
        ">" => ('>', Precedence::Left(5)),
        _ => panic!("unknown operator: {}", s),
    }
}

fn eval(input: &str) -> Result<i64, String> {
    use gazelle::lexer::Scanner;

    let mut src = Scanner::new(input);
    let mut parser = expr::Parser::<Eval>::new();
    let mut actions = Eval;

    loop {
        src.skip_whitespace();
        if src.at_end() {
            break;
        }
        let (lexed, span) = expr::next_token(&mut src)
            .ok_or_else(|| format!("unexpected char at offset {}", src.offset()))?;
        let tok = match lexed {
            expr::Lexed::Token(t) => t,
            expr::Lexed::Raw(raw) => match raw {
                expr::RawToken::Num => expr::Terminal::Num(input[span].parse().unwrap()),
                expr::RawToken::Minus => expr::Terminal::Minus(Precedence::Left(6)),
                expr::RawToken::Op => {
                    let (val, prec) = op(&input[span]);
                    expr::Terminal::Op(val, prec)
                }
            },
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
    println!("Expression Evaluator - Dynamic Precedence Test");
    println!();

    let tests = [
        ("1 + 2 * 3", 7),      // * binds tighter: 1 + (2 * 3)
        ("2 * 3 + 1", 7),      // same result
        ("(1 + 2) * 3", 9),    // parens override
        ("10 - 3 - 2", 5),     // left-assoc: (10 - 3) - 2
        ("2 * 3 * 4", 24),     // left-assoc
        ("1 + 2 + 3 * 4", 15), // 1 + 2 + 12 = 15
        ("1 < 2", 1),          // comparison
        ("2 < 1", 0),
        ("1 + 1 == 2", 1),       // + before ==: (1+1) == 2
        ("1 == 1 && 2 == 2", 1), // == before &&
        ("1 || 0 && 0", 1),      // && before ||: 1 || (0 && 0) = 1
        ("0 || 0 && 1", 0),      // 0 || (0 && 1) = 0
        ("-5", -5),              // unary minus
        ("--5", 5),              // double negative
        ("2 * -3", -6),          // unary in expression
        ("1 - 2 - 3", -4),       // left-assoc: (1-2)-3 = -4
        ("100 / 10 / 2", 5),     // left-assoc: (100/10)/2 = 5
    ];

    let mut passed = 0;
    let mut failed = 0;

    for (expr, expected) in tests {
        match eval(expr) {
            Ok(result) if result == expected => {
                println!("PASS: {} = {}", expr, result);
                passed += 1;
            }
            Ok(result) => {
                println!("FAIL: {} = {} (expected {})", expr, result, expected);
                failed += 1;
            }
            Err(e) => {
                println!("ERROR: {} -> {}", expr, e);
                failed += 1;
            }
        }
    }

    println!();
    println!("{} passed, {} failed", passed, failed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precedence() {
        assert_eq!(eval("1 + 2 * 3").unwrap(), 7); // * before +
        assert_eq!(eval("2 * 3 + 1").unwrap(), 7);
        assert_eq!(eval("(1 + 2) * 3").unwrap(), 9);
    }

    #[test]
    fn test_associativity() {
        assert_eq!(eval("10 - 3 - 2").unwrap(), 5); // left-assoc: (10-3)-2
        assert_eq!(eval("2 * 3 * 4").unwrap(), 24);
        assert_eq!(eval("1 - 2 - 3").unwrap(), -4); // (1-2)-3 = -4
        assert_eq!(eval("100 / 10 / 2").unwrap(), 5); // (100/10)/2 = 5
    }

    #[test]
    fn test_comparison_precedence() {
        assert_eq!(eval("1 + 1 == 2").unwrap(), 1); // + before ==
        assert_eq!(eval("1 == 1 && 2 == 2").unwrap(), 1); // == before &&
        assert_eq!(eval("1 || 0 && 0").unwrap(), 1); // && before ||
    }

    #[test]
    fn test_unary() {
        assert_eq!(eval("-5").unwrap(), -5);
        assert_eq!(eval("--5").unwrap(), 5);
        assert_eq!(eval("2 * -3").unwrap(), -6);
        assert_eq!(eval("- 2 + 3").unwrap(), 1); // (-2)+3
        assert_eq!(eval("2 + -3 + 4").unwrap(), 3); // 2+(-3)+4
        assert_eq!(eval("2 * -3 + 4").unwrap(), -2); // (2*(-3))+4
        assert_eq!(eval("2 + -3 == -1").unwrap(), 1); // (2+(-3)) == (-1)
    }
}
