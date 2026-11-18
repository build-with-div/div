//! Keyboard-focusable scrollers, a click's focus and the sequential focus
//! navigation starting point against pinned Chromium 147 (RFC 0007 section
//! 7, Tier 0 item I2b).
//!
//! `tools/qualification/capture_focus_navigation_oracle.py` recorded what
//! Chrome-for-Testing 147.0.7727.50 does with real XTEST key and button input
//! on X11 under WSL (`tests/fidelity/fixtures/focus-navigation-v1/
//! chromium-linux-x11.json`), at device scale 1. Blink's focus navigation
//! is the same on every platform, so this test replays that capture on every
//! host. It builds the same page in Div, replays each scenario's clicks, keys
//! and page actions, and after every step compares:
//!
//! - the `keydown`, `keyup` and `click` events: type, key, modifier flags
//!   and target;
//! - which scrollers fired `scroll`;
//! - every scroller's offsets and the root's;
//! - the focused element.
//!
//! Div's root stands for `document`, `html` and `body`, so the oracle's
//! `body` is the root. Div has no events for the modifier keys themselves, so
//! the oracle's Shift key events are left out.

use div::prelude::*;
use div::typed_style::parse_literal;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/focus-navigation-v1/chromium-linux-x11.json");

const EVENTS: &[&str] = &["keydown", "keyup", "click"];
/// Scrollers placed after the gap in tree order, as the capture script's
/// `LATE`.
const LATE: &[&str] = &["divChild", "crossAxis", "fractionDown", "fractionUp"];
const MODIFIER_KEYS: &[&str] = &["Shift", "Control", "Alt", "Meta"];

type Log = Rc<RefCell<Vec<Value>>>;
type Scrolled = Rc<RefCell<BTreeSet<String>>>;

struct Page {
    document: Document,
    elements: Vec<(String, Element)>,
    log: Log,
    scrolled: Scrolled,
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

    fn name_of(&self, element: &Element) -> String {
        self.elements
            .iter()
            // A removed element's handle is stale and matches nothing.
            .find(|(_, candidate)| candidate.is_same_node(element).unwrap_or(false))
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "?".into())
    }
}

fn set(element: &Element, property: StyleProperty, literal: &str) {
    element
        .set_property(property, parse_literal(property, literal).unwrap())
        .unwrap();
}

fn place(document: &Document, parent: &Element, tag: &str, at: (f32, f32, f32, f32)) -> Element {
    let element = Element::create(document, tag).unwrap();
    element.set_position(Position::Absolute).unwrap();
    element.set_box_sizing(BoxSizing::BorderBox).unwrap();
    element.set_left(Length::px(at.0)).unwrap();
    element.set_top(Length::px(at.1)).unwrap();
    element.set_width(LengthValue::px(at.2)).unwrap();
    element.set_height(LengthValue::px(at.3)).unwrap();
    set(&element, StyleProperty::Margin, "0px");
    set(&element, StyleProperty::Padding, "0px");
    if tag == "div" {
        set(&element, StyleProperty::Border, "0px none black");
    } else {
        // `button { padding: 0; border: 1px solid #888; font: 12px sans-serif }`
        set(&element, StyleProperty::Border, "1px solid #888888");
        set(&element, StyleProperty::FontSize, "12px");
        set(&element, StyleProperty::FontFamily, "sans-serif");
    }
    parent.append_child(&element).unwrap();
    element
}

