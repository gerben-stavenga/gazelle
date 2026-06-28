//! Runtime Grammar CLI
//!
//! Parses a token stream using a grammar loaded at runtime.
//! The token stream format itself is parsed by a compiled grammar,
//! which directly drives the runtime parser - no intermediate storage.
//!
//! Usage:
//!   cargo run --example runtime_grammar <grammar.gzl> < tokens.txt
//!
//! Token format (space-separated, semicolon-separated expressions):
//!   NAME           - terminal with no value
//!   NAME:value     - terminal with value
//!   NAME@<5        - terminal with left-assoc precedence 5
//!   NAME:value@>3  - terminal with value and right-assoc precedence 3
//!   ;              - expression separator (prints result, resets parser)
//!
//! Example with included files:
//!   $ cat examples/expr_tokens.txt | cargo run --example runtime_grammar examples/expr.gzl
//!
//! Or inline:
//!   $ echo "NUM:1 OP:+@<1 NUM:2 OP:*@<2 NUM:3" | cargo run --example runtime_grammar examples/expr.gzl

use gazelle::lexer::Scanner;
use gazelle::runtime::{Cst, CstParser, Token};
use gazelle::table::CompiledTable;
use gazelle::{Precedence, parse_grammar};
use gazelle_macros::gazelle;
use std::io::{self, Read};

// Token stream format - each => token action drives the runtime parser
// Multiple expressions separated by SEMI, each printed separately
gazelle! {
    grammar token_format {
        start sentences;
        terminals {
            IDENT: _,
            NUM: _,
            COLON, AT, LT, GT, SEMI
        }

        sentences = sentence* => sentences;
        sentence = tokens SEMI => sentence;
        tokens = _ => empty | tokens token => append;
        token = IDENT colon_value? at_precedence? => token;

        colon_value = COLON value => colon_value;
        value = IDENT => ident | NUM => num;

        assoc = LT => left | GT => right;
        at_precedence = AT assoc NUM => at_prec;
    }
}

fn print_tree(tree: &Cst, indent: usize, compiled: &CompiledTable, values: &[Option<String>]) {
    let pad = "  ".repeat(indent);
    match *tree {
        Cst::Leaf {
            symbol,
            token_index,
        } => match values.get(token_index) {
            Some(Some(v)) => println!("{}{}:{}", pad, compiled.symbol_name(symbol), v),
            _ => println!("{}{}", pad, compiled.symbol_name(symbol)),
        },
        Cst::Node { rule, ref children } => {
            let name = compiled.rule_name(rule).unwrap_or("?");
            println!("{}({}", pad, name);
            for c in children {
                print_tree(c, indent + 1, compiled, values);
            }
            println!("{})", pad);
        }
    }
}

/// Error type for runtime grammar actions.
#[derive(Debug)]
enum ActionError {
    Parse(gazelle::SymbolId),
    Runtime(String),
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionError::Parse(t) => write!(f, "parse error: {:?}", t),
            ActionError::Runtime(s) => write!(f, "{}", s),
        }
    }
}

/// Actions that drive the runtime parser directly
struct Actions<'a> {
    compiled: &'a CompiledTable,
}

struct RuntimeParser<'a> {
    cst: CstParser<'a>,
    values: Vec<Option<String>>,
}

impl std::fmt::Debug for RuntimeParser<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeParser")
            .field("values", &self.values)
            .finish()
    }
}

impl<'a> gazelle::ErrorType for Actions<'a> {
    type Error = ActionError;
}

impl<'a> token_format::Types for Actions<'a> {
    type Ident = String;
    type Num = String;
    // Identity types — ReduceNode blanket handles these
    type Assoc = token_format::Assoc<Self>;
    type AtPrecedence = token_format::AtPrecedence<Self>;
    type Value = token_format::Value<Self>;
    type ColonValue = token_format::ColonValue<Self>;
    // Identity types
    type Token = token_format::Token<Self>;
    // Custom types
    type Tokens = RuntimeParser<'a>;
    type Sentences = gazelle::Ignore;
    type Sentence = ();
}

impl<'a> gazelle::Action<token_format::Sentence<Self>> for Actions<'a> {
    fn build(&mut self, node: token_format::Sentence<Self>) -> Result<(), Self::Error> {
        let token_format::Sentence::Sentence(parser) = node;
        match parser.cst.finish() {
            Ok(tree) => {
                print_tree(&tree, 0, self.compiled, &parser.values);
                println!();
            }
            Err((_cst, gazelle::ParseError::Syntax { terminal })) => {
                return Err(ActionError::Parse(terminal));
            }
        }
        Ok(())
    }
}

