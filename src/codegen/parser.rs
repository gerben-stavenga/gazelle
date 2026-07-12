//! Parser struct and trait code generation.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::CodegenContext;
use super::reduction::{self, ReductionInfo, SymbolKind, typed_symbol_indices};
use super::table::CodegenTableInfo;
use crate::lr::AltAction;

/// Generate the parser wrapper, trait, and related types.
pub fn generate(ctx: &CodegenContext, info: &CodegenTableInfo) -> Result<TokenStream, String> {
    let vis: TokenStream = "pub".parse().unwrap();
    let terminal_enum = format_ident!("Terminal");
    let types_trait = format_ident!("Types");
    let parser_struct = format_ident!("Parser");
    let value_union = format_ident!("__Value");
    let table_mod = format_ident!("__table");
    let gazelle_crate_path = ctx.gazelle_crate_path_tokens();

    // Analyze reductions
    let reductions = reduction::analyze_reductions(ctx)?;

    // Collect non-terminals with types (excluding synthetic rules)
    let typed_non_terminals: Vec<_> = ctx
        .grammar
        .symbols
        .non_terminal_ids()
        .filter_map(|id| {
            let ty = ctx.grammar.types.get(&id)?.as_ref()?;
            let name = ctx.grammar.symbols.name(id);
            if name.starts_with("__") {
                return None;
            }
            Some((name.to_string(), ty.clone()))
        })
        .collect();

    // All typed non-terminals including synthetic (for value union)
    let all_typed_non_terminals: Vec<_> = ctx
        .grammar
        .symbols
        .non_terminal_ids()
        .filter_map(|id| {
            let ty = ctx.grammar.types.get(&id)?.as_ref()?;
            Some((ctx.grammar.symbols.name(id).to_string(), ty.clone()))
        })
        .collect();

    // Get start non-terminal name and its type annotation
    let start_nt = &ctx.start_symbol;
    let start_type_annotation = typed_non_terminals
        .iter()
        .find(|(name, _)| name == start_nt)
        .map(|(_, ty)| ty.clone());
    let start_field = format_ident!("__{}", start_nt.to_lowercase());

    // Generate components
    let enum_code =
        generate_nonterminal_enums(ctx, &reductions, &typed_non_terminals, &types_trait, &vis);
    let (traits_code, reducer_bounds) = generate_traits(
        ctx,
        &types_trait,
        &typed_non_terminals,
        &reductions,
        &vis,
        &gazelle_crate_path,
    );
    let value_union_code =
        generate_value_union(ctx, &all_typed_non_terminals, &value_union, &types_trait);
    let shift_arms = generate_terminal_shift_arms(ctx, &terminal_enum, &value_union);
    let reduction_arms =
        generate_reduction_arms(ctx, &reductions, &value_union, &typed_non_terminals);
    let drop_arms = generate_drop_arms(ctx, info);

    // Generate finish method based on whether start symbol has a type
    let finish_method = if let Some(start_type) = start_type_annotation {
        let start_type_ident = format_ident!("{}", start_type);
        quote! {
            pub fn finish(mut self, actions: &mut A) -> Result<A::#start_type_ident, #gazelle_crate_path::ParseError<A::Error, #gazelle_crate_path::RecoveryParser<'static>>> {
                loop {
                    match self.parser.maybe_reduce(None) {
                        Ok(Some((0, _, _))) => {
                            let union_val = self.value_stack.pop().unwrap();
                            return Ok(unsafe { core::mem::ManuallyDrop::into_inner(union_val.#start_field) });
                        }
                        Ok(Some((rule, _, start_idx))) => {
                            if let Err(e) = self.do_reduce(rule, start_idx, actions) {
                                let recovery = self.into_recovery();
                                return Err(#gazelle_crate_path::ParseError::Action { error: e, recovery });
                            }
                        }
                        Ok(None) => unreachable!(),
                        Err(e) => {
                            self.drain_values();
                            self.parser.restore_checkpoint();
                            let #gazelle_crate_path::ParseError::Syntax { terminal, .. } = e;
                            let recovery = self.into_recovery();
                            return Err(#gazelle_crate_path::ParseError::Syntax { terminal, recovery });
                        }
                    }
                }
            }
        }
    } else {
        quote! {
            pub fn finish(mut self, actions: &mut A) -> Result<(), #gazelle_crate_path::ParseError<A::Error, #gazelle_crate_path::RecoveryParser<'static>>> {
                loop {
                    match self.parser.maybe_reduce(None) {
                        Ok(Some((0, _, _))) => {
                            self.value_stack.pop();
                            return Ok(());
                        }
                        Ok(Some((rule, _, start_idx))) => {
                            if let Err(e) = self.do_reduce(rule, start_idx, actions) {
                                let recovery = self.into_recovery();
                                return Err(#gazelle_crate_path::ParseError::Action { error: e, recovery });
                            }
                        }
                        Ok(None) => unreachable!(),
                        Err(e) => {
                            self.drain_values();
                            self.parser.restore_checkpoint();
                            let #gazelle_crate_path::ParseError::Syntax { terminal, .. } = e;
                            let recovery = self.into_recovery();
                            return Err(#gazelle_crate_path::ParseError::Syntax { terminal, recovery });
                        }
                    }
                }
            }
        }
    };

    // Repetition node, generated *locally* so a custom fold
    // (`Action<SeqNode<MyContainer, E>>`) stays coherent with the default `Vec`
    // build — a shared library type would make the custom impl conflict across
    // crates. The default `Vec` build is a closed, orphan-legal impl; any other
    // container is reached by the user's own `Action<SeqNode<..>>`.
    let has_lists = reductions
        .iter()
        .any(|info| matches!(info.action, AltAction::VecAppend));
    let seq_node_code = if has_lists {
        quote! {
            #[doc(hidden)]
            #vis enum SeqNode<S, E> {
                Empty,
                Append(S, E),
            }
            impl<S, E> #gazelle_crate_path::AstNode for SeqNode<S, E> {
                type Output = S;
            }
            impl<E> #gazelle_crate_path::FromAstNode<SeqNode<Vec<E>, E>> for Vec<E> {
                fn from(node: SeqNode<Vec<E>, E>) -> Vec<E> {
                    match node {
                        SeqNode::Empty => Vec::new(),
                        SeqNode::Append(mut acc, elem) => {
                            acc.push(elem);
                            acc
                        }
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #enum_code

        #seq_node_code

        #traits_code

        #value_union_code

        /// Return the grammar name for a symbol ID.
        #vis fn symbol_name(id: #gazelle_crate_path::SymbolId) -> &'static str {
            #table_mod::ERROR_INFO
                .symbol_names
                .get(id.index())
                .copied()
                .unwrap_or("<?>")
        }

        /// Extract a structured, grammar-level syntax diagnosis.
        #vis fn diagnose_error<E>(
            error: &#gazelle_crate_path::ParseError<E, #gazelle_crate_path::RecoveryParser<'static>>,
        ) -> Option<#gazelle_crate_path::SyntaxDiagnostic> {
            match error {
                #gazelle_crate_path::ParseError::Syntax { terminal, recovery } => {
                    Some(recovery.diagnose(*terminal, &#table_mod::ERROR_INFO))
                }
                #gazelle_crate_path::ParseError::Action { .. } => None,
            }
        }

        /// Format a syntax error using Gazelle's default English presentation.
        /// Returns `None` for user action errors.
        #vis fn format_error<E>(
            error: &#gazelle_crate_path::ParseError<E, #gazelle_crate_path::RecoveryParser<'static>>,
            display_names: Option<&[(&str, &str)]>,
            tokens: Option<&[&str]>,
        ) -> Option<String> {
            match error {
                #gazelle_crate_path::ParseError::Syntax { terminal, recovery } => {
                    Some(recovery.format_error(
                        *terminal,
                        &#table_mod::ERROR_INFO,
                        display_names,
                        tokens,
                    ))
                }
                #gazelle_crate_path::ParseError::Action { .. } => None,
            }
        }

        /// Type-safe LR parser.
        #vis struct #parser_struct<A: #types_trait> {
            parser: #gazelle_crate_path::Parser<'static>,
            value_stack: Vec<#value_union<A>>,
        }

        impl<A: #types_trait> #parser_struct<A> {
            /// Create a new parser instance.
            pub fn new() -> Self {
                Self {
                    parser: #gazelle_crate_path::Parser::new(#table_mod::TABLE),
                    value_stack: Vec::new(),
                }
            }

            /// Get the current parser state.
            pub fn state(&self) -> usize {
                self.parser.state()
            }

            /// Format a parse error into a detailed message.
            pub fn format_error(
                &self,
                terminal: #gazelle_crate_path::SymbolId,
                display_names: Option<&[(&str, &str)]>,
                tokens: Option<&[&str]>,
            ) -> String {
                self.parser.format_error(terminal, &#table_mod::ERROR_INFO, display_names, tokens)
            }

            /// Get the error info for custom error formatting.
            pub fn error_info() -> &'static #gazelle_crate_path::ErrorInfo<'static> {
                &#table_mod::ERROR_INFO
            }

            /// Abandon semantic parsing and enter syntax-only recovery.
            pub fn into_recovery(mut self) -> #gazelle_crate_path::RecoveryParser<'static> {
                self.drain_values();
                let parser = core::mem::replace(
                    &mut self.parser,
                    #gazelle_crate_path::Parser::new(#table_mod::TABLE),
                );
                parser.into_recovery()
            }

            /// Consume this semantic parser and search for syntax repairs.
            ///
            /// The returned recovery parser has no semantic `push` or `finish`
            /// methods, so a discarded semantic value stack cannot be reused.
            pub fn recover(
                self,
                buffer: &[#gazelle_crate_path::Token],
            ) -> (
                #gazelle_crate_path::RecoveryParser<'static>,
                Vec<#gazelle_crate_path::RecoveryInfo>,
            ) {
                let mut recovery = self.into_recovery();
                let errors = recovery.recover(buffer);
                (recovery, errors)
            }

            fn drain_values(&mut self) {
                for i in (0..self.value_stack.len()).rev() {
                    let union_val = self.value_stack.pop().unwrap();
                    let sym_id = #table_mod::STATE_SYMBOL[self.parser.state_at(i)];
                    unsafe {
                        match sym_id {
                            #(#drop_arms)*
                            _ => {}
                        }
                    }
                }
            }
        }

        #[allow(clippy::result_large_err)]
        impl<A: #types_trait #(#reducer_bounds)*> #parser_struct<A> {
            /// Push a terminal, performing any reductions.
            pub fn push(
                mut self,
                terminal: #terminal_enum<A>,
                actions: &mut A,
            ) -> Result<Self, #gazelle_crate_path::ParseError<A::Error, #gazelle_crate_path::RecoveryParser<'static>>> {
                let token = #gazelle_crate_path::Token {
                    terminal: terminal.symbol_id(),
                    resolution: terminal.resolution(),
                };

                loop {
                    match self.parser.maybe_reduce(Some(token)) {
                        Ok(Some((rule, _, start_idx))) => {
                            if let Err(e) = self.do_reduce(rule, start_idx, actions) {
                                let recovery = self.into_recovery();
                                return Err(#gazelle_crate_path::ParseError::Action { error: e, recovery });
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            self.drain_values();
                            self.parser.restore_checkpoint();
                            let #gazelle_crate_path::ParseError::Syntax { terminal, .. } = e;
                            let recovery = self.into_recovery();
                            return Err(#gazelle_crate_path::ParseError::Syntax { terminal, recovery });
                        }
                    }
                }

                // Shift the terminal
                self.parser.shift(token);

                match terminal {
                    #(#shift_arms)*
                }

                Ok(self)
            }

            /// Finish parsing and return the result.
            #finish_method

            fn do_reduce(&mut self, rule: usize, start_idx: usize, actions: &mut A) -> Result<(), A::Error> {
                if rule == 0 { return Ok(()); }

                actions.set_token_range(start_idx, self.parser.token_count());
                let original_rule_idx = rule - 1;

                let value = match original_rule_idx {
                    #(#reduction_arms)*
                    _ => return Ok(()),
                };

                self.value_stack.push(value);
                Ok(())
            }
        }

        impl<A: #types_trait> Default for #parser_struct<A> {
            fn default() -> Self { Self::new() }
        }

        impl<A: #types_trait> Drop for #parser_struct<A> {
            fn drop(&mut self) {
                self.drain_values();
            }
        }
    })
}

