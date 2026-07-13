//! Default English rendering of a [`SyntaxDiagnostic`].
//!
//! This is a leaf module: it depends on the diagnostic data and the grammar
//! names in [`ErrorContext`], never on the parser. Everything that turns
//! grammar symbols into prose lives here. Embedders can use the structured
//! diagnostic API instead, allowing an optimizing linker to discard this
//! presentation code when it is unused.

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::{format, vec::Vec};

use crate::diagnostic::SyntaxDiagnostic;
use crate::grammar::SymbolId;
use crate::runtime::{CstParser, ErrorContext, Parser, RecoveryParser};

/// `format_error` convenience wrappers.
///
/// These live here, not in `runtime`, so the dependency points *into* the
/// presentation layer: the engine produces a [`SyntaxDiagnostic`] and knows
/// nothing about strings.
macro_rules! format_error_for {
    ($($ty:ident),*) => {$(
        impl<'a> $ty<'a> {
            /// Format a syntax error using Gazelle's default English presentation.
            ///
            /// Shorthand for `self.diagnose(terminal, ctx).render(ctx, display_names, tokens)`.
            pub fn format_error(
                &self,
                terminal: SymbolId,
                ctx: &(impl ErrorContext + ?Sized),
                display_names: Option<&[(&str, &str)]>,
                tokens: Option<&[&str]>,
            ) -> String {
                self.diagnose(terminal, ctx).render(ctx, display_names, tokens)
            }
        }
    )*};
}

format_error_for!(Parser, CstParser);

impl RecoveryParser<'_> {
    /// Format a syntax error using the grammar metadata carried by recovery.
    pub fn format_error(
        &self,
        terminal: SymbolId,
        display_names: Option<&[(&str, &str)]>,
        tokens: Option<&[&str]>,
    ) -> String {
        self.diagnose(terminal)
            .render(self.error_context(), display_names, tokens)
    }
}

/// Convert `__foo_star` → `foo*`, `__foo_plus` → `foo+`, `__foo_opt` → `foo?`,
/// `__item_sep_comma` → `item % comma`.
fn format_sym(s: &str) -> String {
    if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_star")) {
        format!("{}*", base)
    } else if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_plus")) {
        format!("{}+", base)
    } else if let Some(base) = s.strip_prefix("__").and_then(|s| s.strip_suffix("_opt")) {
        format!("{}?", base)
    } else if let Some(rest) = s.strip_prefix("__") {
        if let Some(idx) = rest.find("_sep_") {
            let base = &rest[..idx];
            let sep = &rest[idx + 5..];
            return format!("{} % {}", base, sep);
        }
        s.to_string()
    } else {
        s.to_string()
    }
}

impl SyntaxDiagnostic {
    /// Render this diagnostic as a detailed English message.
    ///
    /// - `display_names`: optional map from grammar names to user-friendly names
    ///   (e.g., `"SEMI"` → `"';'"`).
    /// - `tokens`: optional token texts by index (must include the error token at
    ///   index [`position`](Self::position)).
    ///
    /// # Adding source locations
    ///
    /// The parser is push-based, so the lexer knows the source position when
    /// each token is pushed. Capture the location before pushing and use it
    /// when rendering the error:
    ///
    /// ```rust,ignore
    /// loop {
    ///     let (line, col) = src.line_col();
    ///     let tok = lex_one_token(&mut src);
    ///     parser = match parser.push(tok, &mut actions) {
    ///         Ok(parser) => parser,
    ///         Err(error) => {
    ///             let msg = match &error {
    ///                 gazelle::ParseError::Syntax { terminal, recovery } =>
    ///                     recovery.format_error(*terminal, None, None),
    ///                 gazelle::ParseError::Action { .. } => "semantic action failed".into(),
    ///             };
    ///             return Err(format!("{line}:{col}: {msg}"));
    ///         }
    ///     };
    /// }
    /// ```
    pub fn render(
        &self,
        ctx: &(impl ErrorContext + ?Sized),
        display_names: Option<&[(&str, &str)]>,
        tokens: Option<&[&str]>,
    ) -> String {
        let display_names = display_names.unwrap_or(&[]);
        let tokens = tokens.unwrap_or(&[]);
        let error_token_idx = self.position;

        let display = |id: SymbolId| -> &str {
            let name = ctx.symbol_name(id);
            display_names
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| *v)
                .unwrap_or(name)
        };

