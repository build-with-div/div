//! Native wheel input against pinned Chromium 147 (Tier 0 item 0.9,
//! sample-interactions F2).
//!
//! `tools/qualification/capture_wheel_oracle.py` gave pinned Chrome real
//! platform wheel input and recorded every `wheel` event and how far the
//! scroller moved: on Windows through posted `WM_MOUSEWHEEL` and
//! `WM_MOUSEHWHEEL`, on Linux through X11 wheel buttons, each at 1x and at
//! 1.25x with `--force-device-scale-factor`. This test gives Div the same
//! input as winit reports it, converts it with `div_platform::WheelSettings`
//! as the desktop backend does, dispatches it, and compares.
#![cfg(any(feature = "windows", feature = "linux"))]

use div::prelude::*;
use div_style::Overflow;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

const WINDOWS: &str =
    include_str!("../../../tests/fidelity/fixtures/wheel-events-v1/chromium-windows.json");
const LINUX_X11: &str =
    include_str!("../../../tests/fidelity/fixtures/wheel-events-v1/chromium-linux-x11.json");

/// Where Div and Chromium still differ: (fixture, scale, step, field, Div,
/// Chromium). Each must still differ, so fixing one fails this test until
/// its row is removed.
const GAPS: &[(&str, &str, usize, &str, f64, f64)] = &[
    // `device-scale-scroll-offset`: at 1.25x Chromium snaps the scroll
    // offset to device pixels (50 CSS px is 62.5 device px, kept as 63, so
    // 50.4 CSS px). Div keeps CSS pixels at every scale.
    ("chromium-windows", "1.25", 3, "moved_y", 50.0, 50.399994),
];

