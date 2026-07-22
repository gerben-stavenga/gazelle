//! Terminal enum code generation.

use alloc::string::ToString;
use alloc::vec::Vec;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::CodegenContext;
use super::table::CodegenTableInfo;

/// Generate the terminal enum and its implementations.
pub fn generate(ctx: &CodegenContext, info: &CodegenTableInfo) -> TokenStream {
    let vis: TokenStream = "pub".parse().unwrap();
    let terminal_enum = format_ident!("Terminal");
    let types_trait = format_ident!("Types");
    let gazelle_crate_path = ctx.gazelle_crate_path_tokens();

    // Check if we have any typed terminals
    let has_typed_terminals = ctx.grammar.symbols.terminal_ids().skip(1).any(|id| {
        ctx.grammar
            .types
            .get(&id)
            .and_then(|t| t.as_ref())
            .is_some()
    });

    // Build enum variants
    let mut variants = Vec::new();

    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        let name = ctx.grammar.symbols.name(id);
        let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
        let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
        let kind = ctx.grammar.symbols.terminal_kind(id);

        use crate::grammar::TerminalKind;
        let extra_field = match kind {
            TerminalKind::Prec => Some(quote! { #gazelle_crate_path::Precedence }),
            TerminalKind::Conflict => Some(quote! { #gazelle_crate_path::Resolution }),
            _ => None,
        };

        match (extra_field, ty) {
            (None, Some(type_name)) => {
                let assoc_type = format_ident!("{}", type_name);
                variants.push(quote! { #variant_name(A::#assoc_type) });
            }
            (None, None) => {
                variants.push(quote! { #variant_name });
            }
            (Some(field_ty), Some(type_name)) => {
                let assoc_type = format_ident!("{}", type_name);
                variants.push(quote! { #variant_name(A::#assoc_type, #field_ty) });
            }
            (Some(field_ty), None) => {
                variants.push(quote! { #variant_name(#field_ty) });
            }
        }
    }

    // Always add phantom data to use the A parameter
    variants.push(quote! {
        #[doc(hidden)]
        __Phantom(core::marker::PhantomData<A>)
    });

    // Build symbol_id match arms
    let symbol_id_arms = build_symbol_id_arms(ctx, info, &gazelle_crate_path, has_typed_terminals);

    // Build to_token match arms
    let to_token_arms = build_to_token_arms(ctx, &gazelle_crate_path, has_typed_terminals);

    // Build precedence match arms
    let precedence_arms = build_resolution_arms(ctx, &gazelle_crate_path, has_typed_terminals);

    // Collect terminal variant info for derive generation
    let terminal_ids: Vec<_> = ctx.grammar.symbols.terminal_ids().skip(1).collect();
    let variant_info: Vec<_> = terminal_ids
        .iter()
        .map(|&id| {
            let name = ctx.grammar.symbols.name(id);
            let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
            let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
            let has_extra_field = ctx.grammar.symbols.has_resolution_field(id);
            (variant_name, ty.is_some(), has_extra_field)
        })
        .collect();

    let derive_impls =
        generate_terminal_derive_impls(ctx, &types_trait, &terminal_enum, &variant_info);
    let serde_derives = super::parser::generate_serde_derives(ctx);

    quote! {
        /// Terminal symbols for the parser.
        #serde_derives
        #vis enum #terminal_enum<A: #types_trait> {
            #(#variants),*
        }

        #(#derive_impls)*

        impl<A: #types_trait> #terminal_enum<A> {
            /// Get the symbol ID for this terminal.
            pub fn symbol_id(&self) -> #gazelle_crate_path::SymbolId {
                match self {
                    #(#symbol_id_arms)*
                }
            }

            /// Convert to a gazelle Token for parsing.
            pub fn to_token(&self, symbol_ids: &impl Fn(&str) -> #gazelle_crate_path::SymbolId) -> #gazelle_crate_path::Token {
                match self {
                    #(#to_token_arms)*
                }
            }

            /// Get resolution info for runtime conflict resolution.
            pub fn resolution(&self) -> Option<#gazelle_crate_path::Resolution> {
                match self {
                    #(#precedence_arms)*
                }
            }
        }
    }
}

fn build_symbol_id_arms(
    ctx: &CodegenContext,
    info: &CodegenTableInfo,
    gazelle_crate_path: &TokenStream,
    _has_typed_terminals: bool,
) -> Vec<TokenStream> {
    let mut arms = Vec::new();

    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        let name = ctx.grammar.symbols.name(id);
        let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
        let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
        let has_extra = ctx.grammar.symbols.has_resolution_field(id);
        let table_id = info
            .terminal_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
            .unwrap_or(0);

        match (has_extra, ty.is_some()) {
            (false, true) => arms.push(quote! { Self::#variant_name(_) => #gazelle_crate_path::SymbolId::new(#table_id), }),
            (false, false) => arms.push(quote! { Self::#variant_name => #gazelle_crate_path::SymbolId::new(#table_id), }),
            (true, true) => arms.push(quote! { Self::#variant_name(_, _) => #gazelle_crate_path::SymbolId::new(#table_id), }),
            (true, false) => arms.push(quote! { Self::#variant_name(_) => #gazelle_crate_path::SymbolId::new(#table_id), }),
        }
    }

    // Always add __Phantom arm since we always include the variant
    arms.push(quote! { Self::__Phantom(_) => unreachable!(), });

    arms
}

fn build_to_token_arms(
    ctx: &CodegenContext,
    gazelle_crate_path: &TokenStream,
    _has_typed_terminals: bool,
) -> Vec<TokenStream> {
    let mut arms = Vec::new();

    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        let name = ctx.grammar.symbols.name(id);
        let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
        let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
        let kind = ctx.grammar.symbols.terminal_kind(id);

        use crate::grammar::TerminalKind;
        match (kind, ty.is_some()) {
            (TerminalKind::Prec, true) => arms.push(quote! {
                Self::#variant_name(_, prec) => #gazelle_crate_path::Token {
                    terminal: symbol_ids(#name),
                    resolution: Some(#gazelle_crate_path::Resolution::Prec(*prec)),
                },
            }),
            (TerminalKind::Prec, false) => arms.push(quote! {
                Self::#variant_name(prec) => #gazelle_crate_path::Token {
                    terminal: symbol_ids(#name),
                    resolution: Some(#gazelle_crate_path::Resolution::Prec(*prec)),
                },
            }),
            (TerminalKind::Conflict, true) => arms.push(quote! {
                Self::#variant_name(_, resolution) => #gazelle_crate_path::Token {
                    terminal: symbol_ids(#name),
                    resolution: Some(*resolution),
                },
            }),
            (TerminalKind::Conflict, false) => arms.push(quote! {
                Self::#variant_name(resolution) => #gazelle_crate_path::Token {
                    terminal: symbol_ids(#name),
                    resolution: Some(*resolution),
                },
            }),
            (_, true) => arms.push(quote! {
                Self::#variant_name(_) => #gazelle_crate_path::Token::new(symbol_ids(#name)),
            }),
            (_, false) => arms.push(quote! {
                Self::#variant_name => #gazelle_crate_path::Token::new(symbol_ids(#name)),
            }),
        }
    }

    // Always add __Phantom arm
    arms.push(quote! { Self::__Phantom(_) => unreachable!(), });

    arms
}

/// Generate derive impls (Debug, Clone, PartialEq, Eq, Hash) for Terminal<A>.
fn generate_terminal_derive_impls(
    ctx: &CodegenContext,
    types_trait: &proc_macro2::Ident,
    terminal_enum: &proc_macro2::Ident,
    variant_info: &[(proc_macro2::Ident, bool, bool)], // (name, has_type, is_prec)
) -> Vec<TokenStream> {
    let mut impls = Vec::new();
    let phantom_arm = quote! { Self::__Phantom(_) => unreachable!(), };

    // Helper: build match pattern and bindings for a variant
    let make_bindings = |vname: &proc_macro2::Ident,
                         has_type: bool,
                         is_prec: bool|
     -> (TokenStream, Vec<proc_macro2::Ident>) {
        let mut bindings = Vec::new();
        if has_type {
            bindings.push(format_ident!("v"));
        }
        if is_prec {
            bindings.push(format_ident!("p"));
        }
        if bindings.is_empty() {
            (quote! { Self::#vname }, bindings)
        } else {
            (quote! { Self::#vname(#(#bindings),*) }, bindings)
        }
    };

    if ctx.has_derive("Debug") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, has_type, is_prec)| {
                let (pat, bindings) = make_bindings(vname, *has_type, *is_prec);
                let variant_str = vname.to_string();
                if bindings.is_empty() {
                    quote! { #pat => f.write_str(#variant_str) }
                } else {
                    let fields: Vec<_> = bindings.iter().map(|b| quote! { .field(#b) }).collect();
                    quote! { #pat => f.debug_tuple(#variant_str)#(#fields)*.finish() }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> core::fmt::Debug for #terminal_enum<A> {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    if ctx.has_derive("Clone") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, has_type, is_prec)| {
                let (pat, bindings) = make_bindings(vname, *has_type, *is_prec);
                if bindings.is_empty() {
                    quote! { #pat => Self::#vname }
                } else {
                    let clones: Vec<_> = bindings.iter().map(|b| quote! { #b.clone() }).collect();
                    quote! { #pat => Self::#vname(#(#clones),*) }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> Clone for #terminal_enum<A> {
                fn clone(&self) -> Self {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    if ctx.has_derive("PartialEq") {
        let arms: Vec<_> = variant_info
            .iter()
            .map(|(vname, has_type, is_prec)| {
                let mut lhs_bindings = Vec::new();
                let mut rhs_bindings = Vec::new();
                if *has_type {
                    lhs_bindings.push(format_ident!("lv"));
                    rhs_bindings.push(format_ident!("rv"));
                }
                if *is_prec {
                    lhs_bindings.push(format_ident!("lp"));
                    rhs_bindings.push(format_ident!("rp"));
                }
                if lhs_bindings.is_empty() {
                    quote! { (Self::#vname, Self::#vname) => true }
                } else {
                    let cmp: Vec<_> = lhs_bindings
                        .iter()
                        .zip(rhs_bindings.iter())
                        .map(|(l, r)| quote! { #l == #r })
                        .collect();
                    quote! {
                        (Self::#vname(#(#lhs_bindings),*), Self::#vname(#(#rhs_bindings),*)) => #(#cmp)&&*
                    }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> PartialEq for #terminal_enum<A> {
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
            impl<A: #types_trait> Eq for #terminal_enum<A> {}
        });
    }

    if ctx.has_derive("Hash") {
        let arms: Vec<_> = variant_info
            .iter()
            .enumerate()
            .map(|(i, (vname, has_type, is_prec))| {
                let (pat, bindings) = make_bindings(vname, *has_type, *is_prec);
                let disc = i as u64;
                if bindings.is_empty() {
                    quote! { #pat => { state.write_u64(#disc); } }
                } else {
                    let hashes: Vec<_> = bindings
                        .iter()
                        .map(|b| quote! { #b.hash(state); })
                        .collect();
                    quote! { #pat => { state.write_u64(#disc); #(#hashes)* } }
                }
            })
            .collect();
        impls.push(quote! {
            impl<A: #types_trait> core::hash::Hash for #terminal_enum<A> {
                fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                    match self { #(#arms,)* #phantom_arm }
                }
            }
        });
    }

    impls
}

fn build_resolution_arms(
    ctx: &CodegenContext,
    gazelle_crate_path: &TokenStream,
    _has_typed_terminals: bool,
) -> Vec<TokenStream> {
    let mut arms = Vec::new();

    for id in ctx.grammar.symbols.terminal_ids().skip(1) {
        let name = ctx.grammar.symbols.name(id);
        let variant_name = format_ident!("{}", crate::lr::to_camel_case(name));
        let ty = ctx.grammar.types.get(&id).and_then(|t| t.as_ref());
        let kind = ctx.grammar.symbols.terminal_kind(id);

        use crate::grammar::TerminalKind;
        match (kind, ty.is_some()) {
            (TerminalKind::Prec, true) => arms.push(quote! {
                Self::#variant_name(_, prec) => Some(#gazelle_crate_path::Resolution::Prec(*prec)),
            }),
            (TerminalKind::Prec, false) => arms.push(quote! {
                Self::#variant_name(prec) => Some(#gazelle_crate_path::Resolution::Prec(*prec)),
            }),
            (TerminalKind::Conflict, true) => arms.push(quote! {
                Self::#variant_name(_, resolution) => Some(*resolution),
            }),
            (TerminalKind::Conflict, false) => arms.push(quote! {
                Self::#variant_name(resolution) => Some(*resolution),
            }),
            (_, true) => arms.push(quote! { Self::#variant_name(_) => None, }),
            (_, false) => arms.push(quote! { Self::#variant_name => None, }),
        }
    }

    // Always add __Phantom arm
    arms.push(quote! { Self::__Phantom(_) => unreachable!(), });

    arms
}
