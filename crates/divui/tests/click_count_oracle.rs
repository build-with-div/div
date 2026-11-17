//! Click counting (`detail` and `dblclick`) against pinned Chromium 147.
//!
//! `tools/qualification/capture_click_count_oracle.py` recorded what
//! Chrome-for-Testing 147.0.7727.50 fires for real platform input: messages
//! posted to its window on Windows, XTEST on Linux (X11), at device scales 1
//! and 1.25, each capture taken twice and required to match
//! (`tests/fidelity/fixtures/click-count-v1`). Touch taps went through CDP.
//!
//! Two checks:
//!
//! - Both platforms' fixtures replay through [`ClickCounter`] and
//!   [`TapCounter`] on a simulated clock: the capture's sleeps, plus the
//!   10 ms it waits after every mouse step. The counts, as `mousedown`,
//!   `mouseup`, `click`, `auxclick` and `dblclick` details, must be
//!   Chromium's.
//! - The fixture of the platform this test runs on replays through a
//!   document, with the counts from the counters given to each input
//!   ([`PointerInput::with_click_count`]), since the document's own counter
//!   runs on the real clock. Every pointer and mouse event but the boundary
//!   events must match Chromium's type, `detail`, `button` and `buttons`.

use div::prelude::*;
use div::typed_style::parse_literal;
use div::{ClickCountRule, ClickCounter, ClickInput, DoubleClickSettings, TapCounter};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const WINDOWS: &str =
    include_str!("../../../tests/fidelity/fixtures/click-count-v1/chromium-windows.json");
const LINUX: &str =
    include_str!("../../../tests/fidelity/fixtures/click-count-v1/chromium-linux-x11.json");

/// The capture waits this long after every mouse step.
const STEP: Duration = Duration::from_millis(10);
/// And this long before every scenario.
const RESET: Duration = Duration::from_millis(1500);

/// The events the document replay compares: all but the boundary events,
/// whose order around a window's edges is not what this oracle is for.
const EVENTS: &[&str] = &[
    "pointerdown",
    "pointermove",
    "pointerup",
    "mousedown",
    "mousemove",
    "mouseup",
    "click",
    "auxclick",
    "contextmenu",
    "dblclick",
];

fn oracle(source: &str) -> Value {
    serde_json::from_str(source).unwrap()
}

fn rule_and_settings(oracle: &Value) -> (ClickCountRule, DoubleClickSettings) {
    let system = &oracle["system"];
    let ms = |key: &str| system[key].as_u64();
    match oracle["platform"].as_str().unwrap() {
        "windows" => (
            ClickCountRule::Windows,
            DoubleClickSettings {
                time: Duration::from_millis(ms("double_click_time_ms").unwrap()),
                width: ms("double_click_width").unwrap() as u32,
                height: ms("double_click_height").unwrap() as u32,
            },
        ),
        "linux-x11" => {
            assert_eq!(ms("double_click_time_ms"), Some(500));
            (ClickCountRule::Linux, DoubleClickSettings::DEFAULT)
        }
        other => panic!("unknown platform {other}"),
    }
}

fn button_of(name: &str) -> MouseButton {
    match name {
        "L" => MouseButton::Main,
        "M" => MouseButton::Auxiliary,
        "R" => MouseButton::Secondary,
        other => panic!("unknown button {other}"),
    }
}

/// One mouse input of a scenario, with its count.
#[derive(Debug, Clone, Copy)]
struct MouseStep {
    kind: PointerInputKind,
    button: MouseButton,
    modifiers: Modifiers,
    /// Device pixels from the start point.
    dx: f64,
    dy: f64,
    count: u32,
}

