//! The `style` attribute (R0b): read as Chromium 147 reads it, against
//! `tests/fidelity/fixtures/style-attribute-v1/chromium-windows.json`, which
//! `tools/qualification/capture_style_attribute_oracle.py` records, and
//! applied at the style attribute's place in the cascade.

use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle, ViewportMetrics};
use div_style::selector_parser::parse_selector_list;
use div_style::stylesheet::{CascadeOrigin, StyleRule, StyleSheet};
use div_style::{
    parse_style_attribute, parse_value, Color, CssWideKeyword, InlineDeclaration, Style,
    StyleProperty, StyleValue, UnparsedValue,
};
use serde_json::Value;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/style-attribute-v1/chromium-windows.json");

/// Style cases Div reads differently from Chromium: (case index, probe,
/// why). Chromium keeps the declaration and Div drops it, with a report.
/// Empty since RFC 0014 step 4 kept `var()` (case 31) and CSS-wide keywords
/// (case 32).
const STYLE_GAPS: &[(usize, &str, &str)] = &[];

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(200.0, 200.0, 1.0).unwrap()).unwrap()
}

fn element(engine: &mut Engine, parent: NodeHandle, tag: ElementTag) -> NodeHandle {
    let element = engine.create_native_element(tag).unwrap();
    engine.append_child(parent, element).unwrap();
    element
}

fn opacity_of(engine: &Engine, element: NodeHandle) -> f32 {
    engine.computed_style(element).unwrap().opacity
}

fn property(name: &str) -> StyleProperty {
    StyleProperty::from_css_name(name).unwrap_or_else(|| panic!("Div has no `{name}`"))
}

/// Colors compare at 8 bits a channel, the precision Chromium serializes.
fn same_value(a: &StyleValue, b: &StyleValue) -> bool {
    fn bytes(color: &Color) -> [i32; 4] {
        [color.r, color.g, color.b, color.a].map(|channel| (channel * 255.0).round() as i32)
    }
    match (a, b) {
        (StyleValue::Color(a), StyleValue::Color(b)) => bytes(a) == bytes(b),
        _ => a == b,
    }
}

/// The value Chromium kept, as a declaration holds it: a CSS-wide keyword,
/// a value with `var()` (kept unparsed until computed-value time), or a
/// literal of the property's type.
fn expected_value(property: StyleProperty, value: &str) -> Result<StyleValue, String> {
    if let Some(keyword) = CssWideKeyword::from_css(value) {
        return Ok(StyleValue::Wide(keyword));
    }
    if value.to_ascii_lowercase().contains("var(") {
        return UnparsedValue::parse(value)
            .map(StyleValue::Unparsed)
            .map_err(|error| error.0);
    }
    parse_value(property, value)
}

/// The declaration that wins for `property` in a parsed attribute: the last
/// `!important` one, or else the last normal one.
fn winner(
    declarations: &[InlineDeclaration],
    property: StyleProperty,
) -> Option<&InlineDeclaration> {
    let mut candidates = declarations.iter().filter(|d| d.property == property);
    candidates
        .clone()
        .filter(|d| d.important)
        .last()
        .or_else(|| candidates.next_back())
}

