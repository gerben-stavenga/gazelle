//! Lookahead alignment must not manufacture runtime precedence decisions.
//!
//! `merge_lookaheads` fills missing reduce transitions across same-core
//! states. For a `prec` terminal, reduce edges live on a virtual twin symbol
//! while shift edges use the real one, so "state lacks the reduce edge" does
//! not imply "the token cannot arrive here". Filling a virtual reduce edge
//! into a state that shifts the real twin turns a canonically unconditional
//! shift into a ShiftOrReduce entry, and token precedence can then drive the
//! parser off a valid input.

use gazelle::parse_grammar;
use gazelle::runtime::{Parser, Precedence, Token};
use gazelle::table::CompiledTable;

/// The q-suffix rule is shared between two contexts: inside `e` (where ADD
/// cannot follow `f`, so shifting ADD is the only canonical action) and in
/// `s2` (where ADD follows `f`, a genuine precedence conflict). Alignment
/// must not leak s2's reduce-on-ADD into the expression context.
const WITNESS: &str = r#"
    start s;
    terminals { A, B, NUM, SEMI, prec MUL: _, prec ADD: _ }
    s = A e SEMI => s1 | B f ADD NUM => s2;
    e = e MUL f => em | f => ef;
    f = NUM q => f1;
    q = ADD NUM => q1 | _ => q2;
"#;

/// Same grammar without s2 — no conflict anywhere, nothing to leak.
const CONTROL: &str = r#"
    start s;
    terminals { A, B, NUM, SEMI, prec MUL: _, prec ADD: _ }
    s = A e SEMI => s1;
    e = e MUL f => em | f => ef;
    f = NUM q => f1;
    q = ADD NUM => q1 | _ => q2;
"#;

fn accepts(grammar_src: &str, tokens: &[Token]) -> bool {
    let grammar = parse_grammar(grammar_src).unwrap();
    let compiled = CompiledTable::build(&grammar).unwrap();
    let mut parser = Parser::new(compiled.table());
    for &token in tokens {
        loop {
            match parser.maybe_reduce(Some(token)) {
                Ok(Some(_)) => continue,
                Ok(None) => {
                    parser.shift(token);
                    break;
                }
                Err(_) => return false,
            }
        }
    }
    loop {
        match parser.maybe_reduce(None) {
            Ok(Some((0, _, _))) => return true,
            Ok(Some(_)) => continue,
            _ => return false,
        }
    }
}

#[test]
fn alignment_does_not_leak_prec_reduces_across_contexts() {
    let build = |src: &str| {
        let grammar = parse_grammar(src).unwrap();
        let compiled = CompiledTable::build(&grammar).unwrap();
        let id = |name: &str| compiled.symbol_id(name).unwrap();
        // A NUM MUL NUM ADD NUM SEMI — derivable via s1 in both grammars.
        // ADD binds looser than the pending MUL; a leaked ShiftOrReduce
        // entry would reduce q -> _ here instead of shifting ADD.
        vec![
            Token::new(id("A")),
            Token::new(id("NUM")),
            Token::with_prec(id("MUL"), Precedence::Left(2)),
            Token::new(id("NUM")),
            Token::with_prec(id("ADD"), Precedence::Left(1)),
            Token::new(id("NUM")),
            Token::new(id("SEMI")),
        ]
    };

    assert!(accepts(CONTROL, &build(CONTROL)), "control grammar rejects its own sentence");
    assert!(accepts(WITNESS, &build(WITNESS)), "unrelated rule s2 changed the parse of an s1 sentence");
}
