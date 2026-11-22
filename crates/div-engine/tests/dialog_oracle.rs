//! The `dialog-v1` oracle (RFC 0015) replayed through the engine.
//!
//! `tools/qualification/capture_dialog_oracle.py` recorded pinned Chromium
//! 147 with real input (`tests/fidelity/fixtures/dialog-v1/
//! chromium-linux-x11.json`). This test builds the same page in the engine,
//! with the oracle's own styles as `style` attributes, and replays the
//! scenarios that need no `divui` API, events or `::backdrop`. It drives the
//! engine as `divui` does:
//!
//! - a press hit-tests, takes the press focus as `mousedown`'s default
//!   action, and clicks the activation target on release;
//! - a key goes to the focused element, else `body`, and Tab moves focus
//!   with [`Engine::advance_focus`];
//! - script calls are the engine's dialog methods.
//!
//! After each step it compares, exactly, the targets of the input events,
//! the focused element and every dialog's `open`, `:modal` and
//! `returnValue`. The toggle-family events (`beforetoggle`, `toggle`,
//! `cancel`, `close`) are `divui`'s: they are a pending field owned by
//! RFC 0015 PR 5, and the test fails once Div produces them as Chromium did,
//! so that the field comes off the list. They are the only events it does
//! not compare. `crates/divui/tests/dialog_oracle.rs` keeps the list of the
//! scenarios still pending as a whole.
//!
//! It also replays all 15 geometry records at their three viewports: the
//! border box, the client and scroll sizes, and the computed (resolved)
//! style Chromium reported, field by field. A few fields are pending with
//! their owner, and a pending field that matches fails the test. The
//! `::backdrop` records are RFC 0015 PR 4's.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use div_dom::ElementTag;
use div_engine::{Engine, EngineError, FocusOrigin, NodeHandle, PointerEventKind, PressFocus};
use div_geometry::{Length, ViewportMetrics};
use div_style::Color;
use serde_json::{json, Value};

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/dialog-v1/chromium-linux-x11.json");

/// The scenarios replayed here, each with every step.
const REPLAYED: &[&str] = &[
    "modal_open_close",
    "modal_tab",
    "click_dialog_content",
    "script_focus_inert",
    "autofocus",
    "no_focusable",
    "non_modal_focus",
    "non_modal_focus_outside",
    "non_modal_click_outside",
    "remove_open_modal",
];

/// The UA `max-width` and `max-height` of a modal dialog,
/// `calc(100% - 6px - 2em)`: Div's lengths cannot hold a percentage less a
/// font-relative length, and RFC 0015's sheet leaves them out until they
/// can.
const CALC: &str = "engine 0.2 PR 2: calc() in width, height, min-* and max-*, with em and %";

/// Geometry records whose box that `max-width` or `max-height` limits, as
/// `(viewport width, dialog, modal)`, and the fields that therefore differ.
/// A tall dialog is clamped and scrolls, which changes its width and both
/// margins; a wide one fills the viewport either way, so only its height
/// and top margin differ.
const LIMITED: &[((u64, &str, bool), &[&str])] = &[
    ((480, "dTall", true), TALL),
    ((800, "dTall", true), TALL),
    ((480, "dWide", true), WIDE),
    ((375, "dWide", true), WIDE),
];
const TALL: &[&str] = &["rect", "client", "marginTop", "marginLeft"];
const WIDE: &[&str] = &["rect", "client", "marginTop"];

/// The owner of a geometry field still pending, if it is.
fn geometry_pending(width: u64, dialog: &str, modal: bool, field: &str) -> Option<&'static str> {
    if modal && matches!(field, "maxWidth" | "maxHeight") {
        return Some(CALC);
    }
    let limited = LIMITED
        .iter()
        .find(|(record, _)| *record == (width, dialog, modal));
    if limited.is_some_and(|(_, fields)| fields.contains(&field)) {
        return Some(CALC);
    }
    None
}

/// Events `divui` dispatches, a pending field of every scenario.
const EVENTS_OWNER: &str = "RFC 0015 PR 5: the divui dialog events";
/// The toggle-family events.
const PENDING_EVENTS: &[&str] = &["beforetoggle", "toggle", "cancel", "close"];

/// The input events this test compares.
const INPUT_EVENTS: &[&str] = &["mousedown", "mouseup", "click", "keydown", "keyup"];

struct Page {
    engine: Engine,
    body: NodeHandle,
    by_name: BTreeMap<String, NodeHandle>,
    names: HashMap<NodeHandle, String>,
    dialogs: Vec<String>,
}

