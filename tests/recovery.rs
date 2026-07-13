//! Tests for automatic error recovery.

use gazelle::parse_grammar;
use gazelle::runtime::{Parser, RecoveryLimits, RecoveryStatus, Repair, Token};
use gazelle::table::CompiledTable;

/// Push tokens until error, then recover with remaining buffer.
fn parse_and_recover(compiled: &CompiledTable, tokens: &[&str]) -> Vec<gazelle::RecoveryInfo> {
    let mut parser = Parser::new(compiled.table());
    let token_ids: Vec<Token> = tokens
        .iter()
        .map(|name| Token::new(compiled.symbol_id(name).unwrap()))
        .collect();

    let mut pos = 0;
    while pos < token_ids.len() {
        loop {
            let token = if pos < token_ids.len() {
                Some(token_ids[pos])
            } else {
                None
            };
            match parser.maybe_reduce(token) {
                Ok(None) => break,                    // shift
                Ok(Some((0, _, _))) => return vec![], // accept
                Ok(Some(_)) => continue,              // reduce, loop
                Err(_) => {
                    // Error: recover with remaining tokens
                    return parser.recover(&token_ids[pos..]);
                }
            }
        }
        if pos < token_ids.len() {
            parser.shift(token_ids[pos]);
            pos += 1;
        }
    }

    // Try to finish
    loop {
        match parser.maybe_reduce(None) {
            Ok(Some((0, _, _))) => return vec![],
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => {
                return parser.recover(&[]);
            }
        }
    }
}

// Grammar: stmt_list = stmt*; stmt = ID SEMI;
const STMT_GRAMMAR: &str = r#"
    start stmts;
    terminals { ID, SEMI, LPAREN, RPAREN, PLUS, STAR }
    stmts = stmt* => stmts;
    stmt = ID SEMI => stmt;
"#;

#[test]
fn recover_valid_input() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    let errors = parse_and_recover(&compiled, &["ID", "SEMI", "ID", "SEMI"]);
    assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
}

#[test]
fn recover_missing_semicolon() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    // "ID ID SEMI" — missing SEMI after first ID
    let errors = parse_and_recover(&compiled, &["ID", "ID", "SEMI"]);
    assert!(!errors.is_empty(), "expected at least one error");

    // Inserting SEMI and deleting the second ID are both cost-1 repairs. Prefer
    // insertion because it preserves more of the user's input.
    let semi_id = compiled.symbol_id("SEMI").unwrap();
    let has_insert_semi = errors[0]
        .repairs
        .iter()
        .any(|r| matches!(r, Repair::Insert(id) if *id == semi_id));
    assert!(
        has_insert_semi,
        "expected insertion to win the equal-cost tie, got: {:?}",
        errors[0].repairs
    );
}

#[test]
fn recover_extra_token() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    // "ID SEMI PLUS ID SEMI" — PLUS is unexpected between statements
    let errors = parse_and_recover(&compiled, &["ID", "SEMI", "PLUS", "ID", "SEMI"]);
    assert!(!errors.is_empty(), "expected at least one error");

    // The repair should delete PLUS
    let has_delete = errors[0]
        .repairs
        .iter()
        .any(|r| matches!(r, Repair::Delete(_)));
    assert!(
        has_delete,
        "expected delete of extra token, got: {:?}",
        errors[0].repairs
    );
}

// Grammar with parens: expr = LPAREN expr RPAREN | ID;
const EXPR_GRAMMAR: &str = r#"
    start expr;
    terminals { ID, LPAREN, RPAREN, PLUS, SEMI }
    expr = LPAREN expr RPAREN => paren
         | ID => id;
"#;

