use super::*;
use gazelle::ErrorContext;
use std::collections::BTreeMap;

fn lex_valid_source(input: &str) -> Vec<(gazelle::Token, Span)> {
    let mut parser = c11::Parser::<CActions>::new();
    let mut actions = CActions::new();
    let mut lexer = C11Lexer::new(input);
    let mut tokens = Vec::new();

    while let Some((terminal, span)) = lexer.next(&actions.ctx).unwrap() {
        tokens.push((
            gazelle::Token {
                terminal: terminal.symbol_id(),
                resolution: terminal.resolution(),
            },
            span,
        ));
        parser = parser.push(terminal, &mut actions).unwrap();
    }
    assert!(
        parser.finish(&mut actions).is_ok(),
        "benchmark input must parse"
    );
    tokens
}

/// Mutation benchmark for grammar-derived recovery. It deletes each token
/// with a unique source span from several valid programs, then checks whether
/// recovery proposes reinserting the deleted terminal. Zero-width lexer
/// feedback tokens (TYPE/VARIABLE) are excluded because deleting their empty
/// source span would not mutate the program.
///
/// Run with:
/// `cargo test --example c11 recovery_mutation_benchmark -- --ignored --nocapture`
#[test]
#[ignore = "diagnostic benchmark; run explicitly"]
fn recovery_mutation_benchmark() {
    let programs = [
        "int main(void) { int x = 1; return x; }",
        "int max(int a, int b) { if (a > b) return a; else return b; }",
        "struct Point { int x; int y; }; int origin(void) { return 0; }",
        "int sum(int n) { int s = 0; for (int i = 0; i < n; i++) s += i; return s; }",
    ];

    let mut mutations = 0usize;
    let mut exact_repairs = 0usize;
    let mut still_valid = 0usize;
    let mut other_repairs = 0usize;
    // terminal -> (mutations, still valid, exact repair, other repair)
    let mut by_terminal: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut mismatch_samples = Vec::new();
    let ctx = c11::Parser::<CActions>::error_info();

    for source in programs {
        let tokens = lex_valid_source(source);
        for (index, (deleted, span)) in tokens.iter().enumerate() {
            if span.is_empty() || tokens.iter().filter(|(_, other)| other == span).count() != 1 {
                continue;
            }

            // Avoid mutating a byte range twice if future lexer changes emit
            // non-adjacent tokens with the same source span.
            if tokens[..index].iter().any(|(_, previous)| previous == span) {
                continue;
            }

            mutations += 1;
            let terminal_name = ctx.symbol_name(deleted.terminal).to_string();
            let stats = by_terminal.entry(terminal_name.clone()).or_default();
            stats.0 += 1;
            let mutated = format!("{}{}", &source[..span.start], &source[span.end..]);
            let result = parse_with_recovery(&mutated);
            if result.errors.is_empty() {
                still_valid += 1;
                stats.1 += 1;
            } else if result.errors.iter().any(|error| {
                error.repairs.iter().any(
                    |repair| matches!(repair, gazelle::Repair::Insert(id) if *id == deleted.terminal),
                )
            }) {
                exact_repairs += 1;
                stats.2 += 1;
            } else {
                other_repairs += 1;
                stats.3 += 1;
                if mismatch_samples.len() < 8 {
                    let repairs = result
                        .errors
                        .iter()
                        .flat_map(|error| &error.repairs)
                        .map(|repair| match repair {
                            gazelle::Repair::Insert(id) => {
                                format!("insert {}", ctx.symbol_name(*id))
                            }
                            gazelle::Repair::Delete(id) => {
                                format!("delete {}", ctx.symbol_name(*id))
                            }
                            gazelle::Repair::Shift => "shift".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    mismatch_samples.push(format!(
                        "delete {terminal_name} ({:?}) -> {repairs}",
                        &source[span.clone()]
                    ));
                }
            }
        }
    }

    let invalid_mutations = mutations - still_valid;
    let exact_percent = if invalid_mutations == 0 {
        0.0
    } else {
        100.0 * exact_repairs as f64 / invalid_mutations as f64
    };
    eprintln!(
        "C11 deletion mutations: {mutations} total, {still_valid} still valid, \
         {exact_repairs} restored exactly, {other_repairs} chose another repair \
        ({exact_percent:.1}% exact among invalid mutations)"
    );
    eprintln!("terminal                 total  valid  exact  other");
    for (terminal, (total, valid, exact, other)) in &by_terminal {
        eprintln!("{terminal:24} {total:5} {valid:6} {exact:6} {other:6}");
    }
    if !mismatch_samples.is_empty() {
        eprintln!("sample alternative repairs:");
        for sample in &mismatch_samples {
            eprintln!("  {sample}");
        }
    }

    assert!(mutations >= 40, "benchmark corpus unexpectedly small");
    assert_eq!(
        exact_repairs + other_repairs,
        invalid_mutations,
        "every invalid mutation should produce a recovery"
    );
}