impl Page {
    fn new(oracle: &Value) -> Self {
        Self::with_viewport(oracle, &oracle["viewport"])
    }

    fn with_viewport(oracle: &Value, viewport: &Value) -> Self {
        let viewport = viewport.as_array().expect("viewport");
        let (width, height) = (viewport[0].as_f64().unwrap(), viewport[1].as_f64().unwrap());
        let mut engine =
            Engine::new(ViewportMetrics::from_logical_size(width, height, 1.0).unwrap()).unwrap();
        let html = engine.create_native_element(ElementTag::Html).unwrap();
        let body = engine.create_native_element(ElementTag::Body).unwrap();
        let root = engine.root();
        engine.append_child(root, html).unwrap();
        engine.append_child(html, body).unwrap();
        let mut page = Page {
            engine,
            body,
            by_name: BTreeMap::new(),
            names: HashMap::new(),
            dialogs: Vec::new(),
        };
        page.name(html, "html");
        page.name(body, "body");
        for node in [html, body] {
            page.style(node, "margin: 0; padding: 0");
        }
        for spec in oracle["outside"].as_array().unwrap() {
            page.placed(spec);
        }
        for dialog in oracle["dialogs"].as_array().unwrap() {
            let name = dialog[0].as_str().unwrap();
            let node = page
                .engine
                .create_native_element_named(ElementTag::Div, "dialog")
                .unwrap();
            page.engine.append_child(body, node).unwrap();
            page.name(node, name);
            page.style(node, "font-size: 12px");
            page.dialogs.push(name.to_owned());
            let row = page.engine.create_native_element(ElementTag::Div).unwrap();
            page.engine.append_child(node, row).unwrap();
            page.style(row, "display: flex");
            for child in dialog[1].as_array().unwrap() {
                page.dialog_child(row, child);
            }
        }
        for spec in oracle["trailing"].as_array().unwrap() {
            page.placed(spec);
        }
        page.engine.update().unwrap();
        page
    }

    fn name(&mut self, node: NodeHandle, name: &str) {
        self.engine.set_attribute(node, "id", name).unwrap();
        self.by_name.insert(name.to_owned(), node);
        self.names.insert(node, name.to_owned());
    }

    fn style(&mut self, node: NodeHandle, style: &str) {
        self.engine.set_attribute(node, "style", style).unwrap();
    }

    fn element(&mut self, tag: &str) -> NodeHandle {
        let kind = match tag {
            "button" => ElementTag::Button,
            "input" => ElementTag::Input,
            "span" => ElementTag::Span,
            "div" => ElementTag::Div,
            other => panic!("the oracle page has no {other}"),
        };
        self.engine.create_native_element(kind).unwrap()
    }

    /// The oracle's `.abs` boxes: `[id, tag, left, top, width, height]`.
    fn placed(&mut self, spec: &Value) {
        let name = spec[0].as_str().unwrap();
        let node = self.element(spec[1].as_str().unwrap());
        self.engine.append_child(self.body, node).unwrap();
        self.name(node, name);
        let control = if spec[1] == "div" { "" } else { CONTROL };
        let style = format!(
            "position: absolute; box-sizing: border-box; margin: 0; left: {}px; top: {}px; \
             width: {}px; height: {}px; {control}",
            spec[2], spec[3], spec[4], spec[5]
        );
        self.style(node, &style);
    }

    /// A dialog's `.row` child: `[id, tag, width, height, extra]`.
    fn dialog_child(&mut self, row: NodeHandle, spec: &Value) {
        let name = spec[0].as_str().unwrap();
        let tag = spec[1].as_str().unwrap();
        let node = self.element(tag);
        self.engine.append_child(row, node).unwrap();
        self.name(node, name);
        let control = if matches!(tag, "button" | "input") {
            CONTROL
        } else {
            ""
        };
        let style = format!(
            "flex-grow: 0; flex-shrink: 0; box-sizing: border-box; margin: 0; width: {}px; \
             height: {}px; {control}",
            spec[2], spec[3]
        );
        self.style(node, &style);
        if spec[4].as_str().unwrap().contains("autofocus") {
            self.engine.set_attribute(node, "autofocus", "").unwrap();
        }
    }

    fn node(&self, name: &str) -> NodeHandle {
        self.by_name[name]
    }

    fn target(&self, node: Option<NodeHandle>) -> String {
        match node {
            Some(node) => self
                .names
                .get(&node)
                .cloned()
                .unwrap_or_else(|| format!("{node:?}")),
            None => "none".to_owned(),
        }
    }

