//! Test example for ?, *, + modifiers.
//!
//! Demonstrates the convenience syntax for optional, zero-or-more, and one-or-more.

use gazelle_macros::gazelle;

gazelle! {
    grammar list {
        start items;
        terminals {
            NUM: _ = "[0-9]+",
            COMMA = ",",
            SEMI = ";"
        }

        // items: zero or more item, separated by nothing
        items = item* => items;

        // item: a number followed by an optional comma
        item = NUM COMMA => with_comma | NUM => without_comma;

        // nums: one or more numbers (for testing +)
        nums = NUM+ => nums;

        // opt_num: optional number followed by semi
        opt_num = NUM? SEMI => opt;

        // semis: zero or more semicolons (untyped terminal with *)
        semis = SEMI* => semis;
    }
}

#[allow(dead_code)] // Only used in tests
struct Builder;

impl gazelle::ErrorType for Builder {
    type Error = core::convert::Infallible;
}

impl list::Types for Builder {
    type Num = i32;
    type Items = Vec<i32>;
    type Item = i32;
    type Nums = Vec<i32>;
    type OptNum = Option<i32>;
    type Semis = usize; // count of semicolons
}

impl gazelle::Action<list::Items<Self>> for Builder {
    fn build(&mut self, node: list::Items<Self>) -> Result<Vec<i32>, Self::Error> {
        let list::Items::Items(items) = node;
        Ok(items)
    }
}

impl gazelle::Action<list::Semis<Self>> for Builder {
    fn build(&mut self, node: list::Semis<Self>) -> Result<usize, Self::Error> {
        match node {
            list::Semis::Semis(semis) => Ok(semis.len()),
        }
    }
}

impl gazelle::Action<list::Item<Self>> for Builder {
    fn build(&mut self, node: list::Item<Self>) -> Result<i32, Self::Error> {
        Ok(match node {
            list::Item::WithComma(n) => n,
            list::Item::WithoutComma(n) => n,
        })
    }
}

impl gazelle::Action<list::Nums<Self>> for Builder {
    fn build(&mut self, node: list::Nums<Self>) -> Result<Vec<i32>, Self::Error> {
        let list::Nums::Nums(nums) = node;
        Ok(nums)
    }
}

impl gazelle::Action<list::OptNum<Self>> for Builder {
    fn build(&mut self, node: list::OptNum<Self>) -> Result<Option<i32>, Self::Error> {
        let list::OptNum::Opt(opt) = node;
        Ok(opt)
    }
}

fn main() {
    println!("Modifier test example. Run with 'cargo test --example modifiers'.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_items(input: &str) -> Result<Vec<i32>, String> {
        let tokens = lex(input)?;
        let mut parser = list::Parser::<Builder>::new();
        let mut actions = Builder;

        for tok in tokens {
            parser = parser
                .push(tok, &mut actions)
                .map_err(|e| format!("Parse error: {:?}", e))?;
        }

        parser
            .finish(&mut actions)
            .map_err(|gazelle::ParseError::Syntax { terminal, recovery }| {
                format!(
                    "Finish error: {}",
                    recovery.format_error(terminal, None, None)
                )
            })
    }

    fn lex(input: &str) -> Result<Vec<list::Terminal<Builder>>, String> {
        use gazelle::lexer::Scanner;
        let mut src = Scanner::new(input);
        let mut tokens = Vec::new();

        loop {
            src.skip_whitespace();
            if src.at_end() {
                break;
            }

            let (lexed, span) = list::next_token(&mut src)
                .ok_or_else(|| format!("Unexpected char at offset {}", src.offset()))?;
            tokens.push(match lexed {
                list::Lexed::Token(t) => t,
                list::Lexed::Raw(list::RawToken::Num) => {
                    list::Terminal::Num(input[span].parse().unwrap())
                }
            });
        }
        Ok(tokens)
    }

    #[test]
    fn test_zero_items() {
        let result = parse_items("").unwrap();
        assert_eq!(result, vec![]);
    }

    #[test]
    fn test_one_item() {
        let result = parse_items("42").unwrap();
        assert_eq!(result, vec![42]);
    }

    #[test]
    fn test_multiple_items() {
        let result = parse_items("1, 2, 3").unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_items_mixed_comma() {
        let result = parse_items("1, 2 3, 4").unwrap();
        assert_eq!(result, vec![1, 2, 3, 4]);
    }
}