fn number(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

/// A scroller from the oracle's `scrollers` list, its content element and
/// the element inside the content, as the capture script's markup.
fn scroller(
    document: &Document,
    body: &Element,
    spec: &Value,
    elements: &mut Vec<(String, Element)>,
) {
    let spec = spec.as_array().unwrap();
    let name = spec[0].as_str().unwrap();
    let element = place(
        document,
        body,
        "div",
        (number(&spec[1]), number(&spec[2]), 100.0, 60.0),
    );
    if name == "negScroller" {
        element.set_attribute("tabindex", "-1").unwrap();
    }
    for declaration in spec[3].as_str().unwrap().split(';') {
        let (property, value) = declaration.split_once(':').unwrap();
        match property {
            "overflow" => set(&element, StyleProperty::Overflow, value),
            "overflow-x" => set(&element, StyleProperty::OverflowX, value),
            "overflow-y" => set(&element, StyleProperty::OverflowY, value),
            other => panic!("unknown scroller style {other}"),
        }
    }
    let content = place(
        document,
        &element,
        "div",
        (0.0, 0.0, number(&spec[4]), number(&spec[5])),
    );
    let inner = match name {
        "negChild" | "disabledChild" | "hiddenChild" => {
            let button = place(document, &content, "button", (5.0, 5.0, 50.0, 20.0));
            match name {
                "negChild" => button.set_attribute("tabindex", "-1").unwrap(),
                "disabledChild" => button.set_attribute("disabled", "").unwrap(),
                _ => set(&button, StyleProperty::Display, "none"),
            }
            button.append_text_node(&name[..1]).unwrap();
            Some((format!("{name}Btn"), button))
        }
        "divChild" => {
            let inner = place(document, &content, "div", (5.0, 5.0, 50.0, 20.0));
            inner.set_attribute("tabindex", "0").unwrap();
            Some(("divChildInner".to_owned(), inner))
        }
        _ => None,
    };
    elements.push((name.to_owned(), element));
    elements.push((format!("{name}Content"), content));
    elements.extend(inner);
}

/// The oracle's page, with every element logging the key and click events
/// it receives at its target and the scroll events it fires.
fn page(oracle: &Value) -> Page {
    let viewport = &oracle["viewport"];
    let (width, height) = (viewport[0].as_f64().unwrap(), viewport[1].as_f64().unwrap());
    let document = Document::new(width as i32, height as i32).unwrap();
    document
        .set_viewport(ViewportMetrics::from_logical_size(width, height, 1.0).unwrap())
        .unwrap();
    let body = document.body();
    let mut elements = vec![("body".to_owned(), body.clone())];
    let specs = oracle["scrollers"].as_array().unwrap();
    // Tree order: the scrollers, the gap, the late scrollers, the buttons.
    for spec in specs
        .iter()
        .filter(|spec| !LATE.contains(&spec[0].as_str().unwrap()))
    {
        scroller(&document, &body, spec, &mut elements);
    }
    let gap_wrap = place(&document, &body, "div", (10.0, 170.0, 200.0, 40.0));
    let gap = place(&document, &gap_wrap, "div", (0.0, 0.0, 200.0, 40.0));
    elements.push(("gapWrap".into(), gap_wrap));
    elements.push(("gap".into(), gap));
    for spec in specs
        .iter()
        .filter(|spec| LATE.contains(&spec[0].as_str().unwrap()))
    {
        scroller(&document, &body, spec, &mut elements);
    }
    for (index, name) in oracle["buttons"].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let button = place(
            &document,
            &body,
            "button",
            (10.0 + 70.0 * index as f32, 250.0, 60.0, 24.0),
        );
        button.append_text_node(name).unwrap();
        elements.push((name.to_owned(), button));
    }
    document.update_all().unwrap();
    let log: Log = Rc::default();
    let scrolled: Scrolled = Rc::default();
    for (name, element) in &elements {
        for event_type in EVENTS {
            let (log, name) = (log.clone(), name.clone());
            element
                .on(event_type, move |event| {
                    if event.phase() == Some(EventPhase::Target) {
                        log.borrow_mut().push(entry(event, &name));
                    }
                })
                .unwrap();
        }
        let (scrolled, name) = (
            scrolled.clone(),
            if name == "body" {
                "root".to_owned()
            } else {
                name.clone()
            },
        );
        element
            .on("scroll", move |event| {
                if event.phase() == Some(EventPhase::Target) {
                    scrolled.borrow_mut().insert(name.clone());
                }
            })
            .unwrap();
    }
    Page {
        document,
        elements,
        log,
        scrolled,
    }
}

/// An event as the oracle's logger records it.
fn entry(event: &Event, target: &str) -> Value {
    let modifier = |flag: Modifiers| event.modifiers as u32 & flag.0 != 0;
    let mut entry = json!({
        "type": event.event_type,
        "shift": modifier(Modifiers::SHIFT),
        "ctrl": modifier(Modifiers::CTRL),
        "alt": modifier(Modifiers::ALT),
        "meta": modifier(Modifiers::META),
        "target": target,
    });
    if event.event_type != "click" {
        entry["key"] = json!(event.key_text);
    }
    entry
}

/// The oracle's events without the modifier keys' own events and the
/// physical `code`, which Div does not report.
fn expected_events(events: &Value) -> Vec<Value> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            !event["key"]
                .as_str()
                .is_some_and(|key| MODIFIER_KEYS.contains(&key))
        })
        .map(|event| {
            let mut event = event.as_object().unwrap().clone();
            event.remove("code");
            Value::Object(event)
        })
        .collect()
}

