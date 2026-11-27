//! `change` events of `matchMedia()` lists: [`MediaQueryList::on_change`]
//! listeners, fired by [`Document::begin_frame`] at HTML's "evaluate media
//! queries and report changes" step.
//!
//! The `match-media-v1` oracle's scripted sequences replay here through the
//! public API, with real listeners and frames, as `div-engine`'s
//! `match_media` test replays them through the engine. The other tests pin
//! the DOM's listener rules and the step's place in the frame.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use div::{ColorScheme, Document, MediaQueryList, MediaQueryListEvent, ViewportMetrics};
use serde_json::{json, Value};

/// Sequences Div cannot replay, with the reason. The same gap as
/// `div-engine`'s `match_media` test.
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

type Fired = Rc<RefCell<Vec<(usize, MediaQueryListEvent)>>>;

/// Replay one sequence through a document and return its steps in the
/// oracle's form.
fn replay(steps: &[Value]) -> Vec<Value> {
    let (mut width, mut scale) = (600.0, 1.0);
    let document = Document::with_viewport_metrics(viewport(width, scale)).unwrap();
    let fired: Fired = Rc::default();
    let mut lists: Vec<MediaQueryList> = Vec::new();
    let mut events_so_far = 0;
    let mut time = 0.0;
    let mut out = Vec::new();
    for step in steps {
        let step = step.as_object().unwrap();
        if let Some(query) = step.get("make") {
            let list = document.match_media(query.as_str().unwrap()).unwrap();
            let index = lists.len();
            let sink = fired.clone();
            list.on_change(move |event| sink.borrow_mut().push((index, event.clone())))
                .unwrap();
            out.push(json!({
                "make": query,
                "media": list.media(),
                "matches": list.matches().unwrap(),
            }));
            lists.push(list);
        } else if step.contains_key("scheme") || step.contains_key("reduced_motion") {
            if let Some(name) = step.get("scheme") {
                document
                    .set_color_scheme(scheme(name.as_str().unwrap()))
                    .unwrap();
            }
            if let Some(reduced) = step.get("reduced_motion") {
                document
                    .set_prefers_reduced_motion(reduced.as_bool().unwrap())
                    .unwrap();
            }
            out.push(Value::Object(step.clone()));
        } else if step.contains_key("width") || step.contains_key("scale") {
            if let Some(value) = step.get("width") {
                width = value.as_f64().unwrap();
            }
            if let Some(value) = step.get("scale") {
                scale = value.as_f64().unwrap();
            }
            document.set_viewport(viewport(width, scale)).unwrap();
            out.push(Value::Object(step.clone()));
        } else if step.contains_key("read") {
            let matches: Vec<bool> = lists.iter().map(|list| list.matches().unwrap()).collect();
            out.push(json!({ "read": matches, "events_so_far": events_so_far }));
        } else if let Some(index) = step.get("read_one") {
            let list = &lists[index.as_u64().unwrap() as usize];
            out.push(json!({
                "read_one": index,
                "matches": list.matches().unwrap(),
            }));
        } else if step.contains_key("frame") {
            time += 16.0;
            document.begin_frame(time).unwrap();
            let events: Vec<Value> = fired
                .borrow_mut()
                .drain(..)
                .map(|(list, event)| {
                    json!({
                        "list": list,
                        "matches": event.matches(),
                        "media": event.media(),
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
fn change_events_fire_when_chromium_fires_them() {
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
            "sequence {name}: Div's events differ from Chromium's"
        );
        replayed += 1;
    }
    assert_eq!(replayed + GAPS.len(), sequences.len());
    assert_eq!(replayed, 23);
}

fn document() -> Document {
    Document::with_viewport_metrics(viewport(600.0, 1.0)).unwrap()
}

fn widen(document: &Document) {
    document.set_viewport(viewport(800.0, 1.0)).unwrap();
}

#[test]
fn a_dropped_list_keeps_listening_until_its_listener_is_removed() {
    let document = document();
    let fired = Rc::new(RefCell::new(Vec::new()));
    let sink = fired.clone();
    let handle = document
        .match_media("(min-width: 700px)")
        .unwrap()
        .on_change(move |event| sink.borrow_mut().push(event.matches()))
        .unwrap();
    widen(&document);
    document.begin_frame(16.0).unwrap();
    assert_eq!(
        *fired.borrow(),
        [true],
        "the document keeps a listened list"
    );

    handle.remove().unwrap();
    document.set_viewport(viewport(600.0, 1.0)).unwrap();
    document.begin_frame(32.0).unwrap();
    assert_eq!(*fired.borrow(), [true], "a removed listener is not called");
    handle.remove().unwrap();
}

#[test]
fn listeners_follow_the_doms_dispatch_rules() {
    let document = document();
    let list = document.match_media("(min-width: 700px)").unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let second: Rc<RefCell<Option<div::ListenerHandle>>> = Rc::default();

    // The first listener removes the second and adds a third: the second is
    // not called, and the third waits for the next event.
    let (log, victim, adder) = (calls.clone(), second.clone(), list.clone());
    list.on_change(move |_| {
        log.borrow_mut().push("first");
        if let Some(handle) = victim.borrow_mut().take() {
            handle.remove().unwrap();
            let log = log.clone();
            adder
                .on_change(move |_| log.borrow_mut().push("third"))
                .unwrap();
        }
    })
    .unwrap();
    let log = calls.clone();
    *second.borrow_mut() = Some(
        list.on_change(move |_| log.borrow_mut().push("second"))
            .unwrap(),
    );

    widen(&document);
    document.begin_frame(16.0).unwrap();
    assert_eq!(*calls.borrow(), ["first"]);
    document.set_viewport(viewport(600.0, 1.0)).unwrap();
    document.begin_frame(32.0).unwrap();
    assert_eq!(*calls.borrow(), ["first", "first", "third"]);
}

#[test]
fn lists_report_oldest_first_and_each_listener_sees_its_list() {
    let document = document();
    let fired = Rc::new(RefCell::new(Vec::new()));
    let narrow = document.match_media("(max-width: 700px)").unwrap();
    let wide = document.match_media("(min-width: 700px)").unwrap();
    // Listen to the newer list first: the report still goes oldest first.
    for list in [&wide, &narrow] {
        let sink = fired.clone();
        list.on_change(move |event| {
            sink.borrow_mut()
                .push((event.media().to_owned(), event.matches()))
        })
        .unwrap();
    }
    widen(&document);
    document.begin_frame(16.0).unwrap();
    assert_eq!(
        *fired.borrow(),
        [
            ("(max-width: 700px)".to_owned(), false),
            ("(min-width: 700px)".to_owned(), true),
        ]
    );
}

#[test]
fn change_events_fire_before_animation_frame_callbacks() {
    let document = document();
    let order = Rc::new(RefCell::new(Vec::new()));
    let log = order.clone();
    document
        .match_media("(prefers-reduced-motion)")
        .unwrap()
        .on_change(move |_| log.borrow_mut().push("change"))
        .unwrap();
    let log = order.clone();
    document.request_animation_frame(move |_| log.borrow_mut().push("animation frame"));
    document.set_prefers_reduced_motion(true).unwrap();
    document.begin_frame(16.0).unwrap();
    assert_eq!(*order.borrow(), ["change", "animation frame"]);
}

#[test]
fn a_listener_can_use_the_document_during_the_report() {
    let document = document();
    let dark = document
        .match_media("(prefers-color-scheme: dark)")
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (sink, target, list) = (seen.clone(), document.clone(), dark.clone());
    dark.on_change(move |event| {
        // Reading during the dispatch, and changing an input, which the next
        // frame reports.
        sink.borrow_mut()
            .push((event.matches(), list.matches().unwrap()));
        target.set_color_scheme(ColorScheme::Light).unwrap();
    })
    .unwrap();
    document.set_color_scheme(ColorScheme::Dark).unwrap();
    document.begin_frame(16.0).unwrap();
    assert_eq!(*seen.borrow(), [(true, true)]);
    document.begin_frame(32.0).unwrap();
    assert_eq!(*seen.borrow(), [(true, true), (false, false)]);
}

#[test]
fn a_listener_outliving_its_document_is_stale() {
    let document = document();
    let list = document.match_media("(min-width: 700px)").unwrap();
    let handle = list.on_change(|_| {}).unwrap();
    drop(document);
    assert!(list.on_change(|_| {}).is_err());
    handle.remove().unwrap();
}
