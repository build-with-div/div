//! Pointer Events against pinned Chromium 147 (RFC 0007 section 4, Tier 0
//! item 0.12 B).
//!
//! `tools/qualification/capture_pointer_events_oracle.py` recorded what
//! Windows Chrome-for-Testing 147.0.7727.50 fires for a fixed page under CDP
//! mouse and touch input
//! (`tests/fidelity/fixtures/pointer-events-v1/chromium-windows.json`). This
//! test builds the same page, installs the same listeners, replays each
//! scenario's inputs and compares every event's type, target, `button`,
//! `buttons`, client and offset points and, for pointer events, the pointer's
//! id, type and `isPrimary`.
//!
//! Two differences are not Div's: Div's root stands for `document`, `html`
//! and `body`, so the oracle's events at those three become one event at the
//! root; and CDP reports a mouse's pressure as 0 where a real mouse reports
//! 0.5 while pressed, so pressure is left out (the RFC's defaults are pinned
//! by `pointer_defaults_follow_the_rfc`). The steps in [`GAPS`] differ for a
//! named reason and must keep differing until the gap is closed.
//!
//! The capture is from Windows, where `contextmenu` follows the secondary
//! button's release. Chromium on Linux fires it right after that button's
//! `mousedown`, with the buttons held then (the real-input click-count
//! oracle, `tests/fidelity/fixtures/click-count-v1/chromium-linux-x11.json`,
//! RFC 0007 section 6), and so does Div. On other hosts [`on_this_host`]
//! moves each expected `contextmenu` to its press.

use div::prelude::*;
use div::typed_style::{parse_literal, RendererStyleValue};
use div::EngineError;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/pointer-events-v1/chromium-windows.json");

/// The id CDP's first touch gets, and the one the platform layer gives the
/// first touch contact.
const TOUCH: PointerId = PointerId(2);

/// Steps (scenario, step index) that differ from Chromium, with the gap's
/// name and owner.
const GAPS: &[(&str, usize, &str)] = &[
    // `click` goes to the common ancestor of the press and release targets in
    // Chromium; Div clicks the release target. Click targeting is I2's (RFC
    // 0007 section 4).
    ("drag_elsewhere", 3, "click-common-ancestor (framework, I2)"),
    // Chromium updates hover after the captor's removal without input and
    // fires `lostpointercapture` then; Div does both at the pointer's next
    // event.
    ("capture_removed", 2, "hover-after-dom-change (framework)"),
    ("capture_removed", 4, "hover-after-dom-change (framework)"),
    // `touch-action: auto` pans on a touch move, which cancels the pointer;
    // Div has no `touch-action` or touch panning.
    ("touch_move", 1, "touch-action-pan (engine and framework)"),
    ("touch_move", 2, "touch-action-pan (engine and framework)"),
];

const EVENTS: &[&str] = &[
    "pointerover",
    "pointerenter",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "pointerout",
    "pointerleave",
    "gotpointercapture",
    "lostpointercapture",
    "mouseover",
    "mouseenter",
    "mousedown",
    "mousemove",
    "mouseup",
    "mouseout",
    "mouseleave",
    "click",
    "auxclick",
    "contextmenu",
    "focus",
    "blur",
];

type Log = Rc<RefCell<Vec<Value>>>;

