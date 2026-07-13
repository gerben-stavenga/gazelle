//! Python Parser for Gazelle
//!
//! Demonstrates indentation-sensitive parsing via synthetic INDENT/DEDENT/NEWLINE
//! tokens emitted by the lexer, and dynamic precedence for binary expressions.

use gazelle::Precedence;
use gazelle::lexer::Scanner;
use gazelle_macros::gazelle;

// =============================================================================
// Grammar Definition
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AugOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    MatMul,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Div,
    FloorDiv,
    Mod,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
}

gazelle! {
    grammar python = "grammars/python.gzl"
}

// Dummy actions — all types are ()
pub struct PyActions;

impl gazelle::ErrorType for PyActions {
    type Error = core::convert::Infallible;
}

impl python::Types for PyActions {
    type Name = String;
    type Number = String;
    type String = String;
    type Augassign = AugOp;
    type CompOp = CompOp;
    type Binop = BinOp;
    type FileInput = gazelle::Ignore;
    type Statements = gazelle::Ignore;
    type Statement = gazelle::Ignore;
    type SimpleStmts = gazelle::Ignore;
    type SimpleStmt = gazelle::Ignore;
    type AssertMsg = gazelle::Ignore;
    type AssignRhs = gazelle::Ignore;
    type YieldExpr = gazelle::Ignore;
    type YieldArg = gazelle::Ignore;
    type RaiseArgs = gazelle::Ignore;
    type RaiseFrom = gazelle::Ignore;
    type DottedName = gazelle::Ignore;
    type DottedAsName = gazelle::Ignore;
    type DottedAsNames = gazelle::Ignore;
    type AsName = gazelle::Ignore;
    type ImportFromPath = gazelle::Ignore;
    type Dots = gazelle::Ignore;
    type ImportTargets = gazelle::Ignore;
    type ImportAsName = gazelle::Ignore;
    type ArithExpr = gazelle::Ignore;
    type StarTarget = gazelle::Ignore;
    type StarTargets = gazelle::Ignore;
    type StarExpression = gazelle::Ignore;
    type StarExpressions = gazelle::Ignore;
    type StarNamedExpression = gazelle::Ignore;
    type Comparison = gazelle::Ignore;
    type CompPair = gazelle::Ignore;
    type Inversion = gazelle::Ignore;
    type Conjunction = gazelle::Ignore;
    type Disjunction = gazelle::Ignore;
    type NamedExpression = gazelle::Ignore;
    type Expression = gazelle::Ignore;
    type LambdaExpr = gazelle::Ignore;
    type LambdaParams = gazelle::Ignore;
    type LambdaParam = gazelle::Ignore;
    type LambdaDefault = gazelle::Ignore;
    type Primary = gazelle::Ignore;
    type Atom = gazelle::Ignore;
    type StringConcat = gazelle::Ignore;
    type ParenBody = gazelle::Ignore;
    type ListBody = gazelle::Ignore;
    type Slices = gazelle::Ignore;
    type Slice = gazelle::Ignore;
    type SliceStep = gazelle::Ignore;
    type Arguments = gazelle::Ignore;
    type Arg = gazelle::Ignore;
    type DictOrSet = gazelle::Ignore;
    type DictItems = gazelle::Ignore;
    type Kvpair = gazelle::Ignore;
    type DictComp = gazelle::Ignore;
    type SetComp = gazelle::Ignore;
    type CompFor = gazelle::Ignore;
    type Filter = gazelle::Ignore;
    type CompoundStmt = gazelle::Ignore;
    type Block = gazelle::Ignore;
    type IfStmt = gazelle::Ignore;
    type ElifClause = gazelle::Ignore;
    type ElseClause = gazelle::Ignore;
    type WhileStmt = gazelle::Ignore;
    type ForStmt = gazelle::Ignore;
    type TryStmt = gazelle::Ignore;
    type ExceptClause = gazelle::Ignore;
    type ExceptAs = gazelle::Ignore;
    type FinallyClause = gazelle::Ignore;
    type WithStmt = gazelle::Ignore;
    type WithItem = gazelle::Ignore;
    type WithAs = gazelle::Ignore;
    type FuncDef = gazelle::Ignore;
    type ReturnAnnot = gazelle::Ignore;
    type Decorators = gazelle::Ignore;
    type Decorator = gazelle::Ignore;
    type Params = gazelle::Ignore;
    type Param = gazelle::Ignore;
    type ParamAnnot = gazelle::Ignore;
    type ParamDefault = gazelle::Ignore;
    type ClassDef = gazelle::Ignore;
    type ClassArgs = gazelle::Ignore;
    type AsyncStmt = gazelle::Ignore;
}

