//! Wheel scrolling limited to, and resolved against, each scroller's size,
//! as Chromium 147's compositor does (`cc::InputHandler::
//! LimitDeltaToScrollerSize` and `ResolveScrollGranularityToPixels` in
//! `cc/input/input_handler.cc`, `ScrollUtils` in `cc/input/scroll_utils.cc`).
//!
//! The numbers come from the real-input oracle
//! `tests/fidelity/fixtures/wheel-events-v1/chromium-windows.json`: at 1x
//! its scroller fills a 464x305 viewport and has a 449x290 client area.
use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle, WheelGranularity};
use div_geometry::ViewportMetrics;
use div_style::{
    Display, Edges, LengthValue, Overflow, RendererStyleValue, ScrollbarWidth, StyleProperty,
    StyleValue,
};

fn set(
    engine: &mut Engine,
    node: NodeHandle,
    property: StyleProperty,
    value: impl Into<StyleValue>,
) {
    engine.set_property(node, property, value.into()).unwrap();
}

fn block(engine: &mut Engine, parent: NodeHandle, width: f32, height: f32) -> NodeHandle {
    let node = engine.create_native_element(ElementTag::Div).unwrap();
    set(engine, node, StyleProperty::Display, Display::Block);
    set(engine, node, StyleProperty::Width, LengthValue::px(width));
    set(engine, node, StyleProperty::Height, LengthValue::px(height));
    set(
        engine,
        node,
        StyleProperty::Margin,
        Edges::all(LengthValue::px(0.0)),
    );
    engine.append_child(parent, node).unwrap();
    node
}

/// A scroller and its 4000px-square content.
fn scroller(
    engine: &mut Engine,
    parent: NodeHandle,
    width: f32,
    height: f32,
) -> (NodeHandle, NodeHandle) {
    let node = block(engine, parent, width, height);
    set(engine, node, StyleProperty::Overflow, Overflow::Scroll);
    let content = block(engine, node, 4000.0, 4000.0);
    (node, content)
}

