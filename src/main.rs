//! Gazelle CLI - parse grammar and output tables or Rust code.

#[cfg(feature = "codegen")]
use gazelle::codegen::{self, CodegenContext};
#[cfg(not(feature = "bootstrap"))]
use gazelle::{CompiledTable, SymbolId, parse_grammar};
use std::env;
use std::fs;
use std::io::{self, Read};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!(
        "gazelle-parser {VERSION}
LR parser generator with runtime operator precedence and natural lexer feedback

USAGE:
    gazelle-parser [OPTIONS] [FILE]

ARGS:
    <FILE>    Input grammar file (reads from stdin if omitted)

OPTIONS:
    --rust    Output generated Rust parser code (requires 'codegen' feature)
    --yacc    Output Bison-compatible .y format (requires 'codegen' feature)
    --help    Print this help message
    --version Print version

Without --rust or --yacc, outputs JSON parse tables."
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut rust_mode = false;
    let mut yacc_mode = false;
    let mut bootstrap_meta = false;
    let mut input_file: Option<&str> = None;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--rust" => rust_mode = true,
            "--yacc" => yacc_mode = true,
            "--bootstrap-meta" => bootstrap_meta = true,
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--version" | "-V" => {
                println!("gazelle-parser {VERSION}");
                return;
            }
            "-" => {}
            s if s.starts_with('-') => {
                eprintln!("unknown option: {s}");
                eprintln!("Run 'gazelle-parser --help' for usage.");
                std::process::exit(1);
            }
            _ => {
                if input_file.is_some() {
                    eprintln!("unexpected argument: {arg}");
                    eprintln!("Run 'gazelle-parser --help' for usage.");
                    std::process::exit(1);
                }
                input_file = Some(arg);
            }
        }
    }

    if bootstrap_meta {
        #[cfg(feature = "codegen")]
        {
            do_bootstrap_meta();
        }
        #[cfg(not(feature = "codegen"))]
        {
            eprintln!("--bootstrap-meta requires the 'codegen' feature");
            std::process::exit(1);
        }
        return;
    }

    let input = if let Some(file) = input_file {
        match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{file}: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            eprintln!("failed to read stdin: {e}");
            std::process::exit(1);
        }
        buf
    };

    if yacc_mode {
        #[cfg(not(feature = "bootstrap"))]
        output_yacc(&input);
        #[cfg(feature = "bootstrap")]
        {
            let _ = &input;
            eprintln!("--yacc mode not available in bootstrap build");
            std::process::exit(1);
        }
    } else if rust_mode {
        #[cfg(all(feature = "codegen", not(feature = "bootstrap")))]
        {
            output_rust(&input);
        }
        #[cfg(not(all(feature = "codegen", not(feature = "bootstrap"))))]
        {
            let _ = &input;
            eprintln!("--rust mode requires the 'codegen' feature (without bootstrap)");
            std::process::exit(1);
        }
    } else {
        #[cfg(not(feature = "bootstrap"))]
        output_json(&input);
        #[cfg(feature = "bootstrap")]
        {
            let _ = &input;
            eprintln!("JSON mode not available in bootstrap build");
            std::process::exit(1);
        }
    }
}

#[cfg(all(feature = "codegen", not(feature = "bootstrap")))]
fn output_rust(input: &str) {
    let grammar_def = match gazelle::parse_grammar(input) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    let ctx = match CodegenContext::from_grammar(&grammar_def, "", "pub ", false) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    match codegen::generate_items(&ctx) {
        Ok(tokens) => {
            let syntax_tree: syn::File = match syn::parse2(tokens) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("internal error: generated invalid Rust code: {e}");
                    std::process::exit(1);
                }
            };
            let formatted = prettyplease::unparse(&syntax_tree);
            println!("{}", formatted);
        }
        Err(e) => {
            eprintln!("Codegen error: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "bootstrap"))]
fn output_yacc(input: &str) {
    #[cfg(feature = "codegen")]
    {
        let grammar = match parse_grammar(input) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Parse error: {}", e);
                std::process::exit(1);
            }
        };
        match codegen::to_yacc(&grammar) {
            Ok(yacc) => print!("{}", yacc),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(feature = "codegen"))]
    {
        let _ = input;
        eprintln!("--yacc mode requires the 'codegen' feature");
        std::process::exit(1);
    }
}