// =============================================================================
// Python Lexer with INDENT/DEDENT
// =============================================================================

type Tok = python::Terminal<PyActions>;
type Parser = python::Parser<PyActions>;

macro_rules! push {
    ($parser:expr, $actions:expr, $tok:expr) => {
        $parser = $parser.push($tok, $actions).map_err(
            |gazelle::ParseError::Syntax { terminal, recovery }| {
                format!(
                    "Parse error: {}",
                    recovery.format_error(terminal, None, None,)
                )
            },
        )?
    };
}

fn lex(input: &str, mut parser: Parser, actions: &mut PyActions) -> Result<Parser, String> {
    let mut src = Scanner::new(input);
    let mut indent_stack: Vec<usize> = vec![0];
    let mut bracket_depth: usize = 0;

    parser = process_line_start(&mut src, &mut indent_stack, parser, actions)?;

    loop {
        // Skip horizontal whitespace, comments, and line continuations
        loop {
            src.skip_while(|c| c == ' ' || c == '\t');
            if src.peek() == Some('#') {
                src.read_until_any(&['\n']);
            }
            if src.peek() == Some('\\') && src.peek_n(1) == Some('\n') {
                src.advance();
                src.advance();
                continue;
            }
            break;
        }

        // Newline
        if matches!(src.peek(), Some('\n' | '\r')) {
            if src.peek() == Some('\r') {
                src.advance();
            }
            if src.peek() == Some('\n') {
                src.advance();
            }
            if bracket_depth > 0 {
                continue;
            }
            push!(parser, actions, Tok::Newline);
            parser = process_line_start(&mut src, &mut indent_stack, parser, actions)?;
            continue;
        }

        // EOF
        if src.at_end() {
            return Ok(parser);
        }

        // Identifier or keyword
        if let Some(span) = src.read_ident() {
            let s = &input[span];
            if is_string_prefix(s) && matches!(src.peek(), Some('\'' | '"')) {
                let str_start = src.offset() - s.len();
                read_string(&mut src)?;
                push!(
                    parser,
                    actions,
                    Tok::String(input[str_start..src.offset()].to_string())
                );
                continue;
            }
            push!(
                parser,
                actions,
                match s {
                    "False" => Tok::False,
                    "None" => Tok::None,
                    "True" => Tok::True,
                    "and" => Tok::And,
                    "as" => Tok::As,
                    "assert" => Tok::Assert,
                    "async" => Tok::Async,
                    "await" => Tok::Await,
                    "break" => Tok::Break,
                    "class" => Tok::Class,
                    "continue" => Tok::Continue,
                    "def" => Tok::Def,
                    "del" => Tok::Del,
                    "elif" => Tok::Elif,
                    "else" => Tok::Else,
                    "except" => Tok::Except,
                    "finally" => Tok::Finally,
                    "for" => Tok::For,
                    "from" => Tok::From,
                    "global" => Tok::Global,
                    "if" => Tok::If,
                    "import" => Tok::Import,
                    "in" => Tok::In,
                    "is" => Tok::Is,
                    "lambda" => Tok::Lambda,
                    "nonlocal" => Tok::Nonlocal,
                    "not" => Tok::Not,
                    "or" => Tok::Or,
                    "pass" => Tok::Pass,
                    "raise" => Tok::Raise,
                    "return" => Tok::Return,
                    "try" => Tok::Try,
                    "while" => Tok::While,
                    "with" => Tok::With,
                    "yield" => Tok::Yield,
                    _ => Tok::Name(s.to_string()),
                }
            );
            continue;
        }

        // Number literal
        if src.peek().is_some_and(|c| c.is_ascii_digit())
            || (src.peek() == Some('.') && src.peek_n(1).is_some_and(|c| c.is_ascii_digit()))
        {
            let start = src.offset();
            read_number(&mut src);
            push!(
                parser,
                actions,
                Tok::Number(input[start..src.offset()].to_string())
            );
            continue;
        }

        // String literal (no prefix)
        if matches!(src.peek(), Some('\'' | '"')) {
            let start = src.offset();
            read_string(&mut src)?;
            push!(
                parser,
                actions,
                Tok::String(input[start..src.offset()].to_string())
            );
            continue;
        }

        // Brackets
        match src.peek() {
            Some('(' | '[' | '{') => {
                let c = src.peek().unwrap();
                src.advance();
                bracket_depth += 1;
                push!(
                    parser,
                    actions,
                    match c {
                        '(' => Tok::Lparen,
                        '[' => Tok::Lbrack,
                        _ => Tok::Lbrace,
                    }
                );
                continue;
            }
            Some(')' | ']' | '}') => {
                let c = src.peek().unwrap();
                src.advance();
                bracket_depth = bracket_depth.saturating_sub(1);
                push!(
                    parser,
                    actions,
                    match c {
                        ')' => Tok::Rparen,
                        ']' => Tok::Rbrack,
                        _ => Tok::Rbrace,
                    }
                );
                continue;
            }
            _ => {}
        }

        // Operators (longest first)
        if let Some((idx, _)) = src.read_one_of(&OPS.map(|(s, _)| s)) {
            push!(parser, actions, OPS[idx].1());
            continue;
        }

        if !src.at_end() {
            src.advance();
            return Err(format!(
                "unexpected character: {:?}",
                &input[src.offset() - 1..src.offset()]
            ));
        }
    }
}

