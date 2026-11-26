//! The match-media-v1 oracle: `Document::match_media` serializes and
//! evaluates media queries as pinned Chromium 147's `matchMedia()` does.
//!
//! `tests/fidelity/fixtures/match-media-v1/queries.json` lists the query
//! texts: every `@media` prelude of `media-queries-v1` and edge cases of
//! parsing and serialization. `tools/qualification/capture_match_media_oracle.py`
//! records `matchMedia(query).media` and `.matches` in Chromium for each
//! environment of `media-queries-v1`; this test sets each environment on a
//! Div document and requires the same values. The scripted change
//! sequences replay in `div-engine`'s `match_media` test.

use std::path::Path;

use div::{ColorScheme, Document, ViewportMetrics};

/// Queries where Div differs from Chromium, with the reason. Every other
/// query must match in `media` and in every environment's `matches`. All
/// are owned by `div-style`'s media query evaluator, and `stylesheet!`
/// rejects each of them at compile time.
const GAPS: &[(&str, &str)] = &[
    ("(color)", "the `color` feature is not evaluated (unknown, so false)"),
    ("((color))", "the `color` feature is not evaluated"),
    (
        "(  color  )",
        "the `color` feature is not evaluated; as `<general-enclosed>` its          text keeps one space each side, where Chromium writes the feature",
    ),
    ("(min-color: 8)", "the `color` feature is not evaluated"),
    ("(min-width: 2ex)", "`ex` is not a supported media query unit"),
    ("(min-width: 2ch)", "`ch` is not a supported media query unit"),
    ("(min-width: 2lh)", "`lh` is not a supported media query unit"),
    ("(min-width: 2dvw)", "dynamic viewport units are not supported"),
    ("(min-width: calc(10px))", "`calc()` in a media query is not evaluated"),
    (
        "screen and (min-width: calc(100px + 2em))",
        "`calc()` in a media query is not evaluated or simplified; Chromium          writes `calc(2em + 100px)`",
    ),
];

#[test]
fn match_media_serializes_and_matches_as_chromium_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fidelity/fixtures/match-media-v1/chromium-windows.json");
    let oracle: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let queries = oracle["queries"].as_array().unwrap();

    let document = Document::new(100, 100).unwrap();
    let lists: Vec<_> = queries
        .iter()
        .map(|query| {
            document
                .match_media(query["query"].as_str().unwrap())
                .unwrap()
        })
        .collect();

    let mut differences: Vec<(String, String)> = Vec::new();
    for (query, list) in queries.iter().zip(&lists) {
        let expected = query["media"].as_str().unwrap();
        if list.media() != expected {
            differences.push((
                query["query"].as_str().unwrap().to_owned(),
                format!("media {:?}, Chromium {expected:?}", list.media()),
            ));
        }
    }
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
        let expected = environment["matches"].as_str().unwrap();
        assert_eq!(expected.len(), lists.len(), "{name}");
        for ((query, list), bit) in queries.iter().zip(&lists).zip(expected.chars()) {
            compared += 1;
            let actual = list.matches().unwrap();
            if actual != (bit == '1') {
                differences.push((
                    query["query"].as_str().unwrap().to_owned(),
                    format!("{name}: matches {actual}, Chromium {}", bit == '1'),
                ));
            }
        }
    }
    assert_eq!(compared, queries.len() * 7);

    let unexpected: Vec<_> = differences
        .iter()
        .filter(|(query, _)| !GAPS.iter().any(|(gap, _)| gap == query))
        .collect();
    assert!(
        unexpected.is_empty(),
        "{} differences outside the named gaps:\n{}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|(query, what)| format!("  {query:?}: {what}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    for (gap, reason) in GAPS {
        assert!(
            differences.iter().any(|(query, _)| query == gap),
            "{gap:?} is a named gap ({reason}) but now agrees with Chromium"
        );
    }
}

#[test]
fn a_list_reads_the_documents_current_inputs() {
    let document = Document::new(600, 400).unwrap();
    let dark = document.match_media("(prefers-color-scheme: dark)").unwrap();
    let wide = document.match_media("(min-width: 700px)").unwrap();
    let copy = wide.clone();
    assert_eq!(dark.media(), "(prefers-color-scheme: dark)");
    assert!(!dark.matches().unwrap());
    assert!(!wide.matches().unwrap());

    document.set_color_scheme(ColorScheme::Dark).unwrap();
    assert!(dark.matches().unwrap());
    document
        .set_viewport(ViewportMetrics::from_logical_size(800.0, 400.0, 1.0).unwrap())
        .unwrap();
    drop(wide);
    assert!(copy.matches().unwrap(), "a clone keeps the list registered");
    assert_eq!(
        document.match_media("garbage!").unwrap().media(),
        "not all"
    );
}
