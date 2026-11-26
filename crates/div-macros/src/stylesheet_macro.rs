//! `stylesheet!`: CSS style rules compiled to a typed `StyleSheet` (RFC 0001).
//!
//! The sheet is read as CSS text by `div-style`'s macro-time sheet parser,
//! which parses selectors with the same selector parser as the rest of the
//! cascade and each value with the runtime `parse_literal`. Every rule is
//! emitted as builder calls on its typed selectors and values, so nothing is
//! parsed at run time.
//!
//! Two forms are accepted:
//!
//! - `stylesheet!(pub cards, "src/cards.css")`. The path is relative to the
//!   crate's `Cargo.toml`, and editing the file rebuilds the crate.
//! - `stylesheet!(pub cards { .card { padding: 8px } })`. The CSS text is
//!   the macro call's own source text, read as `style!` reads its block.
//!
//! Either form defines a module `cards` holding the sheet, as
//! `cards::SHEET: LazyLock<StyleSheet>`, and `cards::classes`, with one
//! `ClassId` constant per class the selectors use, so a misspelt class name
//! does not compile. The sheet is not a `static` named `cards` beside the
//! module: a lowercase static would forbid every `let cards` binding in the
//! enclosing module.

use std::collections::BTreeMap;
use std::path::PathBuf;

use div_style::constructor_tokens::ConstructorTokens;
use div_style::declaration_block::BlockDeclaration;
use div_style::stylesheet_parser::{class_names, parse_stylesheet, SheetItem, SheetRule};
use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::quote;
use syn::parse::{ParseStream, Parser};
use syn::{LitStr, Token, Visibility};

use crate::style_macro::{macro_body, strip_line_comments};
use crate::view::{property_tokens, typed_value_tokens};

enum Body {
    File(LitStr),
    Inline(proc_macro2::Group),
}

/// `source` is the macro call's source text, when the compiler has it.
pub(crate) fn expand(input: TokenStream, source: Option<String>) -> TokenStream {
    match expand_sheet(input, source) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_sheet(input: TokenStream, source: Option<String>) -> syn::Result<TokenStream> {
    let (vis, name, body) = parse_header.parse2(input)?;
    let (text, span, origin, rebuild) = match body {
        Body::File(path) => {
            let (text, full_path) = read_sheet(&path)?;
            let full = full_path.to_string_lossy().into_owned();
            let rebuild = quote!(
                const _: &[u8] = ::core::include_bytes!(#full);
            );
            (
                text,
                path.span(),
                format!("{}", path.value()),
                Some(rebuild),
            )
        }
        Body::Inline(group) => {
            let text = inline_text(&group, source.as_deref())?;
            (text, group.span(), String::from("the sheet"), None)
        }
    };
    let items = parse_stylesheet(&text).map_err(|errors| {
        let mut combined: Option<syn::Error> = None;
        for error in errors {
            let (line, column) = line_column(&text, error.source.start);
            let error = syn::Error::new(
                span,
                format!("{origin}, line {line}, column {column}: {}", error.message),
            );
            match &mut combined {
                Some(combined) => combined.combine(error),
                None => combined = Some(error),
            }
        }
        combined.expect("a failed parse reports an error")
    })?;

    let root = quote!(::div::typed_style);
    let selector_root = quote!(::div::typed_style::selector);
    let media_root = quote!(::div::typed_style::media_query);
    let mut statements = Vec::with_capacity(items.len());
    for item in &items {
        statements.push(match item {
            SheetItem::Layers(names) => quote!(sheet.declare_layers([#(#names),*]);),
            SheetItem::Rule(rule) => {
                let rule = rule_tokens(rule, &text, &root, &selector_root, &media_root)
                    .map_err(|message| syn::Error::new(span, message))?;
                quote!(sheet.push(#rule);)
            }
        });
    }

    let classes = class_constants(&class_names(&items), span)?;
    Ok(quote! {
        #vis mod #name {
            #rebuild
            /// The style sheet, built on first use.
            pub static SHEET: ::std::sync::LazyLock<#root::stylesheet::StyleSheet> =
                ::std::sync::LazyLock::new(|| {
                    let mut sheet = #root::stylesheet::StyleSheet::new(
                        #root::stylesheet::CascadeOrigin::Author,
                    );
                    #(#statements)*
                    sheet
                });
            /// The classes this sheet's selectors use.
            pub mod classes {
                #![allow(non_upper_case_globals)]
                #(#classes)*
            }
        }
    })
}

fn parse_header(input: ParseStream) -> syn::Result<(Visibility, Ident, Body)> {
    let vis: Visibility = input.parse()?;
    let name: Ident = input.parse()?;
    let body = if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
        let path: LitStr = input.parse()?;
        input.parse::<Option<Token![,]>>()?;
        Body::File(path)
    } else {
        match input.parse::<TokenTree>()? {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => Body::Inline(group),
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "expected `stylesheet!(name, \"file.css\")` or `stylesheet!(name { ... })`",
                ))
            }
        }
    };
    if !input.is_empty() {
        return Err(input.error("unexpected tokens after the style sheet"));
    }
    Ok((vis, name, body))
}

fn read_sheet(path: &LitStr) -> syn::Result<(String, PathBuf)> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        syn::Error::new(
            path.span(),
            "stylesheet! needs CARGO_MANIFEST_DIR to find the file",
        )
    })?;
    let full_path = PathBuf::from(manifest).join(path.value());
    let text = std::fs::read_to_string(&full_path).map_err(|error| {
        syn::Error::new(
            path.span(),
            format!("cannot read `{}`: {error}", full_path.display()),
        )
    })?;
    Ok((text, full_path))
}