fn key_of(name: &str) -> Key {
    match name {
        "Tab" => Key::Tab,
        "Down" => Key::ArrowDown,
        other => panic!("unknown key {other}"),
    }
}

/// Blur the focused element, as the oracle's `document.activeElement.blur()`.
fn blur_active(page: &Page) {
    if let Some(active) = page.document.focused_element().unwrap() {
        active.blur().unwrap();
    }
}

/// Send one of the oracle's inputs.
fn send(page: &Page, input: &[Value]) {
    match input[0].as_str().unwrap() {
        "key" => {
            let modifiers = match input[2].as_str().unwrap() {
                "" => Modifiers::NONE,
                "s" => Modifiers::SHIFT,
                other => panic!("unknown modifiers {other}"),
            };
            let key = key_of(input[1].as_str().unwrap());
            for event_type in [KeyEventType::Down, KeyEventType::Up] {
                page.document
                    .dispatch_key(event_type, key, modifiers, false)
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
            let action = input[1].as_str().unwrap();
            if let Some(name) = action.strip_prefix("focus-") {
                blur_active(page);
                // Like the DOM's `focus()`, an element that cannot take
                // focus is left alone.
                let _ = page.element(name).focus();
            } else if let Some(name) = action.strip_prefix("remove-") {
                page.element(name).remove().unwrap();
            } else if action == "blur-active" {
                blur_active(page);
            } else if action == "cancel-mousedown" {
                page.document
                    .body()
                    .on_capture("mousedown", |event| event.prevent_default())
                    .unwrap();
            } else {
                panic!("unknown action {action}");
            }
        }
        other => panic!("unknown input {other}"),
    }
    page.document.dispatch_pending_events().unwrap();
}

/// Div's state after a step, in the oracle's shape.
fn state(page: &Page, oracle_offsets: &Map<String, Value>) -> Value {
    let mut offsets = Map::new();
    for name in oracle_offsets.keys() {
        let element = if name == "root" {
            page.document.body()
        } else {
            page.element(name)
        };
        offsets.insert(
            name.clone(),
            json!([
                element.scroll_left().unwrap(),
                element.scroll_top().unwrap()
            ]),
        );
    }
    let active = page
        .document
        .focused_element()
        .unwrap()
        .map_or_else(|| "body".to_owned(), |element| page.name_of(&element));
    json!({
        "events": page.log.borrow().clone(),
        "scrolled": page.scrolled.borrow().iter().collect::<Vec<_>>(),
        "offsets": offsets,
        "active": active,
    })
}

/// Offsets compare as numbers, so 40 and 40.0 agree.
fn same(got: &Value, want: &Value) -> bool {
    match (got, want) {
        (Value::Number(got), Value::Number(want)) => got.as_f64() == want.as_f64(),
        (Value::Array(got), Value::Array(want)) => {
            got.len() == want.len() && got.iter().zip(want).all(|(got, want)| same(got, want))
        }
        (Value::Object(got), Value::Object(want)) => {
            got.len() == want.len()
                && got
                    .iter()
                    .all(|(key, got)| want.get(key).is_some_and(|want| same(got, want)))
        }
        _ => got == want,
    }
}

/// The capture, replayed through a document: every step of every scenario
/// matches Chromium.
#[test]
fn focus_navigation_matches_chromium() {
    let oracle: Value = serde_json::from_str(LINUX).unwrap();
    assert_eq!(oracle["platform"], "linux-x11");
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let mut found = Vec::new();
    for (name, steps) in oracle["scenarios"].as_object().unwrap() {
        let page = page(&oracle);
        for (index, step) in steps.as_array().unwrap().iter().enumerate() {
            page.log.borrow_mut().clear();
            page.scrolled.borrow_mut().clear();
            send(&page, step["input"].as_array().unwrap());
            let got = state(&page, step["offsets"].as_object().unwrap());
            let mut want = step.as_object().unwrap().clone();
            want.remove("input");
            want["events"] = Value::Array(expected_events(&step["events"]));
            for (key, want) in &want {
                if !same(&got[key.as_str()], want) {
                    found.push(format!(
                        "{name} {index} {key}: Div {} Chromium {want}",
                        got[key.as_str()]
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} steps differ from Chromium:\n{}",
        found.len(),
        found.join("\n")
    );
}