/// Skip blank lines and comments, measure indentation, push INDENT/DEDENTs.
fn process_line_start(
    src: &mut Scanner<std::str::Chars<'_>>,
    indent_stack: &mut Vec<usize>,
    mut parser: Parser,
    actions: &mut PyActions,
) -> Result<Parser, String> {
    loop {
        let start = src.offset();
        src.skip_while(|c| c == ' ' || c == '\t');
        let indent = src.offset() - start;

        if src.peek() == Some('#') {
            src.read_until_any(&['\n']);
        }
        match src.peek() {
            Some('\r' | '\n') => {
                if src.peek() == Some('\r') {
                    src.advance();
                }
                if src.peek() == Some('\n') {
                    src.advance();
                }
                continue;
            }
            None => {
                while indent_stack.len() > 1 {
                    indent_stack.pop();
                    push!(parser, actions, Tok::Dedent);
                }
                return Ok(parser);
            }
            _ => {
                let current = *indent_stack.last().unwrap();
                if indent > current {
                    indent_stack.push(indent);
                    push!(parser, actions, Tok::Indent);
                } else if indent < current {
                    while *indent_stack.last().unwrap() > indent {
                        indent_stack.pop();
                        push!(parser, actions, Tok::Dedent);
                    }
                    if *indent_stack.last().unwrap() != indent {
                        return Err("dedent does not match any outer indentation level".into());
                    }
                }
                return Ok(parser);
            }
        }
    }
}

fn read_string_body(
    src: &mut Scanner<std::str::Chars<'_>>,
    quote: char,
    triple: bool,
) -> Result<(), String> {
    if triple {
        loop {
            match src.peek() {
                None => return Err("unterminated string".into()),
                Some('\\') => {
                    src.advance();
                    src.advance();
                }
                Some(c)
                    if c == quote
                        && src.peek_n(1) == Some(quote)
                        && src.peek_n(2) == Some(quote) =>
                {
                    src.advance();
                    src.advance();
                    src.advance();
                    return Ok(());
                }
                _ => {
                    src.advance();
                }
            }
        }
    } else {
        loop {
            match src.peek() {
                None | Some('\n') => return Err("unterminated string".into()),
                Some('\\') => {
                    src.advance();
                    src.advance();
                }
                Some(c) if c == quote => {
                    src.advance();
                    return Ok(());
                }
                _ => {
                    src.advance();
                }
            }
        }
    }
}

fn read_string(src: &mut Scanner<std::str::Chars<'_>>) -> Result<(), String> {
    let quote = src.peek().unwrap();
    let triple = src.peek_n(1) == Some(quote) && src.peek_n(2) == Some(quote);
    if triple {
        src.advance();
        src.advance();
        src.advance();
    } else {
        src.advance();
    }
    read_string_body(src, quote, triple)
}