/// The CSS between the braces of `stylesheet!(name { ... })`, from the
/// call's source text, checked as `style!` checks its block.
fn inline_text(group: &proc_macro2::Group, source: Option<&str>) -> syn::Result<String> {
    let unreadable = || {
        syn::Error::new(
            group.span(),
            "stylesheet! cannot read this sheet's CSS text from the source, which happens \
             inside another macro; put the CSS in a file: stylesheet!(name, \"file.css\")",
        )
    };
    let body = source.and_then(macro_body).ok_or_else(unreadable)?;
    // The visibility and the name hold no braces, so the first `{` opens
    // the sheet.
    let open = body.find('{').ok_or_else(unreadable)?;
    let inner = body[open + 1..]
        .trim_end()
        .strip_suffix('}')
        .ok_or_else(unreadable)?;
    let inner = strip_line_comments(inner);
    let relexed: TokenStream = inner.parse().map_err(|_| unreadable())?;
    if relexed.to_string() != group.stream().to_string() {
        return Err(unreadable());
    }
    Ok(inner)
}

fn rule_tokens(
    rule: &SheetRule,
    text: &str,
    root: &TokenStream,
    selector_root: &TokenStream,
    media_root: &TokenStream,
) -> Result<TokenStream, String> {
    let selectors = rule
        .selectors
        .constructor_tokens(selector_root)
        .map_err(|error| error.to_string())?;
    let (important, normal): (Vec<_>, Vec<_>) = rule
        .declarations
        .iter()
        .partition(|declaration| declaration.important);
    let normal = style_tokens(&normal, text, root)?;
    let mut tokens = quote!(#root::stylesheet::StyleRule::new(#selectors, #normal));
    if !important.is_empty() {
        let important = style_tokens(&important, text, root)?;
        tokens = quote!(#tokens.with_important(#important));
    }
    if let Some(layer) = &rule.layer {
        tokens = quote!(#tokens.in_layer(#layer));
    }
    for media in &rule.media {
        let media = media
            .constructor_tokens(media_root)
            .map_err(|error| error.to_string())?;
        tokens = quote!(#tokens.with_media(#media));
    }
    Ok(tokens)
}

fn style_tokens(
    declarations: &[&BlockDeclaration],
    text: &str,
    root: &TokenStream,
) -> Result<TokenStream, String> {
    let mut items = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        let property = property_tokens(declaration.property);
        let value = typed_value_tokens(&declaration.value)
            .map_err(|error| format!("`{}`: {error}", text[declaration.source.clone()].trim()))?;
        items.push(quote!(#root::Declaration { property: #property, value: #value }));
    }
    Ok(quote! {
        <#root::Style as ::core::iter::FromIterator<#root::Declaration>>::from_iter([#(#items),*])
    })
}

fn class_constants(names: &[String], span: Span) -> syn::Result<Vec<TokenStream>> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut constants = Vec::with_capacity(names.len());
    for name in names {
        let mangled = class_ident(name)
            .ok_or_else(|| syn::Error::new(span, format!("class `{name}` has no Rust name")))?;
        if let Some(other) = seen.insert(mangled.clone(), name) {
            return Err(syn::Error::new(
                span,
                format!("classes `{other}` and `{name}` both become `classes::{mangled}`"),
            ));
        }
        let ident = ident(&mangled);
        let doc = format!("`.{name}`");
        constants.push(quote! {
            #[doc = #doc]
            pub const #ident: ::div::typed_style::stylesheet::ClassId =
                ::div::typed_style::stylesheet::ClassId::__from_sheet(#name);
        });
    }
    Ok(constants)
}

/// The Rust name of a class's constant. ASCII letters, digits and `_` stay;
/// `-` becomes `_`; any other character `c` becomes `_u<hex of c>_`; a
/// leading digit gets a leading `_`. So `hover:bg-muted` is
/// `hover_u3a_bg_muted` and `w-1/2` is `w_1_u2f_2`. `None` for a name with
/// no identifier (`-`).
pub(crate) fn class_ident(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        match character {
            'a'..='z' | 'A'..='Z' | '_' => out.push(character),
            '0'..='9' => {
                if index == 0 {
                    out.push('_');
                }
                out.push(character);
            }
            '-' => out.push('_'),
            _ => out.push_str(&format!("_u{:x}_", u32::from(character))),
        }
    }
    (!out.is_empty() && out != "_").then_some(out)
}

/// An identifier for `name`, raw when `name` is a keyword. The keywords
/// that cannot be raw get a trailing `_`.
fn ident(name: &str) -> Ident {
    match name {
        "self" | "Self" | "super" | "crate" => Ident::new(&format!("{name}_"), Span::call_site()),
        _ if syn::parse_str::<Ident>(name).is_ok() => Ident::new(name, Span::call_site()),
        _ => Ident::new_raw(name, Span::call_site()),
    }
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_names_mangle_stably() {
        assert_eq!(class_ident("card").as_deref(), Some("card"));
        assert_eq!(class_ident("card-title").as_deref(), Some("card_title"));
        assert_eq!(
            class_ident("hover:bg-muted").as_deref(),
            Some("hover_u3a_bg_muted")
        );
        assert_eq!(
            class_ident("data-[state=open]:p-2").as_deref(),
            Some("data__u5b_state_u3d_open_u5d__u3a_p_2")
        );
        assert_eq!(class_ident("w-1/2").as_deref(), Some("w_1_u2f_2"));
        assert_eq!(class_ident("2xl").as_deref(), Some("_2xl"));
        assert_eq!(class_ident("é").as_deref(), Some("_ue9_"));
        assert_eq!(class_ident("-"), None);
    }

    #[test]
    fn keywords_become_raw_identifiers() {
        assert_eq!(ident("type").to_string(), "r#type");
        assert_eq!(ident("self").to_string(), "self_");
        assert_eq!(ident("card").to_string(), "card");
    }

    #[test]
    fn colliding_class_names_are_an_error() {
        let names = vec!["a-b".to_owned(), "a_b".to_owned()];
        let error = class_constants(&names, Span::call_site()).expect_err("collision");
        assert!(error.to_string().contains("`a-b` and `a_b`"));
    }

    #[test]
    fn errors_name_their_line_and_column() {
        assert_eq!(line_column("a\nbc", 3), (2, 2));
        assert_eq!(line_column("abc", 0), (1, 1));
    }
}
