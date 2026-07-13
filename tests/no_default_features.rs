//! Compile an ordinary external macro consumer without Gazelle's default features.

use std::path::Path;
use std::process::Command;

#[test]
fn generated_parser_compiles_without_default_features() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = root.join("tests/fixtures/no_render_codegen/Cargo.toml");
    let target = root.join("target/no-default-features-test");

    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--locked", "--manifest-path"])
        .arg(manifest)
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("run Cargo for no-default-features fixture");

    assert!(
        output.status.success(),
        "generated parser failed without default features:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
