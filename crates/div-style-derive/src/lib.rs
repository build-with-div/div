//! `#[derive(ConstructorTokens)]` for `div-style` value types.
//!
//! The derive is internal to `div-style`: the generated impl names
//! `crate::constructor_tokens`. It lets `div-macros` parse a style literal
//! at compile time and emit an expression that builds the same typed value,
//! so nothing is parsed at run time (RFC 0001).
//!
//! A variant marked `#[constructor_tokens(unsupported)]` is one that no style
//! literal produces, such as a value sampled from an animation. Asking for its
//! tokens returns `Unsupported` instead of naming its fields.
//!
//! A variant marked `#[constructor_tokens(module = "name")]` holds values
//! whose types are public under `root::name` rather than at `root`, such as
//! the `calc()` tree in `math`. Its fields are built with `root::name` as
//! their root.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Fields, Ident, LitStr};

#[proc_macro_derive(ConstructorTokens, attributes(constructor_tokens))]
pub fn derive_constructor_tokens(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: &DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let module = quote!(crate::constructor_tokens);
    let mut generics = input.generics.clone();
    for parameter in generics.type_params_mut() {
        parameter.bounds.push(syn::parse_quote!(
            crate::constructor_tokens::ConstructorTokens
        ));
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let body = match &input.data {
        Data::Struct(data) => {
            let options = options(&input.attrs)?;
            if options.unsupported || options.module.is_some() {
                return Err(syn::Error::new_spanned(
                    name,
                    "`unsupported` and `module` apply to enum variants only",
                ));
            }
            let (pattern, build) = construct(quote!(Self), &[name.clone()], &data.fields, None);
            quote! {
                let #pattern = self;
                #build
            }
        }
        Data::Enum(data) => {
            let mut arms = Vec::with_capacity(data.variants.len());
            for variant in &data.variants {
                let ident = &variant.ident;
                let options = options(&variant.attrs)?;
                if options.unsupported {
                    let rest = match &variant.fields {
                        Fields::Named(_) => quote!({ .. }),
                        Fields::Unnamed(_) => quote!((..)),
                        Fields::Unit => quote!(),
                    };
                    let what = LitStr::new(&format!("{name}::{ident}"), Span::call_site());
                    arms.push(quote! {
                        Self::#ident #rest => ::core::result::Result::Err(
                            #module::Unsupported::new(#what)
                        ),
                    });
                } else {
                    let (pattern, build) = construct(
                        quote!(Self::#ident),
                        &[name.clone(), ident.clone()],
                        &variant.fields,
                        options.module.as_ref(),
                    );
                    arms.push(quote!(#pattern => { #build }));
                }
            }
            quote! {
                match self {
                    #(#arms)*
                }
            }
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                name,
                "ConstructorTokens cannot be derived for unions",
            ))
        }
    };
    Ok(quote! {
        impl #impl_generics #module::ConstructorTokens for #name #type_generics #where_clause {
            fn constructor_tokens(
                &self,
                root: &#module::TokenStream,
            ) -> ::core::result::Result<#module::TokenStream, #module::Unsupported> {
                #body
            }
        }
    })
}

/// A destructuring pattern for `fields` and the statements that build the
/// constructor expression `root::<segments>` from the bound fields. With
/// `field_module`, the fields are built under `root::<field_module>`.
fn construct(
    pattern_path: Tokens,
    segments: &[Ident],
    fields: &Fields,
    field_module: Option<&LitStr>,
) -> (Tokens, Tokens) {
    let module = quote!(crate::constructor_tokens);
    let names: Vec<String> = segments.iter().map(Ident::to_string).collect();
    let path = quote!(#module::path(root, &[#(#names),*]));
    let field_root = match field_module {
        Some(field_module) => quote!(&#module::path(root, &[#field_module])),
        None => quote!(root),
    };
    match fields {
        Fields::Unit => (pattern_path, quote!(::core::result::Result::Ok(#path))),
        Fields::Unnamed(unnamed) => {
            let bindings: Vec<Ident> = (0..unnamed.unnamed.len())
                .map(|index| format_ident!("__field_{index}"))
                .collect();
            let build = quote! {
                ::core::result::Result::Ok(#module::call(
                    #path,
                    ::std::vec![#(
                        #module::ConstructorTokens::constructor_tokens(#bindings, #field_root)?
                    ),*],
                ))
            };
            (quote!(#pattern_path(#(#bindings),*)), build)
        }
        Fields::Named(named) => {
            let idents: Vec<&Ident> = named
                .named
                .iter()
                .map(|field| field.ident.as_ref().expect("named field"))
                .collect();
            let bindings: Vec<Ident> = idents
                .iter()
                .map(|ident| {
                    format_ident!("__field_{}", ident.to_string().trim_start_matches("r#"))
                })
                .collect();
            let labels: Vec<String> = idents.iter().map(|ident| ident.to_string()).collect();
            let build = quote! {
                ::core::result::Result::Ok(#module::braced(
                    #path,
                    ::std::vec![#((
                        #labels,
                        #module::ConstructorTokens::constructor_tokens(#bindings, #field_root)?,
                    )),*],
                ))
            };
            (quote!(#pattern_path { #(#idents: #bindings),* }), build)
        }
    }
}

/// The `#[constructor_tokens(...)]` options of an item or variant.
#[derive(Default)]
struct Options {
    unsupported: bool,
    module: Option<LitStr>,
}

fn options(attributes: &[Attribute]) -> syn::Result<Options> {
    let mut options = Options::default();
    let mut last = None;
    for attribute in attributes {
        if !attribute.path().is_ident("constructor_tokens") {
            continue;
        }
        last = Some(attribute);
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("unsupported") {
                options.unsupported = true;
                Ok(())
            } else if meta.path.is_ident("module") {
                let module: LitStr = meta.value()?.parse()?;
                syn::parse_str::<Ident>(&module.value())
                    .map_err(|_| meta.error("`module` must name one module"))?;
                options.module = Some(module);
                Ok(())
            } else {
                Err(meta.error("expected `unsupported` or `module = \"...\"`"))
            }
        })?;
    }
    if let (true, Some(_), Some(attribute)) = (options.unsupported, &options.module, last) {
        return Err(syn::Error::new_spanned(
            attribute,
            "`unsupported` and `module` cannot be combined",
        ));
    }
    Ok(options)
}