#[derive(Debug, Clone, PartialEq)]
struct Logged {
    target: Option<String>,
    delta_x: f32,
    delta_y: f32,
    delta_mode: i32,
    button: i32,
    bubbles: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

/// winit 0.30.13's line deltas for the captured input: positive scrolls
/// left or up. Windows: `WM_MOUSEWHEEL` is `LineDelta(0, delta / 120)` and
/// `WM_MOUSEHWHEEL` is `LineDelta(-delta / 120, 0)`. X11: buttons 4 to 7
/// are up, down, left and right, `LineDelta(0, 1)`, `(0, -1)`, `(1, 0)` and
/// `(-1, 0)`.
fn winit_lines(input: &Value) -> (f32, f32) {
    if let Some(message) = input["message"].as_str() {
        let delta = input["wheel_delta"].as_f64().unwrap() as f32 / 120.0;
        return match message {
            "WM_MOUSEWHEEL" => (0.0, delta),
            "WM_MOUSEHWHEEL" => (-delta, 0.0),
            other => panic!("unknown message {other}"),
        };
    }
    match input["button"].as_u64().unwrap() {
        4 => (0.0, 1.0),
        5 => (0.0, -1.0),
        6 => (1.0, 0.0),
        7 => (-1.0, 0.0),
        other => panic!("unknown button {other}"),
    }
}

fn settings(oracle: &Value) -> div_platform::WheelSettings {
    match oracle["platform"].as_str().unwrap() {
        "windows" => div_platform::WheelSettings::windows(
            oracle["system"]["wheel_scroll_lines"].as_u64().unwrap() as u32,
            oracle["system"]["wheel_scroll_chars"].as_u64().unwrap() as u32,
        ),
        "linux-x11" => div_platform::WheelSettings::linux(),
        other => panic!("unknown platform {other}"),
    }
}

fn mode(mode: div_platform::WheelDeltaMode) -> WheelDeltaMode {
    match mode {
        div_platform::WheelDeltaMode::Pixel => WheelDeltaMode::Pixel,
        div_platform::WheelDeltaMode::Line => WheelDeltaMode::Line,
        div_platform::WheelDeltaMode::Page => WheelDeltaMode::Page,
    }
}

fn pair(value: &Value) -> (f64, f64) {
    (value[0].as_f64().unwrap(), value[1].as_f64().unwrap())
}

/// Every difference between Div and one oracle, as GAPS rows.
fn differences(name: &str, oracle: &Value) -> Vec<(String, String, usize, String, f64, f64)> {
    let mut found = Vec::new();
    for (scale_name, capture) in oracle["scales"].as_object().unwrap() {
        let scale: f64 = scale_name.parse().unwrap();
        let (width, height) = pair(&capture["viewport"]);
        let document = Document::with_viewport_metrics(
            ViewportMetrics::from_logical_size(width, height, scale).unwrap(),
        )
        .unwrap();
        // The page: one scroller filling the viewport, 4000px square content.
        let scroller = Element::create(&document, "div").unwrap();
        scroller.set_attribute("id", "scroller").unwrap();
        scroller.set_width(LengthValue::px(width as f32)).unwrap();
        scroller.set_height(LengthValue::px(height as f32)).unwrap();
        scroller
            .set_property(StyleProperty::Overflow, Overflow::Scroll.into())
            .unwrap();
        document.body().append_child(&scroller).unwrap();
        let content = Element::create(&document, "div").unwrap();
        content.set_attribute("id", "content").unwrap();
        content.set_width(LengthValue::px(4000.0)).unwrap();
        content.set_height(LengthValue::px(4000.0)).unwrap();
        scroller.append_child(&content).unwrap();
        document.update_all().unwrap();
        assert_eq!(
            (
                scroller.client_width().unwrap() as f64,
                scroller.client_height().unwrap() as f64
            ),
            pair(&capture["scroller_client"]),
            "{name} {scale_name}: the scroller's client size"
        );

        let log: Rc<RefCell<Vec<Logged>>> = Rc::default();
        {
            let log = log.clone();
            document
                .body()
                .on("wheel", move |event| {
                    let modifiers = Modifiers(event.modifiers as u32);
                    log.borrow_mut().push(Logged {
                        target: event
                            .target()
                            .and_then(|target| target.get_attribute("id").unwrap()),
                        delta_x: event.delta_x,
                        delta_y: event.delta_y,
                        delta_mode: event.delta_mode as i32,
                        button: event.mouse_button,
                        bubbles: event.bubbles(),
                        shift: modifiers.contains(Modifiers::SHIFT),
                        ctrl: modifiers.contains(Modifiers::CTRL),
                        alt: modifiers.contains(Modifiers::ALT),
                        meta: modifiers.contains(Modifiers::META),
                    });
                })
                .unwrap();
        }

        let settings = settings(oracle);
        let (x, y) = (width as f32 / 2.0, height as f32 * 2.0 / 3.0);
        let mut previous = (0.0, 0.0);
        let mut previous_chromium = (0.0, 0.0);
        for (index, step) in capture["steps"].as_array().unwrap().iter().enumerate() {
            let input = &step["input"];
            let (lines_x, lines_y) = winit_lines(input);
            let wheel = settings.deltas(lines_x, lines_y);
            let mut modifiers = Modifiers::NONE;
            if input["shift"].as_bool().unwrap() {
                modifiers |= Modifiers::SHIFT;
            }
            if input["control"].as_bool().unwrap() {
                modifiers |= Modifiers::CTRL;
            }
            document
                .dispatch_wheel_event_with_mode(
                    x,
                    y,
                    wheel.delta_x,
                    wheel.delta_y,
                    mode(wheel.mode),
                    modifiers,
                )
                .unwrap();
            document.update_all().unwrap();

            let expected: Vec<Logged> = step["log"]
                .as_array()
                .unwrap()
                .iter()
                .map(|event| Logged {
                    target: event["target"].as_str().map(str::to_owned),
                    delta_x: event["deltaX"].as_f64().unwrap() as f32,
                    delta_y: event["deltaY"].as_f64().unwrap() as f32,
                    delta_mode: event["deltaMode"].as_i64().unwrap() as i32,
                    button: event["button"].as_i64().unwrap() as i32,
                    bubbles: event["bubbles"].as_bool().unwrap(),
                    shift: event["shiftKey"].as_bool().unwrap(),
                    ctrl: event["ctrlKey"].as_bool().unwrap(),
                    alt: event["altKey"].as_bool().unwrap(),
                    meta: event["metaKey"].as_bool().unwrap(),
                })
                .collect();
            let logged = std::mem::take(&mut *log.borrow_mut());
            assert_eq!(
                logged, expected,
                "{name} {scale_name} step {index} ({}): the wheel events",
                step["name"]
            );

            // How far each step moved the scroller, so one gap does not
            // carry into every later offset.
            let now = (
                scroller.scroll_left().unwrap(),
                scroller.scroll_top().unwrap(),
            );
            let chromium = pair(&step["scroll"]);
            for (field, div, expected) in [
                (
                    "moved_x",
                    now.0 - previous.0,
                    chromium.0 - previous_chromium.0,
                ),
                (
                    "moved_y",
                    now.1 - previous.1,
                    chromium.1 - previous_chromium.1,
                ),
            ] {
                if (div - expected).abs() > 1e-6 {
                    found.push((
                        name.to_owned(),
                        scale_name.clone(),
                        index,
                        field.to_owned(),
                        div,
                        expected,
                    ));
                }
            }
            previous = now;
            previous_chromium = chromium;
        }
    }
    found
}

#[test]
fn native_wheel_input_matches_chromium() {
    let mut found = Vec::new();
    for (name, json) in [
        ("chromium-windows", WINDOWS),
        ("chromium-linux-x11", LINUX_X11),
    ] {
        let oracle: Value = serde_json::from_str(json).unwrap();
        assert_eq!(oracle["chromium"], "147.0.7727.50");
        found.extend(differences(name, &oracle));
    }
    let gaps: Vec<_> = GAPS
        .iter()
        .map(|(name, scale, step, field, div, chromium)| {
            ((*name, *scale, *step, *field), (*div, *chromium))
        })
        .collect();
    for (name, scale, step, field, div, chromium) in &found {
        let key = (name.as_str(), scale.as_str(), *step, field.as_str());
        let gap = gaps.iter().find(|(gap, _)| *gap == key);
        let Some((_, (gap_div, gap_chromium))) = gap else {
            panic!("{name} {scale} step {step} {field}: Div {div}, Chromium {chromium}");
        };
        assert!(
            (div - gap_div).abs() < 1e-3 && (chromium - gap_chromium).abs() < 1e-3,
            "{name} {scale} step {step} {field}: Div {div}, Chromium {chromium}, \
             but the gap says Div {gap_div}, Chromium {gap_chromium}"
        );
    }
    for ((name, scale, step, field), _) in &gaps {
        assert!(
            found
                .iter()
                .any(|f| (f.0.as_str(), f.1.as_str(), f.2, f.3.as_str())
                    == (*name, *scale, *step, *field)),
            "gap {name} {scale} step {step} {field} is fixed: remove it"
        );
    }
}

/// What the oracle cannot capture, from Chromium 147's source: Ctrl+wheel
/// zooms the page instead of scrolling
/// (`WebMouseWheelEvent::GetPlatformSpecificDefaultEventAction`), a line is
/// 40 pixels and a page 87.5% of the smaller of the scroller and the
/// viewport (`cc::InputHandler::ResolveScrollGranularityToPixels`).
#[test]
fn ctrl_does_not_scroll_and_lines_and_pages_resolve_to_pixels() {
    let document = Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(200.0, 160.0, 1.0).unwrap(),
    )
    .unwrap();
    let root = document.body();
    root.set_property(StyleProperty::Overflow, Overflow::Scroll.into())
        .unwrap();
    let content = Element::create(&document, "div").unwrap();
    content.set_width(LengthValue::px(4000.0)).unwrap();
    content.set_height(LengthValue::px(4000.0)).unwrap();
    root.append_child(&content).unwrap();
    document.update_all().unwrap();
    let modes = Rc::new(RefCell::new(Vec::new()));
    {
        let modes = modes.clone();
        root.on("wheel", move |event| {
            modes.borrow_mut().push((event.delta_y, event.delta_mode))
        })
        .unwrap();
    }
    let offset = || (root.scroll_left().unwrap(), root.scroll_top().unwrap());
    // The root's client area, inside its scrollbars, is smaller than the
    // 200x160 viewport, so it sets the page.
    let client_width = root.client_width().unwrap() as f64;
    let client_height = root.client_height().unwrap() as f64;
    assert!(client_width < 200.0 && client_height < 160.0);
    let page_y = 80.0 + 0.875 * client_height;
    let page_x = 0.875 * client_width;