    /// `document.activeElement`: the focused element, else `body`.
    fn active(&self) -> String {
        self.engine
            .focused()
            .map_or_else(|| "body".to_owned(), |node| self.target(Some(node)))
    }

    fn click(&mut self, x: f32, y: f32) -> Vec<Value> {
        let mut events = Vec::new();
        let down = self
            .engine
            .pointer_event(1, PointerEventKind::Down, x, y)
            .unwrap();
        events.push(json!({"type": "mousedown", "target": self.target(down.target)}));
        if let Some(target) = down.target {
            match self.engine.press_focus(target, x, y).unwrap() {
                PressFocus::Focus(next) => {
                    self.engine
                        .focus_with_origin(next, FocusOrigin::Pointer)
                        .unwrap();
                }
                PressFocus::Clear => {
                    if let Some(focused) = self.engine.focused() {
                        self.engine.blur(focused).unwrap();
                    }
                }
                PressFocus::Keep => {}
            }
        }
        let up = self
            .engine
            .pointer_event(1, PointerEventKind::Up, x, y)
            .unwrap();
        self.engine.end_pointer_capture(1);
        events.push(json!({"type": "mouseup", "target": self.target(up.target)}));
        if let (Some(target), Some(_)) = (up.target, up.pressed) {
            let click = up.activation.unwrap_or(target);
            events.push(json!({"type": "click", "target": self.target(Some(click))}));
        }
        events
    }

    fn tab(&mut self, shift: bool) -> Vec<Value> {
        let mut events = Vec::new();
        if shift {
            events.push(
                json!({"type": "keydown", "target": self.active(), "key": "Shift", "shift": true}),
            );
        }
        events.push(
            json!({"type": "keydown", "target": self.active(), "key": "Tab", "shift": shift}),
        );
        self.engine
            .advance_focus(if shift { -1 } else { 1 })
            .unwrap();
        events
            .push(json!({"type": "keyup", "target": self.active(), "key": "Tab", "shift": shift}));
        if shift {
            events.push(
                json!({"type": "keyup", "target": self.active(), "key": "Shift", "shift": false}),
            );
        }
        events
    }

    fn act(&mut self, action: &str) {
        let d1 = self.node("d1");
        match action {
            "show-d1" => self.engine.show_dialog(d1).unwrap(),
            "close-d1" => {
                self.engine.close_dialog(d1, None).unwrap();
            }
            // The oracle lists only the dialogs in the document.
            "remove-d1" => {
                self.engine.detach(d1).unwrap();
                self.dialogs.retain(|name| name != "d1");
            }
            // Blink's `focus()` on an element that cannot take focus does
            // nothing.
            "focus-outside" => match self.engine.focus(self.node("outside")) {
                Ok(()) | Err(EngineError::NotFocusable) => {}
                Err(error) => panic!("{error}"),
            },
            other => {
                let dialog = other
                    .strip_prefix("show-modal-")
                    .unwrap_or_else(|| panic!("action {other} is not replayed here"));
                let dialog = self.node(dialog);
                self.engine.show_modal(dialog).unwrap();
            }
        }
    }

