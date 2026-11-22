//! `calc()` in `line-height` against pinned Chromium 147 (Tier 0 item 0.2).
//!
//! `tools/qualification/capture_calc_oracle.py` recorded, in the pinned
//! Windows Chrome-for-Testing at device scale 1, how Chromium reads 328
//! `line-height` values at a font size each: 8 Tailwind text sizes, then 82
//! literals and `calc()` expressions at 14px, 16px, 13.5px and 15.75px. For
//! each it reports whether the inline style kept the value, the height of a
//! block holding one line of Arial text, and the height of an empty block of
//! the same style holding a child of `height: 1lh` (the computed line height
//! in layout units). Each case sits in its own 80px block, so none starts at
//! a fractional position.
//!
//! This test builds the same page through the public API, parsing each
//! value with Div's literal parser, and requires the same acceptance and
//! the same two heights, exactly, except for the gaps named below.
#![cfg(target_os = "windows")]

use div::prelude::*;
use div::typed_style::parse_literal;
use serde_json::Value;

const ORACLE: &str = include_str!("../../../tests/fidelity/fixtures/calc-v1/chromium-windows.json");

// Cases that differ from Chromium: (line-height, font size, what differs).
const GAPS: &[(&str, f64, &str)] = &[
    // div-style stores a line-height number as an f32 number, where Blink
    // stores the f32 of the number times 100 as a percentage; the product
    // with the font size then truncates one layout unit lower.
    (
        "1.4285714",
        14.0,
        "height 19.984375 (Chromium 20); 1lh 19.984375 (Chromium 20)",
    ),
    (
        "calc(1 / 3)",
        13.5,
        "height 4.5 (Chromium 4.484375); 1lh 4.5 (Chromium 4.484375)",
    ),
    (
        "1.4285714",
        15.75,
        "height 22.484375 (Chromium 22.5); 1lh 22.484375 (Chromium 22.5)",
    ),
    // An infinite line height saturates the layout unit; Chromium reports
    // the saturated box as 33554428px, Div as 33554432px (div-layout).
    ("calc(1 / 0)", 14.0, "1lh 33554432 (Chromium 33554428)"),
    (
        "calc(1px / 0)",
        14.0,
        "height 33554432 (Chromium 33554428); 1lh 33554432 (Chromium 33554428)",
    ),
    ("calc(1 / 0)", 16.0, "1lh 33554432 (Chromium 33554428)"),
    (
        "calc(1px / 0)",
        16.0,
        "height 33554432 (Chromium 33554428); 1lh 33554432 (Chromium 33554428)",
    ),
    ("calc(1 / 0)", 13.5, "1lh 33554432 (Chromium 33554428)"),
    (
        "calc(1px / 0)",
        13.5,
        "height 33554432 (Chromium 33554428); 1lh 33554432 (Chromium 33554428)",
    ),
    ("calc(1 / 0)", 15.75, "1lh 33554432 (Chromium 33554428)"),
    (
        "calc(1px / 0)",
        15.75,
        "height 33554432 (Chromium 33554428); 1lh 33554432 (Chromium 33554428)",
    ),
];

fn set(element: &Element, property: StyleProperty, literal: &str) {
    element
        .set_property(property, parse_literal(property, literal).unwrap())
        .unwrap();
}

#[test]
fn calc_line_heights_match_chromium() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let cases = oracle["cases"].as_array().unwrap();
    let document = Document::new(400, 300).unwrap();
    let body = document.body();
    set(&body, StyleProperty::Margin, "0px");
    set(&body, StyleProperty::FontFamily, "Arial");
    let mut blocks = Vec::new();
    for case in cases {
        let font_size = case["font_size"].as_f64().unwrap();
        let line_height = case["line_height"].as_str().unwrap();
        let accepted = parse_literal(StyleProperty::LineHeight, line_height).ok();
        let style = |element: &Element| {
            set(element, StyleProperty::FontSize, &format!("{font_size}px"));
            if let Some(value) = &accepted {
                element
                    .set_property(StyleProperty::LineHeight, value.clone())
                    .unwrap();
            }
        };
        let wrapper = Element::create(&document, "div").unwrap();
        set(&wrapper, StyleProperty::Height, "80px");
        body.append_child(&wrapper).unwrap();
        let text = Element::create(&document, "div").unwrap();
        style(&text);
        text.append_text_node("x").unwrap();
        wrapper.append_child(&text).unwrap();
        let probe = Element::create(&document, "div").unwrap();
        style(&probe);
        let child = Element::create(&document, "div").unwrap();
        set(&child, StyleProperty::Height, "1lh");
        probe.append_child(&child).unwrap();
        wrapper.append_child(&probe).unwrap();
        blocks.push((accepted.is_some(), text, probe));
    }
    let mut differences = Vec::new();
    for (case, (accepted, text, probe)) in cases.iter().zip(&blocks) {
        let font_size = case["font_size"].as_f64().unwrap();
        let line_height = case["line_height"].as_str().unwrap();
        let kept = !case["declared"].as_str().unwrap().is_empty();
        let height = text.bounding_rect().unwrap().unwrap().height as f64;
        let lh = probe.bounding_rect().unwrap().unwrap().height as f64;
        let mut what = Vec::new();
        if *accepted != kept {
            what.push(format!("Div accepted {accepted}, Chromium kept {kept}"));
        }
        let expected = case["height"].as_f64().unwrap();
        if height != expected {
            what.push(format!("height {height} (Chromium {expected})"));
        }
        let expected = case["lh"].as_f64().unwrap();
        if lh != expected {
            what.push(format!("1lh {lh} (Chromium {expected})"));
        }
        if !what.is_empty() {
            differences.push((line_height.to_string(), font_size, what.join("; ")));
        }
    }
    let unexpected: Vec<_> = differences
        .iter()
        .filter(|(value, size, what)| {
            !GAPS
                .iter()
                .any(|gap| gap.0 == value && gap.1 == *size && gap.2 == what)
        })
        .collect();
    let fixed: Vec<_> = GAPS
        .iter()
        .filter(|gap| {
            !differences
                .iter()
                .any(|(value, size, what)| gap.0 == value && gap.1 == *size && gap.2 == what)
        })
        .collect();
    assert!(
        unexpected.is_empty() && fixed.is_empty(),
        "{} of {} cases differ from Chromium.\nUnexpected: {unexpected:#?}\nGaps that no longer differ: {fixed:#?}",
        differences.len(),
        cases.len()
    );
}