struct Page {
    document: Document,
    elements: Vec<(&'static str, Element)>,
    log: Log,
}

impl Page {
    fn element(&self, name: &str) -> Element {
        self.elements
            .iter()
            .find(|(id, _)| *id == name)
            .unwrap()
            .1
            .clone()
    }
}

fn rect(oracle: &Value) -> (f32, f32, f32, f32) {
    let value = |index: usize| oracle[index].as_f64().unwrap() as f32;
    (value(0), value(1), value(2), value(3))
}

fn place(document: &Document, parent: &Element, tag: &str, at: (f32, f32, f32, f32)) -> Element {
    let element = Element::create(document, tag).unwrap();
    element.set_position(Position::Absolute).unwrap();
    element.set_box_sizing(BoxSizing::BorderBox).unwrap();
    element.set_left(Length::px(at.0)).unwrap();
    element.set_top(Length::px(at.1)).unwrap();
    element.set_width(LengthValue::px(at.2)).unwrap();
    element.set_height(LengthValue::px(at.3)).unwrap();
    for (property, literal) in [
        (StyleProperty::Margin, "0px"),
        (StyleProperty::Padding, "0px"),
        (StyleProperty::Border, "0px none black"),
    ] {
        element
            .set_property(property, parse_literal(property, literal).unwrap())
            .unwrap();
    }
    parent.append_child(&element).unwrap();
    element
}

/// The oracle's page, with every element logging the events it receives
/// at its target.
fn page(oracle: &Value) -> Page {
    let viewport = &oracle["viewport"];
    let document = Document::new(
        viewport[0].as_i64().unwrap() as i32,
        viewport[1].as_i64().unwrap() as i32,
    )
    .unwrap();
    let body = document.body();
    let boxes = &oracle["boxes"];
    let outer = place(&document, &body, "div", rect(&boxes["outer"]));
    let (x, y, width, height) = rect(&boxes["inner"]);
    let (outer_x, outer_y, ..) = rect(&boxes["outer"]);
    let inner = place(
        &document,
        &outer,
        "div",
        (x - outer_x, y - outer_y, width, height),
    );
    let sibling = place(&document, &body, "div", rect(&boxes["sibling"]));
    let transformed = place(&document, &body, "div", rect(&boxes["transformed"]));
    transformed
        .set_property(
            StyleProperty::Transform,
            parse_literal(StyleProperty::Transform, "scale(2)").unwrap(),
        )
        .unwrap();
    transformed
        .set_property(
            StyleProperty::TransformOrigin,
            StyleValue::Renderer(RendererStyleValue::TransformOrigin((
                Length::px(0.0),
                Length::px(0.0),
            ))),
        )
        .unwrap();
    let button = place(&document, &body, "button", rect(&oracle["button"]));
    let field = place(&document, &body, "input", rect(&oracle["field"]));
    document.update_all().unwrap();
    let elements = vec![
        ("root", body),
        ("outer", outer),
        ("inner", inner),
        ("sibling", sibling),
        ("transformed", transformed),
        ("button", button),
        ("field", field),
    ];
    let log: Log = Rc::default();
    for (name, element) in &elements {
        for event_type in EVENTS {
            let log = log.clone();
            let name = *name;
            element
                .on(event_type, move |event| {
                    if event.phase() == Some(EventPhase::Target) {
                        log.borrow_mut().push(entry(event, name));
                    }
                })
                .unwrap();
        }
    }
    Page {
        document,
        elements,
        log,
    }
}

/// An event as the oracle's logger records it.
fn entry(event: &Event, target: &str) -> Value {
    let mut entry = json!({"type": event.event_type, "target": target});
    if let Some(mouse) = event.mouse_info() {
        entry["button"] = json!(mouse.button.map_or(-1, |button| button as i32));
        entry["buttons"] = json!(mouse.buttons.bits());
        entry["client"] = json!([mouse.client.x, mouse.client.y]);
        entry["offset"] = json!([mouse.offset.x, mouse.offset.y]);
    }
    if let Some(pointer) = event.pointer_info() {
        entry["pointerId"] = json!(pointer.pointer_id.0);
        entry["pointerType"] = json!(pointer.pointer_type.as_str());
        entry["isPrimary"] = json!(pointer.is_primary);
    }
    entry
}

fn attempt(log: &Log, label: &str, result: Result<(), div::Error>) {
    let result = match result {
        Ok(()) => "ok",
        Err(div::Error::Engine(EngineError::PointerNotFound)) => "NotFoundError",
        Err(div::Error::Engine(EngineError::PointerCaptureDisconnected)) => "InvalidStateError",
        Err(error) => panic!("{label}: {error}"),
    };
    log.borrow_mut()
        .push(json!({"type": label, "result": result}));
}

fn prevent(element: &Element, event_type: &str) {
    element
        .on(event_type, |event| event.prevent_default())
        .unwrap();
}

fn capture_on_pointerdown(element: &Element) {
    let captor = element.clone();
    element
        .on("pointerdown", move |event| {
            let id = event.pointer_info().unwrap().pointer_id;
            captor.set_pointer_capture(id).unwrap();
        })
        .unwrap();
}

/// The listeners of the oracle's `SETUP` scripts, installed after the
/// logger as there.
fn setup(page: &Page, scenario: &str) {
    let inner = page.element("inner");
    let button = page.element("button");
    match scenario {
        "prevent_pointerdown" => prevent(&inner, "pointerdown"),
        "prevent_mousedown" => prevent(&button, "mousedown"),
        "prevent_pointerdown_focus" => prevent(&button, "pointerdown"),
        "prevent_mouseup" => prevent(&inner, "mouseup"),
        "capture" | "capture_removed" => capture_on_pointerdown(&inner),
        "capture_errors" => {
            let (log, element, document) = (page.log.clone(), inner.clone(), page.document.clone());
            let tried = Rc::new(std::cell::Cell::new(false));
            inner
                .on("pointermove", move |event| {
                    if tried.replace(true) {
                        return;
                    }
                    let id = event.pointer_info().unwrap().pointer_id;
                    attempt(
                        &log,
                        "set-inactive-7",
                        element.set_pointer_capture(PointerId(7)),
                    );
                    attempt(
                        &log,
                        "set-mouse-no-buttons",
                        element.set_pointer_capture(id),
                    );
                    log.borrow_mut().push(json!({
                        "type": "has-capture",
                        "result": element.has_pointer_capture(id).unwrap(),
                    }));
                    let detached = Element::create(&document, "div").unwrap();
                    attempt(&log, "set-detached", detached.set_pointer_capture(id));
                })
                .unwrap();
            let (log, element) = (page.log.clone(), inner.clone());
            inner
                .on("pointerdown", move |event| {
                    let id = event.pointer_info().unwrap().pointer_id;
                    attempt(&log, "set-pressed", element.set_pointer_capture(id));
                    log.borrow_mut().push(json!({
                        "type": "has-capture-now",
                        "result": element.has_pointer_capture(id).unwrap(),
                    }));
                })
                .unwrap();
        }
        _ => {}
    }
}

fn button_of(name: &str) -> MouseButton {
    match name {
        "left" => MouseButton::Main,
        "middle" => MouseButton::Auxiliary,
        "right" => MouseButton::Secondary,
        other => panic!("unknown button {other}"),
    }
}

/// Send one of the oracle's inputs.
fn send(page: &Page, scenario: &str, input: &[Value]) {
    let number = |index: usize| input[index].as_f64().unwrap() as f32;
    let kind = input[0].as_str().unwrap();
    let pointer = match kind {
        "move" => PointerInput::mouse(PointerInputKind::Move, number(1), number(2)),
        "down" | "up" => PointerInput::mouse(
            if kind == "down" {
                PointerInputKind::Down
            } else {
                PointerInputKind::Up
            },
            number(1),
            number(2),
        )
        .with_button(button_of(input[3].as_str().unwrap())),
        "touch" => PointerInput::touch(
            TOUCH,
            match input[1].as_str().unwrap() {
                "touchStart" => PointerInputKind::Down,
                "touchMove" => PointerInputKind::Move,
                "touchEnd" => PointerInputKind::Up,
                other => panic!("unknown touch input {other}"),
            },
            number(2),
            number(3),
        ),
        // The oracle's timer: `capture_removed` removes the captor.
        "wait" => {
            assert_eq!(scenario, "capture_removed");
            page.element("inner").remove().unwrap();
            return;
        }
        other => panic!("unknown input {other}"),
    };
    page.document.dispatch_pointer_input(pointer).unwrap();
}

/// The oracle's entries with `document`, `html` and `body` as Div's root,
/// one entry where the three received the same event, numbers as floats and
/// without pressure.
fn normalize(events: &[Value]) -> Vec<Value> {
    let mut normalized: Vec<Value> = Vec::new();
    for event in events {
        let mut event = event.clone();
        let object = event.as_object_mut().unwrap();
        object.remove("pressure");
        if let Some(target) = object.get_mut("target") {
            if matches!(target.as_str(), Some("document" | "html" | "body")) {
                *target = json!("root");
            }
        }
        for key in ["client", "offset"] {
            if let Some(point) = object.get_mut(key) {
                *point = json!([point[0].as_f64().unwrap(), point[1].as_f64().unwrap()]);
            }
        }
        let duplicate = normalized.last().is_some_and(|last| {
            last["target"] == "root" && last["type"] == event["type"] && event["target"] == "root"
        });
        if !duplicate {
            normalized.push(event);
        }
    }
    normalized
}

/// The scenario's steps as Chromium fires them on this host: on hosts other
/// than Windows, each `contextmenu` that follows a secondary-button release
/// moves to just after that button's last `mousedown`, with that event's
/// buttons, point and target.
fn on_this_host(steps: &[Value]) -> Vec<Value> {
    let mut steps = steps.to_vec();
    if cfg!(target_os = "windows") {
        return steps;
    }
    let is_secondary = |step: &Value, kind: &str| {
        let input = step["input"].as_array().unwrap();
        input[0] == kind && input.get(3).is_some_and(|button| button == "right")
    };
    let mut press: Option<usize> = None;
    for index in 0..steps.len() {
        if is_secondary(&steps[index], "down") {
            press = Some(index);
            continue;
        }
        if !is_secondary(&steps[index], "up") {
            continue;
        }
        let events = steps[index]["events"].as_array_mut().unwrap();
        let Some(at) = events
            .iter()
            .position(|event| event["type"] == "contextmenu")
        else {
            continue;
        };
        let mut menu = events.remove(at);
        let press = press.take().expect("a secondary release follows its press");
        let events = steps[press]["events"].as_array_mut().unwrap();
        let down = events
            .iter()
            .rposition(|event| event["type"] == "mousedown")
            .expect("the press fires mousedown");
        for key in ["buttons", "client", "offset", "target"] {
            menu[key] = events[down][key].clone();
        }
        events.insert(down + 1, menu);
    }
    steps
}

fn focused(page: &Page) -> &'static str {
    page.elements
        .iter()
        .skip(1)
        .find(|(_, element)| element.has_focus().unwrap())
        .map_or("body", |(name, _)| *name)
}

