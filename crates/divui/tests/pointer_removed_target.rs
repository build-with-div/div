//! A listener that removes the pointer's target, against pinned Chromium 147.
//!
//! The oracle is `tests/fidelity/fixtures/pointer-removed-target-v1`:
//! `capture.py` drives `page.html` in Chrome-for-Testing 147.0.7727.50 on
//! Linux with CDP mouse and touch input, and writes the result only when three
//! runs agree. The page nests three focusable boxes, and a listener on the
//! target removes the target or its parent during the named event. Enter and
//! leave events are not recorded, so they are not compared.
//!
//! Two differences are named gaps, and the comparison fails if either closes:
//!
//! - `focusout-on-removal` (framework): Chromium fires `focusout` when the
//!   focused element is removed. Div clears focus without an event, so
//!   [`FOCUSOUT_GAP`] is left out of the expected sequence.
//! - `hover-after-dom-change` (framework, see `pointer_events_oracle.rs`):
//!   after a mouse release that removed the target, Chromium fires
//!   `pointerover` and `mouseover` at the surviving element without new
//!   input. Div fires them at the pointer's next event, so [`expected`]
//!   checks that those two entries end the recording and leaves them out.

use div::prelude::*;
use div::{PointerInput, PointerInputKind};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/pointer-removed-target-v1/chromium-linux.json");
const TOUCH: PointerId = PointerId(2);
const FOCUSOUT_GAP: &str = "focusout:child";

/// The event types `page.html` records.
const EVENTS: &[&str] = &[
    "pointerover",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointerout",
    "gotpointercapture",
    "lostpointercapture",
    "mouseover",
    "mousedown",
    "mousemove",
    "mouseup",
    "mouseout",
    "click",
    "auxclick",
    "dblclick",
    "contextmenu",
    "focusin",
    "focusout",
];

struct Scenario {
    when: String,
    remove: String,
    log: Vec<String>,
    focus: String,
}

fn scenarios(group: &str, count: usize) -> Vec<Scenario> {
    let oracle: serde_json::Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    assert_eq!(oracle["repeats"], 3);
    let recorded = oracle[group].as_object().unwrap();
    assert_eq!(recorded.len(), count, "{group} scenarios");
    recorded
        .iter()
        .map(|(key, value)| {
            let (when, remove) = key.split_once('/').unwrap();
            Scenario {
                when: when.to_owned(),
                remove: remove.to_owned(),
                log: value["log"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| entry.as_str().unwrap().to_owned())
                    .collect(),
                focus: value["focus"].as_str().unwrap().to_owned(),
            }
        })
        .collect()
}

/// Chromium's sequence without the two named gaps.
fn expected(scenario: &Scenario, touch: bool) -> Vec<String> {
    let mut log: Vec<String> = scenario
        .log
        .iter()
        .filter(|entry| *entry != FOCUSOUT_GAP)
        .cloned()
        .collect();
    if !touch && matches!(scenario.when.as_str(), "pointerup" | "mouseup") {
        let survivor = if scenario.remove == "child" {
            "parent"
        } else {
            "grand"
        };
        let hover = log.split_off(log.len() - 2);
        assert_eq!(
            hover,
            [
                format!("pointerover:{survivor}"),
                format!("mouseover:{survivor}")
            ],
            "the hover-after-dom-change gap in {} removing {}",
            scenario.when,
            scenario.remove
        );
    }
    log
}

fn focusable_box(document: &Document, parent: &Element) -> Element {
    let element = Element::create(document, "div").unwrap();
    element.set_position(Position::Absolute).unwrap();
    element.set_left(Length::px(0.0)).unwrap();
    element.set_top(Length::px(0.0)).unwrap();
    element.set_width(LengthValue::px(100.0)).unwrap();
    element.set_height(LengthValue::px(50.0)).unwrap();
    element.set_attribute("tabindex", "0").unwrap();
    parent.append_child(&element).unwrap();
    element
}

fn run(scenario: &Scenario, touch: bool) -> (Vec<String>, &'static str) {
    let document = Document::new(800, 600).unwrap();
    let root = document.body();
    let grand = focusable_box(&document, &root);
    let parent = focusable_box(&document, &grand);
    let child = focusable_box(&document, &parent);
    document.update_all().unwrap();
    let boxes = [
        ("#document", root),
        ("grand", grand),
        ("parent", parent),
        ("child", child),
    ];
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let recording = Rc::new(Cell::new(false));
    for (name, element) in &boxes {
        for event_type in EVENTS {
            let (log, recording, name) = (log.clone(), recording.clone(), *name);
            element
                .on(event_type, move |event| {
                    if recording.get() && event.phase() == Some(EventPhase::Target) {
                        log.borrow_mut().push(format!("{event_type}:{name}"));
                    }
                })
                .unwrap();
        }
    }
    let doomed = boxes
        .iter()
        .find(|(name, _)| *name == scenario.remove)
        .map(|(_, element)| element.clone())
        .unwrap();
    let removed = Rc::new(Cell::new(false));
    for (name, element) in &boxes {
        if *name != "parent" && *name != "child" {
            continue;
        }
        let (doomed, removed, recording) = (doomed.clone(), removed.clone(), recording.clone());
        element
            .on(&scenario.when, move |_| {
                if recording.get() && !removed.replace(true) {
                    doomed.remove().unwrap();
                }
            })
            .unwrap();
    }
    let send = |kind| {
        let input = if touch {
            PointerInput::touch(TOUCH, kind, 50.0, 25.0)
        } else {
            PointerInput::mouse(kind, 50.0, 25.0)
        };
        document.dispatch_pointer_input(input).unwrap();
    };
    if !touch {
        send(PointerInputKind::Move);
    }
    recording.set(true);
    send(PointerInputKind::Down);
    send(PointerInputKind::Up);
    let gone = |name: &str| {
        name == "#document"
            || name == scenario.remove
            || (scenario.remove == "parent" && name == "child")
    };
    let focus = boxes
        .iter()
        .filter(|(name, _)| !gone(name))
        .find(|(_, element)| element.has_focus().unwrap())
        .map_or("body", |(name, _)| *name);
    let log = log.borrow().clone();
    (log, focus)
}

fn compare(scenarios: &[Scenario], touch: bool) {
    let mut failures = Vec::new();
    for scenario in scenarios {
        let expected = expected(scenario, touch);
        let (log, focus) = run(scenario, touch);
        if log != expected || focus != scenario.focus {
            failures.push(format!(
                "{} removing {}:\n  expected {:?} focus {}\n  got      {:?} focus {}",
                scenario.when, scenario.remove, expected, scenario.focus, log, focus
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_mouse_step_continues_at_the_surviving_element() {
    compare(&scenarios("mouse", 8), false);
}

#[test]
fn a_tap_continues_at_the_surviving_element() {
    compare(&scenarios("tap", 4), true);
}
