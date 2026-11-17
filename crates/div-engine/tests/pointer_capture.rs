//! Pointer capture and event offsets, as Chromium 147's
//! `PointerEventManager` keeps them (`pointer_event_manager.cc`):
//! `setPointerCapture` makes capture pending, the pointer's next event makes
//! it active, an up or cancel releases it, and a removed captor loses it to
//! the document.
//!
//! The page is the one of `tools/qualification/capture_pointer_events_oracle.py`
//! (`tests/fidelity/fixtures/pointer-events-v1/chromium-windows.json`):
//! `outer` (20, 20, 200x150) holds `inner` (60, 50, 80x60), `sibling` is at
//! (240, 20, 120x80) and `transformed` (240, 140, 40x30) is scaled by 2
//! about its top-left corner.
use div_dom::ElementTag;
use div_engine::{
    CaptureChange, Engine, EngineError, NodeHandle, PointerEventKind, ViewportMetrics,
};
use div_geometry::Length;
use div_style::{parse_literal, RendererStyleValue, StyleProperty, StyleValue};

const MOUSE: u64 = 1;
const INNER: (f32, f32) = (100.0, 80.0);
const SIBLING: (f32, f32) = (300.0, 60.0);

fn set(engine: &mut Engine, node: NodeHandle, property: StyleProperty, literal: &str) {
    let value = parse_literal(property, literal)
        .unwrap_or_else(|error| panic!("{property:?} {literal}: {error:?}"));
    engine.set_property(node, property, value).unwrap();
}

fn place(engine: &mut Engine, parent: NodeHandle, rect: [f32; 4]) -> NodeHandle {
    let node = engine.create_native_element(ElementTag::Div).unwrap();
    engine.append_child(parent, node).unwrap();
    set(engine, node, StyleProperty::Position, "absolute");
    for (property, value) in [
        (StyleProperty::Left, rect[0]),
        (StyleProperty::Top, rect[1]),
        (StyleProperty::Width, rect[2]),
        (StyleProperty::Height, rect[3]),
    ] {
        set(engine, node, property, &format!("{value}px"));
    }
    node
}

struct Page {
    engine: Engine,
    root: NodeHandle,
    outer: NodeHandle,
    inner: NodeHandle,
    sibling: NodeHandle,
    transformed: NodeHandle,
}