#[test]
fn style_attributes_keep_what_chromium_keeps() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let probes: Vec<&str> = oracle["probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|probe| probe.as_str().unwrap())
        .collect();
    let mut failures = Vec::new();
    let mut gaps_seen = 0;
    for (index, case) in oracle["style"].as_array().unwrap().iter().enumerate() {
        let text = case["style"].as_str().unwrap();
        let parsed = parse_style_attribute(text);
        for &probe in &probes {
            let property = property(probe);
            let div = winner(&parsed.declarations, property);
            let chromium = case["kept"].get(probe);
            if let Some((_, _, why)) = STYLE_GAPS
                .iter()
                .find(|(gap, name, _)| *gap == index && *name == probe)
            {
                assert!(
                    chromium.is_some() && div.is_none(),
                    "case {index} is no longer a gap: {why}"
                );
                assert!(
                    !parsed.dropped.is_empty(),
                    "case {index}: the gap must be reported"
                );
                gaps_seen += 1;
                continue;
            }
            match (div, chromium) {
                (None, None) => {}
                (Some(div), Some(chromium)) => {
                    let value = chromium["value"].as_str().unwrap();
                    let important = chromium["important"].as_bool().unwrap();
                    let expected = expected_value(property, value);
                    match expected {
                        Ok(expected) if same_value(&div.value, &expected) => {}
                        other => failures.push(format!(
                            "case {index} {text:?}: {probe} is {:?}, Chromium {value:?} ({other:?})",
                            div.value
                        )),
                    }
                    if div.important != important {
                        failures.push(format!(
                            "case {index} {text:?}: {probe} importance {} vs Chromium {important}",
                            div.important
                        ));
                    }
                }
                (div, chromium) => failures.push(format!(
                    "case {index} {text:?}: {probe}: Div {div:?}, Chromium {chromium:?} (dropped {:?})",
                    parsed.dropped
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(gaps_seen, STYLE_GAPS.len());
}

#[test]
fn the_attribute_applies_restyles_and_reports_what_it_dropped() {
    let mut engine = engine();
    let root = engine.root();
    let parent = element(&mut engine, root, ElementTag::Div);
    let child = element(&mut engine, parent, ElementTag::Span);
    engine
        .set_attribute(
            parent,
            "style",
            "opacity: 0.5; colr: red; color: rgb(0, 0, 255)",
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, parent), 0.5);
    // `color` inherits.
    let blue = engine.computed_style(child).unwrap().color;
    assert_eq!(blue, Color::from_rgba8(0, 0, 255, 255));
    let dropped = engine.dropped_style_declarations(parent).unwrap();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].text, "colr: red");

    engine
        .set_attribute(parent, "style", "opacity: 0.7")
        .unwrap();
    assert_eq!(opacity_of(&engine, parent), 0.7);
    assert_ne!(engine.computed_style(child).unwrap().color, blue);
    assert!(engine
        .dropped_style_declarations(parent)
        .unwrap()
        .is_empty());

    // A clone keeps the attribute's declarations.
    let clone = engine.clone_subtree(parent).unwrap();
    engine.append_child(root, clone).unwrap();
    assert_eq!(opacity_of(&engine, clone), 0.7);

    assert!(engine.remove_attribute(parent, "STYLE").unwrap());
    assert_eq!(opacity_of(&engine, parent), 1.0);
    assert_eq!(opacity_of(&engine, clone), 0.7);
}

fn rule(selectors: &str, declarations: Style) -> StyleRule {
    StyleRule::new(parse_selector_list(selectors, None).unwrap(), declarations)
}

#[test]
fn the_attribute_ranks_as_cascade_5_places_it() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "id", "t").unwrap();
    let author = engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author)
                .with_rule(rule("#t", Style::default().opacity(0.2))),
        )
        .unwrap();
    // The attribute beats author normal rules of any specificity.
    engine
        .set_attribute(target, "style", "opacity: 0.3")
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.3);
    // The element's own declarations override the attribute's.
    engine
        .set_property(target, StyleProperty::Opacity, 0.4_f32.into())
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.4);

    // Author `!important` beats both; the attribute's `!important` beats
    // author `!important`; user-agent `!important` beats everything.
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author).with_rule(
                rule("div", Style::default()).with_important(Style::default().opacity(0.5)),
            ),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.5);
    engine
        .set_attribute(target, "style", "opacity: 0.6 !important; opacity: 0.1")
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.6);
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::UserAgent).with_rule(
                rule("div", Style::default()).with_important(Style::default().opacity(0.8)),
            ),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.8);
    engine.remove_style_sheet(author).unwrap();
    assert_eq!(opacity_of(&engine, target), 0.8);
}

#[test]
fn without_sheets_the_attribute_alone_applies() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine
        .set_attribute(target, "style", "display: flex !important; opacity: 0.25")
        .unwrap();
    let style = engine.computed_style(target).unwrap();
    assert_eq!(style.opacity, 0.25);
    assert_eq!(style.display, div_style::Display::Flex);
}
