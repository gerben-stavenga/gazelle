//! Tests for parse error messages with toy grammars.

use gazelle::parse_grammar;
use gazelle::runtime::{Parser, Token};
use gazelle::table::CompiledTable;

/// Simple grammar: S -> a
#[test]
fn error_unexpected_token_simple() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a, b }
        S = a => a;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    // Feed 'b' when 'a' is expected
    let b_id = compiled.symbol_id("b").unwrap();
    let token = Token::new(b_id);

    let err = parser.maybe_reduce(Some(token)).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(msg, "unexpected 'b', expected: S");
}

/// Simple grammar: S -> a, but we send EOF immediately
#[test]
fn error_unexpected_eof() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a }
        S = a => a;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    // Feed EOF when 'a' is expected
    let err = parser.maybe_reduce(None).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(msg, "unexpected '$', expected: S");
}

/// Grammar with multiple expected tokens: S -> a | b
#[test]
fn error_multiple_expected() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a, b, c }
        S = a => a | b => b;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    // Feed 'c' when 'a' or 'b' is expected
    let c_id = compiled.symbol_id("c").unwrap();
    let token = Token::new(c_id);

    let err = parser.maybe_reduce(Some(token)).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(msg, "unexpected 'c', expected: S");
}

#[test]
fn error_deduplicates_display_names() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { x, a, b, bad }
        S = x A => a | x B => b;
        A = a => a;
        B = b => b;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());
    let x = Token::new(compiled.symbol_id("x").unwrap());
    parser.shift(x);

    let bad = Token::new(compiled.symbol_id("bad").unwrap());
    let err = parser.maybe_reduce(Some(bad)).unwrap_err();
    let gazelle::ParseError::Syntax { terminal, .. } = err;
    let msg = parser.format_error(
        terminal,
        &compiled,
        Some(&[("A", "value"), ("B", "value")]),
        None,
    );

    assert!(
        msg.starts_with("unexpected 'bad', expected: value"),
        "{msg}"
    );
    assert!(!msg.contains("value, value"), "{msg}");
}

/// Sequence grammar: S -> a b c
#[test]
fn error_in_sequence() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a, b, c, x }
        S = a b c => s;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let a_id = compiled.symbol_id("a").unwrap();
    let x_id = compiled.symbol_id("x").unwrap();

    // Shift 'a'
    let token_a = Token::new(a_id);
    assert!(parser.maybe_reduce(Some(token_a)).unwrap().is_none());
    parser.shift(token_a);

    // Try 'x' when 'b' is expected
    let token_x = Token::new(x_id);
    let err = parser.maybe_reduce(Some(token_x)).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(
        msg,
        "unexpected 'x', expected: b\n  after: a\n  in S: a \u{2022} b c"
    );
}

/// Expression grammar: E -> E PLUS NUM | NUM
#[test]
fn error_in_expression() {
    let grammar = parse_grammar(
        r#"
        start E;
        terminals { PLUS, NUM, STAR }
        E = E PLUS NUM => add | NUM => num;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let num_id = compiled.symbol_id("NUM").unwrap();
    let plus_id = compiled.symbol_id("PLUS").unwrap();
    let star_id = compiled.symbol_id("STAR").unwrap();

    // Parse "NUM PLUS STAR" - error on STAR
    // Shift NUM
    let token_num = Token::new(num_id);
    while parser.maybe_reduce(Some(token_num)).unwrap().is_some() {}
    parser.shift(token_num);

    // Reduce E -> NUM, then check for PLUS
    let token_plus = Token::new(plus_id);
    while parser.maybe_reduce(Some(token_plus)).unwrap().is_some() {}

    // Shift PLUS
    parser.shift(token_plus);

    // Try STAR when NUM is expected after PLUS
    let token_star = Token::new(star_id);
    let err = parser.maybe_reduce(Some(token_star)).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(
        msg,
        "unexpected 'STAR', expected: NUM\n  after: E PLUS\n  in E: E PLUS \u{2022} NUM"
    );
}

/// Test error at EOF after partial parse
#[test]
fn error_unexpected_eof_after_partial() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a, b }
        S = a b => s;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let a_id = compiled.symbol_id("a").unwrap();
    let token_a = Token::new(a_id);

    assert!(parser.maybe_reduce(Some(token_a)).unwrap().is_none());
    parser.shift(token_a);

    // Try EOF when 'b' is expected
    let err = parser.maybe_reduce(None).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    assert_eq!(
        msg,
        "unexpected '$', expected: b\n  after: a\n  in S: a \u{2022} b"
    );
}

/// Test that EOF is included in expected when at end of valid input.
#[test]
fn error_expects_eof() {
    let grammar = parse_grammar(
        r#"
        start expr;
        terminals { NUM, OP, X }
        expr = expr OP expr => binop | NUM => num;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let num_id = compiled.symbol_id("NUM").unwrap();
    let op_id = compiled.symbol_id("OP").unwrap();
    let x_id = compiled.symbol_id("X").unwrap();

    // Parse NUM
    let tok_num = Token::new(num_id);
    while parser.maybe_reduce(Some(tok_num)).unwrap().is_some() {}
    parser.shift(tok_num);

    // Reduce NUM to expr (use OP as lookahead to allow reduction)
    let tok_op = Token::new(op_id);
    while parser.maybe_reduce(Some(tok_op)).unwrap().is_some() {}

    // Now X (invalid) - should expect OP or $ (EOF)
    let tok_x = Token::new(x_id);
    let err = parser.maybe_reduce(Some(tok_x)).unwrap_err();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );

    println!("Error message: {}", msg);
    assert!(msg.contains("OP"), "should expect OP: {}", msg);
    assert!(msg.contains("$"), "should expect $: {}", msg);
}

