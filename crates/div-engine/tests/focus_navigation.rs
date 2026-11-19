//! Keyboard-focusable scrollers, a press's focus and the sequential focus
//! navigation starting point (`focus_navigation.rs`), as the real-input
//! oracle `tests/fidelity/fixtures/focus-navigation-v1/` measured pinned
//! Chromium 147. Every scroller here is 100x60 with 80px-wide content, so
//! one that overflows vertically has a 15px vertical scrollbar only.
use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle, PointerEventKind, PressFocus};
use div_geometry::ViewportMetrics;
use div_style::{Display, Edges, LengthValue, Overflow, Position, StyleProperty, StyleValue};

fn set(
    engine: &mut Engine,
    node: NodeHandle,
    property: StyleProperty,
    value: impl Into<StyleValue>,
) {
    engine.set_property(node, property, value.into()).unwrap();
}

fn place(
    engine: &mut Engine,
    parent: NodeHandle,
    tag: ElementTag,
    at: (f32, f32, f32, f32),
) -> NodeHandle {
    let node = engine.create_native_element(tag).unwrap();
    set(engine, node, StyleProperty::Display, Display::Block);
    set(engine, node, StyleProperty::Position, Position::Absolute);
    set(engine, node, StyleProperty::Left, LengthValue::px(at.0));
    set(engine, node, StyleProperty::Top, LengthValue::px(at.1));
    set(engine, node, StyleProperty::Width, LengthValue::px(at.2));
    set(engine, node, StyleProperty::Height, LengthValue::px(at.3));
    set(
        engine,
        node,
        StyleProperty::Margin,
        Edges::all(LengthValue::px(0.0)),
    );
    engine.append_child(parent, node).unwrap();
    node
}

/// A 100x60 scroller at (`left`, `top`) whose content is 80 wide and
/// `content_height` tall.
fn scroller(
    engine: &mut Engine,
    left: f32,
    top: f32,
    overflow: (Overflow, Overflow),
    content_height: f32,
) -> (NodeHandle, NodeHandle) {
    let root = engine.root();
    let node = place(engine, root, ElementTag::Div, (left, top, 100.0, 60.0));
    set(engine, node, StyleProperty::OverflowX, overflow.0);
    set(engine, node, StyleProperty::OverflowY, overflow.1);
    let content = place(
        engine,
        node,
        ElementTag::Div,
        (0.0, 0.0, 80.0, content_height),
    );
    (node, content)
}

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap()).unwrap()
}

fn press(engine: &mut Engine, x: f32, y: f32) {
    engine
        .pointer_event(1, PointerEventKind::Down, x, y)
        .unwrap();
    engine.pointer_event(1, PointerEventKind::Up, x, y).unwrap();
}

const AUTO: (Overflow, Overflow) = (Overflow::Auto, Overflow::Auto);

#[test]
fn a_scroller_is_focusable_when_it_can_scroll_an_axis_that_overflows() {
    let mut engine = engine();
    let (fits, _) = scroller(&mut engine, 10.0, 10.0, AUTO, 40.0);
    let (scroll_fits, _) = scroller(
        &mut engine,
        120.0,
        10.0,
        (Overflow::Scroll, Overflow::Scroll),
        40.0,
    );
    let (hidden, _) = scroller(
        &mut engine,
        230.0,
        10.0,
        (Overflow::Hidden, Overflow::Hidden),
        200.0,
    );
    let (cross_axis, _) = scroller(
        &mut engine,
        340.0,
        10.0,
        (Overflow::Auto, Overflow::Hidden),
        200.0,
    );
    let (plain, _) = scroller(&mut engine, 10.0, 90.0, AUTO, 200.0);
    // 0.4px of overflow counts, though the snapped scroll range is 0.
    let (fraction, _) = scroller(&mut engine, 120.0, 90.0, AUTO, 60.4);
    engine.update().unwrap();
    let focusable: Vec<_> = [fits, scroll_fits, hidden, cross_axis, plain, fraction]
        .iter()
        .map(|node| engine.can_focus(*node).unwrap())
        .collect();
    assert_eq!(focusable, [false, false, false, false, true, true]);
    // Script focus reaches it; a press never does.
    engine.focus(plain).unwrap();
    assert_eq!(engine.focused(), Some(plain));
}