        // Convert to display names
        let mut expected: Vec<_> = self
            .expected
            .iter()
            .map(|&sym| format_sym(display(sym)))
            .collect();
        expected.sort();
        expected.dedup();

        // Show actual token text if available, otherwise display name
        let found_name = tokens
            .get(error_token_idx)
            .copied()
            .unwrap_or_else(|| display(self.unexpected));

        let mut msg = format!("unexpected '{}'", found_name);
        if !expected.is_empty() {
            msg.push_str(&format!(", expected: {}", expected.join(", ")));
        }

        // Show parsed stack with token spans
        if !tokens.is_empty() && error_token_idx <= tokens.len() {
            if !self.stack.is_empty() {
                // Build two lines: tokens and underlines with names
                let mut token_line = String::new();
                let mut label_line = String::new();

                for entry in self.stack.iter().rev().take(4).rev() {
                    let start = entry.start;
                    let end = entry.end;
                    let name = format_sym(display(entry.symbol));

                    // Get token text for this span
                    let span_text = if end - start == 1 {
                        tokens[start].to_string()
                    } else if end - start <= 3 {
                        tokens[start..end].join(" ")
                    } else {
                        format!("{} ... {}", tokens[start], tokens[end - 1])
                    };

                    let width = span_text.chars().count().max(name.len());

                    if !token_line.is_empty() {
                        token_line.push_str("  ");
                        label_line.push_str("  ");
                    }
                    token_line.push_str(&format!("{:^width$}", span_text, width = width));
                    label_line.push_str(&format!("{:^width$}", name, width = width));
                }

                msg.push_str(&format!("\n  {}\n  {}", token_line, label_line));
            }
        } else if !self.stack.is_empty() {
            // Fallback: show grammar symbols from stack
            let path: Vec<_> = self
                .stack
                .iter()
                .map(|entry| display(entry.symbol))
                .collect();
            msg.push_str(&format!("\n  after: {}", path.join(" ")));
        }

        // Show informative items that explain what's expected
        // Prefer a short explanation over dumping every active item. In large
        // grammars, a single error state can contain many sibling alternatives
        // (postfix operators are a common example). Keep the most progressed
        // item per nonterminal, then show at most four surrounding contexts.
        let mut display_items = self.contexts.clone();
        display_items.sort_by(|a, b| {
            b.dot
                .cmp(&a.dot)
                .then_with(|| b.rhs.len().cmp(&a.rhs.len()))
                .then_with(|| a.rule.cmp(&b.rule))
        });
        let mut seen_lhs = BTreeSet::new();
        display_items.retain(|item| seen_lhs.insert(format_sym(display(item.lhs))));
        display_items.truncate(4);
        let mut seen = BTreeSet::new();

        for item in &display_items {
            if ctx.symbol_name(item.lhs) == "__start" {
                continue;
            }
            let lhs_name = format_sym(display(item.lhs));

            let before: Vec<_> = item.rhs[..item.dot]
                .iter()
                .map(|&id| format_sym(display(id)))
                .collect();
            let after: Vec<_> = item.rhs[item.dot..]
                .iter()
                .map(|&id| format_sym(display(id)))
                .collect();
            let line = format!(
                "\n  in {}: {} \u{2022} {}",
                lhs_name,
                before.join(" "),
                after.join(" ")
            );
            if seen.insert(line.clone()) {
                msg.push_str(&line);
            }
        }
        msg
    }
}
