//! `style!` compiles a CSS declaration block to a typed `Style` (RFC 0001).
//! Each declaration must equal the runtime parse of the same text, in order.

use div::style;
use div::typed_style::{parse_literal, Declaration, Style, StyleProperty};

/// The `Style` the runtime parser builds from `(property, value)` pairs.
fn parsed(pairs: &[(&str, &str)]) -> Style {
    pairs
        .iter()
        .map(|(name, value)| {
            let property = StyleProperty::from_css_name(name).expect(name);
            Declaration {
                property,
                value: parse_literal(property, value).unwrap_or_else(|e| panic!("{name}: {e}")),
            }
        })
        .collect()
}

fn assert_same(built: &Style, expected: &Style) {
    // Debug text tells every float bit pattern apart, including -0.0.
    assert_eq!(format!("{built:?}"), format!("{expected:?}"));
    assert_eq!(built, expected);
}

#[test]
fn braced_blocks_match_the_runtime_parse() {
    let built = style! {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 8px 16px;
        margin-left: -10px;
        width: 50%;
        opacity: 0.25;
        border: 1px solid red;
        color: rgb(0 0 0 / 0.5);
        background-color: #0a0a0a;
        font-family: "Inter", sans-serif;
        font-weight: 600;
    };
    let expected = parsed(&[
        ("display", "flex"),
        ("justify-content", "space-between"),
        ("align-items", "center"),
        ("padding", "8px 16px"),
        ("margin-left", "-10px"),
        ("width", "50%"),
        ("opacity", "0.25"),
        ("border", "1px solid red"),
        ("color", "rgb(0 0 0 / 0.5)"),
        ("background-color", "#0a0a0a"),
        ("font-family", "\"Inter\", sans-serif"),
        ("font-weight", "600"),
    ]);
    assert_same(&built, &expected);
}

#[test]
fn comments_and_layout_do_not_change_the_block() {
    let built = style! {
        // Rust comments are not CSS.
        padding: /* top and bottom */ 8px
                 16px; // left and right
        DISPLAY: flex
    };
    assert_same(
        &built,
        &parsed(&[("padding", "8px 16px"), ("display", "flex")]),
    );
}

#[test]
fn string_blocks_take_css_rust_cannot_tokenize() {
    let built = style!("color: #3e3; font-family: 'Inter'");
    assert_same(
        &built,
        &parsed(&[("color", "#3e3"), ("font-family", "'Inter'")]),
    );
}

macro_rules! wrapped {
    ($css:literal) => {
        style!($css)
    };
}

#[test]
fn string_blocks_work_inside_other_macros() {
    assert_same(&wrapped!("display: flex"), &parsed(&[("display", "flex")]));
}

#[test]
fn an_empty_block_is_an_empty_style() {
    assert!(style! {}.declarations().is_empty());
    assert!(style!("").declarations().is_empty());
}

#[test]
fn applied_blocks_reach_the_computed_style() {
    use div::prelude::*;
    use div::typed_style::{Display, Length};

    let mut app =
        HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap();
    app.mount(|| view! { <div /> }).unwrap();
    let element = app.document().body().first_child().unwrap().unwrap();
    element
        .apply_style(&style! { display: flex; padding: 8px 16px; opacity: 0.25 })
        .unwrap();
    let style = element.computed_style().unwrap();
    assert_eq!(*style.display(), Display::Flex);
    assert_eq!(*style.opacity(), 0.25);
    assert_eq!(*style.padding_top(), Length::px(8.0));
    assert_eq!(*style.padding_left(), Length::px(16.0));
}

#[test]
fn custom_properties_and_var_match_the_runtime_parse_in_order() {
    use div::typed_style::declaration_block::{parse_declaration_block, BlockTarget};
    use div::typed_style::StyleValue;

    let text = "--a: 1px  2px; color: var(--a, red)";
    let expected = parse_declaration_block(text).unwrap().into_iter().fold(
        Style::default(),
        |style, declaration| match declaration.target {
            BlockTarget::Property(property, value) => {
                style.with_declaration(Declaration { property, value })
            }
            BlockTarget::Custom(custom) => {
                style.with_custom_property(custom.name().clone(), custom.value().clone())
            }
        },
    );
    let built = style! { --a: 1px  2px; color: var(--a, red) };
    assert_same(&built, &expected);
    let custom = &built.custom_declarations()[0];
    assert_eq!(custom.name().as_str(), "--a");
    assert_eq!(custom.value().as_str(), "1px  2px");
    assert!(matches!(
        &built.declarations()[0].value,
        StyleValue::Unparsed(value) if value.as_str() == "var(--a, red)"
    ));
    // The same declarations in another order are another `Style`.
    assert_ne!(built, style! { color: var(--a, red); --a: 1px  2px });
}