/// The oracle's page: one scroller filling a 464x305 viewport.
fn oracle_page() -> (Engine, NodeHandle, NodeHandle) {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(464.0, 305.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    let (port, content) = scroller(&mut engine, root, 464.0, 305.0);
    let metrics = engine.scroll_metrics(port).unwrap().unwrap();
    assert_eq!(
        (metrics.client_width, metrics.client_height),
        (449.0, 290.0)
    );
    (engine, port, content)
}

#[test]
fn a_wheel_scroll_moves_at_most_the_scrollers_client_size() {
    let (mut engine, port, content) = oracle_page();
    // The oracle's Windows steps at 1x: deltaY 100, -100, 100, 50, then a
    // three-notch 300 that Chromium limits to the 290px client height.
    let mut offsets = Vec::new();
    for delta_y in [100.0, -100.0, 100.0, 50.0, 300.0] {
        engine
            .scroll_wheel_from_target(content, 0.0, delta_y)
            .unwrap();
        offsets.push(engine.scroll_offset(port).unwrap().1);
    }
    assert_eq!(offsets, [100.0, 0.0, 100.0, 150.0, 440.0]);
    // Horizontally the limit is the 449px client width.
    engine
        .scroll_wheel_from_target(content, 1000.0, 0.0)
        .unwrap();
    assert_eq!(engine.scroll_offset(port).unwrap(), (449.0, 440.0));
    engine.scroll_wheel(port, -1000.0, -1000.0).unwrap();
    assert_eq!(engine.scroll_offset(port).unwrap(), (0.0, 150.0));
}

#[test]
fn each_scroller_in_the_chain_limits_by_its_own_size() {
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(464.0, 305.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    let (outer, _) = scroller(&mut engine, root, 464.0, 305.0);
    let inner = block(&mut engine, outer, 60.0, 60.0);
    set(
        &mut engine,
        inner,
        StyleProperty::Overflow,
        Overflow::Scroll,
    );
    set(
        &mut engine,
        inner,
        StyleProperty::ScrollbarWidth,
        StyleValue::Renderer(RendererStyleValue::ScrollbarWidth(ScrollbarWidth::None)),
    );
    let content = block(&mut engine, inner, 60.0, 100.0);
    // The 60px-high inner scroller takes a 300px wheel as 40px, its range.
    engine
        .scroll_wheel_from_target(content, 0.0, 300.0)
        .unwrap();
    assert_eq!(engine.scroll_offset(inner).unwrap(), (0.0, 40.0));
    assert_eq!(engine.scroll_offset(outer).unwrap(), (0.0, 0.0));
    // At its end the wheel chains to the outer scroller, limited by 290px.
    engine
        .scroll_wheel_from_target(content, 0.0, 300.0)
        .unwrap();
    assert_eq!(engine.scroll_offset(inner).unwrap(), (0.0, 40.0));
    assert_eq!(engine.scroll_offset(outer).unwrap(), (0.0, 290.0));
}

#[test]
fn lines_and_pages_resolve_against_the_scroller() {
    let (mut engine, port, content) = oracle_page();
    // A line is 40px, still limited by the client height.
    engine
        .scroll_wheel_from_target_by(content, 0.0, 2.0, WheelGranularity::Line)
        .unwrap();
    assert_eq!(engine.scroll_offset(port).unwrap(), (0.0, 80.0));
    engine
        .scroll_wheel_from_target_by(content, 0.0, 10.0, WheelGranularity::Line)
        .unwrap();
    assert_eq!(engine.scroll_offset(port).unwrap(), (0.0, 370.0));
    // A page is 0.875 x min(client, viewport): 0.875 x 449 = 392.875 and
    // 0.875 x 290 = 253.75. `scroll_to` keeps whole-pixel offsets at 1x.
    engine
        .scroll_wheel_from_target_by(content, 1.0, 1.0, WheelGranularity::Page)
        .unwrap();
    assert_eq!(engine.scroll_offset(port).unwrap(), (393.0, 624.0));
    // Pages are not limited to the client size: two pages are 507.5.
    engine
        .scroll_wheel_from_target_by(content, 0.0, -2.0, WheelGranularity::Page)
        .unwrap();
    let (x, y) = engine.scroll_offset(port).unwrap();
    assert_eq!(x, 393.0);
    assert_eq!(y, (624.0f64 - 507.5).round());

    // A scroller taller than the viewport pages by the viewport.
    let mut engine =
        Engine::new(ViewportMetrics::from_logical_size(464.0, 305.0, 1.0).unwrap()).unwrap();
    let root = engine.root();
    let (tall, content) = scroller(&mut engine, root, 464.0, 815.0);
    engine
        .scroll_wheel_from_target_by(content, 0.0, 1.0, WheelGranularity::Page)
        .unwrap();
    // 0.875 x 305 = 266.875.
    assert_eq!(engine.scroll_offset(tall).unwrap(), (0.0, 267.0));
}

/// The limit is the client box pixel-snapped in device pixels at its place
/// in the layout (`container_bounds`), as the real-input oracle
/// `tests/fidelity/fixtures/wheel-limit-v1/chromium-linux-x11.json`
/// measured: one 120px notch moves a 100.4px-high client box at 160.3px by
/// 101px at 1x, and one at 226.6px by 126 device px (100.8 CSS px) at
/// 1.25x. Div keeps an element's offset in whole CSS pixels, so that is
/// 101 too; the unsnapped 100.4 would round to 100 in both.
#[test]
fn the_limit_is_the_client_box_snapped_to_device_pixels() {
    for (scale, spacer, expected) in [
        (1.0, 160.3, 101.0),
        (1.25, 226.6, 101.0),
        (1.0, 160.6, 100.0),
    ] {
        let mut engine =
            Engine::new(ViewportMetrics::from_logical_size(480.0, 344.0, scale).unwrap()).unwrap();
        let root = engine.root();
        block(&mut engine, root, 480.0, spacer);
        let port = block(&mut engine, root, 300.0, 100.4);
        set(
            &mut engine,
            port,
            StyleProperty::OverflowX,
            Overflow::Hidden,
        );
        set(
            &mut engine,
            port,
            StyleProperty::OverflowY,
            Overflow::Scroll,
        );
        let content = block(&mut engine, port, 280.0, 4000.0);
        engine
            .scroll_wheel_from_target(content, 0.0, 120.0)
            .unwrap();
        assert_eq!(
            engine.scroll_offset(port).unwrap().1,
            expected,
            "scale {scale}, top {spacer}"
        );
    }
}
