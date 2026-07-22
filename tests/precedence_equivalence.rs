//! Test that dynamic precedence parsing matches fixed grammar parsing.
//!
//! Generates all expressions with +, *, ^ operators up to 7 numbers
//! (1,093 sequences) and verifies both approaches produce identical ASTs.

use gazelle::Precedence;
use gazelle_macros::gazelle;

// ============================================================================
// AST representation (shared between both parsers)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Num(i32),
    BinOp(Box<Expr>, char, Box<Expr>),
}

impl Expr {
    fn binop(l: Expr, op: char, r: Expr) -> Expr {
        Expr::BinOp(Box::new(l), op, Box::new(r))
    }
}

// ============================================================================
// Dynamic precedence grammar (single rule with prec terminal)
// ============================================================================

gazelle! {
    grammar dynamic {
        start expr;
        terminals {
            NUM: _,
            prec OP: _
        }

        expr = expr OP expr => binop
                   | NUM => num;
    }
}

struct DynBuilder;

impl gazelle::ErrorType for DynBuilder {
    type Error = core::convert::Infallible;
}

impl dynamic::Types for DynBuilder {
    type Num = i32;
    type Op = char;
    type Expr = Expr;
}

impl gazelle::Action<dynamic::Expr<Self>> for DynBuilder {
    fn build(&mut self, node: dynamic::Expr<Self>) -> Result<Expr, Self::Error> {
        Ok(match node {
            dynamic::Expr::Binop(l, op, r) => Expr::binop(l, op, r),
            dynamic::Expr::Num(n) => Expr::Num(n),
        })
    }
}

fn parse_dynamic(input: &str) -> Result<Expr, String> {
    let tokens = lex_dynamic(input)?;
    let mut parser = dynamic::Parser::<DynBuilder>::new();
    let mut actions = DynBuilder;

    for tok in tokens {
        parser = parser
            .push(tok, &mut actions)
            .map_err(|e| format!("{:?}", e))?;
    }

    parser.finish(&mut actions).map_err(|e| {
        let gazelle::ParseError::Syntax { terminal, recovery } = e;
        recovery.format_error(terminal, None, None)
    })
}

fn lex_dynamic(input: &str) -> Result<Vec<dynamic::Terminal<DynBuilder>>, String> {
    let mut tokens = Vec::new();
    for c in input.chars() {
        match c {
            ' ' => {}
            '0'..='9' => tokens.push(dynamic::Terminal::Num(c as i32 - '0' as i32)),
            '+' => tokens.push(dynamic::Terminal::Op('+', Precedence::Left(1))),
            '*' => tokens.push(dynamic::Terminal::Op('*', Precedence::Left(2))),
            '^' => tokens.push(dynamic::Terminal::Op('^', Precedence::Right(3))),
            _ => return Err(format!("unexpected char: {}", c)),
        }
    }
    Ok(tokens)
}

// ============================================================================
// Fixed precedence grammar (explicit rule hierarchy)
// ============================================================================

gazelle! {
    grammar fixed {
        start expr;
        terminals {
            NUM: _,
            PLUS, STAR, CARET
        }

        // Lowest precedence: addition (left-associative)
        expr = expr PLUS term => add
                   | term => term;

        // Medium precedence: multiplication (left-associative)
        term = term STAR factor => mul
                   | factor => factor;

        // Highest precedence: exponentiation (right-associative)
        factor = base CARET factor => pow
                       | base => base;

        base = NUM => num;
    }
}

struct FixedBuilder;

impl gazelle::ErrorType for FixedBuilder {
    type Error = core::convert::Infallible;
}

impl fixed::Types for FixedBuilder {
    type Num = i32;
    type Expr = Expr;
    type Term = Expr;
    type Factor = Expr;
    type Base = Expr;
}

impl gazelle::Action<fixed::Expr<Self>> for FixedBuilder {
    fn build(&mut self, node: fixed::Expr<Self>) -> Result<Expr, Self::Error> {
        Ok(match node {
            fixed::Expr::Add(l, r) => Expr::binop(l, '+', r),
            fixed::Expr::Term(t) => t,
        })
    }
}

impl gazelle::Action<fixed::Term<Self>> for FixedBuilder {
    fn build(&mut self, node: fixed::Term<Self>) -> Result<Expr, Self::Error> {
        Ok(match node {
            fixed::Term::Mul(l, r) => Expr::binop(l, '*', r),
            fixed::Term::Factor(f) => f,
        })
    }
}

impl gazelle::Action<fixed::Factor<Self>> for FixedBuilder {
    fn build(&mut self, node: fixed::Factor<Self>) -> Result<Expr, Self::Error> {
        Ok(match node {
            fixed::Factor::Pow(l, r) => Expr::binop(l, '^', r),
            fixed::Factor::Base(b) => b,
        })
    }
}

impl gazelle::Action<fixed::Base<Self>> for FixedBuilder {
    fn build(&mut self, node: fixed::Base<Self>) -> Result<Expr, Self::Error> {
        Ok(match node {
            fixed::Base::Num(n) => Expr::Num(n),
        })
    }
}