fn read_number(src: &mut Scanner<std::str::Chars<'_>>) {
    if src.peek() == Some('0') {
        src.advance();
        match src.peek() {
            Some('x' | 'X') => {
                src.advance();
                src.read_hex_digits();
            }
            Some('o' | 'O') => {
                src.advance();
                src.read_while(|c| matches!(c, '0'..='7' | '_'));
            }
            Some('b' | 'B') => {
                src.advance();
                src.read_while(|c| matches!(c, '0' | '1' | '_'));
            }
            _ => {
                src.read_digits();
            }
        }
    } else {
        src.read_digits();
    }
    if src.peek() == Some('.') && src.peek_n(1).is_some_and(|c| c.is_ascii_digit()) {
        src.advance();
        src.read_digits();
    }
    if matches!(src.peek(), Some('e' | 'E')) {
        src.advance();
        if matches!(src.peek(), Some('+' | '-')) {
            src.advance();
        }
        src.read_digits();
    }
    if matches!(src.peek(), Some('j' | 'J')) {
        src.advance();
    }
}

fn is_string_prefix(s: &str) -> bool {
    matches!(
        s,
        "r" | "R"
            | "b"
            | "B"
            | "f"
            | "F"
            | "u"
            | "U"
            | "rb"
            | "Rb"
            | "rB"
            | "RB"
            | "br"
            | "Br"
            | "bR"
            | "BR"
            | "rf"
            | "Rf"
            | "rF"
            | "RF"
            | "fr"
            | "Fr"
            | "fR"
            | "FR"
    )
}

// Operator table: longest first for correct matching.
type OpFactory = fn() -> Tok;
const OPS: [(&str, OpFactory); 41] = [
    ("...", || Tok::Ellipsis),
    ("**=", || Tok::Augassign(AugOp::Pow)),
    ("//=", || Tok::Augassign(AugOp::FloorDiv)),
    ("<<=", || Tok::Augassign(AugOp::Shl)),
    (">>=", || Tok::Augassign(AugOp::Shr)),
    ("**", || Tok::Doublestar(Precedence::Right(12))),
    ("//", || Tok::Binop(BinOp::FloorDiv, Precedence::Left(11))),
    ("<<", || Tok::Binop(BinOp::Shl, Precedence::Left(8))),
    (">>", || Tok::Binop(BinOp::Shr, Precedence::Left(8))),
    ("+=", || Tok::Augassign(AugOp::Add)),
    ("-=", || Tok::Augassign(AugOp::Sub)),
    ("*=", || Tok::Augassign(AugOp::Mul)),
    ("/=", || Tok::Augassign(AugOp::Div)),
    ("%=", || Tok::Augassign(AugOp::Mod)),
    ("&=", || Tok::Augassign(AugOp::BitAnd)),
    ("|=", || Tok::Augassign(AugOp::BitOr)),
    ("^=", || Tok::Augassign(AugOp::BitXor)),
    ("@=", || Tok::Augassign(AugOp::MatMul)),
    ("==", || Tok::CompOp(CompOp::Eq)),
    ("!=", || Tok::CompOp(CompOp::Ne)),
    ("<=", || Tok::CompOp(CompOp::Le)),
    (">=", || Tok::CompOp(CompOp::Ge)),
    ("->", || Tok::Arrow),
    (":=", || Tok::Walrus),
    (".", || Tok::Dot),
    (":", || Tok::Colon),
    (";", || Tok::Semicolon),
    (",", || Tok::Comma),
    ("~", || Tok::Tilde),
    ("@", || Tok::At),
    ("=", || Tok::Eq),
    ("<", || Tok::CompOp(CompOp::Lt)),
    (">", || Tok::CompOp(CompOp::Gt)),
    ("|", || Tok::Binop(BinOp::BitOr, Precedence::Left(5))),
    ("^", || Tok::Binop(BinOp::BitXor, Precedence::Left(6))),
    ("&", || Tok::Binop(BinOp::BitAnd, Precedence::Left(7))),
    ("/", || Tok::Binop(BinOp::Div, Precedence::Left(11))),
    ("%", || Tok::Binop(BinOp::Mod, Precedence::Left(11))),
    ("+", || Tok::Plus(Precedence::Left(9))),
    ("-", || Tok::Minus(Precedence::Left(9))),
    ("*", || Tok::Star(Precedence::Left(10))),
];

// =============================================================================
// Parse Function
// =============================================================================