#[test]
fn a_scroller_with_a_keyboard_focusable_descendant_leaves_the_order_to_it() {
    let mut engine = engine();
    let (neg_child, neg_content) = scroller(&mut engine, 10.0, 10.0, AUTO, 200.0);
    let neg_button = place(
        &mut engine,
        neg_content,
        ElementTag::Button,
        (5.0, 5.0, 50.0, 20.0),
    );
    engine.set_attribute(neg_button, "tabindex", "-1").unwrap();
    let (with_button, content) = scroller(&mut engine, 120.0, 10.0, AUTO, 200.0);
    let button = place(
        &mut engine,
        content,
        ElementTag::Button,
        (5.0, 5.0, 50.0, 20.0),
    );
    let (neg_scroller, _) = scroller(&mut engine, 230.0, 10.0, AUTO, 200.0);
    engine
        .set_attribute(neg_scroller, "tabindex", "-1")
        .unwrap();
    engine.update().unwrap();
    let mut order = Vec::new();
    for _ in 0..3 {
        order.push(engine.advance_focus(1).unwrap().unwrap());
    }
    // `tabindex="-1"` keeps the button out and does not hide the scroller;
    // a focusable button takes the scroller's place.
    assert_eq!(order, [neg_child, button, neg_child]);
    assert!(engine.can_focus(with_button).unwrap());
    assert!(engine.can_focus(neg_scroller).unwrap());
}

#[test]
fn a_press_focuses_a_mouse_focusable_ancestor_else_clears_unless_on_a_scrollbar() {
    let mut engine = engine();
    let (plain, plain_content) = scroller(&mut engine, 20.0, 20.0, AUTO, 200.0);
    let (neg_scroller, neg_content) = scroller(&mut engine, 140.0, 20.0, AUTO, 200.0);
    engine
        .set_attribute(neg_scroller, "tabindex", "-1")
        .unwrap();
    // Content (85px client width): no element a click can focus.
    assert_eq!(
        engine.press_focus(plain_content, 50.0, 40.0).unwrap(),
        PressFocus::Clear
    );
    // The scrollbar, x 105 to 120: focus stays.
    assert_eq!(
        engine.press_focus(plain, 112.0, 40.0).unwrap(),
        PressFocus::Keep
    );
    // `tabindex` makes the scroller mouse focusable.
    assert_eq!(
        engine.press_focus(neg_content, 170.0, 40.0).unwrap(),
        PressFocus::Focus(neg_scroller)
    );
}

#[test]
fn navigation_starts_at_the_press_and_at_a_removed_start_s_position() {
    let mut engine = engine();
    let root = engine.root();
    let (first, _) = scroller(&mut engine, 10.0, 10.0, AUTO, 200.0);
    let wrap = place(
        &mut engine,
        root,
        ElementTag::Div,
        (10.0, 90.0, 200.0, 40.0),
    );
    let gap = place(&mut engine, wrap, ElementTag::Div, (0.0, 0.0, 200.0, 40.0));
    let (last, _) = scroller(&mut engine, 10.0, 150.0, AUTO, 200.0);
    engine.update().unwrap();
    // Nothing focused: Tab goes to the first after the pressed node, and
    // Shift+Tab to the one before it.
    press(&mut engine, 50.0, 110.0);
    assert_eq!(engine.mouse_press_node(), Some(gap));
    assert_eq!(engine.next_focus_target(1).unwrap(), Some(last));
    assert_eq!(engine.next_focus_target(-1).unwrap(), Some(first));
    // Removing the start's parent keeps its position, not the root's.
    engine.remove(wrap).unwrap();
    assert_eq!(engine.next_focus_target(1).unwrap(), Some(last));
    assert_eq!(engine.next_focus_target(-1).unwrap(), Some(first));
    // Focus and then `blur` set the start too.
    engine.focus(first).unwrap();
    engine.blur(first).unwrap();
    assert_eq!(engine.next_focus_target(1).unwrap(), Some(last));
}