    document
        .dispatch_wheel_event(50.0, 50.0, 0.0, 100.0, Modifiers::CTRL)
        .unwrap();
    assert_eq!(offset(), (0.0, 0.0), "Ctrl+wheel does not scroll");
    document
        .dispatch_wheel_event_with_mode(50.0, 50.0, 0.0, 2.0, WheelDeltaMode::Line, Modifiers::NONE)
        .unwrap();
    assert_eq!(offset(), (0.0, 80.0), "two lines are 80px");
    document
        .dispatch_wheel_event_with_mode(50.0, 50.0, 0.0, 1.0, WheelDeltaMode::Page, Modifiers::NONE)
        .unwrap();
    assert_eq!(
        offset(),
        (0.0, page_y),
        "a page is 87.5% of the client height"
    );
    document
        .dispatch_wheel_event_with_mode(
            50.0,
            50.0,
            0.0,
            1.0,
            WheelDeltaMode::Page,
            Modifiers::SHIFT,
        )
        .unwrap();
    assert_eq!(
        offset(),
        (page_x, page_y),
        "Shift pages horizontally, 87.5% of the client width"
    );
    assert_eq!(
        *modes.borrow(),
        [
            (100.0, WheelDeltaMode::Pixel),
            (2.0, WheelDeltaMode::Line),
            (1.0, WheelDeltaMode::Page),
            (1.0, WheelDeltaMode::Page),
        ]
    );
}
