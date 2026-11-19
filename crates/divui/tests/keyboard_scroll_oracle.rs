//! Keyboard scrolling and `keypress` against pinned Chromium 147 (RFC 0007
//! section 7, Tier 0 item I2).
//!
//! `tools/qualification/capture_keyboard_scroll_oracle.py` recorded what
//! Chrome-for-Testing 147.0.7727.50 does with real key input on a fixed page:
//! posted Win32 key messages to the foreground window on Windows
//! (`tests/fidelity/fixtures/keyboard-scroll-v1/chromium-windows.json`) and
//! XTEST keys on X11 under WSL (`chromium-linux-x11.json`), at device scales
//! 1 and 1.25. This test builds the same page in Div, replays each
//! scenario's clicks, keys and page actions on this host's capture, and after
//! every step compares:
//!
//! - the `keydown`, `keypress`, `keyup` and `click` events: type, key,
//!   modifier flags and target;
//! - which scrollers fired `scroll`;
//! - every scroller's offsets and the root's;
//! - the focused element and the form controls' values.
//!
//! Div's root stands for `document`, `html` and `body`, so the oracle's
//! `body` is the root. Div has no events for the modifier keys themselves, so
//! the oracle's Shift, Control and Alt key events are left out. The steps in
//! [`GAPS`] differ for a named reason and must keep differing until the gap
//! is closed.

use div::prelude::*;
use div::typed_style::parse_literal;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

const WINDOWS: &str =
    include_str!("../../../tests/fidelity/fixtures/keyboard-scroll-v1/chromium-windows.json");
const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/keyboard-scroll-v1/chromium-linux-x11.json");

/// This host's capture and its platform name.
const ORACLE: (&str, &str) = if cfg!(target_os = "windows") {
    (WINDOWS, "windows")
} else {
    (LINUX, "linux-x11")
};

/// Steps (scale, scenario, step indices) that differ from Chromium, with the
/// gap's name and owner. A scale of `None` means both scales.
const GAPS: &[(Option<&str>, &str, &[usize], &str)] = &[
    // `overscroll-behavior: contain` ends the scroll chain at `contain`;
    // Div's style system has no `overscroll-behavior`, so the keys go on to
    // the root.
    (None, "contain", &[2, 3], "overscroll-behavior (engine)"),
    // Typing a space into the textarea scrolled off the top reveals the
    // caret, scrolling the root to it; Div edits without revealing.
    (None, "textarea", &[4], "edit-caret-reveal (framework)"),
    // At 1.25 Chromium's scrollbars are 19 device pixels (15.2 CSS) and
    // offsets land on device pixels; Div's are 15 CSS pixels with CSS-pixel
    // offsets (RFC 0011).
    (
        Some("1.25"),
        "root",
        &[6],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "press_inner",
        &[4, 5, 6, 7, 8, 9],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "contain",
        &[1],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "removed_press_target",
        &[3],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "removed_press_ancestor",
        &[3],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "hscroll",
        &[5],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "padded",
        &[1, 2, 3],
        "device-scale-scroll-offset (engine)",
    ),
    (
        Some("1.25"),
        "modifiers",
        &[3],
        "device-scale-scroll-offset (engine)",
    ),
];