/// An element with `color: rgb(0, 0, 255)` applied, and its app.
fn blue_element() -> (div::HeadlessApp, div::Element) {
    use div::prelude::*;

    let mut app =
        HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap();
    app.mount(|| view! { <div /> }).unwrap();
    let element = app.document().body().first_child().unwrap().unwrap();
    element
        .apply_style(&style! { color: rgb(0, 0, 255) })
        .unwrap();
    (app, element)
}

fn assert_refused(error: div::Error, limit: &str) {
    match error {
        div::Error::Engine(div::EngineError::InvalidInput(message)) => {
            assert!(message.contains(limit), "{message}");
            assert!(message.contains("RFC 0014 step 5"), "{message}");
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn set_property_refuses_keywords_and_var_until_step_5() {
    use div::typed_style::{CssWideKeyword, StyleValue, UnparsedValue};

    let (_app, element) = blue_element();
    let before = element.computed_style().unwrap();
    let error = element
        .set_property(
            StyleProperty::MarginLeft,
            StyleValue::Wide(CssWideKeyword::Inherit),
        )
        .unwrap_err();
    assert_refused(error, "CSS-wide keywords");
    let error = element
        .set_property(
            StyleProperty::MarginLeft,
            StyleValue::Unparsed(UnparsedValue::parse("var(--x)").unwrap()),
        )
        .unwrap_err();
    assert_refused(error, "var()");
    assert_eq!(element.computed_style().unwrap(), before);
}

#[test]
fn apply_style_refuses_custom_properties_and_changes_nothing() {
    let (_app, element) = blue_element();
    let before = element.computed_style().unwrap();
    let error = element.apply_style(&style! { --a: 1px }).unwrap_err();
    assert_refused(error, "custom properties");
    assert_eq!(element.computed_style().unwrap(), before);
}

#[test]
fn apply_style_checks_every_declaration_before_applying_any() {
    use div::typed_style::Color;

    let (_app, element) = blue_element();
    let blue = *element.computed_style().unwrap().color();
    assert_eq!(blue, Color::from_rgba8(0, 0, 255, 255));
    let error = element
        .apply_style(&style! { color: red; margin: var(--x) })
        .unwrap_err();
    assert_refused(error, "var()");
    assert_eq!(*element.computed_style().unwrap().color(), blue);
    let error = element
        .apply_style(&style! { margin-left: 4px; color: inherit })
        .unwrap_err();
    assert_refused(error, "CSS-wide keywords");
    assert_eq!(*element.computed_style().unwrap().color(), blue);
    assert_eq!(
        *element.computed_style().unwrap().margin_left(),
        div::typed_style::Length::px(0.0)
    );
    // The first such declaration in source order is the one named.
    let error = element
        .apply_style(&style! { --a: 1px; color: inherit })
        .unwrap_err();
    assert_refused(error, "custom properties");
    let error = element
        .apply_style(&style! { color: inherit; --a: 1px })
        .unwrap_err();
    assert_refused(error, "CSS-wide keywords");
}

#[test]
fn pseudo_styles_take_custom_properties_and_var() {
    use div::typed_style::{Color, PseudoStyleTarget};

    let (_app, element) = blue_element();
    element
        .set_pseudo_style(
            PseudoStyleTarget::FirstLine,
            &style! { --c: rgb(0, 128, 0); color: var(--c) },
        )
        .unwrap();
    let style = element.computed_style().unwrap();
    let first_line = style
        .first_line_style
        .as_deref()
        .expect("a ::first-line style");
    assert_eq!(first_line.color, Color::from_rgba8(0, 128, 0, 255));
    assert_eq!(first_line.custom_property("--c"), Some("rgb(0, 128, 0)"));
    // The originating element is unchanged.
    assert_eq!(*style.color(), Color::from_rgba8(0, 0, 255, 255));
    assert_eq!(style.custom_property("--c"), None);
}
