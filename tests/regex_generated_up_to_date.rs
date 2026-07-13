//! Verify that `src/regex_generated.rs` matches the output of `gazelle --rust grammars/regex.gzl`.

use std::process::Command;

#[test]
fn regex_generated_matches_codegen_output() {
    // Build and run: cargo run -- --rust grammars/regex.gzl
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--features",
            "codegen",
            "--",
            "--rust",
            "grammars/regex.gzl",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run gazelle CLI");

    assert!(
        output.status.success(),
        "gazelle --rust grammars/regex.gzl failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = String::from_utf8(output.stdout).expect("non-UTF-8 output");
    let actual = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/regex_generated.rs"),
    )
    .expect("failed to read src/regex_generated.rs");

    if expected != actual {
        panic!(
            "src/regex_generated.rs is out of date!\n\
             Regenerate with: ./bootstrap.sh"
        );
    }
}