/// Replay a mouse scenario's steps through a counter on the simulated
/// clock. The first step is the move to the start point that begins every
/// scenario.
fn count_mouse(
    rule: ClickCountRule,
    settings: DoubleClickSettings,
    steps: &Value,
) -> Vec<MouseStep> {
    let mut counter = ClickCounter::new(rule, settings);
    let mut time = RESET;
    let mut held = 0_u32;
    let (mut dx, mut dy) = (0.0, 0.0);
    let mut out = Vec::new();
    let mut input = |kind, button, modifiers, dx, dy, held: u32, time| {
        let count = counter.count(ClickInput {
            kind,
            button,
            buttons_held: held != 0,
            modifiers,
            x: dx,
            y: dy,
            time,
        });
        out.push(MouseStep {
            kind,
            button,
            modifiers,
            dx,
            dy,
            count,
        });
    };
    input(
        PointerInputKind::Move,
        MouseButton::Main,
        Modifiers::NONE,
        0.0,
        0.0,
        0,
        time,
    );
    for step in steps.as_array().unwrap() {
        let kind = step[0].as_str().unwrap();
        match kind {
            "sleep" => time += Duration::from_millis(step[1].as_u64().unwrap()),
            "sleep-limit" => {
                let offset = step[1].as_i64().unwrap();
                let limit = settings.time.as_millis() as i64;
                time += Duration::from_millis((limit + offset) as u64);
            }
            "move" => {
                (dx, dy) = (step[1].as_f64().unwrap(), step[2].as_f64().unwrap());
                input(
                    PointerInputKind::Move,
                    MouseButton::Main,
                    Modifiers::NONE,
                    dx,
                    dy,
                    held,
                    time,
                );
            }
            "down" | "up" => {
                let button = button_of(step[1].as_str().unwrap());
                let modifiers = if step[2].as_str().unwrap().contains('s') {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                };
                let bit = 1 << (button as u32);
                let kind = if kind == "down" {
                    held |= bit;
                    PointerInputKind::Down
                } else {
                    held &= !bit;
                    PointerInputKind::Up
                };
                input(kind, button, modifiers, dx, dy, held, time);
            }
            other => panic!("unknown step {other}"),
        }
        time += STEP;
    }
    out
}

/// One tap of a touch scenario: its point (CSS pixels from the viewport's
/// centre), its count, and whether `pointerdown` is cancelled.
#[derive(Debug, Clone, Copy)]
struct TapStep {
    dx: f64,
    dy: f64,
    count: u32,
}

fn count_taps(steps: &Value) -> (Vec<TapStep>, bool) {
    let mut counter = TapCounter::new();
    let mut time = RESET;
    let mut cancelled = false;
    let mut out = Vec::new();
    for step in steps.as_array().unwrap() {
        match step[0].as_str().unwrap() {
            "sleep" => time += Duration::from_millis(step[1].as_u64().unwrap()),
            "cancel-pointerdown" => cancelled = true,
            "tap" => {
                let (dx, dy) = (step[1].as_f64().unwrap(), step[2].as_f64().unwrap());
                // CDP's touchStart and touchEnd follow each other at once.
                let count = counter.tap((dx, dy), time, time);
                out.push(TapStep { dx, dy, count });
            }
            other => panic!("unknown step {other}"),
        }
    }
    (out, cancelled)
}

