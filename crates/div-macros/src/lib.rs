//! Proc macros for Div — JSX-like `view!` and `#[component]` attribute.

use proc_macro::TokenStream;

mod component;
mod style_macro;
mod typed_style_cases;
mod view;

/// JSX-like declarative UI macro.
///
/// Generates Rust code that creates DOM elements, sets attributes and styles,
/// wires event handlers, and builds a reactive UI tree.
///
/// # Syntax
///
/// ```ignore
/// view! {
///     <div class="container">
///         <h1>"Hello!"</h1>
///         <button on:click=move |_| count.set(count.get() + 1)>
///             "Count: " {count.get()}
///         </button>
///     </div>
/// }
/// ```
///
/// ## Supported features
///
/// - Static HTML elements with children and self-closing tags
/// - String literal text nodes: `"text"`
/// - Expression children: `{expression}` (see below)
/// - Event handlers: `on:click=expr`
/// - Static styles: `style:width="100px"`
/// - Dynamic typed styles: `style:opacity={signal.get()}`
/// - Standard and boolean HTML attributes
/// - Component instantiation via PascalCase tags
/// - Multiple root nodes (returns a `Fragment`)
///
/// ## Components
///
/// On a component tag, `name=value` and `name="text"` set props. An
/// `on:event=handler` is attached to the element the component returns (each
/// top-level element of a fragment), so `<Button on:click=...>` handles
/// clicks on the button it renders. A component that returns no element (only
/// text, or nothing) panics when built with a listener; give it a handler
/// prop instead. `style:` and valueless attributes are compile errors
/// on a component: style the element it renders, or write `name=true`.
///
/// ## Expression children
///
/// A `{expression}` child shows text or a view, chosen by its type:
///
/// - Text (`ToString`, such as `{count.get()}` or `{name}`) becomes a text
///   node, re-run in an effect so it follows the signals it reads. Its
///   expression must be `'static`, as the effect keeps it.
/// - A view (`IntoView`: `ViewNode`, `Vec<ViewNode>`, `Option<..>`), such as
///   `{For(..)}`, `{Show(..)}`, `{children}` or
///   `{items.iter().map(..).collect::<Vec<_>>()}`, is built once and mounted
///   as itself. It may borrow locals.
///
/// A variable moves into the view, as text always did. A field or other
/// place (`{props.title}`) is borrowed, so it shows as text; to show a view
/// stored in a field, move it out with `{props.content.into_view()}`.
///
/// ## Note
///
/// Expressions containing bare `>` (comparison) in attribute values must
/// be wrapped in braces: `attr={a > b}`.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    view::view_macro(input.into()).into()
}

/// A CSS declaration block compiled to a typed `div::Style`.
///
/// Each value is parsed at compile time with the same grammar as `view!`'s
/// `style:` attributes and emitted as the constructor of its typed value, so
/// nothing is parsed at run time. An unknown property or an invalid value is
/// a compile error.
///
/// ```ignore
/// let card = style! { display: flex; justify-content: space-between; padding: 8px 16px };
/// element.apply_style(&card)?;
/// ```
///
/// CSS that Rust cannot split into tokens, such as `#3e3` or a single-quoted
/// string, goes in a string literal: `style!("color: #3e3")`. The braced form
/// reads the macro call's source text, so inside another macro use the
/// string form as well.
#[proc_macro]
pub fn style(input: TokenStream) -> TokenStream {
    let source = proc_macro::Span::call_site().source_text();
    style_macro::expand(input.into(), source).into()
}

/// Component attribute macro.
///
/// Transforms a function into a component by generating a props struct
/// and rewriting the function to accept it.
///
/// # Examples
///
/// ```ignore
/// #[component]
/// fn Counter(initial: i32) -> impl IntoView {
///     let count = create_signal(initial);
///     view! { <p>{count}</p> }
/// }
///
/// // Generates:
/// // pub struct CounterProps { pub initial: i32 }
/// // pub fn Counter(props: CounterProps) -> impl IntoView { ... }
/// ```
///
/// ## Optional props
///
/// ```ignore
/// #[component]
/// fn Card(title: String, #[prop(optional)] subtitle: String) -> impl IntoView {
///     // subtitle is Option<String>, unwrapped with Default
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn component(attr: TokenStream, input: TokenStream) -> TokenStream {
    component::component_macro(attr.into(), input.into()).into()
}

/// Test support, not public API: every property's initial value that
/// `parse_literal` accepts, plus every line of `typed_style_corpus.txt`,
/// parsed at compile time and emitted as
/// `(StyleProperty, literal, constructed StyleValue)` tuples. The tests
/// compare each constructed value with the runtime parse of its literal.
#[doc(hidden)]
#[proc_macro]
pub fn __typed_style_literal_cases(input: TokenStream) -> TokenStream {
    typed_style_cases::expand(input.into()).into()
}