pub fn parse(input: &str) -> Result<(), String> {
    let parser = python::Parser::<PyActions>::new();
    let mut actions = PyActions;
    let parser = lex(input, parser, &mut actions)?;
    parser
        .finish(&mut actions)
        .map_err(|gazelle::ParseError::Syntax { terminal, recovery }| {
            format!(
                "Finish error: {}",
                recovery.format_error(terminal, None, None,)
            )
        })?;
    Ok(())
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("Python Parser");
        println!("Usage: python <file.py>");
        println!();
        println!("Run tests with: cargo test --example python");
        return;
    }

    let path = &args[1];
    let input = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    match parse(&input) {
        Ok(()) => println!("{}: parsed successfully", path),
        Err(e) => {
            eprintln!("{}: {}", path, e);
            std::process::exit(1);
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Basic statements ----

    #[test]
    fn test_simple_assignment() {
        parse("x = 1\n").unwrap();
    }

    #[test]
    fn test_expression_stmt() {
        parse("x + y\n").unwrap();
    }

    #[test]
    fn test_arithmetic() {
        parse("x = 1 + 2 * 3\n").unwrap();
    }

    #[test]
    fn test_pass() {
        parse("pass\n").unwrap();
    }

    #[test]
    fn test_return() {
        parse("return\n").unwrap();
    }

    #[test]
    fn test_return_value() {
        parse("return x\n").unwrap();
    }

    #[test]
    fn test_multiline() {
        parse("x = 1\ny = 2\nz = x + y\n").unwrap();
    }

    #[test]
    fn test_augmented_assign() {
        parse("x += 1\n").unwrap();
    }

    #[test]
    fn test_annotation() {
        parse("x: int = 5\n").unwrap();
    }

    #[test]
    fn test_del() {
        parse("del x\n").unwrap();
    }

    #[test]
    fn test_assert() {
        parse("assert x\n").unwrap();
    }

    #[test]
    fn test_global() {
        parse("global x\n").unwrap();
    }

    #[test]
    fn test_nonlocal() {
        parse("nonlocal x\n").unwrap();
    }

    #[test]
    fn test_yield() {
        parse("yield x\n").unwrap();
    }

    #[test]
    fn test_raise() {
        parse("raise ValueError()\n").unwrap();
    }

    // ---- Expressions ----

    #[test]
    fn test_call() {
        parse("foo(x, y)\n").unwrap();
    }

    #[test]
    fn test_method_call() {
        parse("obj.method(x)\n").unwrap();
    }

    #[test]
    fn test_subscript() {
        parse("x[0]\n").unwrap();
    }

    #[test]
    fn test_slice() {
        parse("x[1:2]\n").unwrap();
    }

    #[test]
    fn test_unary_minus() {
        parse("x = -1\n").unwrap();
    }

    #[test]
    fn test_power() {
        parse("x = 2 ** 3\n").unwrap();
    }

    #[test]
    fn test_comparison() {
        parse("x = a < b\n").unwrap();
    }

    #[test]
    fn test_chained_comparison() {
        parse("x = a < b < c\n").unwrap();
    }

    #[test]
    fn test_in_comparison() {
        parse("x = a in b\n").unwrap();
    }

    #[test]
    fn test_is_comparison() {
        parse("x = a is b\n").unwrap();
    }

    #[test]
    fn test_is_not_comparison() {
        parse("x = a is not b\n").unwrap();
    }

    #[test]
    fn test_not_in_comparison() {
        parse("x = a not in b\n").unwrap();
    }

    #[test]
    fn test_logical() {
        parse("x = a and b or c\n").unwrap();
    }

    #[test]
    fn test_not() {
        parse("x = not a\n").unwrap();
    }

    #[test]
    fn test_ternary() {
        parse("x = a if b else c\n").unwrap();
    }

    #[test]
    fn test_lambda() {
        parse("f = lambda x: x + 1\n").unwrap();
    }

    #[test]
    fn test_star_unpack() {
        parse("a, *b = [1, 2, 3]\n").unwrap();
    }

    // ---- Literals ----

    #[test]
    fn test_string() {
        parse("x = \"hello\"\n").unwrap();
    }

    #[test]
    fn test_fstring() {
        parse("x = f\"hello {name}\"\n").unwrap();
    }

    #[test]
    fn test_tuple() {
        parse("x = (1, 2, 3)\n").unwrap();
    }

    #[test]
    fn test_list_literal() {
        parse("x = [1, 2, 3]\n").unwrap();
    }

    #[test]
    fn test_dict_literal() {
        parse("x = {1: 2, 3: 4}\n").unwrap();
    }

    // ---- Control flow ----

    #[test]
    fn test_if_stmt() {
        parse("if x > 0:\n    pass\n").unwrap();
    }

    #[test]
    fn test_if_else() {
        parse("if x:\n    a\nelse:\n    b\n").unwrap();
    }

    #[test]
    fn test_elif() {
        parse("if a:\n    pass\nelif b:\n    pass\nelse:\n    pass\n").unwrap();
    }

    #[test]
    fn test_while() {
        parse("while True:\n    break\n").unwrap();
    }

    #[test]
    fn test_for() {
        parse("for x in items:\n    pass\n").unwrap();
    }

    #[test]
    fn test_try_except() {
        parse("try:\n    pass\nexcept:\n    pass\n").unwrap();
    }

    #[test]
    fn test_with() {
        parse("with open(f) as fh:\n    pass\n").unwrap();
    }

    #[test]
    fn test_nested_blocks() {
        parse("if True:\n    if False:\n        pass\n").unwrap();
    }

    #[test]
    fn test_multiple_dedent() {
        parse("if True:\n    if True:\n        pass\nx = 1\n").unwrap();
    }

    // ---- Definitions ----

    #[test]
    fn test_function_def() {
        parse("def foo(x, y):\n    return x + y\n").unwrap();
    }

    #[test]
    fn test_class() {
        parse("class Foo:\n    pass\n").unwrap();
    }

    #[test]
    fn test_decorator() {
        parse("@foo\ndef bar():\n    pass\n").unwrap();
    }

    // ---- Imports ----

    #[test]
    fn test_import() {
        parse("import os\n").unwrap();
    }

    #[test]
    fn test_from_import() {
        parse("from os.path import join\n").unwrap();
    }

    // ---- Comprehensions ----

    #[test]
    fn test_list_comprehension() {
        parse("x = [i for i in range(10)]\n").unwrap();
    }

    // ---- Misc ----

    #[test]
    fn test_implicit_line_join() {
        parse("x = (1 +\n     2)\n").unwrap();
    }

    #[test]
    fn test_empty() {
        parse("").unwrap();
    }

    #[test]
    fn test_only_newlines() {
        parse("\n\n\n").unwrap();
    }

    #[test]
    fn test_no_trailing_newline() {
        parse("x = 1").unwrap_err();
    }

    // ---- Multi-line programs ----

    #[test]
    fn test_multiline_function() {
        parse(
            "\
def fibonacci(n):
    if n <= 1:
        return n
    a = 0
    b = 1
    for i in range(2, n + 1):
        a, b = b, a + b
    return b
",
        )
        .unwrap();
    }

    #[test]
    fn test_class_with_methods() {
        parse(
            "\
class Counter:
    def __init__(self):
        self.count = 0
    def increment(self):
        self.count += 1
    def get(self):
        return self.count
",
        )
        .unwrap();
    }

    #[test]
    fn test_nested_control_flow() {
        parse(
            "\
def process(items):
    result = []
    for item in items:
        if item > 0:
            result.append(item)
        elif item == 0:
            pass
        else:
            result.append(-item)
    return result
",
        )
        .unwrap();
    }

    #[test]
    fn test_try_except_finally() {
        parse(
            "\
try:
    x = 1
except ValueError:
    x = 0
finally:
    cleanup()
",
        )
        .unwrap();
    }

    #[test]
    fn test_nested_comprehension() {
        parse("matrix = [[i * j for j in range(3)] for i in range(3)]\n").unwrap();
    }

    #[test]
    fn test_multiple_decorators() {
        parse(
            "\
@staticmethod
@decorator
def foo():
    pass
",
        )
        .unwrap();
    }

    #[test]
    fn test_multiline_dict() {
        parse(
            "\
config = {
    'host': 'localhost',
    'port': 8080,
    'debug': True,
}
",
        )
        .unwrap();
    }

    #[test]
    fn test_while_else() {
        parse(
            "\
while x > 0:
    x = x - 1
else:
    done()
",
        )
        .unwrap();
    }

    #[test]
    fn test_for_else() {
        parse(
            "\
for x in items:
    if x == target:
        break
else:
    not_found()
",
        )
        .unwrap();
    }
}
