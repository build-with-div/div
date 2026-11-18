//! Keyboard scrolling (`Engine::scroll_logical`), as Chromium 147's
//! `ScrollManager::LogicalScroll` performs a key's default action.
//!
//! The numbers come from the real-input oracle
//! `tests/fidelity/fixtures/keyboard-scroll-v1/` at 1x, whose page this test
//! reduces: a 480x400 viewport over a 1500x2500 document, an `outer`
//! scroller (200x150, content 600x900) holding an `inner` one (150x100,
//! content 400x500), and a `padded` one (150x120, `scroll-padding-top:
//! 30px`, content 150x1000). Every scroller here has both scrollbars, 15px
//! each.
use div_dom::ElementTag;
use div_engine::{
    Engine, NodeHandle, PointerEventKind, ScrollButtonDirection as D, ScrollGranularity as G,
};
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

fn place(engine: &mut Engine, parent: NodeHandle, at: (f32, f32, f32, f32)) -> NodeHandle {
    let node = engine.create_native_element(ElementTag::Div).unwrap();
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

struct Page {
    engine: Engine,
    outer: NodeHandle,
    inner: NodeHandle,
    inner_text: NodeHandle,
    padded: NodeHandle,
    padded_content: NodeHandle,
}

fn page() -> Page {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    place(&mut engine, root, (0.0, 0.0, 1500.0, 2500.0));
    let outer = place(&mut engine, root, (20.0, 20.0, 200.0, 150.0));
    set(&mut engine, outer, StyleProperty::Overflow, Overflow::Auto);
    let outer_content = place(&mut engine, outer, (0.0, 0.0, 600.0, 900.0));
    let inner = place(&mut engine, outer_content, (10.0, 10.0, 150.0, 100.0));
    set(&mut engine, inner, StyleProperty::Overflow, Overflow::Auto);
    let inner_content = place(&mut engine, inner, (0.0, 0.0, 400.0, 500.0));
    let inner_text = place(&mut engine, inner_content, (20.0, 20.0, 60.0, 40.0));
    let padded = place(&mut engine, root, (20.0, 190.0, 150.0, 120.0));
    set(&mut engine, padded, StyleProperty::Overflow, Overflow::Auto);
    set(
        &mut engine,
        padded,
        StyleProperty::ScrollPaddingTop,
        LengthValue::px(30.0),
    );
    let padded_content = place(&mut engine, padded, (0.0, 0.0, 150.0, 1000.0));
    Page {
        engine,
        outer,
        inner,
        inner_text,
        padded,
        padded_content,
    }
}

fn press(engine: &mut Engine, x: f32, y: f32) {
    engine
        .pointer_event(1, PointerEventKind::Down, x, y)
        .unwrap();
    engine.pointer_event(1, PointerEventKind::Up, x, y).unwrap();
}

fn top(engine: &Engine, node: NodeHandle) -> f64 {
    engine.scroll_offset(node).unwrap().1
}

#[test]
fn without_focus_or_press_the_document_scrolls() {
    let Page { mut engine, .. } = page();
    let root = engine.root();
    let mut offsets = Vec::new();
    for (direction, granularity) in [
        (D::Down, G::Line),
        (D::Down, G::Line),
        (D::Up, G::Line),
        // A page is 87.5% of the 385px client height, truncated.
        (D::BlockEnd, G::Page),
        (D::BlockStart, G::Page),
        (D::BlockEnd, G::Document),
        (D::BlockStart, G::Document),
    ] {
        assert!(engine.scroll_logical(direction, granularity).unwrap());
        offsets.push(top(&engine, root));
    }
    assert_eq!(offsets, [40.0, 80.0, 40.0, 376.0, 40.0, 2115.0, 0.0]);
    // At the start nothing moves.
    assert!(!engine.scroll_logical(D::Up, G::Line).unwrap());
    assert!(engine.scroll_logical(D::Right, G::Line).unwrap());
    assert_eq!(engine.scroll_offset(root).unwrap(), (40.0, 0.0));
}

#[test]
fn the_pressed_node_starts_the_chain_and_each_scroller_scrolls_until_its_end() {
    let page = page();
    let Page {
        mut engine,
        outer,
        inner,
        ..
    } = page;
    // innerText's centre.
    press(&mut engine, 80.0, 72.0);
    assert_eq!(engine.mouse_press_node(), Some(page.inner_text));
    assert!(engine.scroll_logical(D::Down, G::Line).unwrap());
    assert_eq!((top(&engine, inner), top(&engine, outer)), (40.0, 0.0));
    // Inner's range is 500 - 85 = 415; then outer's turn.
    assert!(engine.scroll_logical(D::BlockEnd, G::Document).unwrap());
    assert_eq!(top(&engine, inner), 415.0);
    assert!(engine.scroll_logical(D::Down, G::Line).unwrap());
    // Outer's page: 87.5% of its 135px client height.
    assert!(engine.scroll_logical(D::BlockEnd, G::Page).unwrap());
    assert_eq!(top(&engine, outer), 158.0);
    // Home scrolls the innermost scroller that can move back.
    assert!(engine.scroll_logical(D::BlockStart, G::Document).unwrap());
    assert_eq!((top(&engine, inner), top(&engine, outer)), (0.0, 158.0));
}

#[test]
fn a_removed_pressed_node_hands_the_chain_to_its_former_parent() {
    let page = page();
    let Page {
        mut engine,
        outer,
        inner,
        inner_text,
        ..
    } = page;
    let inner_content = engine.children(inner).unwrap()[0];
    press(&mut engine, 80.0, 72.0);
    engine.remove(inner_text).unwrap();
    assert_eq!(engine.mouse_press_node(), Some(inner_content));
    assert!(engine.scroll_logical(D::BlockEnd, G::Page).unwrap());
    // Inner's page: 87.5% of its 85px client height.
    assert_eq!(top(&engine, inner), 74.0);
    engine.remove(inner).unwrap();
    assert!(engine.scroll_logical(D::Down, G::Line).unwrap());
    assert_eq!(top(&engine, outer), 40.0);
}

#[test]
fn a_page_is_the_scroll_snapport_less_its_padding() {
    let page = page();
    let Page {
        mut engine,
        padded,
        padded_content,
        ..
    } = page;
    let content = engine.bounds(padded_content).unwrap().unwrap();
    press(&mut engine, content.x + 40.0, content.y + 40.0);
    // 120 - 15 (scrollbar) - 30 (padding) = 75; 87.5% of it is 65.
    assert!(engine.scroll_logical(D::BlockEnd, G::Page).unwrap());
    assert_eq!(top(&engine, padded), 65.0);
    assert!(engine.scroll_logical(D::BlockEnd, G::Page).unwrap());
    assert_eq!(top(&engine, padded), 130.0);
}

#[test]
fn a_hidden_axis_passes_the_scroll_on() {
    let page = page();
    let Page {
        mut engine, outer, ..
    } = page;
    set(
        &mut engine,
        outer,
        StyleProperty::OverflowY,
        Overflow::Hidden,
    );
    // outerContent, beside inner.
    press(&mut engine, 200.0, 150.0);
    assert!(engine.scroll_logical(D::Down, G::Line).unwrap());
    assert_eq!(top(&engine, outer), 0.0);
    assert_eq!(top(&engine, engine.root()), 40.0);
    assert!(engine.scroll_logical(D::Right, G::Line).unwrap());
    assert_eq!(engine.scroll_offset(outer).unwrap(), (40.0, 0.0));
}