fn page() -> Page {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(400.0, 300.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    let outer = place(&mut engine, root, [20.0, 20.0, 200.0, 150.0]);
    let inner = place(&mut engine, outer, [40.0, 30.0, 80.0, 60.0]);
    let sibling = place(&mut engine, root, [240.0, 20.0, 120.0, 80.0]);
    let transformed = place(&mut engine, root, [240.0, 140.0, 40.0, 30.0]);
    set(
        &mut engine,
        transformed,
        StyleProperty::Transform,
        "scale(2)",
    );
    engine
        .set_property(
            transformed,
            StyleProperty::TransformOrigin,
            StyleValue::Renderer(RendererStyleValue::TransformOrigin((
                Length::px(0.0),
                Length::px(0.0),
            ))),
        )
        .unwrap();
    Page {
        engine,
        root,
        outer,
        inner,
        sibling,
        transformed,
    }
}

fn event(
    engine: &mut Engine,
    kind: PointerEventKind,
    at: (f32, f32),
    buttons: u16,
) -> div_engine::PointerUpdate {
    engine
        .pointer_event_with_buttons(MOUSE, kind, at.0, at.1, buttons)
        .unwrap()
}

#[test]
fn capture_is_pending_until_the_pointers_next_event() {
    let Page {
        mut engine,
        root,
        outer,
        inner,
        sibling,
        ..
    } = page();
    event(&mut engine, PointerEventKind::Move, INNER, 0);
    event(&mut engine, PointerEventKind::Down, INNER, 1);
    engine.set_pointer_capture(MOUSE, inner).unwrap();
    // `hasPointerCapture` sees the pending capture at once.
    assert!(engine.has_pointer_capture(MOUSE, inner));
    assert_eq!(engine.pointer_capture(MOUSE), None);

    // The oracle's `capture` scenario: the move to the sibling first fires
    // `gotpointercapture`, then goes to `inner`, and no boundary is crossed.
    let update = event(&mut engine, PointerEventKind::Move, SIBLING, 1);
    assert_eq!(update.capture_changes, [CaptureChange::Got(inner)]);
    assert_eq!(update.target, Some(inner));
    assert_eq!(update.previous_target, Some(inner));
    assert!(update.left.is_empty() && update.entered.is_empty());
    assert_eq!(engine.pointer_capture(MOUSE), Some(inner));

    // The up still goes to the captor. Capture ends only when the caller
    // releases it after dispatching the up, with `lostpointercapture`.
    let update = event(&mut engine, PointerEventKind::Up, SIBLING, 0);
    assert_eq!(update.target, Some(inner));
    assert!(update.capture_changes.is_empty());
    assert_eq!(update.activation, Some(inner));
    assert_eq!(
        engine.end_pointer_capture(MOUSE),
        [CaptureChange::Lost(inner)]
    );
    assert_eq!(engine.pointer_capture(MOUSE), None);
    assert!(!engine.has_pointer_capture(MOUSE, inner));

    // A fresh hit test then crosses from `inner` to the sibling.
    let update = engine.pointer_rehover(MOUSE, SIBLING.0, SIBLING.1).unwrap();
    assert_eq!(update.previous_target, Some(inner));
    assert_eq!(update.target, Some(sibling));
    assert_eq!(update.left, [inner, outer]);
    assert_eq!(update.entered, [sibling]);
    assert!(!update.left.contains(&root));
    assert!(engine.end_pointer_capture(MOUSE).is_empty());
}

#[test]
fn set_pointer_capture_checks_the_pointer_and_the_node() {
    let Page {
        mut engine, inner, ..
    } = page();
    // The oracle's `capture_errors` scenario.
    assert_eq!(
        engine.set_pointer_capture(MOUSE, inner),
        Err(EngineError::PointerNotFound)
    );
    event(&mut engine, PointerEventKind::Move, INNER, 0);
    assert_eq!(
        engine.set_pointer_capture(7, inner),
        Err(EngineError::PointerNotFound)
    );
    assert_eq!(
        engine.release_pointer_capture(7, inner),
        Err(EngineError::PointerNotFound)
    );
    // Without a pressed button the call succeeds and does nothing.
    engine.set_pointer_capture(MOUSE, inner).unwrap();
    assert!(!engine.has_pointer_capture(MOUSE, inner));
    let detached = engine.create_native_element(ElementTag::Div).unwrap();
    assert_eq!(
        engine.set_pointer_capture(MOUSE, detached),
        Err(EngineError::PointerCaptureDisconnected)
    );
    event(&mut engine, PointerEventKind::Down, INNER, 1);
    engine.set_pointer_capture(MOUSE, inner).unwrap();
    assert!(engine.has_pointer_capture(MOUSE, inner));
    // Releasing before the next event cancels the pending capture.
    engine.release_pointer_capture(MOUSE, inner).unwrap();
    assert!(!engine.has_pointer_capture(MOUSE, inner));
    let update = event(&mut engine, PointerEventKind::Up, INNER, 0);
    assert!(update.capture_changes.is_empty());
}

#[test]
fn a_removed_captor_loses_capture_to_the_root() {
    let Page {
        mut engine,
        root,
        inner,
        sibling,
        ..
    } = page();
    event(&mut engine, PointerEventKind::Move, INNER, 0);
    event(&mut engine, PointerEventKind::Down, INNER, 1);
    engine.set_pointer_capture(MOUSE, inner).unwrap();
    let update = event(&mut engine, PointerEventKind::Move, SIBLING, 1);
    assert_eq!(update.capture_changes, [CaptureChange::Got(inner)]);
    engine.detach(inner).unwrap();
    assert_eq!(engine.pointer_capture(MOUSE), None);
    // The oracle's `capture_removed` scenario fires `lostpointercapture` at
    // the document, which the root stands for.
    let update = event(&mut engine, PointerEventKind::Move, SIBLING, 1);
    assert_eq!(update.capture_changes, [CaptureChange::Lost(root)]);
    assert_eq!(update.target, Some(sibling));
    assert!(event(&mut engine, PointerEventKind::Move, SIBLING, 1)
        .capture_changes
        .is_empty());
}

#[test]
fn a_cancel_keeps_capture_until_the_caller_releases_it() {
    let Page {
        mut engine, inner, ..
    } = page();
    event(&mut engine, PointerEventKind::Down, INNER, 1);
    engine.set_pointer_capture(MOUSE, inner).unwrap();
    event(&mut engine, PointerEventKind::Move, SIBLING, 1);
    // `pointercancel` goes to the captor, then `lostpointercapture`.
    let update = event(&mut engine, PointerEventKind::Cancel, SIBLING, 0);
    assert_eq!(update.target, Some(inner));
    assert_eq!(
        engine.end_pointer_capture(MOUSE),
        [CaptureChange::Lost(inner)]
    );
    engine.forget_pointer(MOUSE);
    assert_eq!(
        engine.set_pointer_capture(MOUSE, inner),
        Err(EngineError::PointerNotFound)
    );
}

#[test]
fn local_points_are_in_the_padding_box_through_transforms() {
    let Page {
        mut engine,
        root,
        outer,
        inner,
        transformed,
        ..
    } = page();
    // The oracle's offsets: (40, 30) in `inner` and (80, 60) in `outer` at
    // (100, 80); (5, 5) in `transformed` at (250, 150); the root gets the
    // client point.
    assert_eq!(
        engine.local_point(inner, 100.0, 80.0).unwrap(),
        (40.0, 30.0)
    );
    assert_eq!(
        engine.local_point(outer, 100.0, 80.0).unwrap(),
        (80.0, 60.0)
    );
    assert_eq!(
        engine.local_point(root, 100.0, 80.0).unwrap(),
        (100.0, 80.0)
    );
    assert_eq!(
        engine.local_point(transformed, 250.0, 150.0).unwrap(),
        (5.0, 5.0)
    );
    // Outside the box the point is still mapped: (300, 60) is (240, 10) in
    // `inner`, as the oracle's captured move reports.
    assert_eq!(
        engine.local_point(inner, 300.0, 60.0).unwrap(),
        (240.0, 10.0)
    );
    // A border moves the padding box.
    set(&mut engine, inner, StyleProperty::Border, "3px solid black");
    assert_eq!(
        engine.local_point(inner, 100.0, 80.0).unwrap(),
        (37.0, 27.0)
    );
}
