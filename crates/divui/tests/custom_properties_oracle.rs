//! The custom-properties-v1 oracle: Div computes custom properties, `var()`
//! and the CSS-wide keywords as pinned Chromium 147 does.
//!
//! Each case in `tests/fidelity/fixtures/custom-properties-v1/` is a sheet
//! and a page body, in `cascade-v1`'s format.
//! `tools/qualification/capture_custom_properties_oracle.py` records
//! Chromium's `getComputedStyle` for every element with an `id`: the
//! standard probes and every custom property the case names. This test
//! compiles the sheet with `stylesheet!`, builds the same tree and requires
//! the same values, outside the named gaps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use div::typed_style::stylesheet::StyleSheet;
use div::typed_style::{Color, Display, FontWeight, Length, LineHeight};
use div::{stylesheet, ComputedStyle, Document, Element};

stylesheet!(
    cycles,
    "../../tests/fidelity/fixtures/custom-properties-v1/cycles.css"
);
stylesheet!(
    fallback,
    "../../tests/fidelity/fixtures/custom-properties-v1/fallback.css"
);
stylesheet!(
    importance_and_layers,
    "../../tests/fidelity/fixtures/custom-properties-v1/importance-and-layers.css"
);
stylesheet!(
    inheritance,
    "../../tests/fidelity/fixtures/custom-properties-v1/inheritance.css"
);
stylesheet!(
    invalid_at_computed_time,
    "../../tests/fidelity/fixtures/custom-properties-v1/invalid-at-computed-time.css"
);
stylesheet!(
    serialization,
    "../../tests/fidelity/fixtures/custom-properties-v1/serialization.css"
);
stylesheet!(
    shorthands,
    "../../tests/fidelity/fixtures/custom-properties-v1/shorthands.css"
);
stylesheet!(
    style_attribute,
    "../../tests/fidelity/fixtures/custom-properties-v1/style-attribute.css"
);
stylesheet!(
    token_merging,
    "../../tests/fidelity/fixtures/custom-properties-v1/token-merging.css"
);
stylesheet!(
    wide_keywords,
    "../../tests/fidelity/fixtures/custom-properties-v1/wide-keywords.css"
);

/// Every case, in the oracle's (sorted) order.
fn sheets() -> Vec<(&'static str, &'static StyleSheet)> {
    vec![
        ("cycles", &*cycles::SHEET),
        ("fallback", &*fallback::SHEET),
        ("importance-and-layers", &*importance_and_layers::SHEET),
        ("inheritance", &*inheritance::SHEET),
        (
            "invalid-at-computed-time",
            &*invalid_at_computed_time::SHEET,
        ),
        ("serialization", &*serialization::SHEET),
        ("shorthands", &*shorthands::SHEET),
        ("style-attribute", &*style_attribute::SHEET),
        ("token-merging", &*token_merging::SHEET),
        ("wide-keywords", &*wide_keywords::SHEET),
    ]
}

/// Values where Div differs from Chromium: (case, element, probe, reason).
/// Each must still differ, so a fix removes its entry. None comes from
/// custom properties or `var()`: each substituted value is the text
/// Chromium substitutes, and Div's grammar for the property rejects it, as
/// it rejects the same text written directly.
const GAPS: &[(&str, &str, &str, &str)] = &[
    (
        "shorthands",
        "s3",
        "border-top-width",
        "div-style's `border` shorthand rejects a comma-separated `rgb()`",
    ),
    (
        "shorthands",
        "s5",
        "margin-top",
        "`calc()` over lengths in margins waits for calc() PR 2",
    ),
    (
        "shorthands",
        "s5",
        "padding-left",
        "`calc()` over lengths in padding waits for calc() PR 2",
    ),
    (
        "token-merging",
        "m2",
        "margin-left",
        "`calc()` over lengths in margins waits for calc() PR 2",
    ),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fidelity/fixtures/custom-properties-v1")
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

/// Whether Div's line height resolves to Chromium's: `normal`, or a px
/// length (a number resolves against the font size).
fn line_height(style: &ComputedStyle, value: &str) -> bool {
    match (style.line_height, value) {
        (LineHeight::Normal, "normal") => true,
        (LineHeight::Normal, _) | (_, "normal") => false,
        (LineHeight::Length(length), value) => length == px(value),
        (LineHeight::Number(number), value) => number * style.font_size == px(value),
        (LineHeight::Percentage(percent), value) => percent / 100.0 * style.font_size == px(value),
    }
}

/// Div's value for one probe, and whether it equals Chromium's.
fn probe(style: &ComputedStyle, name: &str, value: &str) -> (bool, String) {
    if name.starts_with("--") {
        // `getPropertyValue` gives "" both for an empty value and for none.
        let div = style.custom_property(name).unwrap_or("");
        return (div == value, format!("{div:?}"));
    }
    match name {
        "background-color" => (
            bytes(style.background_color) == color(value),
            format!("{:?}", style.background_color),
        ),
        "border-top-width" => (
            style.effective_border_top() as f32 == px(value),
            style.effective_border_top().to_string(),
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
        "line-height" => (
            line_height(style, value),
            format!("{:?}", style.line_height),
        ),
        "margin-left" => (
            style.margin_left == Length::px(px(value)),
            format!("{:?}", style.margin_left),
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
        "padding-top" => (
            style.padding_top == Length::px(px(value)),
            format!("{:?}", style.padding_top),
        ),
        other => panic!("no comparison for probe `{other}`"),
    }
}

#[test]
fn custom_properties_compute_as_chromium_computes_them() {
    let oracle: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("chromium-windows.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let cases = oracle["cases"].as_object().unwrap();
    let sheets = sheets();
    assert_eq!(
        cases.keys().map(String::as_str).collect::<Vec<_>>(),
        sheets.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "every captured case has its sheet here"
    );
    let mut differences = Vec::new();
    let mut compared = 0;
    for (name, sheet) in sheets {
        let document = Document::new(200, 200).unwrap();
        let html = std::fs::read_to_string(fixtures().join(format!("{name}.html"))).unwrap();
        let ids = build(&document, &html);
        document.adopt_stylesheet(sheet).unwrap();
        let expected = cases[name]["elements"].as_object().unwrap();
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
            for (probe_name, value) in &values {
                compared += 1;
                let (same, div) = probe(&style, probe_name, value);
                if !same {
                    differences.push((
                        name,
                        id.clone(),
                        probe_name.clone(),
                        format!("Chromium {value:?}, Div {div}"),
                    ));
                }
            }
        }
    }
    let unexpected: Vec<_> = differences
        .iter()
        .filter(|(case, id, probe, _)| {
            !GAPS
                .iter()
                .any(|gap| gap.0 == *case && gap.1 == id && gap.2 == probe)
        })
        .map(|(case, id, probe, what)| format!("{case} #{id} {probe}: {what}"))
        .collect();
    assert!(
        unexpected.is_empty(),
        "{} differences outside the named gaps:\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );
    for (case, id, probe, reason) in GAPS {
        assert!(
            differences.iter().any(|difference| difference.0 == *case
                && difference.1 == *id
                && difference.2 == *probe),
            "{case} #{id} {probe} is a named gap ({reason}) but now agrees with Chromium"
        );
    }
    assert!(compared > 0);
}