    /// A dialog's box, client and scroll sizes, and the style Chromium's
    /// `getComputedStyle` reports, serialized as CSSOM does. Div has no
    /// resolved-value API, so the test resolves an `auto` inset or margin of
    /// the positioned dialog from Div's layout, as CSSOM does: its
    /// containing block is the viewport less its scrollbars.
    fn geometry(&mut self, dialog: NodeHandle) -> serde_json::Map<String, Value> {
        let engine = &mut self.engine;
        let bounds = engine.bounds(dialog).unwrap().unwrap();
        let root = engine.root();
        let viewport = engine.scroll_metrics(root).unwrap().unwrap();
        let (block_width, block_height) =
            (viewport.client_width as f32, viewport.client_height as f32);
        let style = engine.computed_style(dialog).unwrap().clone();
        let px = |length: Length| {
            assert!(length.is_fixed(), "{length:?} is not a length");
            length.value()
        };
        let (x, y, width, height) = (bounds.x, bounds.y, bounds.width, bounds.height);
        let left = if style.left.is_auto() {
            None
        } else {
            Some(px(style.left))
        };
        let right = if style.right.is_auto() {
            None
        } else {
            Some(px(style.right))
        };
        let top = if style.top.is_auto() {
            None
        } else {
            Some(px(style.top))
        };
        let bottom = if style.bottom.is_auto() {
            None
        } else {
            Some(px(style.bottom))
        };
        let margin_left = if style.margin_left.is_auto() {
            x - left.expect("auto horizontal margins need a left inset")
        } else {
            px(style.margin_left)
        };
        let vertical_auto = |margin: Length| match (top, bottom) {
            (Some(top), Some(_)) if margin.is_auto() => Some(y - top),
            _ if margin.is_auto() => Some(0.0),
            _ => None,
        };
        let margin_top = vertical_auto(style.margin_top).unwrap_or_else(|| px(style.margin_top));
        let margin_bottom = match (top, bottom) {
            (Some(_), Some(bottom)) if style.margin_bottom.is_auto() => {
                block_height - y - height - bottom
            }
            _ if style.margin_bottom.is_auto() => 0.0,
            _ => px(style.margin_bottom),
        };
        let top = top.unwrap_or(y - margin_top);
        let bottom = bottom.unwrap_or(block_height - y - height - margin_bottom);
        let left = left.unwrap_or(x - margin_left);
        let right = right.unwrap_or_else(|| block_width - x - width);
        let max = |length: Length| {
            if length.is_none() {
                json!("none")
            } else {
                json!(format!("{length:?}"))
            }
        };
        let mut fields = serde_json::Map::new();
        fields.insert("rect".into(), json!([x, y, width, height]));
        fields.insert(
            "client".into(),
            json!([
                engine.client_width(dialog).unwrap(),
                engine.client_height(dialog).unwrap(),
                engine.scroll_width(dialog).unwrap(),
                engine.scroll_height(dialog).unwrap(),
            ]),
        );
        fields.insert("position".into(), json!(keyword(&style.position)));
        fields.insert("display".into(), json!(keyword(&style.display)));
        fields.insert("top".into(), json!(css_px(top)));
        fields.insert("right".into(), json!(css_px(right)));
        fields.insert("bottom".into(), json!(css_px(bottom)));
        fields.insert("left".into(), json!(css_px(left)));
        fields.insert("marginTop".into(), json!(css_px(margin_top)));
        fields.insert("marginLeft".into(), json!(css_px(margin_left)));
        fields.insert("maxWidth".into(), max(style.max_width));
        fields.insert("maxHeight".into(), max(style.max_height));
        fields.insert("overflowY".into(), json!(keyword(&style.overflow_y)));
        fields.insert("paddingTop".into(), json!(css_px(px(style.padding_top))));
        fields.insert(
            "borderTopWidth".into(),
            json!(css_px(style.border_top_width as f32)),
        );
        fields.insert(
            "borderTopStyle".into(),
            json!(keyword(&style.border_top_style)),
        );
        fields.insert(
            "backgroundColor".into(),
            json!(css_color(style.background_color)),
        );
        fields.insert("color".into(), json!(css_color(style.color)));
        fields
    }

    fn dialogs(&self) -> Value {
        let mut dialogs = serde_json::Map::new();
        for name in &self.dialogs {
            let node = self.node(name);
            dialogs.insert(
                name.clone(),
                json!({
                    "open": self.engine.attribute(node, "open").unwrap().is_some(),
                    "modal": self.engine.is_modal_dialog(node).unwrap(),
                    "returnValue": self.engine.dialog_return_value(node).unwrap(),
                }),
            );
        }
        Value::Object(dialogs)
    }
}

/// A keyword as CSSOM serializes it, from the enum's name.
fn keyword(value: &impl std::fmt::Debug) -> String {
    format!("{value:?}").to_ascii_lowercase()
}

/// A pixel length as CSSOM serializes it: `105px`, `52.5px`.
fn css_px(value: f32) -> String {
    format!("{value}px")
}

/// An opaque color as CSSOM serializes it.
fn css_color(color: Color) -> String {
    let byte = |channel: f32| (channel * 255.0).round() as u8;
    assert_eq!(color.a, 1.0, "{color:?}");
    format!(
        "rgb({}, {}, {})",
        byte(color.r),
        byte(color.g),
        byte(color.b)
    )
}

/// The oracle's style for buttons and inputs.
const CONTROL: &str = "padding: 0; border: 1px solid #888888";

fn oracle() -> Value {
    serde_json::from_str(LINUX).expect("the dialog oracle is JSON")
}

/// The oracle's input events of one step, with only the fields compared.
fn input_events(events: &Value) -> Vec<Value> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| INPUT_EVENTS.contains(&event["type"].as_str().unwrap()))
        .cloned()
        .collect()
}

