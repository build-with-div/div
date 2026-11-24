//! The focus fixup against pinned Chromium 147: what happens to focus when
//! the focused element stops being rendered or stops being focusable (H4,
//! RFC 0015).
//!
//! `tools/qualification/capture_focus_fixup_oracle.py` recorded what
//! Chrome-for-Testing 147.0.7727.50 does with real XTEST input on X11 under
//! WSL (`tests/fidelity/fixtures/focus-fixup-v1/chromium-linux-x11.json`),
//! at device scale 1. This test builds the same page in Div, replays each
//! scenario's clicks, keys and page actions, and after every step compares:
//!
//! - the input, `focus`, `blur`, `focusin` and `focusout` events in order,
//!   with their targets, keys and related targets;
//! - the `mark` records of each page action: the focused element right
//!   after the mutation (`sync`), after a forced layout (`layout`) and in
//!   the second animation frame's callbacks (`frames`), in order with the
//!   events;
//! - the focused element.
//!
//! Div's root stands for `document`, `html` and `body`, so the oracle's
//! `body` is the root. A Chromium page action is script; here each action
//! is the same mutation through `divui`, `update_all` stands for the forced
//! layout, and two `begin_frame` calls for the two animation frames. Every
//! step then runs two more frames, as Chromium's page renders frames while
//! the capture waits for its state to settle.
//!
//! `show()` and `showModal()` have no `divui` method until RFC 0015 PR 5, so
//! the two dialog scenarios are replayed by the engine
//! (`crates/div-engine/tests/focus_fixup_oracle.rs`) and listed here.

use div::prelude::*;
use div::typed_style::parse_literal;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/focus-fixup-v1/chromium-linux-x11.json");

const EVENTS: &[&str] = &[
    "keydown",
    "keyup",
    "mousedown",
    "mouseup",
    "click",
    "focus",
    "blur",
    "focusin",
    "focusout",
];

/// Replayed by the engine, without their events: they need `show()` and
/// `showModal()`, which `divui` gains in RFC 0015 PR 5.
const ENGINE_REPLAYED: &[&str] = &["remove_open_non_modal", "remove_open_modal"];

/// A frame's length in the replay; the time is not observed.
const FRAME_MS: f64 = 16.0;

type Log = Rc<RefCell<Vec<Value>>>;
type Names = Rc<Vec<(String, Element)>>;

struct Page {
    document: Document,
    elements: Names,
    log: Log,
}

/// An element's name on the oracle's page.
fn name_of(elements: &Names, element: &Element) -> String {
    elements
        .iter()
        .find(|(_, candidate)| candidate.is_same_node(element).unwrap_or(false))
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "?".into())
}

/// `document.activeElement`: the focused element, else `body`.
fn active(document: &Document, elements: &Names) -> String {
    document
        .focused_element()
        .unwrap()
        .map_or_else(|| "body".to_owned(), |element| name_of(elements, &element))
}

fn mark(log: &Log, document: &Document, elements: &Names, at: &str) {
    let active = active(document, elements);
    log.borrow_mut()
        .push(json!({"type": "mark", "at": at, "active": active}));
}

impl Page {
    fn element(&self, name: &str) -> Element {
        self.elements
            .iter()
            .find(|(id, _)| id == name)
            .unwrap_or_else(|| panic!("no element {name}"))
            .1
            .clone()
    }

    fn active(&self) -> String {
        active(&self.document, &self.elements)
    }

    fn mark(&self, at: &str) {
        mark(&self.log, &self.document, &self.elements, at);
    }

    fn frame(&self) {
        let time = self.document.get_time().unwrap();
        self.document.begin_frame(time + FRAME_MS).unwrap();
    }
}

fn set(element: &Element, property: StyleProperty, literal: &str) {
    element
        .set_property(property, parse_literal(property, literal).unwrap())
        .unwrap();
}

/// `button { box-sizing: border-box; margin: 0; padding: 0; border: 1px
/// solid #888; font: 12px sans-serif }`
fn button_style(element: &Element) {
    element.set_box_sizing(BoxSizing::BorderBox).unwrap();
    set(element, StyleProperty::Margin, "0px");
    set(element, StyleProperty::Padding, "0px");
    set(element, StyleProperty::Border, "1px solid #888888");
    set(element, StyleProperty::FontSize, "12px");
    set(element, StyleProperty::FontFamily, "sans-serif");
}

