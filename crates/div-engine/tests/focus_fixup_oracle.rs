//! The `focus-fixup-v1` oracle's dialog scenarios, replayed through the
//! engine (H4, RFC 0015).
//!
//! `tools/qualification/capture_focus_fixup_oracle.py` recorded pinned
//! Chromium 147 with real input (`tests/fidelity/fixtures/focus-fixup-v1/
//! chromium-linux-x11.json`). `crates/divui/tests/focus_fixup_oracle.rs`
//! replays its other scenarios with every event. These two open a dialog
//! with `show()` or `showModal()`, which `divui` gains in RFC 0015 PR 5, so
//! this test replays them here, driving the engine as `divui` does:
//!
//! - a press hit-tests, takes the press focus as `mousedown`'s default
//!   action, and clicks the activation target on release;
//! - a key goes to the focused element, else `body`, and Tab moves focus
//!   with [`Engine::advance_focus`];
//! - a frame updates style and layout, then blurs a focused element that
//!   can no longer take focus, as `divui`'s `Document::begin_frame` does.
//!
//! After each step it compares, exactly, the targets of the input events,
//! the focused element at each of the page action's marks (right after the
//! mutation, after a forced layout, and two frames later), the focused
//! element and each dialog's `open` and `:modal`. The `focus`, `blur`,
//! `focusin` and `focusout` events are `divui`'s: they are the one field
//! not compared, and these scenarios move to the `divui` replay with PR 5.

use std::collections::{BTreeMap, HashMap};

use div_dom::ElementTag;
use div_engine::{Engine, FocusOrigin, NodeHandle, PointerEventKind, PressFocus};
use div_geometry::ViewportMetrics;
use serde_json::{json, Value};

const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/focus-fixup-v1/chromium-linux-x11.json");

/// The scenarios replayed here, each with every step.
const REPLAYED: &[&str] = &["remove_open_non_modal", "remove_open_modal"];

/// The input events this test compares.
const INPUT_EVENTS: &[&str] = &["mousedown", "mouseup", "click", "keydown", "keyup"];

/// `divui`'s events, which the engine does not dispatch.
const FOCUS_EVENTS: &[&str] = &["focus", "blur", "focusin", "focusout"];

/// The oracle's style for buttons.
const CONTROL: &str = "box-sizing: border-box; margin: 0; padding: 0; \
                       border: 1px solid #888888; font-size: 12px";

struct Page {
    engine: Engine,
    body: NodeHandle,
    by_name: BTreeMap<String, NodeHandle>,
    names: HashMap<NodeHandle, String>,
    dialogs: Vec<String>,
}