#[cfg(feature = "codegen")]
fn do_bootstrap_meta() {
    use gazelle::grammar as g;

    let grammar = g::Grammar {
        start: "grammar_def".to_string(),
        expect_rr: 0,
        expect_sr: 0,
        terminals: vec![
            g::TerminalDef {
                name: "IDENT".into(),
                has_type: true,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "NUM".into(),
                has_type: true,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "REGEX".into(),
                has_type: true,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "MODIFIER".into(),
                has_type: true,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "KW_START".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "KW_TERMINALS".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "KW_EXPECT".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "UNDERSCORE".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "LBRACE".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "RBRACE".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "LPAREN".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "RPAREN".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "COLON".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "COMMA".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "EQ".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "PIPE".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "SEMI".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "FAT_ARROW".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "QUESTION".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "STAR".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "PLUS".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
            g::TerminalDef {
                name: "PERCENT".into(),
                has_type: false,
                kind: g::TerminalKind::Plain,
                pattern: None,
            },
        ],
        rules: vec![
            g::Rule {
                name: "grammar_def".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("KW_START".into()),
                        g::Term::Symbol("IDENT".into()),
                        g::Term::Symbol("SEMI".into()),
                        g::Term::ZeroOrMore { symbol: "expect_decl".into(), name: None },
                        g::Term::Symbol("KW_TERMINALS".into()),
                        g::Term::Symbol("LBRACE".into()),
                        g::Term::SeparatedBy {
                            symbol: "terminal_item".into(),
                            sep: "COMMA".into(),
                            name: None,
                        },
                        g::Term::Symbol("RBRACE".into()),
                        g::Term::OneOrMore { symbol: "rule".into(), name: None },
                    ],
                    name: "grammar_def".into(),
                }],
            },
            g::Rule {
                name: "expect_decl".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("KW_EXPECT".into()),
                        g::Term::Symbol("NUM".into()),
                        g::Term::Symbol("IDENT".into()),
                        g::Term::Symbol("SEMI".into()),
                    ],
                    name: "expect_decl".into(),
                }],
            },
            g::Rule {
                name: "terminal_item".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Optional("MODIFIER".into()),
                        g::Term::Symbol("IDENT".into()),
                        g::Term::Optional("type_annot".into()),
                        g::Term::Optional("regex_annot".into()),
                    ],
                    name: "terminal_item".into(),
                }],
            },
            g::Rule {
                name: "type_annot".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("COLON".into()),
                        g::Term::Symbol("UNDERSCORE".into()),
                    ],
                    name: "type_annot".into(),
                }],
            },
            g::Rule {
                name: "regex_annot".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("EQ".into()),
                        g::Term::Symbol("REGEX".into()),
                    ],
                    name: "regex_annot".into(),
                }],
            },
            g::Rule {
                name: "rule".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("IDENT".into()),
                        g::Term::Symbol("EQ".into()),
                        g::Term::SeparatedBy {
                            symbol: "alt".into(),
                            sep: "PIPE".into(),
                            name: None,
                        },
                        g::Term::Symbol("SEMI".into()),
                    ],
                    name: "rule".into(),
                }],
            },
            g::Rule {
                name: "alt".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::OneOrMore { symbol: "term".into(), name: None },
                        g::Term::Symbol("variant".into()),
                    ],
                    name: "alt".into(),
                }],
            },
            g::Rule {
                name: "variant".into(),
                alts: vec![g::Alt {
                    terms: vec![
                        g::Term::Symbol("FAT_ARROW".into()),
                        g::Term::Symbol("IDENT".into()),
                    ],
                    name: "variant".into(),
                }],
            },
            g::Rule {
                name: "term".into(),
                alts: vec![
                    g::Alt {
                        terms: vec![
                            g::Term::Symbol("LPAREN".into()),
                            g::Term::Symbol("IDENT".into()),
                            g::Term::Symbol("PERCENT".into()),
                            g::Term::Symbol("IDENT".into()),
                            g::Term::Symbol("RPAREN".into()),
                        ],
                        name: "sym_sep".into(),
                    },
                    g::Alt {
                        terms: vec![
                            g::Term::Symbol("IDENT".into()),
                            g::Term::Symbol("QUESTION".into()),
                        ],
                        name: "sym_opt".into(),
                    },
                    g::Alt {
                        terms: vec![
                            g::Term::Symbol("IDENT".into()),
                            g::Term::Symbol("STAR".into()),
                        ],
                        name: "sym_star".into(),
                    },
                    g::Alt {
                        terms: vec![
                            g::Term::Symbol("IDENT".into()),
                            g::Term::Symbol("PLUS".into()),
                        ],
                        name: "sym_plus".into(),
                    },
                    g::Alt {
                        terms: vec![g::Term::Symbol("IDENT".into())],
                        name: "sym_plain".into(),
                    },
                    g::Alt {
                        terms: vec![g::Term::Symbol("UNDERSCORE".into())],
                        name: "sym_empty".into(),
                    },
                ],
            },
        ],
    };

    let ctx = CodegenContext::from_grammar(&grammar, "", "pub ", false)
        .expect("failed to build codegen context");

    match codegen::generate_items(&ctx) {
        Ok(tokens) => {
            let syntax_tree: syn::File = match syn::parse2(tokens) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("internal error: generated invalid Rust code: {e}");
                    std::process::exit(1);
                }
            };
            let formatted = prettyplease::unparse(&syntax_tree);
            println!("{}", formatted);
        }
        Err(e) => {
            eprintln!("Codegen error: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "bootstrap"))]
fn output_json(input: &str) {
    let grammar = match parse_grammar(input) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    let table = match CompiledTable::build(&grammar) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if table.has_conflicts() {
        for msg in table.format_conflicts() {
            eprintln!("{}\n", msg);
        }
    }

    // Output JSON
    println!("{{");

    // Action encoding documentation
    println!(
        "  \"_action_encoding\": \"0=error, (1|state<<2)=shift, (2|rule<<2)=reduce, 3=accept\","
    );

    // Symbol names
    println!("  \"symbols\": [");
    let num_symbols = table.num_symbols();
    for i in 0..num_symbols {
        let name = table.symbol_name(SymbolId::new(i));
        let comma = if i + 1 < num_symbols { "," } else { "" };
        println!("    \"{}\"{}", escape_json(name), comma);
    }
    println!("  ],");

    // Terminal count (symbols 0..num_terminals are terminals)
    println!("  \"num_terminals\": {},", table.num_terminals());

    // Rules: [lhs_symbol_id, rhs_length]
    println!("  \"rules\": [");
    let rules = table.rules();
    for (i, (lhs, len)) in rules.iter().enumerate() {
        let comma = if i + 1 < rules.len() { "," } else { "" };
        println!("    [{}, {}]{}", lhs, len, comma);
    }
    println!("  ],");

    // Number of states
    println!("  \"num_states\": {},", table.num_states());

    // Shared displacement table (bison-style: action + goto share data/check)
    print!("  \"data\": [");
    print_u32_array(table.table_data());
    println!("],");

    print!("  \"check\": [");
    print_u32_array(table.table_check());
    println!("],");

    print!("  \"action_base\": [");
    print_i32_array(table.action_base());
    println!("],");

    print!("  \"goto_base\": [");
    print_i32_array(table.goto_base());
    println!("],");

    // Default reduce per state
    print!("  \"default_reduce\": [");
    print_u32_array(table.default_reduce());
    println!("],");

    // Default goto per non-terminal
    print!("  \"default_goto\": [");
    print_u32_array(table.default_goto());
    println!("]");

    println!("}}");
}

#[cfg(not(feature = "bootstrap"))]
fn print_u32_array(arr: &[u32]) {
    for (i, v) in arr.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("{}", v);
    }
}

#[cfg(not(feature = "bootstrap"))]
fn print_i32_array(arr: &[i32]) {
    for (i, v) in arr.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("{}", v);
    }
}

#[cfg(not(feature = "bootstrap"))]
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
