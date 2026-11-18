//! The cascade-v1 oracle: Div cascades `stylesheet!` sheets as pinned
//! Chromium 147 cascades the same CSS in a `<style>` element.
//!
//! Each case in `tests/fidelity/fixtures/cascade-v1/` is a sheet and a page
//! body. `tools/qualification/capture_cascade_oracle.py` records Chromium's
//! `getComputedStyle` for every element with an `id`; this test compiles the
//! sheet, builds the same tree and requires the same value for every probe.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use div::typed_style::stylesheet::StyleSheet;
use div::typed_style::{Color, Display, FontWeight, Length};
use div::{stylesheet, ComputedStyle, Document, Element};

stylesheet!(
    attributes,
    "../../tests/fidelity/fixtures/cascade-v1/attributes.css"
);
stylesheet!(
    functional,
    "../../tests/fidelity/fixtures/cascade-v1/functional.css"
);
stylesheet!(
    inheritance,
    "../../tests/fidelity/fixtures/cascade-v1/inheritance.css"
);
stylesheet!(
    layers,
    "../../tests/fidelity/fixtures/cascade-v1/layers.css"
);
stylesheet!(
    nesting,
    "../../tests/fidelity/fixtures/cascade-v1/nesting.css"
);
stylesheet!(
    specificity,
    "../../tests/fidelity/fixtures/cascade-v1/specificity.css"
);
stylesheet!(
    structural,
    "../../tests/fidelity/fixtures/cascade-v1/structural.css"
);
stylesheet!(
    style_attribute,
    "../../tests/fidelity/fixtures/cascade-v1/style-attribute.css"
);

/// Every case, in the oracle's (sorted) order.
fn sheets() -> Vec<(&'static str, &'static StyleSheet)> {
    vec![
        ("attributes", &*attributes::SHEET),
        ("functional", &*functional::SHEET),
        ("inheritance", &*inheritance::SHEET),
        ("layers", &*layers::SHEET),
        ("nesting", &*nesting::SHEET),
        ("specificity", &*specificity::SHEET),
        ("structural", &*structural::SHEET),
        ("style-attribute", &*style_attribute::SHEET),
    ]
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fidelity/fixtures/cascade-v1")
}

/// Build a page body: elements with double-quoted attributes, and text.
/// This reads the HTML the cases use; it is not a general parser.
fn build(document: &Document, html: &str) -> BTreeMap<String, Element> {
    let mut ids = BTreeMap::new();
    let mut open = vec![document.body()];
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("</") {
            let close = after.find('>').expect("closing tag ends");
            open.pop().expect("closing tag has an element");
            rest = &after[close + 1..];
        } else if let Some(after) = rest.strip_prefix('<') {
            let close = after.find('>').expect("tag ends");
            let (tag, mut attributes) = after[..close]
                .split_once(' ')
                .unwrap_or((&after[..close], ""));
            let element = Element::create(document, tag).unwrap();
            loop {
                attributes = attributes.trim_start();
                if attributes.is_empty() {
                    break;
                }
                let equals = attributes.find("=\"").expect("attributes are quoted");
                let name = &attributes[..equals];
                let value_start = equals + 2;
                let value_end =
                    value_start + attributes[value_start..].find('"').expect("quote ends");
                let value = &attributes[value_start..value_end];
                element.set_attribute(name, value).unwrap();
                if name == "id" {
                    ids.insert(value.to_owned(), element.clone());
                }
                attributes = &attributes[value_end + 1..];
            }
            open.last().unwrap().append_child(&element).unwrap();
            open.push(element);
            rest = &after[close + 1..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            open.last()
                .unwrap()
                .create_text_child(&rest[..end])
                .unwrap();
            rest = &rest[end..];
        }
    }
    assert_eq!(open.len(), 1, "every element is closed");
    ids
}

fn px(value: &str) -> f32 {
    value
        .strip_suffix("px")
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("`{value}` is not a px length"))
}