impl<'a> gazelle::Action<token_format::Tokens<Self>> for Actions<'a> {
    fn build(
        &mut self,
        node: token_format::Tokens<Self>,
    ) -> Result<RuntimeParser<'a>, Self::Error> {
        match node {
            token_format::Tokens::Empty => Ok(RuntimeParser {
                cst: CstParser::new(self.compiled.table()),
                values: Vec::new(),
            }),
            token_format::Tokens::Append(mut parser, token_cst) => {
                let token_format::Token::Token(name, colon_value, at_prec) = token_cst;

                let value = colon_value.map(|token_format::ColonValue::ColonValue(v)| match v {
                    token_format::Value::Ident(s) | token_format::Value::Num(s) => s,
                });

                let prec = at_prec.map(|token_format::AtPrecedence::AtPrec(assoc, level)| {
                    let level: u8 = level.parse().unwrap_or(10);
                    match assoc {
                        token_format::Assoc::Left => Precedence::Left(level),
                        token_format::Assoc::Right => Precedence::Right(level),
                    }
                });

                let id = self
                    .compiled
                    .symbol_id(&name)
                    .ok_or_else(|| ActionError::Runtime(format!("unknown terminal '{}'", name)))?;
                let token = match prec {
                    Some(p) => Token::with_prec(id, p),
                    None => Token::new(id),
                };

                parser.values.push(value);
                let res = parser.cst.push(token);
                if let Err(gazelle::ParseError::Syntax { terminal }) = res {
                    return Err(ActionError::Parse(terminal));
                }
                Ok(parser)
            }
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <grammar.gzl>", args[0]);
        eprintln!("Reads token stream from stdin. Format: NAME:value@<prec");
        std::process::exit(1);
    }

    // Load grammar
    let src =
        std::fs::read_to_string(&args[1]).map_err(|e| format!("cannot read {}: {}", args[1], e))?;
    let grammar = parse_grammar(&src)?;
    let compiled = CompiledTable::build(&grammar).map_err(|e| format!("grammar error: {e}"))?;

    // Read input
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;

    // The token format parser drives the runtime parser via => token actions
    let mut actions = Actions {
        compiled: &compiled,
    };
    let mut src = Scanner::new(&input);
    let mut parser = token_format::Parser::<Actions>::new();

    loop {
        src.skip_whitespace();
        while src.skip_line_comment("//") || src.skip_block_comment("/*", "*/") {
            src.skip_whitespace();
        }
        if src.at_end() {
            break;
        }

        let terminal = if let Some(span) = src.read_ident() {
            token_format::Terminal::Ident(input[span].to_string())
        } else if let Some(span) = src.read_digits() {
            token_format::Terminal::Num(input[span].to_string())
        } else if let Some(c) = src.peek() {
            src.advance();
            match c {
                ':' => token_format::Terminal::Colon,
                '@' => token_format::Terminal::At,
                '<' => token_format::Terminal::Lt,
                '>' => token_format::Terminal::Gt,
                ';' => token_format::Terminal::Semi,
                _ => token_format::Terminal::Ident(c.to_string()),
            }
        } else {
            break;
        };

        parser.push(terminal, &mut actions).map_err(|e| match e {
            gazelle::ParseError::Syntax { terminal } => {
                format!("parse error: {}", parser.format_error(terminal, None, None))
            }
            gazelle::ParseError::Action(ActionError::Parse(t)) => {
                format!(
                    "runtime parse error: {}",
                    parser.format_error(t, None, None)
                )
            }
            gazelle::ParseError::Action(ActionError::Runtime(e)) => format!("action error: {}", e),
        })?;
    }
    parser.finish(&mut actions).map_err(|(p, e)| match e {
        gazelle::ParseError::Syntax { terminal } => {
            format!(
                "parse error at end: {}",
                p.format_error(terminal, None, None)
            )
        }
        gazelle::ParseError::Action(ActionError::Parse(t)) => {
            format!(
                "runtime parse error at end: {}",
                p.format_error(t, None, None)
            )
        }
        gazelle::ParseError::Action(ActionError::Runtime(e)) => {
            format!("action error at end: {}", e)
        }
    })?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