const EVENTS: &[&str] = &["keydown", "keypress", "keyup", "click"];
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
            .find(|(_, candidate)| candidate.is_same_node(element).unwrap())
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
    if tag == "div" {
        set(&element, StyleProperty::Padding, "0px");
        set(&element, StyleProperty::Border, "0px none black");
    } else {
        // `button, input, textarea { padding: 0; border: 1px solid #888;
        // font: 12px sans-serif }`
        set(&element, StyleProperty::Padding, "0px");
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

/// A scroller from the oracle's `scrollers` list and its content element.
fn scroller(document: &Document, parent: &Element, spec: &Value) -> (Element, Element) {
    let spec = spec.as_array().unwrap();
    let element = place(
        document,
        parent,
        "div",
        (
            number(&spec[1]),
            number(&spec[2]),
            number(&spec[3]),
            number(&spec[4]),
        ),
    );
    for declaration in spec[5].as_str().unwrap().split(';') {
        let (property, value) = declaration.split_once(':').unwrap();
        match property {
            "overflow" => set(&element, StyleProperty::Overflow, value),
            "overflow-x" => set(&element, StyleProperty::OverflowX, value),
            "overflow-y" => set(&element, StyleProperty::OverflowY, value),
            "scroll-padding-top" => set(&element, StyleProperty::ScrollPaddingTop, value),
            // Div has no `overscroll-behavior`: gap overscroll-behavior.
            "overscroll-behavior" => {}
            other => panic!("unknown scroller style {other}"),
        }
    }
    let content = place(
        document,
        &element,
        "div",
        (0.0, 0.0, number(&spec[6]), number(&spec[7])),
    );
    (element, content)
}

/// The oracle's page, with every element logging the key and click events
/// it receives at its target and the scroll events it fires.
fn page(oracle: &Value, scale: f64) -> Page {
    let viewport = &oracle["viewport"];
    let (width, height) = (viewport[0].as_f64().unwrap(), viewport[1].as_f64().unwrap());
    let document = Document::new(width as i32, height as i32).unwrap();
    document
        .set_viewport(ViewportMetrics::from_logical_size(width, height, scale).unwrap())
        .unwrap();
    let body = document.body();
    let size = &oracle["document_size"];
    let spacer = place(
        &document,
        &body,
        "div",
        (0.0, 0.0, number(&size[0]), number(&size[1])),
    );
    let mut elements = vec![("body".to_owned(), body.clone()), ("spacer".into(), spacer)];
    let specs = oracle["scrollers"].as_array().unwrap();
    let spec = |name: &str| {
        specs
            .iter()
            .find(|spec| spec[0] == name)
            .unwrap_or_else(|| panic!("no scroller {name}"))
    };
    for name in ["outer", "contain", "hscroll", "padded", "withButton"] {
        let (element, content) = scroller(&document, &body, spec(name));
        if name == "outer" {
            let (inner, inner_content) = scroller(&document, &content, spec("inner"));
            let text = place(&document, &inner_content, "div", (20.0, 20.0, 60.0, 40.0));
            elements.push(("inner".into(), inner));
            elements.push(("innerContent".into(), inner_content));
            elements.push(("innerText".into(), text));
        }
        if name == "withButton" {
            let button = place(&document, &content, "button", (10.0, 10.0, 60.0, 24.0));
            button.append_text_node("in").unwrap();
            elements.push(("btnIn".into(), button));
        }
        elements.push((name.into(), element));
        elements.push((format!("{name}Content"), content));
    }
    let field = place(&document, &body, "input", (20.0, 330.0, 100.0, 24.0));
    field.set_attribute("value", "text").unwrap();
    let area = place(&document, &body, "textarea", (140.0, 325.0, 120.0, 40.0));
    area.set_control_value("one").unwrap();
    let button = place(&document, &body, "button", (280.0, 330.0, 60.0, 24.0));
    button.append_text_node("b").unwrap();
    let check = place(&document, &body, "input", (360.0, 334.0, 16.0, 16.0));
    check.set_attribute("type", "checkbox").unwrap();
    let radio = place(&document, &body, "input", (390.0, 334.0, 16.0, 16.0));
    radio.set_attribute("type", "radio").unwrap();
    radio.set_attribute("name", "r").unwrap();
    elements.extend([
        ("field".into(), field),
        ("area".into(), area),
        ("btn".into(), button),
        ("check".into(), check),
        ("radio".into(), radio),
    ]);
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

fn modifiers_of(flags: &str) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    for flag in flags.chars() {
        modifiers = modifiers
            | match flag {
                's' => Modifiers::SHIFT,
                'c' => Modifiers::CTRL,
                'a' => Modifiers::ALT,
                other => panic!("unknown modifier {other}"),
            };
    }
    modifiers
}

/// The key the oracle's key name types with these modifiers, as the
/// platform layer reports it.
fn key_of(name: &str, modifiers: Modifiers) -> Key {
    let shift = modifiers.contains(Modifiers::SHIFT);
    match name {
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "Space" => Key::Space,
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "A" => Key::Char(if shift { 'A' } else { 'a' }),
        "Digit1" => Key::Char(if shift { '!' } else { '1' }),
        other => panic!("unknown key {other}"),
    }
}

/// Send one of the oracle's inputs.
fn send(page: &Page, input: &[Value]) {
    match input[0].as_str().unwrap() {
        "key" => {
            let modifiers = modifiers_of(input[2].as_str().unwrap());
            let key = key_of(input[1].as_str().unwrap(), modifiers);
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
            let prevent = |event_type: &str| {
                page.document
                    .body()
                    .on_capture(event_type, |event| event.prevent_default())
                    .unwrap();
            };
            match input[1].as_str().unwrap() {
                "remove-inner-text" => page.element("innerText").remove().unwrap(),
                "remove-inner" => page.element("inner").remove().unwrap(),
                "cancel-keydown" => prevent("keydown"),
                "cancel-keypress" => prevent("keypress"),
                "cancel-mousedown" => prevent("mousedown"),
                other => panic!("unknown action {other}"),
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
    let value = |name: &str| page.element(name).control_value().unwrap();
    json!({
        "events": page.log.borrow().clone(),
        "scrolled": page.scrolled.borrow().iter().collect::<Vec<_>>(),
        "offsets": offsets,
        "active": active,
        "values": {
            "field": value("field"),
            "area": value("area"),
            "check": page.element("check").is_checked().unwrap(),
            "radio": page.element("radio").is_checked().unwrap(),
        },
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

/// Replay every scenario at every scale; return the steps that differ, with
/// what differed.
fn differences(oracle: &Value) -> Vec<(String, String, usize, String)> {
    let mut found = Vec::new();
    for (scale_name, scenarios) in oracle["scales"].as_object().unwrap() {
        let scale: f64 = scale_name.parse().unwrap();
        for (name, steps) in scenarios.as_object().unwrap() {
            let page = page(oracle, scale);
            for (index, step) in steps.as_array().unwrap().iter().enumerate() {
                page.log.borrow_mut().clear();
                page.scrolled.borrow_mut().clear();
                send(&page, step["input"].as_array().unwrap());
                let got = state(&page, step["offsets"].as_object().unwrap());
                let mut want = step.as_object().unwrap().clone();
                want.remove("input");
                want["events"] = Value::Array(expected_events(&step["events"]));
                let differing: Vec<_> = want
                    .iter()
                    .filter(|(key, want)| !same(&got[key.as_str()], want))
                    .map(|(key, want)| format!("{key}: Div {} Chromium {want}", got[key.as_str()]))
                    .collect();
                if !differing.is_empty() {
                    found.push((
                        scale_name.clone(),
                        name.clone(),
                        index,
                        differing.join("; "),
                    ));
                }
            }
        }
    }
    found
}

/// The host platform's capture, replayed through a document. Every step
/// matches except the named gaps, and every named gap still differs.
#[test]
fn keyboard_scrolling_matches_chromium_on_this_platform() {
    let oracle: Value = serde_json::from_str(ORACLE.0).unwrap();
    assert_eq!(oracle["platform"], ORACLE.1);
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let found = differences(&oracle);
    let is_gap = |scale: &str, scenario: &str, index: usize| {
        GAPS.iter()
            .any(|(gap_scale, gap_scenario, gap_indices, _)| {
                gap_scale.is_none_or(|gap_scale| gap_scale == scale)
                    && *gap_scenario == scenario
                    && gap_indices.contains(&index)
            })
    };
    let unexpected: Vec<_> = found
        .iter()
        .filter(|(scale, scenario, index, _)| !is_gap(scale, scenario, *index))
        .collect();
    assert!(
        unexpected.is_empty(),
        "steps that differ from Chromium:\n{}",
        unexpected
            .iter()
            .map(|(scale, scenario, index, what)| format!("{scale} {scenario} {index}: {what}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    for (gap_scale, scenario, indices, gap) in GAPS {
        for scale in oracle["scales"].as_object().unwrap().keys() {
            if gap_scale.is_some_and(|gap_scale| gap_scale != scale) {
                continue;
            }
            for index in *indices {
                if !scenario_exists(&oracle, scale, scenario) {
                    continue;
                }
                assert!(
                    found
                        .iter()
                        .any(|(s, n, i, _)| s == scale && n == scenario && i == index),
                    "gap {gap} at {scale} {scenario} {index} now matches Chromium: close it"
                );
            }
        }
    }
}

fn scenario_exists(oracle: &Value, scale: &str, scenario: &str) -> bool {
    oracle["scales"][scale].get(scenario).is_some()
}

/// The two captures agree except where the platforms differ: the Linux
/// capture's extra modifier scenarios, and Delete's keypress, which X11 sends
/// with the character 127 and Windows does not.
#[test]
fn captures_differ_only_in_delete_keypress_and_modifiers() {
    let windows: Value = serde_json::from_str(WINDOWS).unwrap();
    let linux: Value = serde_json::from_str(LINUX).unwrap();
    for (scale, scenarios) in windows["scales"].as_object().unwrap() {
        for (name, steps) in scenarios.as_object().unwrap() {
            let other = &linux["scales"][scale][name];
            for (index, (windows, linux)) in steps
                .as_array()
                .unwrap()
                .iter()
                .zip(other.as_array().unwrap())
                .enumerate()
            {
                if windows != linux {
                    assert_eq!(
                        (name.as_str(), index, &windows["input"]),
                        ("keypress_keys", 5, &json!(["key", "Delete", ""])),
                        "{scale}"
                    );
                }
            }
        }
        let extra: Vec<_> = linux["scales"][scale]
            .as_object()
            .unwrap()
            .keys()
            .filter(|name| !scenarios.as_object().unwrap().contains_key(*name))
            .collect();
        assert_eq!(extra, ["modified_keypress", "modifiers"], "{scale}");
    }
}