#[test]
fn pointer_events_match_chromium() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let scenarios = oracle["scenarios"].as_object().unwrap();
    assert_eq!(scenarios.len(), 17);
    let mut failures = Vec::new();
    let mut gaps_seen = 0;
    for (name, scenario) in scenarios {
        let page = page(&oracle);
        setup(&page, name);
        for (index, step) in on_this_host(scenario["steps"].as_array().unwrap())
            .iter()
            .enumerate()
        {
            page.log.borrow_mut().clear();
            send(&page, name, step["input"].as_array().unwrap());
            let actual = page.log.borrow().clone();
            let expected = normalize(step["events"].as_array().unwrap());
            let gap = GAPS
                .iter()
                .find(|(scenario, step, _)| *scenario == name && *step == index);
            match gap {
                Some((.., gap)) => {
                    gaps_seen += 1;
                    if actual == expected {
                        failures.push(format!(
                            "{name} step {index} now matches Chromium: remove gap {gap}"
                        ));
                    }
                }
                None if actual != expected => failures.push(format!(
                    "{name} step {index} {}:\n  Chromium {}\n  Div      {}",
                    step["input"],
                    serde_json::to_string(&expected).unwrap(),
                    serde_json::to_string(&actual).unwrap(),
                )),
                None => {}
            }
        }
        let expected_focus = scenario["focused"].as_str().unwrap();
        if focused(&page) != expected_focus {
            failures.push(format!(
                "{name}: focus is on {}, Chromium's on {expected_focus}",
                focused(&page)
            ));
        }
    }
    assert_eq!(gaps_seen, GAPS.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// RFC 0007 section 4's defaults for a device that reports only position
/// and buttons, which the oracle cannot show: CDP's mouse reports pressure 0.
#[test]
fn pointer_defaults_follow_the_rfc() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let page = page(&oracle);
    let seen = Rc::new(RefCell::new(Vec::new()));
    for event_type in ["pointermove", "pointerdown", "pointerup"] {
        let seen = seen.clone();
        page.element("inner")
            .on(event_type, move |event| {
                seen.borrow_mut()
                    .push(event.pointer_info().unwrap().clone());
            })
            .unwrap();
    }
    for kind in [
        PointerInputKind::Move,
        PointerInputKind::Down,
        PointerInputKind::Up,
    ] {
        page.document
            .dispatch_pointer_input(PointerInput::mouse(kind, 100.0, 80.0))
            .unwrap();
    }
    let seen = seen.borrow();
    let pressures: Vec<f32> = seen.iter().map(|info| info.pressure).collect();
    assert_eq!(pressures, [0.0, 0.5, 0.0]);
    for info in seen.iter() {
        assert_eq!((info.width, info.height), (1.0, 1.0));
        assert_eq!((info.tilt_x, info.tilt_y, info.twist), (0, 0, 0));
        assert_eq!(info.tangential_pressure, 0.0);
        assert_eq!(info.altitude_angle, std::f64::consts::FRAC_PI_2);
        assert_eq!(info.azimuth_angle, 0.0);
        // Headless, the screen point is the client point.
        assert_eq!(info.mouse.screen, info.mouse.client);
    }
}