/// Generate per-nonterminal enums for typed non-terminals.
///
/// All enums are generic over `A: Types`. Enums whose fields don't reference A
/// get a `#[doc(hidden)] _Phantom(Infallible, PhantomData<A>)` variant to
/// satisfy Rust's unused type parameter check. Since `Infallible` is uninhabited,
/// `min_exhaustive_patterns` (stable since 1.82) means the variant never needs
/// to be matched.
fn generate_nonterminal_enums(
    ctx: &CodegenContext,
    reductions: &[ReductionInfo],
    typed_non_terminals: &[(String, String)],
    types_trait: &syn::Ident,
    vis: &TokenStream,
) -> TokenStream {
    let mut enums = Vec::new();

    // Map from terminal name to associated type name
    let terminal_assoc_types: alloc::collections::BTreeMap<&str, &str> = ctx
        .grammar
        .symbols
        .terminal_ids()
        .skip(1)
        .filter_map(|id| {
            let type_name = ctx.grammar.types.get(&id)?.as_ref()?;
            Some((ctx.grammar.symbols.name(id), type_name.as_str()))
        })
        .collect();

    // Map from non-terminal name to result type
    let nt_result_types: alloc::collections::BTreeMap<&str, &str> = typed_non_terminals
        .iter()
        .map(|(name, result_type)| (name.as_str(), result_type.as_str()))
        .collect();

    // Group reductions by non-terminal, only for typed non-synthetic NTs with variants
    let mut nt_variants: alloc::collections::BTreeMap<&str, Vec<&ReductionInfo>> =
        alloc::collections::BTreeMap::new();
    for info in reductions {
        if info.variant_name.is_some() {
            nt_variants
                .entry(&info.non_terminal)
                .or_default()
                .push(info);
        }
    }

    for (nt_name, variants) in &nt_variants {
        let enum_ident = enum_name(nt_name);

        let variant_defs: Vec<_> = variants
            .iter()
            .map(|info| {
                let variant_name = format_ident!(
                    "{}",
                    crate::lr::to_camel_case(info.variant_name.as_ref().unwrap())
                );
                let fields: Vec<_> = typed_symbol_indices(&info.rhs_symbols)
                    .iter()
                    .map(|&idx| {
                        let sym = &info.rhs_symbols[idx];
                        symbol_to_field_type(sym, &nt_result_types, &terminal_assoc_types, ctx)
                    })
                    .collect();

                if fields.is_empty() {
                    quote! { #variant_name }
                } else {
                    quote! { #variant_name(#(#fields),*) }
                }
            })
            .collect();

        // Check if any variant field type actually references the type parameter A.
        let uses_a = variants.iter().any(|info| {
            typed_symbol_indices(&info.rhs_symbols).iter().any(|&idx| {
                let sym = &info.rhs_symbols[idx];
                symbol_references_a(sym, &nt_result_types, &terminal_assoc_types, ctx)
            })
        });

        // All enums get <A>. If A isn't used in fields, add uninhabited phantom variant.
        let (phantom_variant, phantom_arm) = if !uses_a {
            (
                quote! {
                    , #[doc(hidden)] _Phantom(core::convert::Infallible, core::marker::PhantomData<A>)
                },
                quote! { _ => unreachable!(), },
            )
        } else {
            (quote! {}, quote! {})
        };

        // Generate optional derive impls and derive attributes.
        let derive_impls =
            generate_enum_derive_impls(ctx, types_trait, &enum_ident, variants, &phantom_arm);
        let serde_derives = generate_serde_derives(ctx);

        enums.push(quote! {
            #serde_derives
            #vis enum #enum_ident<A: #types_trait> {
                #(#variant_defs),*
                #phantom_variant
            }

            #(#derive_impls)*
        });
    }

    quote! { #(#enums)* }
}

/// The element type of a sequence symbol, qualified by `prefix` (`Self` for a
/// trait default, `A` for a reducer bound). Untyped symbols contribute `()`.
fn element_assoc_type(
    sym: &reduction::SymbolInfo,
    nt_result_types: &alloc::collections::BTreeMap<&str, &str>,
    terminal_assoc_types: &alloc::collections::BTreeMap<&str, &str>,
    prefix: &TokenStream,
) -> TokenStream {
    let assoc = if sym.kind == SymbolKind::NonTerminal {
        nt_result_types.get(sym.name.as_str())
    } else {
        terminal_assoc_types.get(sym.name.as_str())
    };
    match assoc {
        Some(name) => {
            let ident = format_ident!("{}", name);
            quote! { #prefix::#ident }
        }
        None => quote! { () },
    }
}

/// Convert a symbol to its field type tokens for use in an enum variant.
fn symbol_to_field_type(
    sym: &reduction::SymbolInfo,
    nt_result_types: &alloc::collections::BTreeMap<&str, &str>,
    terminal_assoc_types: &alloc::collections::BTreeMap<&str, &str>,
    ctx: &CodegenContext,
) -> TokenStream {
    if sym.kind == SymbolKind::NonTerminal {
        if let Some(&result_type) = nt_result_types.get(sym.name.as_str()) {
            let assoc = format_ident!("{}", result_type);
            quote! { A::#assoc }
        } else if sym.name.starts_with("__") {
            if let Some(result_type) = ctx.get_type(&sym.name) {
                synthetic_type_to_tokens_with_prefix(result_type, false)
            } else {
                quote! { () }
            }
        } else {
            quote! { () }
        }
    } else if let Some(assoc_name) = terminal_assoc_types.get(sym.name.as_str()) {
        let assoc = format_ident!("{}", assoc_name);
        quote! { A::#assoc }
    } else {
        quote! { () }
    }
}

/// Check if a symbol's field type would reference the generic parameter A.
fn symbol_references_a(
    sym: &reduction::SymbolInfo,
    nt_result_types: &alloc::collections::BTreeMap<&str, &str>,
    terminal_assoc_types: &alloc::collections::BTreeMap<&str, &str>,
    ctx: &CodegenContext,
) -> bool {
    if sym.kind == SymbolKind::NonTerminal {
        if nt_result_types.contains_key(sym.name.as_str()) {
            // Non-synthetic typed NT -> A::ResultType
            true
        } else if sym.name.starts_with("__") {
            if let Some(result_type) = ctx.get_type(&sym.name) {
                // Synthetic types like Vec<Foo> reference A if Foo is not "()"
                let inner = result_type
                    .strip_prefix("Option<")
                    .and_then(|s| s.strip_suffix('>'))
                    .or_else(|| {
                        result_type
                            .strip_prefix("Vec<")
                            .and_then(|s| s.strip_suffix('>'))
                    });
                match inner {
                    Some("()") => false,
                    Some(_) => true,
                    None => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        terminal_assoc_types.contains_key(sym.name.as_str())
    }
}

fn generate_traits(
    ctx: &CodegenContext,
    types_trait: &syn::Ident,
    typed_non_terminals: &[(String, String)],
    reductions: &[ReductionInfo],
    vis: &TokenStream,
    gazelle_crate_path: &TokenStream,
) -> (TokenStream, Vec<TokenStream>) {
    let mut assoc_types = Vec::new();
    let mut seen_types = alloc::collections::BTreeSet::new();

    // Build bounds for associated types based on derives
    let bounds = derive_bounds(ctx);

    // When `#[ast_defaults]` is set, map each non-terminal's result type to its
    // generated AST enum so we can emit `type Foo = Foo<Self>;` defaults. Only
    // non-terminals that actually produce an enum (have `=> name` variants) get
    // a default; directly-recursive ones still need a manual `Box<...>` override.
    let mut result_to_enum = alloc::collections::BTreeMap::new();
    if ctx.ast_defaults {
        let mut seen = alloc::collections::BTreeSet::new();
        for info in reductions {
            if info.variant_name.is_some() && seen.insert(&info.non_terminal) {
                if let Some((_, result_type)) = typed_non_terminals
                    .iter()
                    .find(|(n, _)| n == &info.non_terminal)
                {
                    result_to_enum.insert(result_type.as_str(), enum_name(&info.non_terminal));
                }
            }
        }
    }

    // Maps for resolving a named sequence's element type — used both for the
    // `FromAstSeq` where-bound and the `#[ast_defaults]` `Vec` default. A named
    // list (`* as Args`) is a typed NT with a `VecAppend` reduction; the element
    // is that reduction's last symbol. (Anonymous `__`-lists aren't typed NTs.)
    let nt_result_types: alloc::collections::BTreeMap<&str, &str> = typed_non_terminals
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_str()))
        .collect();
    let terminal_assoc_types: alloc::collections::BTreeMap<&str, &str> = ctx
        .grammar
        .symbols
        .terminal_ids()
        .skip(1)
        .filter_map(|id| {
            let t = ctx.grammar.types.get(&id)?.as_ref()?;
            Some((ctx.grammar.symbols.name(id), t.as_str()))
        })
        .collect();
    let mut reducer_bounds = Vec::new();
    let mut result_to_vec_elem = alloc::collections::BTreeMap::new();
    for info in reductions {
        if matches!(&info.action, AltAction::VecAppend)
            && let Some((_, result_type)) = typed_non_terminals
                .iter()
                .find(|(n, _)| n == &info.non_terminal)
            && let Some(elem) = info.rhs_symbols.last()
            && !result_to_vec_elem.contains_key(result_type.as_str())
        {
            // A named list dispatches through the same `Action` as every node —
            // `A: Action<SeqNode<A::Args, A::Elem>>` — satisfied by the
            // `FromAstNode` blanket for a `Default + Extend` container, or by a
            // custom `Action` (arena) otherwise. No trait where-bound needed.
            let result_ident = format_ident!("{}", result_type);
            let elem_a =
                element_assoc_type(elem, &nt_result_types, &terminal_assoc_types, &quote! { A });
            reducer_bounds.push(quote! {
                + #gazelle_crate_path::Action<SeqNode<A::#result_ident, #elem_a>>
            });
            result_to_vec_elem.insert(
                result_type.as_str(),
                element_assoc_type(
                    elem,
                    &nt_result_types,
                    &terminal_assoc_types,
                    &quote! { Self },
                ),
            );
        }
    }

    // Terminal associated types - use payload type name directly
    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        if let Some(type_name) = ctx.grammar.types.get(&id).and_then(|t| t.as_ref())
            && seen_types.insert(type_name.as_str())
        {
            let type_ident = format_ident!("{}", type_name);
            assoc_types.push(quote! { type #type_ident #bounds; });
        }
    }

    // Non-terminal associated types (deduplicated by result_type)
    for (_, result_type) in typed_non_terminals {
        if seen_types.insert(result_type.as_str()) {
            let type_name = format_ident!("{}", result_type);
            if let Some(enum_ident) = result_to_enum.get(result_type.as_str()) {
                assoc_types.push(quote! { type #type_name #bounds = #enum_ident<Self>; });
            } else if ctx.ast_defaults
                && let Some(elem) = result_to_vec_elem.get(result_type.as_str())
            {
                // Named sequence: default the knob to `Vec<Elem>` (nightly).
                assoc_types.push(quote! { type #type_name #bounds = Vec<#elem>; });
            } else {
                assoc_types.push(quote! { type #type_name #bounds; });
            }
        }
    }

    // Collect AstNode impls and Action bounds for non-terminals with enum variants
    let mut ast_node_impls = Vec::new();
    let mut seen_nt = alloc::collections::BTreeSet::new();
    for info in reductions {
        if info.variant_name.is_some() && seen_nt.insert(&info.non_terminal) {
            let enum_ident = enum_name(&info.non_terminal);

            // All enums are now Foo<A>
            if let Some((_, result_type)) = typed_non_terminals
                .iter()
                .find(|(n, _)| n == &info.non_terminal)
            {
                let result_ident = format_ident!("{}", result_type);
                ast_node_impls.push(quote! {
                    impl<A: #types_trait> #gazelle_crate_path::AstNode for #enum_ident<A> {
                        type Output = A::#result_ident;
                    }
                });
            } else {
                // Untyped NT with => name — side-effect enum, output is ()
                ast_node_impls.push(quote! {
                    impl<A: #types_trait> #gazelle_crate_path::AstNode for #enum_ident<A> {
                        type Output = ();
                    }
                });
            }

            reducer_bounds.push(quote! { + #gazelle_crate_path::Action<#enum_ident<A>> });
        }
    }

    (
        quote! {
            /// Associated types for parser symbols.
            #vis trait #types_trait: #gazelle_crate_path::ErrorType + Sized {
                #(#assoc_types)*

                /// Called before each reduction with the token range `[start..end)`.
                /// Override to track source spans. Default is no-op.
                #[allow(unused_variables)]
                fn set_token_range(&mut self, start: usize, end: usize) {}
            }

            #(#ast_node_impls)*
        },
        reducer_bounds,
    )
}

fn generate_value_union(
    ctx: &CodegenContext,
    typed_non_terminals: &[(String, String)],
    value_union: &syn::Ident,
    types_trait: &syn::Ident,
) -> TokenStream {
    let mut fields = Vec::new();

    // Terminals with payloads - use payload type name as associated type
    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        if let Some(type_name) = ctx.grammar.types.get(&id).and_then(|t| t.as_ref()) {
            let name = ctx.grammar.symbols.name(id);
            let field_name = format_ident!("__{}", name.to_lowercase());
            let assoc_type = format_ident!("{}", type_name);
            fields.push(quote! { #field_name: core::mem::ManuallyDrop<A::#assoc_type>, });
        }
    }

    // Typed non-terminals
    for (name, result_type) in typed_non_terminals {
        let field_name = format_ident!("__{}", name.to_lowercase());

        // Check if this is a synthetic rule
        if name.starts_with("__") {
            let field_type = synthetic_type_to_tokens_with_prefix(result_type, false);
            fields.push(quote! { #field_name: core::mem::ManuallyDrop<#field_type>, });
        } else {
            let assoc_type = format_ident!("{}", result_type);
            fields.push(quote! { #field_name: core::mem::ManuallyDrop<A::#assoc_type>, });
        }
    }

    quote! {
        #[doc(hidden)]
        union #value_union<A: #types_trait> {
            #(#fields)*
            __unit: (),
            __phantom: core::mem::ManuallyDrop<core::marker::PhantomData<A>>,
        }
    }
}

/// Convert a synthetic type like "Option<Foo>" or "Vec<Bar>" to tokens with associated type.
fn synthetic_type_to_tokens_with_prefix(type_str: &str, use_self: bool) -> TokenStream {
    if let Some(inner) = type_str
        .strip_prefix("Option<")
        .and_then(|s| s.strip_suffix('>'))
    {
        if inner == "()" {
            quote! { Option<()> }
        } else {
            let inner_ident = format_ident!("{}", inner);
            if use_self {
                quote! { Option<Self::#inner_ident> }
            } else {
                quote! { Option<A::#inner_ident> }
            }
        }
    } else if let Some(inner) = type_str
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
    {
        if inner == "()" {
            quote! { Vec<()> }
        } else {
            let inner_ident = format_ident!("{}", inner);
            if use_self {
                quote! { Vec<Self::#inner_ident> }
            } else {
                quote! { Vec<A::#inner_ident> }
            }
        }
    } else {
        let ident = format_ident!("{}", type_str);
        if use_self {
            quote! { Self::#ident }
        } else {
            quote! { A::#ident }
        }
    }
}

fn generate_terminal_shift_arms(
    ctx: &CodegenContext,
    terminal_enum: &syn::Ident,
    value_union: &syn::Ident,
) -> Vec<TokenStream> {
    let mut arms = Vec::new();

    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        let name = ctx.grammar.symbols.name(id);
        let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
        let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
        let has_extra = ctx.grammar.symbols.has_resolution_field(id);

        match (has_extra, ty.is_some()) {
            (false, true) => {
                let field_name = format_ident!("__{}", name.to_lowercase());
                arms.push(quote! {
                    #terminal_enum::#variant_name(v) => {
                        self.value_stack.push(
                            #value_union { #field_name: core::mem::ManuallyDrop::new(v) }
                        );
                    }
                });
            }
            (false, false) => {
                arms.push(quote! {
                    #terminal_enum::#variant_name => {
                        self.value_stack.push(#value_union { __unit: () });
                    }
                });
            }
            (true, true) => {
                let field_name = format_ident!("__{}", name.to_lowercase());
                arms.push(quote! {
                    #terminal_enum::#variant_name(v, _prec) => {
                        self.value_stack.push(
                            #value_union { #field_name: core::mem::ManuallyDrop::new(v) }
                        );
                    }
                });
            }
            (true, false) => {
                arms.push(quote! {
                    #terminal_enum::#variant_name(_prec) => {
                        self.value_stack.push(#value_union { __unit: () });
                    }
                });
            }
        }
    }

    // Always handle phantom variant
    arms.push(quote! {
        #terminal_enum::__Phantom(_) => unreachable!(),
    });

    arms
}

fn generate_reduction_arms(
    ctx: &CodegenContext,
    reductions: &[ReductionInfo],
    value_union: &syn::Ident,
    typed_non_terminals: &[(String, String)],
) -> Vec<TokenStream> {
    let gazelle_crate_path = ctx.gazelle_crate_path_tokens();

    // Element type per list non-terminal (the `VecAppend` rule's last symbol),
    // needed to spell `SeqNode::<_, Elem>::Empty` where there's no element value.
    let nt_result_types: alloc::collections::BTreeMap<&str, &str> = typed_non_terminals
        .iter()
        .map(|(n, r)| (n.as_str(), r.as_str()))
        .collect();
    let terminal_assoc_types: alloc::collections::BTreeMap<&str, &str> = ctx
        .grammar
        .symbols
        .terminal_ids()
        .skip(1)
        .filter_map(|id| {
            let t = ctx.grammar.types.get(&id)?.as_ref()?;
            Some((ctx.grammar.symbols.name(id), t.as_str()))
        })
        .collect();
    // Per list non-terminal: its container/output type `S` and element type `E`,
    // both `A`-prefixed. A list reduction dispatches through the same `Action` as
    // every node, but `Action::build`'s `N` can't be inferred from the argument
    // when `A` already carries an unrelated explicit `Action<..>` bound (Rust
    // fixes `N` to that bound). So the call is fully qualified:
    // `<A as Action<SeqNode<S, E>>>::build(..)`.
    let mut list_nt_se: alloc::collections::BTreeMap<&str, (TokenStream, TokenStream)> =
        alloc::collections::BTreeMap::new();
    for info in reductions {
        if matches!(&info.action, AltAction::VecAppend)
            && let Some(elem) = info.rhs_symbols.last()
            && !list_nt_se.contains_key(info.non_terminal.as_str())
        {
            let e_ty = symbol_to_field_type(elem, &nt_result_types, &terminal_assoc_types, ctx);
            let s_ty = match typed_non_terminals
                .iter()
                .find(|(n, _)| n == &info.non_terminal)
            {
                Some((_, result)) => {
                    let r = format_ident!("{}", result);
                    quote! { A::#r }
                }
                None => quote! { Vec<#e_ty> },
            };
            list_nt_se.insert(info.non_terminal.as_str(), (s_ty, e_ty));
        }
    }

    let mut arms = Vec::new();

    for (idx, info) in reductions.iter().enumerate() {
        let lhs_field = format_ident!("__{}", info.non_terminal.to_lowercase());
        let idx_lit = idx;

        // Build the pop and extract statements
        let mut stmts = Vec::new();

        for (i, sym) in info.rhs_symbols.iter().enumerate().rev() {
            let pop_expr = quote! { self.value_stack.pop().unwrap() };

            if sym.ty.is_some() {
                let field_name = match sym.kind {
                    SymbolKind::UnitTerminal => {
                        stmts.push(quote! { let _ = #pop_expr; });
                        continue;
                    }
                    SymbolKind::PayloadTerminal | SymbolKind::PrecTerminal => {
                        format_ident!("__{}", sym.name.to_lowercase())
                    }
                    SymbolKind::NonTerminal => {
                        format_ident!("__{}", sym.name.to_lowercase())
                    }
                };

                let var_name = format_ident!("v{}", i);
                let extract = quote! { core::mem::ManuallyDrop::into_inner(#pop_expr.#field_name) };

                stmts.push(quote! { let #var_name = unsafe { #extract }; });
            } else {
                stmts.push(quote! { let _ = #pop_expr; });
            }
        }

        // Check if NT has a result type
        let has_result_type = ctx
            .grammar
            .symbols
            .non_terminal_ids()
            .find(|&id| ctx.grammar.symbols.name(id) == info.non_terminal)
            .and_then(|id| ctx.grammar.types.get(&id)?.as_ref())
            .is_some();

        // Generate result based on reduction kind
        let result = if let Some(variant_name) = &info.variant_name {
            // Non-terminal with enum variant: construct variant, call reduce
            let enum_name = enum_name(&info.non_terminal);
            let variant_ident = format_ident!("{}", crate::lr::to_camel_case(variant_name));
            let args: Vec<_> = typed_symbol_indices(&info.rhs_symbols)
                .iter()
                .map(|sym_idx| format_ident!("v{}", sym_idx))
                .collect();

            let node_expr = if args.is_empty() {
                quote! { #enum_name::#variant_ident }
            } else {
                quote! { #enum_name::#variant_ident(#(#args),*) }
            };

            if has_result_type {
                quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(
                    #gazelle_crate_path::Action::build(actions, #node_expr)?
                ) } }
            } else {
                // Untyped NT with => name — side-effect reduction
                quote! { {
                    #gazelle_crate_path::Action::build(actions, #node_expr)?;
                    #value_union { __unit: () }
                } }
            }
        } else {
            match &info.action {
                AltAction::Named(_) => {
                    quote! { #value_union { __unit: () } }
                }
                AltAction::OptSome => {
                    let is_unit = info
                        .rhs_symbols
                        .first()
                        .map(|s| s.ty.is_none())
                        .unwrap_or(true);
                    if is_unit {
                        quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(Some(())) } }
                    } else {
                        quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(Some(v0)) } }
                    }
                }
                AltAction::OptNone => {
                    quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(None) } }
                }
                // Sequence (`*`/`+`) building goes through `FromAstSeq` (and
                // `Default` for the empty case, which is `FromAstSeq::empty`):
                // codegen names no container, so the target type is the user's
                // choice. The default target stays `Vec`, via the blanket impl.
                AltAction::VecEmpty => {
                    let (s_ty, e_ty) = list_nt_se
                        .get(info.non_terminal.as_str())
                        .cloned()
                        .unwrap_or_else(|| (quote! { Vec<()> }, quote! { () }));
                    quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(
                        <A as #gazelle_crate_path::Action<SeqNode<#s_ty, #e_ty>>>::build(
                            actions,
                            SeqNode::Empty,
                        )?
                    ) } }
                }
                AltAction::VecSingle => {
                    let is_unit = info
                        .rhs_symbols
                        .first()
                        .map(|s| s.ty.is_none())
                        .unwrap_or(true);
                    let single_elem = if is_unit {
                        quote! { () }
                    } else {
                        quote! { v0 }
                    };
                    let (s_ty, e_ty) = list_nt_se
                        .get(info.non_terminal.as_str())
                        .cloned()
                        .unwrap_or_else(|| (quote! { Vec<()> }, quote! { () }));
                    // `let` the empty first: `build(actions, Append(build(actions,
                    // ..)?, ..))` would borrow `actions` twice in one call.
                    quote! { {
                        let __seq = <A as #gazelle_crate_path::Action<SeqNode<#s_ty, #e_ty>>>::build(
                            actions,
                            SeqNode::Empty,
                        )?;
                        #value_union { #lhs_field: core::mem::ManuallyDrop::new(
                            <A as #gazelle_crate_path::Action<SeqNode<#s_ty, #e_ty>>>::build(
                                actions,
                                SeqNode::Append(__seq, #single_elem),
                            )?
                        ) }
                    } }
                }
                AltAction::VecAppend => {
                    let last_idx = info.rhs_symbols.len() - 1;
                    let is_unit = info
                        .rhs_symbols
                        .get(last_idx)
                        .map(|s| s.ty.is_none())
                        .unwrap_or(true);
                    let elem = if is_unit {
                        quote! { () }
                    } else {
                        let elem_var = format_ident!("v{}", last_idx);
                        quote! { #elem_var }
                    };
                    let (s_ty, e_ty) = list_nt_se
                        .get(info.non_terminal.as_str())
                        .cloned()
                        .unwrap_or_else(|| (quote! { Vec<()> }, quote! { () }));
                    quote! { #value_union { #lhs_field: core::mem::ManuallyDrop::new(
                        <A as #gazelle_crate_path::Action<SeqNode<#s_ty, #e_ty>>>::build(
                            actions,
                            SeqNode::Append(v0, #elem),
                        )?
                    ) } }
                }
            }
        };

        arms.push(quote! {
            #idx_lit => {
                #(#stmts)*
                #result
            }
        });
    }

    arms
}

fn generate_drop_arms(ctx: &CodegenContext, info: &CodegenTableInfo) -> Vec<TokenStream> {
    let mut arms = Vec::new();

    // Terminals with payloads
    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        if ctx
            .grammar
            .types
            .get(&id)
            .and_then(|t| t.as_ref())
            .is_some()
        {
            let name = ctx.grammar.symbols.name(id);
            if let Some((_, table_id)) = info.terminal_ids.iter().find(|(n, _)| n == name) {
                let field_name = format_ident!("__{}", name.to_lowercase());
                arms.push(quote! {
                    #table_id => { core::mem::ManuallyDrop::into_inner(union_val.#field_name); }
                });
            }
        }
    }

    // Non-terminals
    for id in ctx.grammar.symbols.non_terminal_ids() {
        if ctx
            .grammar
            .types
            .get(&id)
            .and_then(|t| t.as_ref())
            .is_some()
        {
            let name = ctx.grammar.symbols.name(id);
            let field_name = format_ident!("__{}", name.to_lowercase());
            if let Some((_, table_id)) = info.non_terminal_ids.iter().find(|(n, _)| n == name) {
                arms.push(quote! {
                    #table_id => { core::mem::ManuallyDrop::into_inner(union_val.#field_name); }
                });
            }
        }
    }

    arms
}

fn enum_name(nt_name: &str) -> syn::Ident {
    format_ident!("{}", crate::lr::to_camel_case(nt_name))
}

/// Build the trait bound tokens for associated types from the derives set.
fn derive_bounds(ctx: &CodegenContext) -> TokenStream {
    let mut bounds: Vec<TokenStream> = Vec::new();
    if ctx.has_derive("Debug") {
        bounds.push(quote! { core::fmt::Debug });
    }
    if ctx.has_derive("Clone") {
        bounds.push(quote! { Clone });
    }
    if ctx.has_derive("PartialEq") {
        bounds.push(quote! { PartialEq });
    }
    if ctx.has_derive("Eq") {
        bounds.push(quote! { Eq });
    }
    if ctx.has_derive("Hash") {
        bounds.push(quote! { core::hash::Hash });
    }
    if ctx.has_derive("Serialize") {
        bounds.push(quote! { serde::Serialize });
    }
    if ctx.has_derive("Deserialize") {
        bounds.push(quote! { serde::de::DeserializeOwned });
    }
    if bounds.is_empty() {
        quote! {}
    } else {
        quote! { : #(#bounds)+* }
    }
}

/// Generate derive impls (Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)
/// for a nonterminal enum.
fn generate_enum_derive_impls(
    ctx: &CodegenContext,
    types_trait: &syn::Ident,
    enum_ident: &syn::Ident,
    variants: &[&ReductionInfo],
    phantom_arm: &TokenStream,
) -> Vec<TokenStream> {
    let mut impls = Vec::new();

    // Helper: build variant name + bindings for each variant
    let variant_info: Vec<_> = variants
        .iter()
        .map(|info| {
            let vname = format_ident!(
                "{}",
                crate::lr::to_camel_case(info.variant_name.as_ref().unwrap())
            );
            let field_count = typed_symbol_indices(&info.rhs_symbols).len();
            let bindings: Vec<_> = (0..field_count).map(|i| format_ident!("f{}", i)).collect();
            (vname, bindings)
        })
        .collect();

    if ctx.has_derive("Debug") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, bindings)| {
                let variant_str = vname.to_string();
                if bindings.is_empty() {
                    quote! { Self::#vname => f.write_str(#variant_str) }
                } else {
                    let fields: Vec<_> = bindings.iter().map(|b| quote! { .field(#b) }).collect();
                    quote! { Self::#vname(#(#bindings),*) => f.debug_tuple(#variant_str)#(#fields)*.finish() }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> core::fmt::Debug for #enum_ident<A> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    if ctx.has_derive("Clone") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, bindings)| {
                if bindings.is_empty() {
                    quote! { Self::#vname => Self::#vname }
                } else {
                    let clones: Vec<_> = bindings.iter().map(|b| quote! { #b.clone() }).collect();
                    quote! { Self::#vname(#(#bindings),*) => Self::#vname(#(#clones),*) }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> Clone for #enum_ident<A> {
                fn clone(&self) -> Self {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    if ctx.has_derive("PartialEq") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, bindings)| {
                if bindings.is_empty() {
                    quote! { (Self::#vname, Self::#vname) => true }
                } else {
                    let lhs: Vec<_> = bindings.iter().map(|b| format_ident!("l{}", b)).collect();
                    let rhs: Vec<_> = bindings.iter().map(|b| format_ident!("r{}", b)).collect();
                    let cmp: Vec<_> = lhs
                        .iter()
                        .zip(rhs.iter())
                        .map(|(l, r)| quote! { #l == #r })
                        .collect();
                    quote! {
                        (Self::#vname(#(#lhs),*), Self::#vname(#(#rhs),*)) => #(#cmp)&&*
                    }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> PartialEq for #enum_ident<A> {
                fn eq(&self, other: &Self) -> bool {
                    match (self, other) {
                        #(#arms,)*
                        #[allow(unreachable_patterns)]
                        _ => false,
                    }
                }
            }
        });
    }

    if ctx.has_derive("Eq") {
        impls.push(quote! {
            impl<A: #types_trait> Eq for #enum_ident<A> {}
        });
    }

    if ctx.has_derive("Hash") {
        let arms: Vec<_> = variant_info
            .iter()
            .enumerate()
            .map(|(i, (vname, bindings))| {
                let disc = i as u64;
                if bindings.is_empty() {
                    quote! { Self::#vname => { state.write_u64(#disc); } }
                } else {
                    let hashes: Vec<_> = bindings
                        .iter()
                        .map(|b| quote! { #b.hash(state); })
                        .collect();
                    quote! { Self::#vname(#(#bindings),*) => { state.write_u64(#disc); #(#hashes)* } }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> core::hash::Hash for #enum_ident<A> {
                fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    impls
}

/// Generate `#[derive(serde::Serialize)]` / `#[derive(serde::Deserialize)]` attributes
/// with `#[serde(bound = "")]` to avoid wrong bounds on the type parameter.
pub(super) fn generate_serde_derives(ctx: &CodegenContext) -> TokenStream {
    let has_ser = ctx.has_derive("Serialize");
    let has_de = ctx.has_derive("Deserialize");
    if !has_ser && !has_de {
        return quote! {};
    }

    let mut derives = Vec::new();
    let mut bounds = Vec::new();
    if has_ser {
        derives.push(quote! { serde::Serialize });
        bounds.push(quote! { serialize = "" });
    }
    if has_de {
        derives.push(quote! { serde::Deserialize });
        bounds.push(quote! { deserialize = "" });
    }
    quote! {
        #[derive(#(#derives),*)]
        #[serde(#(bound(#bounds)),*)]
    }
}