fn parse_fixed(input: &str) -> Result<Expr, String> {
    let tokens = lex_fixed(input)?;
    let mut parser = fixed::Parser::<FixedBuilder>::new();
    let mut actions = FixedBuilder;

    for tok in tokens {
        parser = parser
            .push(tok, &mut actions)
            .map_err(|e| format!("{:?}", e))?;
    }

    parser.finish(&mut actions).map_err(|e| {
        let gazelle::ParseError::Syntax { terminal, recovery } = e;
        recovery.format_error(terminal, None, None)
    })
}

fn lex_fixed(input: &str) -> Result<Vec<fixed::Terminal<FixedBuilder>>, String> {
    let mut tokens = Vec::new();
    for c in input.chars() {
        match c {
            ' ' => {}
            '0'..='9' => tokens.push(fixed::Terminal::Num(c as i32 - '0' as i32)),
            '+' => tokens.push(fixed::Terminal::Plus),
            '*' => tokens.push(fixed::Terminal::Star),
            '^' => tokens.push(fixed::Terminal::Caret),
            _ => return Err(format!("unexpected char: {}", c)),
        }
    }
    Ok(tokens)
}

// ============================================================================
// Expression generator
// ============================================================================

fn generate_expressions(max_nums: usize) -> Vec<String> {
    let ops = ['+', '*', '^'];
    let mut results = Vec::new();

    // Generate expressions with 1 to max_nums numbers
    for num_count in 1..=max_nums {
        generate_with_nums(num_count, &ops, &mut results);
    }

    results
}

fn generate_with_nums(num_count: usize, ops: &[char], results: &mut Vec<String>) {
    if num_count == 0 {
        return;
    }

    // Numbers 1-9 (single digit for simplicity)
    let nums: Vec<char> = (1..=9)
        .take(num_count)
        .map(|n| char::from_digit(n, 10).unwrap())
        .collect();

    if num_count == 1 {
        results.push(nums[0].to_string());
        return;
    }

    // For n numbers, we have n-1 operator positions
    // Each can be +, *, or ^
    let op_count = num_count - 1;
    let total_combinations = ops.len().pow(op_count as u32);

    for combo in 0..total_combinations {
        let mut expr = String::new();
        let mut remaining = combo;

        for (i, num) in nums.iter().enumerate().take(num_count) {
            expr.push(*num);
            if i < op_count {
                let op_idx = remaining % ops.len();
                remaining /= ops.len();
                expr.push(' ');
                expr.push(ops[op_idx]);
                expr.push(' ');
            }
        }

        results.push(expr);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_precedence_equivalence() {
    let expressions = generate_expressions(7);
    let mut passed = 0;
    let mut failed = 0;

    for expr in &expressions {
        let dynamic_result = parse_dynamic(expr);
        let fixed_result = parse_fixed(expr);

        match (&dynamic_result, &fixed_result) {
            (Ok(d), Ok(f)) if d == f => {
                passed += 1;
            }
            (Ok(d), Ok(f)) => {
                eprintln!("MISMATCH: {}", expr);
                eprintln!("  dynamic: {:?}", d);
                eprintln!("  fixed:   {:?}", f);
                failed += 1;
            }
            (Err(e1), Err(e2)) => {
                eprintln!("BOTH FAILED: {} -> {} / {}", expr, e1, e2);
                failed += 1;
            }
            (Err(e), Ok(_)) => {
                eprintln!("DYNAMIC FAILED: {} -> {}", expr, e);
                failed += 1;
            }
            (Ok(_), Err(e)) => {
                eprintln!("FIXED FAILED: {} -> {}", expr, e);
                failed += 1;
            }
        }
    }

    eprintln!(
        "\nTotal: {} expressions, {} passed, {} failed",
        expressions.len(),
        passed,
        failed
    );
    assert_eq!(failed, 0, "Some expressions produced different ASTs");
}

#[test]
fn test_specific_cases() {
    // Test precedence: * binds tighter than +
    let expr = "1 + 2 * 3";
    let expected = Expr::binop(
        Expr::Num(1),
        '+',
        Expr::binop(Expr::Num(2), '*', Expr::Num(3)),
    );
    assert_eq!(parse_dynamic(expr).unwrap(), expected);
    assert_eq!(parse_fixed(expr).unwrap(), expected);

    // Test left-associativity of +
    let expr = "1 + 2 + 3";
    let expected = Expr::binop(
        Expr::binop(Expr::Num(1), '+', Expr::Num(2)),
        '+',
        Expr::Num(3),
    );
    assert_eq!(parse_dynamic(expr).unwrap(), expected);
    assert_eq!(parse_fixed(expr).unwrap(), expected);

    // Test right-associativity of ^
    let expr = "2 ^ 3 ^ 4";
    let expected = Expr::binop(
        Expr::Num(2),
        '^',
        Expr::binop(Expr::Num(3), '^', Expr::Num(4)),
    );
    assert_eq!(parse_dynamic(expr).unwrap(), expected);
    assert_eq!(parse_fixed(expr).unwrap(), expected);

    // Test mixed: 1 + 2 ^ 3 * 4 = 1 + ((2^3) * 4)
    let expr = "1 + 2 ^ 3 * 4";
    let expected = Expr::binop(
        Expr::Num(1),
        '+',
        Expr::binop(
            Expr::binop(Expr::Num(2), '^', Expr::Num(3)),
            '*',
            Expr::Num(4),
        ),
    );
    assert_eq!(parse_dynamic(expr).unwrap(), expected);
    assert_eq!(parse_fixed(expr).unwrap(), expected);
}
