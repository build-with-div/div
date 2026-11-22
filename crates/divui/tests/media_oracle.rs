//! The media-queries-v1 oracle: Div applies `@media` rules of a
//! `stylesheet!` sheet where pinned Chromium 147 applies them.
//!
//! `tests/fidelity/fixtures/media-queries-v1/media.css` makes each element
//! of `media.html` opaque under one media query.
//! `tools/qualification/capture_media_oracle.py` records each element's
//! computed `opacity` in Chromium for several environments; this test sets
//! each environment on a Div document and requires the same values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use div::{stylesheet, ColorScheme, Document, Element, ViewportMetrics};

stylesheet!(
    media,
    "../../tests/fidelity/fixtures/media-queries-v1/media.css"
);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fidelity/fixtures/media-queries-v1")
}

/// The ids of `media.html`'s `<div id="...">` elements, in order.
fn ids(html: &str) -> Vec<String> {
    html.split("<div id=\"")
        .skip(1)
        .map(|rest| rest[..rest.find('"').expect("id ends")].to_owned())
        .collect()
}

#[test]
fn media_rules_apply_where_chromium_applies_them() {
    let oracle: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("chromium-windows.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let html = std::fs::read_to_string(fixtures().join("media.html")).unwrap();
    let ids = ids(&html);

    let document = Document::new(100, 100).unwrap();
    let body = document.body();
    let elements: Vec<Element> = ids
        .iter()
        .map(|id| {
            let element = Element::create(&document, "div").unwrap();
            element.set_attribute("id", id).unwrap();
            body.append_child(&element).unwrap();
            element
        })
        .collect();
    document.adopt_stylesheet(&media::SHEET).unwrap();

    let mut mismatches = Vec::new();
    let mut compared = 0;
    for environment in oracle["environments"].as_array().unwrap() {
        let name = environment["name"].as_str().unwrap();
        let metrics = ViewportMetrics::from_logical_size(
            environment["width"].as_f64().unwrap(),
            environment["height"].as_f64().unwrap(),
            environment["scale"].as_f64().unwrap(),
        )
        .unwrap();
        document.set_viewport(metrics).unwrap();
        document
            .set_color_scheme(match environment["color_scheme"].as_str().unwrap() {
                "dark" => ColorScheme::Dark,
                _ => ColorScheme::Light,
            })
            .unwrap();
        document
            .set_prefers_reduced_motion(environment["reduced_motion"].as_bool().unwrap())
            .unwrap();
        document.update_all().unwrap();
        let expected: BTreeMap<String, String> =
            serde_json::from_value(environment["opacity"].clone()).unwrap();
        assert_eq!(expected.len(), ids.len(), "{name}");
        for (id, element) in ids.iter().zip(&elements) {
            let actual = element.computed_style().unwrap().opacity;
            let chromium: f32 = expected[id].parse().unwrap();
            compared += 1;
            if actual != chromium {
                mismatches.push(format!("{name} #{id}: Div {actual}, Chromium {chromium}"));
            }
        }
    }
    assert_eq!(compared, 7 * 84);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
