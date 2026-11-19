//! `:active` covers the pressed element and its ancestors, and a disabled
//! pressed element too, as pinned Chromium 147 reports through real mouse
//! input (RFC 0001, state sources). The oracle is
//! `tools/qualification/capture_active_chain_oracle.py`.

use div::prelude::*;
use div::{PointerInput, PointerInputKind};
use std::collections::BTreeSet;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/active-chain-v1/chromium-windows.json");

fn boxed(
    document: &Document,
    parent: &Element,
    tag: &str,
    id: &str,
    rect: (f32, f32, f32, f32),
) -> Element {
    let element = Element::create(document, tag).unwrap();
    element.set_attribute("id", id).unwrap();
    element.set_position(Position::Absolute).unwrap();
    element.set_box_sizing(BoxSizing::BorderBox).unwrap();
    element.set_left(Length::px(rect.0)).unwrap();
    element.set_top(Length::px(rect.1)).unwrap();
    element.set_width(LengthValue::px(rect.2)).unwrap();
    element.set_height(LengthValue::px(rect.3)).unwrap();
    parent.append_child(&element).unwrap();
    element
}

/// The oracle page's tree. Div has no `html` element: its root plays
/// `body`, so `html` is left out of the comparison.
fn page() -> (Document, Vec<(&'static str, Element)>) {
    let document = Document::new(200, 200).unwrap();
    let body = document.body();
    let outer = boxed(&document, &body, "div", "outer", (0.0, 0.0, 200.0, 100.0));
    let middle = boxed(
        &document,
        &outer,
        "div",
        "middle",
        (10.0, 10.0, 100.0, 80.0),
    );
    let inner = boxed(&document, &middle, "div", "inner", (10.0, 10.0, 40.0, 40.0));
    let sibling = boxed(
        &document,
        &outer,
        "div",
        "sibling",
        (130.0, 10.0, 40.0, 40.0),
    );
    let controls = boxed(
        &document,
        &body,
        "div",
        "controls",
        (0.0, 100.0, 200.0, 100.0),
    );
    let enabled = boxed(
        &document,
        &controls,
        "button",
        "enabled",
        (10.0, 10.0, 60.0, 30.0),
    );
    let disabled = boxed(
        &document,
        &controls,
        "button",
        "disabled",
        (100.0, 10.0, 60.0, 30.0),
    );
    disabled.set_attribute("disabled", "").unwrap();
    document.update_all().unwrap();
    let elements = vec![
        ("body", body),
        ("outer", outer),
        ("middle", middle),
        ("inner", inner),
        ("sibling", sibling),
        ("controls", controls),
        ("enabled", enabled),
        ("disabled", disabled),
    ];
    (document, elements)
}

#[test]
fn active_chain_matches_chromium() {
    let oracle: serde_json::Value = serde_json::from_str(ORACLE).unwrap();
    let (document, elements) = page();
    let steps = oracle["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 7);
    for step in steps {
        for event in step["events"].as_array().unwrap() {
            let kind = match event[0].as_str().unwrap() {
                "press" => PointerInputKind::Down,
                "move" => PointerInputKind::Move,
                "release" => PointerInputKind::Up,
                other => panic!("unknown event {other}"),
            };
            let x = event[1].as_f64().unwrap() as f32;
            let y = event[2].as_f64().unwrap() as f32;
            document
                .dispatch_pointer_input(PointerInput::mouse(kind, x, y))
                .unwrap();
        }
        let expected: BTreeSet<&str> = step["active"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap())
            .filter(|id| *id != "html")
            .collect();
        let actual: BTreeSet<&str> = elements
            .iter()
            .filter(|(_, element)| element.is_active().unwrap())
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(actual, expected, "step {}", step["step"]);
    }
}

/// A disabled button becomes active when pressed but is not activated.
#[test]
fn a_pressed_disabled_button_is_active_but_not_clicked() {
    let (document, elements) = page();
    let disabled = &elements.iter().find(|(id, _)| *id == "disabled").unwrap().1;
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = clicks.clone();
    disabled
        .on("click", move |_| counted.set(counted.get() + 1))
        .unwrap();
    for kind in [PointerInputKind::Down, PointerInputKind::Up] {
        document
            .dispatch_pointer_input(PointerInput::mouse(kind, 130.0, 125.0))
            .unwrap();
        if kind == PointerInputKind::Down {
            assert!(disabled.is_active().unwrap());
        }
    }
    assert!(!disabled.is_active().unwrap());
    assert_eq!(clicks.get(), 0);
}

const REMOVAL_ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/active-chain-removal-v1/chromium-windows.json");

/// Scenarios where Div still differs from the removal oracle. Moving a
/// connected element keeps its press state in Div, where Chromium handles
/// the move as a removal (gap `moved-node-press-state`).
const REMOVAL_GAPS: &[&str] = &["move the pressed element"];

/// The oracle's script steps, through the public API. `detached` is the
/// element a step takes out.
fn run_script(script: &str, elements: &[(&'static str, Element)], detached: &mut Option<Element>) {
    let find = |id: &str| {
        elements
            .iter()
            .find(|(name, _)| *name == id)
            .unwrap()
            .1
            .clone()
    };
    for id in ["inner", "middle", "enabled"] {
        let element = find(id);
        if script == format!("detached = document.getElementById('{id}'); detached.remove()") {
            element.detach().unwrap();
            *detached = Some(element);
            return;
        }
        if script
            == format!(
                "detached = document.getElementById('{id}'); detached.style.display = 'none'"
            )
        {
            element.set_display(Display::None).unwrap();
            *detached = Some(element);
            return;
        }
    }
    match script {
        "document.getElementById('sibling').append(detached)" => {
            find("sibling")
                .append_child(detached.as_ref().unwrap())
                .unwrap();
        }
        "detached = document.getElementById('inner'); document.getElementById('sibling').append(detached)" =>
        {
            let inner = find("inner");
            find("sibling").append_child(&inner).unwrap();
            *detached = Some(inner);
        }
        other => panic!("unknown script {other}"),
    }
}

/// One scenario's steps as Div reports them: the `:active` ids, whether
/// the detached element matches `:active`, and the events' targets.
fn removal_scenario(steps: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let (document, elements) = page();
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    for event_type in ["pointerdown", "mousedown", "pointerup", "mouseup", "click"] {
        let log = log.clone();
        document
            .body()
            .on_capture(event_type, move |event| {
                let target = event
                    .target()
                    .and_then(|target| target.get_attribute("id").unwrap())
                    .unwrap_or_else(|| "body".to_owned());
                log.borrow_mut().push(format!("{event_type} {target}"));
            })
            .unwrap();
    }
    let mut detached: Option<Element> = None;
    let mut actual = Vec::new();
    for step in steps {
        let step = step["step"].as_array().unwrap();
        let name = step[0].as_str().unwrap();
        if name == "script" {
            run_script(step[1].as_str().unwrap(), &elements, &mut detached);
        } else {
            let kind = match name {
                "press" => PointerInputKind::Down,
                "move" => PointerInputKind::Move,
                "release" => PointerInputKind::Up,
                other => panic!("unknown event {other}"),
            };
            let x = step[1].as_f64().unwrap() as f32;
            let y = step[2].as_f64().unwrap() as f32;
            document
                .dispatch_pointer_input(PointerInput::mouse(kind, x, y))
                .unwrap();
        }
        document.update_all().unwrap();
        let mut active: Vec<&str> = elements
            .iter()
            .filter(|(_, element)| element.is_active().unwrap())
            .map(|(id, _)| *id)
            .collect();
        active.sort_unstable();
        actual.push(serde_json::json!({
            "active": active,
            "detached_active": detached.as_ref().map(|element| element.is_active().unwrap()),
            "events": log.borrow_mut().drain(..).collect::<Vec<_>>(),
        }));
    }
    actual
}

/// Removing the pressed element or an ancestor while the button is held
/// hands `:active` to the removed subtree's former parent, and the release
/// clicks nothing; hiding them changes neither. Pinned Chromium 147 through
/// CDP mouse input, read after each step's next frame.
#[test]
fn removing_the_pressed_element_matches_chromium() {
    let oracle: serde_json::Value = serde_json::from_str(REMOVAL_ORACLE).unwrap();
    let scenarios = oracle["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 8);
    for scenario in scenarios {
        let name = scenario["scenario"].as_str().unwrap();
        let steps = scenario["steps"].as_array().unwrap();
        let expected: Vec<serde_json::Value> = steps
            .iter()
            .map(|step| {
                let mut active: Vec<&str> = step["active"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .filter(|id| *id != "html")
                    .collect();
                active.sort_unstable();
                serde_json::json!({
                    "active": active,
                    "detached_active": step["detached_active"],
                    "events": step["events"],
                })
            })
            .collect();
        let actual = removal_scenario(steps);
        if REMOVAL_GAPS.contains(&name) {
            assert_ne!(actual, expected, "gap scenario {name} now matches");
        } else {
            assert_eq!(actual, expected, "scenario {name}");
        }
    }
}