#[test]
fn recover_missing_rparen() {
    let grammar = parse_grammar(EXPR_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    // "(ID" — missing closing paren
    let errors = parse_and_recover(&compiled, &["LPAREN", "ID"]);
    assert!(!errors.is_empty(), "expected at least one error");

    // Should insert RPAREN
    let rparen_id = compiled.symbol_id("RPAREN").unwrap();
    let has_insert = errors[0]
        .repairs
        .iter()
        .any(|r| matches!(r, Repair::Insert(id) if *id == rparen_id));
    assert!(
        has_insert,
        "expected insert RPAREN, got: {:?}",
        errors[0].repairs
    );
}

#[test]
fn recover_multiple_errors() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    // "ID ID SEMI ID ID SEMI" — missing SEMI twice
    let errors = parse_and_recover(&compiled, &["ID", "ID", "SEMI", "ID", "ID", "SEMI"]);
    assert!(
        errors.len() >= 2,
        "expected at least 2 errors, got: {:?}",
        errors
    );
}

/// Test that recovery works via the low-level Parser API directly.
#[test]
fn recover_direct_api() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    let mut parser = Parser::new(compiled.table());
    let id = Token::new(compiled.symbol_id("ID").unwrap());
    let semi = Token::new(compiled.symbol_id("SEMI").unwrap());

    // Push "ID" successfully
    loop {
        match parser.maybe_reduce(Some(id)) {
            Ok(None) => break,
            Ok(Some(_)) => continue,
            Err(_) => panic!("unexpected error on ID"),
        }
    }
    parser.shift(id);

    // Now push another ID without SEMI — should error
    let result = parser.maybe_reduce(Some(id));
    assert!(result.is_err(), "expected error on second ID without SEMI");

    // Recovery carries the dynamic grammar's diagnostic context.
    let mut recovery = parser.into_recovery(&compiled);
    let diagnostic = recovery.diagnose(id.terminal);
    assert_eq!(diagnostic.unexpected, id.terminal);
    assert!(diagnostic.expected.contains(&semi.terminal));

    // Recover with remaining tokens: [ID, SEMI]
    let remaining = vec![id, semi];
    let errors = recovery.recover(&remaining);
    assert!(!errors.is_empty(), "expected recovery errors");

    let outcome = recovery.recover_with_limits(&[], RecoveryLimits::default());
    assert_eq!(
        outcome.status,
        RecoveryStatus::Recovered,
        "recover should leave the parser in the successful repaired state"
    );
}

/// Test recovery at EOF (missing semicolon, no remaining tokens).
#[test]
fn recover_at_eof() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();

    let mut parser = Parser::new(compiled.table());
    let id = Token::new(compiled.symbol_id("ID").unwrap());

    // Parse just "ID" — missing the SEMI before EOF
    loop {
        match parser.maybe_reduce(Some(id)) {
            Ok(None) => break,
            Ok(Some(_)) => continue,
            Err(_) => panic!("unexpected error on ID"),
        }
    }
    parser.shift(id);

    // Recover with empty buffer (EOF)
    let errors = parser.recover(&[]);
    assert!(!errors.is_empty(), "expected recovery at EOF");
    // Should insert SEMI
    let semi_id = compiled.symbol_id("SEMI").unwrap();
    let has_insert = errors.iter().any(|e| {
        e.repairs
            .iter()
            .any(|r| matches!(r, Repair::Insert(id) if *id == semi_id))
    });
    assert!(has_insert, "expected insert SEMI at EOF, got: {:?}", errors);
}

#[test]
fn recovery_reports_configured_limit_exhaustion() {
    let grammar = parse_grammar(STMT_GRAMMAR).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());
    let id = Token::new(compiled.symbol_id("ID").unwrap());

    while let Ok(Some(_)) = parser.maybe_reduce(Some(id)) {}
    parser.shift(id);

    let outcome = parser.recover_with_limits(
        &[],
        RecoveryLimits {
            max_states: 64,
            max_cost: 0,
        },
    );
    assert_eq!(outcome.status, RecoveryStatus::LimitReached);
    assert!(outcome.errors.is_empty());

    let outcome = parser.recover_with_limits(&[], RecoveryLimits::default());
    assert_eq!(outcome.status, RecoveryStatus::Recovered);
    assert!(!outcome.errors.is_empty());
}