/// Test that checkpoint restore undoes spurious reductions for error reporting.
///
/// Grammar: S = a A c | b A d; A = x y.
/// The two uses of A have different lookaheads (c vs d). merge_lookaheads adds
/// spurious reductions so both states reduce A on {c, d}. Input "a x y d" triggers
/// the spurious reduction (d is only valid in "b A d" path), then errors.
/// Without checkpoint restore, the stack shows "a A" (post-reduction).
/// With checkpoint restore, the stack shows "a x y" (pre-reduction).
#[test]
fn error_checkpoint_restores_pre_reduction_stack() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { a, b, c, d, x, y }
        S = a A c => s1 | b A d => s2;
        A = x y => xy;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let tok_a = Token::new(compiled.symbol_id("a").unwrap());
    let tok_x = Token::new(compiled.symbol_id("x").unwrap());
    let tok_y = Token::new(compiled.symbol_id("y").unwrap());
    let tok_d = Token::new(compiled.symbol_id("d").unwrap());

    // Parse "a x y"
    while parser.maybe_reduce(Some(tok_a)).unwrap().is_some() {}
    parser.shift(tok_a);
    while parser.maybe_reduce(Some(tok_x)).unwrap().is_some() {}
    parser.shift(tok_x);
    while parser.maybe_reduce(Some(tok_y)).unwrap().is_some() {}
    parser.shift(tok_y);

    // Feed 'd' — invalid because after "a A", only "c" is valid.
    // The spurious reduction A -> x y fires on 'd', then error.
    let mut reductions = 0;
    let err = loop {
        match parser.maybe_reduce(Some(tok_d)) {
            Ok(Some(_)) => {
                reductions += 1;
                continue;
            }
            Ok(None) => panic!("expected error, not shift"),
            Err(e) => break e,
        }
    };

    // Verify the spurious reduction actually fired (test is meaningless without it)
    assert!(
        reductions > 0,
        "test requires spurious reductions to exercise checkpoint"
    );

    parser.restore_checkpoint();
    let msg = parser.format_error(
        {
            let gazelle::ParseError::Syntax { terminal, .. } = err;
            terminal
        },
        &compiled,
        None,
        None,
    );
    // Checkpoint restored: stack shows original tokens, not the reduced nonterminal
    assert!(
        msg.contains("after: a x y"),
        "expected pre-reduction stack: {}",
        msg
    );
}

/// Test that state merging doesn't cause spurious lookaheads.
/// Grammar: S -> A | B; A -> '(' expr ')'; B -> '[' expr ']'; expr -> x
/// After parsing '(' x, only ')' should be expected, not ']'.
#[test]
fn error_no_spurious_lalr_lookahead() {
    let grammar = parse_grammar(
        r#"
        start S;
        terminals { LPAREN, RPAREN, LBRACKET, RBRACKET, x }
        S = A => a | B => b;
        A = LPAREN expr RPAREN => a;
        B = LBRACKET expr RBRACKET => b;
        expr = x => x;
    "#,
    )
    .unwrap();

    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());

    let lparen = compiled.symbol_id("LPAREN").unwrap();
    let x_id = compiled.symbol_id("x").unwrap();
    let rbracket = compiled.symbol_id("RBRACKET").unwrap();

    // Parse '(' x - shift '('
    let tok_lparen = Token::new(lparen);
    while parser.maybe_reduce(Some(tok_lparen)).unwrap().is_some() {}
    parser.shift(tok_lparen);

    // Shift 'x'
    let tok_x = Token::new(x_id);
    while parser.maybe_reduce(Some(tok_x)).unwrap().is_some() {}
    parser.shift(tok_x);

    // Try ']' - this should cause reductions (expr -> x) and then error
    let tok_rbracket = Token::new(rbracket);

    // Do any reductions possible with ']' as lookahead
    loop {
        match parser.maybe_reduce(Some(tok_rbracket)) {
            Ok(Some(_)) => continue, // reduction happened
            Ok(None) => {
                break;
            }
            Err(gazelle::ParseError::Syntax { terminal, .. }) => {
                let msg = parser.format_error(terminal, &compiled, None, None);
                // Should only expect RPAREN, not RBRACKET
                assert!(
                    msg.contains("expected: RPAREN"),
                    "msg should expect RPAREN: {}",
                    msg
                );
                assert!(
                    !msg.contains("expected: RBRACKET") && !msg.contains(", RBRACKET"),
                    "msg should not expect RBRACKET: {}",
                    msg
                );
                return;
            }
        }
    }

    panic!("Expected parse error but got shift");
}