fn details(events: &Value, types: &[&str]) -> Vec<(String, u64)> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| types.contains(&event["type"].as_str().unwrap()))
        .map(|event| {
            (
                event["type"].as_str().unwrap().to_owned(),
                event["detail"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// The click events a mouse scenario's counts imply: a release's `click`
/// (main button) or `auxclick` (another) with its count, and `dblclick`
/// after a main-button `click` whose count is 2.
fn mouse_details(steps: &[MouseStep]) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for step in steps {
        let count = u64::from(step.count);
        match step.kind {
            PointerInputKind::Down => out.push(("mousedown".to_owned(), count)),
            PointerInputKind::Up => {
                out.push(("mouseup".to_owned(), count));
                if step.button == MouseButton::Main {
                    out.push(("click".to_owned(), count));
                    if count == 2 {
                        out.push(("dblclick".to_owned(), 2));
                    }
                } else {
                    out.push(("auxclick".to_owned(), count));
                }
            }
            _ => assert_eq!(step.count, 0, "a move counts 0"),
        }
    }
    out
}

fn check_counters(source: &str) {
    let oracle = oracle(source);
    let (rule, settings) = rule_and_settings(&oracle);
    let mut checked = 0;
    for (scale, data) in oracle["scales"].as_object().unwrap() {
        for (name, scenario) in data["mouse"].as_object().unwrap() {
            let steps = count_mouse(rule, settings, &scenario["steps"]);
            let types = ["mousedown", "mouseup", "click", "auxclick", "dblclick"];
            assert_eq!(
                mouse_details(&steps),
                details(&scenario["events"], &types),
                "{rule:?} mouse {name} at {scale}"
            );
            checked += 1;
        }
        for (name, scenario) in data["touch"].as_object().unwrap() {
            let (taps, _) = count_taps(&scenario["steps"]);
            let mut expected = Vec::new();
            for tap in &taps {
                expected.push(("click".to_owned(), u64::from(tap.count)));
                if tap.count == 2 {
                    expected.push(("dblclick".to_owned(), 2));
                }
            }
            assert_eq!(
                expected,
                details(&scenario["events"], &["click", "dblclick"]),
                "{rule:?} touch {name} at {scale}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * (21 + 13), "every scenario at both scales");
}

#[test]
fn windows_counts_match_chromium() {
    check_counters(WINDOWS);
}

#[test]
fn linux_counts_match_chromium() {
    check_counters(LINUX);
}

type Log = Rc<RefCell<Vec<Value>>>;

/// The oracle's page: one target filling the viewport, logging the events
/// it receives, with `contextmenu` and the middle button's `mousedown`
/// cancelled, and `pointerdown` too when the scenario asks.
fn page(oracle: &Value, scale: f64, cancel_pointerdown: bool) -> (Document, Log) {
    let viewport = &oracle["viewport"];
    let (width, height) = (viewport[0].as_f64().unwrap(), viewport[1].as_f64().unwrap());
    let document = Document::new(width as i32, height as i32).unwrap();
    document
        .set_viewport(ViewportMetrics::from_logical_size(width, height, scale).unwrap())
        .unwrap();
    let target = Element::create(&document, "div").unwrap();
    target.set_position(Position::Absolute).unwrap();
    target.set_left(Length::px(0.0)).unwrap();
    target.set_top(Length::px(0.0)).unwrap();
    target.set_width(LengthValue::px(width as f32)).unwrap();
    target.set_height(LengthValue::px(height as f32)).unwrap();
    target
        .set_property(
            StyleProperty::Margin,
            parse_literal(StyleProperty::Margin, "0px").unwrap(),
        )
        .unwrap();
    document.body().append_child(&target).unwrap();
    document.update_all().unwrap();
    let log: Log = Rc::default();
    for event_type in EVENTS {
        let log = log.clone();
        target
            .on(event_type, move |event| {
                if event.phase() != Some(EventPhase::Target) {
                    return;
                }
                let mouse = event.mouse_info().unwrap();
                log.borrow_mut().push(json!({
                    "type": event.event_type,
                    "detail": mouse.detail,
                    "button": mouse.button.map_or(-1, |button| button as i32),
                    "buttons": mouse.buttons.bits(),
                }));
            })
            .unwrap();
    }
    target
        .on("contextmenu", |event| event.prevent_default())
        .unwrap();
    target
        .on("mousedown", |event| {
            if event.mouse_info().unwrap().button == Some(MouseButton::Auxiliary) {
                event.prevent_default();
            }
        })
        .unwrap();
    if cancel_pointerdown {
        target
            .on("pointerdown", |event| event.prevent_default())
            .unwrap();
    }
    (document, log)
}

fn expected(events: &Value) -> Vec<Value> {
    events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| EVENTS.contains(&event["type"].as_str().unwrap()))
        .map(|event| {
            json!({
                "type": event["type"],
                "detail": event["detail"],
                "button": event["button"],
                "buttons": event["buttons"],
            })
        })
        .collect()
}

fn check_document(source: &str) {
    let oracle = oracle(source);
    let (rule, settings) = rule_and_settings(&oracle);
    let viewport = &oracle["viewport"];
    let centre = (
        viewport[0].as_f64().unwrap() / 2.0,
        viewport[1].as_f64().unwrap() / 2.0,
    );
    for (scale_name, data) in oracle["scales"].as_object().unwrap() {
        let scale: f64 = scale_name.parse().unwrap();
        for (name, scenario) in data["mouse"].as_object().unwrap() {
            let (document, log) = page(&oracle, scale, false);
            let steps = count_mouse(rule, settings, &scenario["steps"]);
            for (index, step) in steps.iter().enumerate() {
                // The start point, in device pixels, is the centre's.
                let x = (centre.0 * scale + step.dx) / scale;
                let y = (centre.1 * scale + step.dy) / scale;
                let mut input = PointerInput::mouse(step.kind, x as f32, y as f32)
                    .with_button(step.button)
                    .with_modifiers(step.modifiers);
                if matches!(step.kind, PointerInputKind::Down | PointerInputKind::Up) {
                    input = input.with_click_count(step.count);
                }
                document.dispatch_pointer_input(input).unwrap();
                if index == 0 {
                    log.borrow_mut().clear();
                }
            }
            assert_eq!(
                *log.borrow(),
                expected(&scenario["events"]),
                "mouse {name} at {scale_name}"
            );
        }
        for (name, scenario) in data["touch"].as_object().unwrap() {
            let (taps, cancelled) = count_taps(&scenario["steps"]);
            let (document, log) = page(&oracle, scale, cancelled);
            let touch = PointerId(2);
            for tap in &taps {
                let (x, y) = ((centre.0 + tap.dx) as f32, (centre.1 + tap.dy) as f32);
                document
                    .dispatch_pointer_input(PointerInput::touch(
                        touch,
                        PointerInputKind::Down,
                        x,
                        y,
                    ))
                    .unwrap();
                document
                    .dispatch_pointer_input(
                        PointerInput::touch(touch, PointerInputKind::Up, x, y)
                            .with_click_count(tap.count),
                    )
                    .unwrap();
            }
            assert_eq!(
                *log.borrow(),
                expected(&scenario["events"]),
                "touch {name} at {scale_name}"
            );
        }
    }
}

/// The host platform's fixture through a document. On Windows
/// `contextmenu` follows the secondary button's `auxclick`; on Linux its
/// `mousedown`.
#[test]
fn document_dispatch_matches_chromium_on_this_platform() {
    if cfg!(target_os = "windows") {
        check_document(WINDOWS);
    } else {
        check_document(LINUX);
    }
}

/// The document's own counter, on the real clock: two presses at once at
/// one point make a double click, by either rule.
#[test]
fn the_document_counts_clicks_itself() {
    let oracle = oracle(WINDOWS);
    let (document, log) = page(&oracle, 1.0, false);
    for kind in [
        PointerInputKind::Move,
        PointerInputKind::Down,
        PointerInputKind::Up,
        PointerInputKind::Down,
        PointerInputKind::Up,
    ] {
        document
            .dispatch_pointer_input(PointerInput::mouse(kind, 100.0, 100.0))
            .unwrap();
    }
    let got: Vec<(String, u64)> = log
        .borrow()
        .iter()
        .filter(|event| event["type"] != "pointermove" && event["type"] != "mousemove")
        .map(|event| {
            (
                event["type"].as_str().unwrap().to_owned(),
                event["detail"].as_u64().unwrap(),
            )
        })
        .collect();
    let want: Vec<(String, u64)> = [
        ("pointerdown", 0),
        ("mousedown", 1),
        ("pointerup", 0),
        ("mouseup", 1),
        ("click", 1),
        ("pointerdown", 0),
        ("mousedown", 2),
        ("pointerup", 0),
        ("mouseup", 2),
        ("click", 2),
        ("dblclick", 2),
    ]
    .into_iter()
    .map(|(name, detail)| (name.to_owned(), detail))
    .collect();
    assert_eq!(got, want);
}