/// Chromium's `rgb()` or `rgba()` serialization, as 8-bit channels. The
/// cases use only alpha 0 and 1.
fn color(value: &str) -> [u8; 4] {
    let (inner, has_alpha) = match value.strip_prefix("rgba(") {
        Some(inner) => (inner, true),
        None => (value.strip_prefix("rgb(").expect("rgb()"), false),
    };
    let parts: Vec<&str> = inner.strip_suffix(')').unwrap().split(", ").collect();
    let channel = |index: usize| parts[index].parse::<u8>().unwrap();
    let alpha = match (has_alpha, parts.get(3).copied()) {
        (false, _) => 255,
        (true, Some("0")) => 0,
        (true, Some("1")) => 255,
        (true, other) => panic!("alpha {other:?} is not 0 or 1"),
    };
    [channel(0), channel(1), channel(2), alpha]
}

fn bytes(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a].map(|channel| (channel * 255.0).round() as u8)
}

fn display(value: &str) -> Display {
    match value {
        "block" => Display::Block,
        "inline" => Display::Inline,
        "none" => Display::None,
        "flex" => Display::Flex,
        other => panic!("display `{other}` is not in the cases"),
    }
}

/// The probes where Div's computed style differs from Chromium's values.
fn compare(style: &ComputedStyle, expected: &BTreeMap<String, String>) -> Vec<String> {
    let mut differences = Vec::new();
    for (name, value) in expected {
        let value = value.as_str();
        let (same, div) = match name.as_str() {
            "background-color" => (
                bytes(style.background_color) == color(value),
                format!("{:?}", style.background_color),
            ),
            "color" => (
                bytes(style.color) == color(value),
                format!("{:?}", style.color),
            ),
            "display" => (
                style.display == display(value),
                format!("{:?}", style.display),
            ),
            "font-size" => (style.font_size == px(value), style.font_size.to_string()),
            "font-weight" => (
                style.font_weight == FontWeight(value.parse().unwrap()),
                format!("{:?}", style.font_weight),
            ),
            "margin-top" => (
                style.margin_top == Length::px(px(value)),
                format!("{:?}", style.margin_top),
            ),
            "opacity" => (
                style.opacity == value.parse::<f32>().unwrap(),
                style.opacity.to_string(),
            ),
            "padding-left" => (
                style.padding_left == Length::px(px(value)),
                format!("{:?}", style.padding_left),
            ),
            other => panic!("no comparison for probe `{other}`"),
        };
        if !same {
            differences.push(format!("{name}: Chromium {value}, Div {div}"));
        }
    }
    differences
}

#[test]
fn stylesheets_cascade_as_chromium_cascades_them() {
    let oracle: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("chromium-windows.json")).unwrap(),
    )
    .unwrap();
    let cases = oracle["cases"].as_object().unwrap();
    let sheets = sheets();
    assert_eq!(
        cases.keys().map(String::as_str).collect::<Vec<_>>(),
        sheets.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "every captured case has its sheet here"
    );
    let mut failures = Vec::new();
    let mut compared = 0;
    for (name, sheet) in sheets {
        let document = Document::new(200, 200).unwrap();
        let html = std::fs::read_to_string(fixtures().join(format!("{name}.html"))).unwrap();
        let ids = build(&document, &html);
        document.adopt_stylesheet(sheet).unwrap();
        let expected = cases[name].as_object().unwrap();
        let mut expected_ids: Vec<&str> = expected.keys().map(String::as_str).collect();
        expected_ids.sort_unstable();
        assert_eq!(
            expected_ids,
            ids.keys().map(String::as_str).collect::<Vec<_>>(),
            "{name}: the same elements"
        );
        for (id, values) in expected {
            let values: BTreeMap<String, String> = serde_json::from_value(values.clone()).unwrap();
            let style = ids[id].computed_style().unwrap();
            compared += values.len();
            for difference in compare(&style, &values) {
                failures.push(format!("{name} #{id} {difference}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // 43 elements, 8 probes each.
    assert_eq!(compared, 43 * 8);
}