#[test]
fn the_replayed_scenarios_match_chromium() {
    let oracle = oracle();
    let mut failures = Vec::new();
    for name in REPLAYED {
        let mut page = Page::new(&oracle);
        // Div's engine dispatches no toggle-family event; `divui` will.
        let mut pending_differs = false;
        for (index, step) in oracle["scenarios"][name]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            for event in step["events"]
                .as_array()
                .unwrap()
                .iter()
                .chain(step["later"].as_array().unwrap())
            {
                let kind = event["type"].as_str().unwrap();
                assert!(
                    INPUT_EVENTS.contains(&kind) || PENDING_EVENTS.contains(&kind),
                    "{name} step {index}: {kind} is neither compared nor pending"
                );
            }
            let input = step["input"].as_array().unwrap();
            let events = match input[0].as_str().unwrap() {
                "click" => page.click(
                    input[2].as_f64().unwrap() as f32,
                    input[3].as_f64().unwrap() as f32,
                ),
                "key" => {
                    assert_eq!(input[1], "Tab", "{name}: only Tab is replayed here");
                    page.tab(input[2] == "s")
                }
                "do" => {
                    page.act(input[1].as_str().unwrap());
                    Vec::new()
                }
                other => panic!("{name}: unknown input {other}"),
            };
            let toggles: Vec<&Value> = step["events"]
                .as_array()
                .unwrap()
                .iter()
                .chain(step["later"].as_array().unwrap())
                .filter(|event| PENDING_EVENTS.contains(&event["type"].as_str().unwrap()))
                .collect();
            let div_toggles: Vec<&Value> = Vec::new();
            pending_differs |= toggles != div_toggles;
            let expected = input_events(&step["events"]);
            let actual = json!({
                "events": events,
                "active": page.active(),
                "dialogs": page.dialogs(),
            });
            let wanted = json!({
                "events": expected,
                "active": step["active"],
                "dialogs": step["dialogs"],
            });
            if actual != wanted {
                failures.push(format!(
                    "{name} step {index} {input:?}:\n  div:      {actual}\n  chromium: {wanted}"
                ));
                break;
            }
        }
        if !pending_differs {
            failures.push(format!(
                "{name}: its toggle-family events ({EVENTS_OWNER}) match; take them off the pending fields"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_geometry_and_computed_style_match_chromium_field_by_field() {
    let oracle = oracle();
    let mut failures = Vec::new();
    let mut pending = 0;
    let records = oracle["geometry"].as_array().unwrap();
    assert_eq!(records.len(), 15);
    for record in records {
        let width = record["viewport"][0].as_u64().unwrap();
        let name = record["dialog"].as_str().unwrap();
        let modal = record["modal"].as_bool().unwrap();
        let mut page = Page::with_viewport(&oracle, &record["viewport"]);
        let dialog = page.node(name);
        if modal {
            page.engine.show_modal(dialog).unwrap();
        } else {
            page.engine.show_dialog(dialog).unwrap();
        }
        page.engine.update().unwrap();
        let actual = page.geometry(dialog);
        let mut wanted = serde_json::Map::new();
        wanted.insert("rect".into(), record["rect"].clone());
        wanted.insert("client".into(), record["client"].clone());
        for (key, value) in record["style"].as_object().unwrap() {
            wanted.insert(key.clone(), value.clone());
        }
        for (field, expected) in &wanted {
            let Some(value) = actual.get(field) else {
                failures.push(format!("{width} {name} modal={modal}: Div has no {field}"));
                continue;
            };
            let same = same_numbers(value, expected);
            match geometry_pending(width, name, modal, field) {
                Some(owner) if same => failures.push(format!(
                    "{width} {name} modal={modal}: {field} matches; it is no longer pending on {owner}"
                )),
                Some(_) => pending += 1,
                None if !same => failures.push(format!(
                    "{width} {name} modal={modal}: {field}: div {value}, chromium {expected}"
                )),
                None => {}
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    // Nine modal records with two calc fields, and the limited boxes.
    let limited: usize = LIMITED.iter().map(|(_, fields)| fields.len()).sum();
    assert_eq!(pending, 9 * 2 + limited);
}

/// Equal JSON, comparing numbers by value (`105` and `105.0`).
fn same_numbers(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| same_numbers(l, r))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|other| same_numbers(value, other))
                })
        }
        _ => left == right,
    }
}

#[test]
fn the_replayed_list_names_captured_scenarios_once() {
    let oracle = oracle();
    let captured: BTreeSet<&str> = oracle["scenarios"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let replayed: BTreeSet<&str> = REPLAYED.iter().copied().collect();
    assert_eq!(replayed.len(), REPLAYED.len(), "a scenario is listed twice");
    let unknown: Vec<_> = replayed.difference(&captured).collect();
    assert!(unknown.is_empty(), "replayed but not captured: {unknown:?}");
}
