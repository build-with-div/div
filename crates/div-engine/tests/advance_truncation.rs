//! Div lays out long single-font texts at pinned Chromium 147's widths (T3,
//! T8).
//!
//! Blink hands HarfBuzz each glyph advance truncated to 1/65536 px
//! (`SkiaScalarToHarfBuzzPosition`) and sums a run's advances in fixed point;
//! `div_text` shapes with HarfBuzz the same way (RFC 0013). One glyph moves by
//! at most 1/65536 px, so each case repeats a short text until those units add
//! up to layout units.
//!
//! Oracle: `tests/fidelity/fixtures/advance-truncation-v1/chromium-windows.json`,
//! captured by `tools/qualification/capture_advance_truncation_oracle.py`:
//! each case is one inline-block span with `white-space: pre`, in a web font
//! from this repository, at one CSS size. Windows only: Linux's FreeType
//! advances sit on 1/64 px, and the oracle is Windows Chrome's.
//!
//! `known-gaps.txt` beside the oracle lists the cases that still differ, with
//! Div's width; since T8 it lists none.

#![cfg(windows)]

use std::collections::BTreeMap;
use std::path::Path;

use div_dom::ElementTag;
use div_engine::{Engine, FontCollection, NodeHandle, ViewportMetrics};
use div_style::{
    Display, FontFamily, FontFamilyList, LengthValue, StyleProperty, TextWrapMode,
    WhiteSpaceCollapse, WhiteSpaceShorthand,
};
use div_text::FontFaceDescriptor;
use serde_json::Value;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/advance-truncation-v1/chromium-windows.json");
const KNOWN_GAPS: &str =
    include_str!("../../../tests/fidelity/fixtures/advance-truncation-v1/known-gaps.txt");

/// `(font, text, size)` to Div's listed width.
fn known_gaps() -> BTreeMap<(String, String, String), f32> {
    KNOWN_GAPS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split(" | ");
            let font = fields.next().unwrap().to_owned();
            let text = fields.next().unwrap().to_owned();
            let (size, width) = fields.next().unwrap().split_once(' ').unwrap();
            assert!(fields.next().is_none(), "{line}");
            let key = (font, text, size.to_owned());
            (key, width.parse().unwrap())
        })
        .collect()
}

fn engine(oracle: &Value, font: &str) -> Engine {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(oracle["fonts"][font].as_str().expect("font path"));
    let bytes = std::fs::read(&path).expect("font bytes");
    let fonts = FontCollection::deterministic_test();
    fonts
        .register(bytes.into(), FontFaceDescriptor::new("Probe"))
        .expect("register the probe face");
    let mut engine = Engine::new_with_font_collection(
        ViewportMetrics::from_logical_size(800.0, 600.0, 1.0).unwrap(),
        fonts,
    )
    .unwrap();
    let root = engine.root();
    engine
        .set_property(
            root,
            StyleProperty::FontFamily,
            FontFamilyList {
                families: vec![FontFamily::Named("Probe".into())],
            }
            .into(),
        )
        .unwrap();
    engine
        .set_property(
            root,
            StyleProperty::WhiteSpace,
            WhiteSpaceShorthand {
                collapse: WhiteSpaceCollapse::Preserve,
                wrap: TextWrapMode::Nowrap,
            }
            .into(),
        )
        .unwrap();
    engine
}

fn span(engine: &mut Engine, text: &str, size: f32) -> NodeHandle {
    let block = engine.create_native_element(ElementTag::Div).unwrap();
    engine.append_child(engine.root(), block).unwrap();
    let span = engine.create_native_element(ElementTag::Span).unwrap();
    engine
        .set_property(span, StyleProperty::Display, Display::InlineBlock.into())
        .unwrap();
    engine
        .set_property(span, StyleProperty::FontSize, LengthValue::px(size).into())
        .unwrap();
    engine.append_child(block, span).unwrap();
    let node = engine.create_text(text).unwrap();
    engine.append_child(span, node).unwrap();
    span
}

#[test]
fn long_text_widths_match_chromium() {
    let oracle: Value = serde_json::from_str(ORACLE).expect("oracle JSON");
    let texts = oracle["texts"].as_array().unwrap();
    let mut gaps = known_gaps();
    let mut compared = 0;
    let mut failures = Vec::new();
    for run in oracle["runs"].as_array().unwrap() {
        let font = run["font"].as_str().unwrap();
        let name = run["text"].as_str().unwrap();
        let entry = texts.iter().find(|text| text["name"] == name).unwrap();
        let text = entry["unit"]
            .as_str()
            .unwrap()
            .repeat(entry["count"].as_u64().unwrap() as usize);
        let mut engine = engine(&oracle, font);
        let cases = run["cases"].as_array().unwrap();
        let spans: Vec<_> = cases
            .iter()
            .map(|case| {
                let size: f32 = case["size"].as_str().unwrap().parse().unwrap();
                span(&mut engine, &text, size)
            })
            .collect();
        for (case, span) in cases.iter().zip(spans) {
            let size = case["size"].as_str().unwrap();
            let chromium = case["width"].as_f64().unwrap() as f32;
            let div = engine.bounds(span).unwrap().unwrap().width;
            compared += 1;
            let key = (font.to_owned(), name.to_owned(), size.to_owned());
            match gaps.remove(&key) {
                None if div != chromium => failures.push(format!(
                    "{font} | {name} | {size} {div}: unlisted, Chromium {chromium}"
                )),
                Some(_) if div == chromium => {
                    failures.push(format!("{font} | {name} | {size}: listed but now exact"))
                }
                Some(listed) if listed != div => failures.push(format!(
                    "{font} | {name} | {size} {div}: listed as {listed}, Chromium {chromium}"
                )),
                _ => {}
            }
        }
    }
    assert_eq!(compared, 160);
    for (font, text, size) in gaps.keys() {
        failures.push(format!(
            "{font} | {text} | {size}: listed but not in the oracle"
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