impl Page {
    fn new(oracle: &Value) -> Self {
        let viewport = oracle["viewport"].as_array().unwrap();
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
        for spec in oracle["row"].as_array().unwrap() {
            page.placed(oracle, spec);
        }
        for spec in oracle["dialogs"].as_array().unwrap() {
            let dialog = page
                .engine
                .create_native_element_named(ElementTag::Div, "dialog")
                .unwrap();
            page.engine.append_child(body, dialog).unwrap();
            page.name(dialog, spec[0].as_str().unwrap());
            page.style(dialog, "font-size: 12px");
            page.dialogs.push(spec[0].as_str().unwrap().to_owned());
            let child = page
                .engine
                .create_native_element(ElementTag::Button)
                .unwrap();
            page.engine.append_child(dialog, child).unwrap();
            page.name(child, spec[1].as_str().unwrap());
            page.style(
                child,
                &format!("display: block; width: 60px; height: 24px; {CONTROL}"),
            );
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

    /// The oracle's row: `[id, tag, left, top, width, height, extra]`, with
    /// `w` inside `wrap`.
    fn placed(&mut self, oracle: &Value, spec: &Value) {
        let (name, tag) = (spec[0].as_str().unwrap(), spec[1].as_str().unwrap());
        let node = match tag {
            "button" => self.engine.create_native_element(ElementTag::Button),
            "div" => self.engine.create_native_element(ElementTag::Div),
            other => panic!("the oracle page has no {other}"),
        }
        .unwrap();
        self.engine.append_child(self.body, node).unwrap();
        self.name(node, name);
        let own = match (tag, name) {
            ("button", _) => CONTROL,
            (_, "t") => "box-sizing: border-box; margin: 0; border: 1px solid #888888",
            _ => "box-sizing: border-box; margin: 0",
        };
        let style = format!(
            "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; {own}",
            spec[2], spec[3], spec[4], spec[5]
        );
        self.style(node, &style);
        match spec[6].as_str().unwrap() {
            "" => {}
            r#" tabindex="0""# => self.engine.set_attribute(node, "tabindex", "0").unwrap(),
            other => panic!("unknown attributes {other}"),
        }
        if name == "wrap" {
            let wrapped = &oracle["wrapped"];
            let inner = self
                .engine
                .create_native_element(ElementTag::Button)
                .unwrap();
            self.engine.append_child(node, inner).unwrap();
            self.name(inner, wrapped[0].as_str().unwrap());
            let style = format!(
                "width: {}px; height: {}px; {CONTROL}",
                wrapped[2], wrapped[3]
            );
            self.style(inner, &style);
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

    /// One frame as `divui`'s `begin_frame` ends it: style and layout, then
    /// the focus fixup.
    fn frame(&mut self) {
        self.engine.update().unwrap();
        if let Some(focused) = self.engine.focused() {
            if !self.engine.can_focus(focused).unwrap() {
                self.engine.blur(focused).unwrap();
            }
        }
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

    fn tab(&mut self) -> Vec<Value> {
        let mut events =
            vec![json!({"type": "keydown", "target": self.active(), "key": "Tab", "shift": false})];
        self.engine.advance_focus(1).unwrap();
        events
            .push(json!({"type": "keyup", "target": self.active(), "key": "Tab", "shift": false}));
        events
    }

    fn mark(&self, at: &str) -> Value {
        json!({"type": "mark", "at": at, "active": self.active()})
    }

    /// A page action, with the capture script's marks for a mutation.
    fn act(&mut self, action: &str) -> Vec<Value> {
        match action {
            "show-d1" => {
                let d1 = self.node("d1");
                self.engine.show_dialog(d1).unwrap();
                Vec::new()
            }
            "show-modal-d2" => {
                let d2 = self.node("d2");
                self.engine.show_modal(d2).unwrap();
                Vec::new()
            }
            other => {
                let dialog = other
                    .strip_prefix("remove-open-")
                    .unwrap_or_else(|| panic!("action {other} is not replayed here"));
                let dialog = self.node(dialog);
                self.engine.remove_attribute(dialog, "open").unwrap();
                let mut marks = vec![self.mark("sync")];
                self.engine.update().unwrap();
                marks.push(self.mark("layout"));
                self.frame();
                self.frame();
                marks.push(self.mark("frames"));
                marks
            }
        }
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
                }),
            );
        }
        Value::Object(dialogs)
    }
}

fn oracle() -> Value {
    serde_json::from_str(LINUX).expect("the focus fixup oracle is JSON")
}

#[test]
fn the_dialog_scenarios_match_chromium() {
    let oracle = oracle();
    let mut failures = Vec::new();
    for name in REPLAYED {
        let mut page = Page::new(&oracle);
        for (index, step) in oracle["scenarios"][name]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let mut wanted_events = Vec::new();
            for event in step["events"].as_array().unwrap() {
                let kind = event["type"].as_str().unwrap();
                if INPUT_EVENTS.contains(&kind) || kind == "mark" {
                    let mut event = event.clone();
                    event.as_object_mut().unwrap().remove("code");
                    wanted_events.push(event);
                } else {
                    assert!(
                        FOCUS_EVENTS.contains(&kind),
                        "{name} step {index}: {kind} is neither compared nor divui's"
                    );
                }
            }
            let input = step["input"].as_array().unwrap();
            let events = match input[0].as_str().unwrap() {
                "click" => page.click(
                    input[2].as_f64().unwrap() as f32,
                    input[3].as_f64().unwrap() as f32,
                ),
                "key" => {
                    assert_eq!(input[1], "Tab", "{name}: only Tab is replayed here");
                    page.tab()
                }
                "do" => page.act(input[1].as_str().unwrap()),
                other => panic!("{name}: unknown input {other}"),
            };
            // The capture waits while the page renders frames.
            page.frame();
            page.frame();
            let actual = json!({
                "events": events,
                "active": page.active(),
                "dialogs": page.dialogs(),
            });
            let wanted = json!({
                "events": wanted_events,
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
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