fn number(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

/// The capture script's page: the row of placed elements, `w` inside
/// `wrap`, and the two closed dialogs, in tree order.
fn page(oracle: &Value) -> Page {
    let document = Document::new(480, 400).unwrap();
    let body = document.body();
    let mut elements = vec![("body".to_owned(), body.clone())];
    for spec in oracle["row"].as_array().unwrap() {
        let spec = spec.as_array().unwrap();
        let (name, tag) = (spec[0].as_str().unwrap(), spec[1].as_str().unwrap());
        let element = Element::create(&document, tag).unwrap();
        element.set_position(Position::Absolute).unwrap();
        element.set_left(Length::px(number(&spec[2]))).unwrap();
        element.set_top(Length::px(number(&spec[3]))).unwrap();
        element
            .set_width(LengthValue::px(number(&spec[4])))
            .unwrap();
        element
            .set_height(LengthValue::px(number(&spec[5])))
            .unwrap();
        if tag == "button" {
            button_style(&element);
        } else {
            // `.abs { box-sizing: border-box; margin: 0 }`, and `#t`'s border.
            element.set_box_sizing(BoxSizing::BorderBox).unwrap();
            set(&element, StyleProperty::Margin, "0px");
            if name == "t" {
                set(&element, StyleProperty::Border, "1px solid #888888");
            }
        }
        match spec[6].as_str().unwrap() {
            "" => {}
            r#" tabindex="0""# => element.set_attribute("tabindex", "0").unwrap(),
            other => panic!("unknown attributes {other}"),
        }
        body.append_child(&element).unwrap();
        if name == "wrap" {
            let wrapped = oracle["wrapped"].as_array().unwrap();
            let inner = Element::create(&document, wrapped[1].as_str().unwrap()).unwrap();
            inner
                .set_width(LengthValue::px(number(&wrapped[2])))
                .unwrap();
            inner
                .set_height(LengthValue::px(number(&wrapped[3])))
                .unwrap();
            button_style(&inner);
            element.append_child(&inner).unwrap();
            elements.push((name.to_owned(), element));
            elements.push((wrapped[0].as_str().unwrap().to_owned(), inner));
        } else {
            elements.push((name.to_owned(), element));
        }
    }
    for spec in oracle["dialogs"].as_array().unwrap() {
        let spec = spec.as_array().unwrap();
        let dialog = Element::create(&document, "dialog").unwrap();
        set(&dialog, StyleProperty::FontSize, "12px");
        let child = Element::create(&document, "button").unwrap();
        button_style(&child);
        set(&child, StyleProperty::Display, "block");
        child.set_width(LengthValue::px(60.0)).unwrap();
        child.set_height(LengthValue::px(24.0)).unwrap();
        dialog.append_child(&child).unwrap();
        body.append_child(&dialog).unwrap();
        elements.push((spec[0].as_str().unwrap().to_owned(), dialog));
        elements.push((spec[1].as_str().unwrap().to_owned(), child));
    }
    let log: Log = Rc::default();
    let elements: Names = Rc::new(elements);
    for (name, element) in elements.iter() {
        for event_type in EVENTS {
            let (log, name, names) = (log.clone(), name.clone(), elements.clone());
            element
                .on(event_type, move |event| {
                    if event.phase() == Some(EventPhase::Target) {
                        log.borrow_mut().push(entry(event, &name, &names));
                    }
                })
                .unwrap();
        }
    }
    let page = Page {
        document,
        elements,
        log,
    };
    page.document.update_all().unwrap();
    page
}

/// An event as the oracle's logger records it.
fn entry(event: &Event, target: &str, elements: &Names) -> Value {
    let mut entry = json!({"type": event.event_type, "target": target});
    if event.event_type.starts_with("key") {
        entry["key"] = json!(event.key_text);
        entry["shift"] = json!(event.modifiers as u32 & Modifiers::SHIFT.0 != 0);
    }
    if event.event_type.starts_with("focus") || event.event_type == "blur" {
        entry["related"] = match event.related_target() {
            Some(related) => json!(name_of(elements, &related)),
            None => Value::Null,
        };
    }
    entry
}

fn key_of(name: &str) -> Key {
    match name {
        "Tab" => Key::Tab,
        other => panic!("unknown key {other}"),
    }
}

/// One of the page's mutations, as the capture script's `ACTIONS`.
fn mutate(page: &Page, action: &str) {
    match action {
        "hide-a" => set(&page.element("a"), StyleProperty::Display, "none"),
        "show-a" => {
            page.element("a")
                .remove_property(StyleProperty::Display)
                .unwrap();
        }
        "hide-wrap" => set(&page.element("wrap"), StyleProperty::Display, "none"),
        "invisible-a" => set(&page.element("a"), StyleProperty::Visibility, "hidden"),
        "disable-a" => page.element("a").set_attribute("disabled", "").unwrap(),
        "untab-t" => {
            page.element("t").remove_attribute("tabindex").unwrap();
        }
        // The DOM's `remove()` keeps the node and its listeners, as
        // `detach` does; `divui`'s `remove` destroys them.
        "remove-a" => page.element("a").detach().unwrap(),
        other => panic!("unknown action {other}"),
    }
}

/// Send one of the oracle's inputs.
fn send(page: &Page, input: &[Value]) {
    match input[0].as_str().unwrap() {
        "key" => {
            assert_eq!(input[2], "", "no scenario holds a modifier");
            let key = key_of(input[1].as_str().unwrap());
            for event_type in [KeyEventType::Down, KeyEventType::Up] {
                page.document
                    .dispatch_key(event_type, key, Modifiers::NONE, false)
                    .unwrap();
            }
        }
        "click" => {
            let (x, y) = (number(&input[2]), number(&input[3]));
            for kind in [
                PointerInputKind::Move,
                PointerInputKind::Down,
                PointerInputKind::Up,
            ] {
                page.document
                    .dispatch_pointer_input(PointerInput::mouse(kind, x, y))
                    .unwrap();
            }
        }
        "do" => {
            // The capture script's `fix`: the mutation, the focused element
            // after it and after a forced layout, then a mark in the second
            // animation frame's callbacks.
            mutate(page, input[1].as_str().unwrap());
            page.mark("sync");
            page.document.update_all().unwrap();
            page.mark("layout");
            let (document, log, names) = (
                page.document.clone(),
                page.log.clone(),
                page.elements.clone(),
            );
            page.document.request_animation_frame(move |_| {
                let next = document.clone();
                next.request_animation_frame(move |_| mark(&log, &document, &names, "frames"));
            });
            page.frame();
            page.frame();
        }
        other => panic!("unknown input {other}"),
    }
    page.document.dispatch_pending_events().unwrap();
    // The capture waits while the page renders frames.
    page.frame();
    page.frame();
}

/// The capture, replayed through a document: every step of every scenario
/// that needs no dialog method matches Chromium.
#[test]
fn focus_fixup_matches_chromium() {
    let oracle: Value = serde_json::from_str(LINUX).unwrap();
    assert_eq!(oracle["platform"], "linux-x11");
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let mut found = Vec::new();
    let mut replayed = 0;
    for (name, steps) in oracle["scenarios"].as_object().unwrap() {
        if ENGINE_REPLAYED.contains(&name.as_str()) {
            continue;
        }
        replayed += 1;
        let page = page(&oracle);
        for (index, step) in steps.as_array().unwrap().iter().enumerate() {
            page.log.borrow_mut().clear();
            send(&page, step["input"].as_array().unwrap());
            let got = json!({"events": page.log.borrow().clone(), "active": page.active()});
            for key in ["events", "active"] {
                if got[key] != step[key] {
                    found.push(format!(
                        "{name} {index} {key}: Div {} Chromium {}",
                        got[key], step[key]
                    ));
                }
            }
        }
    }
    assert_eq!(replayed, 7, "the oracle's scenarios changed");
    assert!(
        found.is_empty(),
        "{} steps differ from Chromium:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// Every scenario is replayed here or by the engine.
#[test]
fn every_scenario_is_replayed() {
    let oracle: Value = serde_json::from_str(LINUX).unwrap();
    let scenarios = oracle["scenarios"].as_object().unwrap();
    for name in ENGINE_REPLAYED {
        assert!(scenarios.contains_key(*name), "{name} is not in the oracle");
    }
    assert_eq!(scenarios.len(), 9);
}
