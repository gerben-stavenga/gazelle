# Migrating from Gazelle 0.9 to 0.10

Gazelle 0.10 separates grammar-level diagnostics from the built-in default
English renderer. This is a source-breaking release.

## Dependency versions

Update both crates together:

```toml
[dependencies]
gazelle-parser = "0.10"
gazelle-macros = "0.10"
```

## Structured errors

`GrammarDiagnostic::message` and `RegexError::message` are no longer public
string fields. Inspect their structured `cause` fields when presentation is
application-owned:

```rust
match &error.cause {
    GrammarCause::Message(message) => report_text(message),
    GrammarCause::Syntax(diagnostic) => report_syntax(diagnostic),
}
```

Call `error.message()` to obtain Gazelle's English presentation. `Display` and
`std::error::Error` are also available. The renderer adds no dependency and is
always compiled; applications that own presentation can leave it unused so an
optimizing linker can discard it.

## Recovery diagnostics

Generated module-level `diagnose_error` and `format_error` helpers were
removed. Match the returned error and use its recovery state directly:

```rust
match &error {
    gazelle::ParseError::Syntax { terminal, recovery } => {
        let diagnostic = recovery.diagnose(*terminal);
        let message = recovery.format_error(*terminal, None, None);
    }
    gazelle::ParseError::Action { error, .. } => handle_action_error(error),
}
```

The recovery methods no longer take a separate `ErrorContext`; that metadata is
captured when the generated semantic parser enters recovery.
