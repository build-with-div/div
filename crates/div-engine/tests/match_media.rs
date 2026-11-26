//! `matchMedia()` lists in the engine (RFC 0014, step 3).
//!
//! The `match-media-v1` oracle's scripted sequences, replayed through the
//! engine API: Chromium 147's `matches` reads and `change` reports as
//! viewport, color scheme and reduced-motion inputs change, recorded over
//! CDP by `tools/qualification/capture_match_media_oracle.py`.

use std::path::Path;

use div_engine::{Engine, EngineError, MediaQueryListId, ViewportMetrics};
use div_style::media_query::ColorScheme;
use serde_json::{json, Value};

/// Sequences Div cannot replay, with the reason.
const GAPS: &[(&str, &str)] = &[(
    "scheme-and-motion-together",
    "CDP changes the color scheme and reduced motion in one command, which \
     Chromium evaluates at the next frame; Div has one setter per input, and \
     the reduced-motion setter evaluates at once",
)];

fn oracle() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fidelity/fixtures/match-media-v1/chromium-windows.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn viewport(width: f64, scale: f64) -> ViewportMetrics {
    // The sequences run in the oracle's "boundary" environment: 600 x 400.
    ViewportMetrics::from_logical_size(width, 400.0, scale).unwrap()
}

fn scheme(name: &str) -> ColorScheme {
    match name {
        "dark" => ColorScheme::Dark,
        _ => ColorScheme::Light,
    }
}

/// Replay one sequence and return its steps in the oracle's form.
fn replay(steps: &[Value]) -> Vec<Value> {
    let (mut width, mut scale) = (600.0, 1.0);
    let mut engine = Engine::new(viewport(width, scale)).unwrap();
    let mut lists: Vec<MediaQueryListId> = Vec::new();
    let mut events_so_far = 0;
    let mut out = Vec::new();
    for step in steps {
        let step = step.as_object().unwrap();
        if let Some(query) = step.get("make") {
            let id = engine.match_media(query.as_str().unwrap());
            lists.push(id);
            out.push(json!({
                "make": query,
                "media": engine.media_query_list_media(id).unwrap(),
                "matches": engine.media_query_list_matches(id).unwrap(),
            }));
        } else if step.contains_key("scheme") || step.contains_key("reduced_motion") {
            if let Some(name) = step.get("scheme") {
                engine.set_color_scheme(scheme(name.as_str().unwrap()));
            }
            if let Some(reduced) = step.get("reduced_motion") {
                engine.set_prefers_reduced_motion(reduced.as_bool().unwrap());
            }
            out.push(Value::Object(step.clone()));
        } else if step.contains_key("width") || step.contains_key("scale") {
            if let Some(value) = step.get("width") {
                width = value.as_f64().unwrap();
            }
            if let Some(value) = step.get("scale") {
                scale = value.as_f64().unwrap();
            }
            engine.set_viewport(viewport(width, scale)).unwrap();
            out.push(Value::Object(step.clone()));
        } else if step.contains_key("read") {
            let matches: Vec<bool> = lists
                .iter()
                .map(|&id| engine.media_query_list_matches(id).unwrap())
                .collect();
            out.push(json!({ "read": matches, "events_so_far": events_so_far }));
        } else if let Some(index) = step.get("read_one") {
            let id = lists[index.as_u64().unwrap() as usize];
            out.push(json!({
                "read_one": index,
                "matches": engine.media_query_list_matches(id).unwrap(),
            }));
        } else if step.contains_key("frame") {
            let events: Vec<Value> = engine
                .evaluate_media_queries_and_report_changes()
                .into_iter()
                .map(|(id, matches)| {
                    json!({
                        "list": lists.iter().position(|&list| list == id).unwrap(),
                        "matches": matches,
                        "media": engine.media_query_list_media(id).unwrap(),
                    })
                })
                .collect();
            events_so_far += events.len();
            out.push(json!({ "frame": events }));
        } else {
            panic!("unknown step {step:?}");
        }
    }
    out
}

#[test]
fn match_media_reports_changes_when_chromium_does() {
    let oracle = oracle();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let sequences = oracle["sequences"].as_object().unwrap();
    let mut replayed = 0;
    for (name, sequence) in sequences {
        let expected = sequence["steps"].as_array().unwrap();
        let actual = replay(expected);
        let same = actual.as_slice() == expected.as_slice();
        if let Some((_, reason)) = GAPS.iter().find(|(gap, _)| gap == name) {
            assert!(
                !same,
                "{name} is listed as a gap ({reason}) but now matches"
            );
            continue;
        }
        assert_eq!(
            actual, *expected,
            "sequence {name}: Div's steps differ from Chromium's"
        );
        replayed += 1;
    }
    assert_eq!(replayed + GAPS.len(), sequences.len());
    assert_eq!(replayed, 23);
}

#[test]
fn a_released_list_is_stale_and_never_reported() {
    let mut engine = Engine::new(viewport(600.0, 1.0)).unwrap();
    let kept = engine.match_media("(min-width: 700px)");
    let released = engine.match_media("(min-width: 700px)");
    engine.release_media_query_list(released).unwrap();
    assert!(matches!(
        engine.media_query_list_matches(released),
        Err(EngineError::StaleHandle)
    ));
    assert!(matches!(
        engine.release_media_query_list(released),
        Err(EngineError::StaleHandle)
    ));
    engine.set_viewport(viewport(800.0, 1.0)).unwrap();
    assert_eq!(
        engine.evaluate_media_queries_and_report_changes(),
        vec![(kept, true)]
    );
    assert_eq!(engine.evaluate_media_queries_and_report_changes(), vec![]);
}

#[test]
fn invalid_text_is_not_all() {
    let mut engine = Engine::new(viewport(600.0, 1.0)).unwrap();
    let id = engine.match_media("screen and");
    assert_eq!(engine.media_query_list_media(id).unwrap(), "not all");
    assert!(!engine.media_query_list_matches(id).unwrap());
    let empty = engine.match_media("");
    assert_eq!(engine.media_query_list_media(empty).unwrap(), "");
    assert!(engine.media_query_list_matches(empty).unwrap());
}
