//! Verify that `src/meta_generated.rs` matches what codegen currently emits.

use std::process::Command;

#[test]
fn meta_generated_matches_codegen_output() {
    // Build and run: cargo run -- --rust grammars/meta.gzl
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--features",
            "codegen",
            "--",
            "--rust",
            "grammars/meta.gzl",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run gazelle CLI");

    assert!(
        output.status.success(),
        "gazelle --rust grammars/meta.gzl failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = String::from_utf8(output.stdout).expect("non-UTF-8 output");
    let actual = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/meta_generated.rs"),
    )
    .expect("failed to read src/meta_generated.rs");

    if expected != actual {
        panic!(
            "src/meta_generated.rs is out of date!\n\
             Regenerate with: ./bootstrap.sh"
        );
    }
}
