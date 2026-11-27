//! RFC 0006 B: an animation tick restyles only the targets whose effect
//! changed and the descendants that inherit from them, and a start restyles
//! only the new target. In debug builds every targeted sample is also checked
//! against a full cascade of every root, which these tests exercise.
use div_dom::ElementTag;
use div_engine::{AnimationId, AnimationTimeline, Engine, NodeHandle, ViewportMetrics};
use div_style::{
    AnimationOptions, Color, FillMode, Keyframes, LengthValue, PropertyKeyframes, StyleProperty,
    StyleValue,
};

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(160.0, 120.0, 1.0).unwrap()).unwrap()
}

fn child(e: &mut Engine, parent: NodeHandle) -> NodeHandle {
    let node = e.create_native_element(ElementTag::Div).unwrap();
    e.append_child(parent, node).unwrap();
    node
}

fn options(fill: FillMode) -> AnimationOptions {
    AnimationOptions {
        duration_ms: 1000.0,
        fill,
        ..AnimationOptions::default()
    }
}

fn animate_color(e: &mut Engine, node: NodeHandle, fill: FillMode) -> AnimationId {
    e.animate(
        node,
        PropertyKeyframes::typed(
            StyleProperty::Color,
            Keyframes::from_values(Color::RED, Color::BLUE),
        )
        .unwrap(),
        options(fill),
        AnimationTimeline::Document,
    )
    .unwrap()
}

fn animate_opacity(e: &mut Engine, node: NodeHandle) -> AnimationId {
    e.animate(
        node,
        PropertyKeyframes::typed(
            StyleProperty::Opacity,
            Keyframes::from_values(0.0_f32, 1.0_f32),
        )
        .unwrap(),
        options(FillMode::Both),
        AnimationTimeline::Document,
    )
    .unwrap()
}

fn restyled(e: &mut Engine) -> Vec<NodeHandle> {
    e.take_selector_restyles()
}

#[test]
fn a_tick_restyles_only_changed_targets_and_their_descendants() {
    let mut e = engine();
    let root = e.root();
    let a = child(&mut e, root);
    let a_child = child(&mut e, a);
    let b = child(&mut e, root);
    let still = child(&mut e, root);
    let still_child = child(&mut e, still);
    animate_color(&mut e, a, FillMode::Both);
    animate_opacity(&mut e, b);
    e.record_selector_restyles();
    e.set_animation_time(250.0).unwrap();
    let log = restyled(&mut e);
    assert_eq!(log, vec![a, a_child, b]);
    assert!(!log.contains(&still) && !log.contains(&still_child));
    let color = e.computed_style(a).unwrap().color;
    assert_ne!(color, Color::RED);
    assert_eq!(e.computed_style(a_child).unwrap().color, color);
    assert_eq!(e.computed_style(b).unwrap().opacity, 0.25);
    // An unchanged sample restyles nothing.
    e.set_animation_time(250.0).unwrap();
    assert!(restyled(&mut e).is_empty());
}

#[test]
fn nested_targets_restyle_the_outer_subtree_once() {
    let mut e = engine();
    let root = e.root();
    let outer = child(&mut e, root);
    let inner = child(&mut e, outer);
    let leaf = child(&mut e, inner);
    // The inner effect is older, so the outer target comes later in the
    // changed list and must still be staged before its descendants.
    animate_opacity(&mut e, inner);
    animate_color(&mut e, outer, FillMode::Both);
    e.record_selector_restyles();
    e.set_animation_time(500.0).unwrap();
    let log = restyled(&mut e);
    assert_eq!(log, vec![outer, inner, leaf]);
    let color = e.computed_style(outer).unwrap().color;
    assert_eq!(e.computed_style(inner).unwrap().color, color);
    assert_eq!(e.computed_style(leaf).unwrap().color, color);
    assert_eq!(e.computed_style(inner).unwrap().opacity, 0.5);
}

#[test]
fn a_start_restyles_only_the_new_target() {
    let mut e = engine();
    let root = e.root();
    let cells: Vec<_> = (0..4).map(|_| child(&mut e, root)).collect();
    let nested = child(&mut e, cells[3]);
    for cell in &cells[..3] {
        animate_opacity(&mut e, *cell);
    }
    e.set_animation_time(250.0).unwrap();
    e.drain_animation_events();
    e.record_selector_restyles();
    animate_color(&mut e, cells[3], FillMode::Both);
    assert_eq!(restyled(&mut e), vec![cells[3], nested]);
    // Starting an effect emits no event for the others.
    assert!(e
        .drain_animation_events()
        .iter()
        .all(|event| event.target == cells[3]));
    for cell in &cells[..3] {
        assert_eq!(e.computed_style(*cell).unwrap().opacity, 0.25);
    }
    assert_eq!(e.computed_style(nested).unwrap().color, Color::RED);
}

#[test]
fn an_ended_effect_without_fill_restores_its_target() {
    let mut e = engine();
    let root = e.root();
    e.set_property(
        root,
        StyleProperty::Color,
        Color::from_rgba8(0, 128, 0, 255).into(),
    )
    .unwrap();
    let node = child(&mut e, root);
    let leaf = child(&mut e, node);
    animate_color(&mut e, node, FillMode::None);
    e.set_animation_time(500.0).unwrap();
    assert_ne!(
        e.computed_style(leaf).unwrap().color,
        Color::from_rgba8(0, 128, 0, 255)
    );
    e.record_selector_restyles();
    e.set_animation_time(1500.0).unwrap();
    assert_eq!(restyled(&mut e), vec![node, leaf]);
    assert_eq!(
        e.computed_style(node).unwrap().color,
        Color::from_rgba8(0, 128, 0, 255)
    );
    assert_eq!(
        e.computed_style(leaf).unwrap().color,
        Color::from_rgba8(0, 128, 0, 255)
    );
}

#[test]
fn a_root_element_effect_restyles_every_tree() {
    let mut e = engine();
    let root = e.root();
    let attached = child(&mut e, root);
    let detached = e.create_native_element(ElementTag::Div).unwrap();
    let rem: StyleValue = LengthValue::Rem(2.0).into();
    e.set_property(attached, StyleProperty::FontSize, rem.clone())
        .unwrap();
    e.set_property(detached, StyleProperty::FontSize, rem)
        .unwrap();
    e.animate(
        root,
        PropertyKeyframes::typed(
            StyleProperty::FontSize,
            Keyframes::from_values(LengthValue::px(10.0), LengthValue::px(20.0)),
        )
        .unwrap(),
        options(FillMode::Both),
        AnimationTimeline::Document,
    )
    .unwrap();
    e.set_animation_time(500.0).unwrap();
    let root_font = e.computed_style(root).unwrap().font_size;
    assert_eq!(root_font, 15.0);
    assert_eq!(e.computed_style(attached).unwrap().font_size, 30.0);
    assert_eq!(e.computed_style(detached).unwrap().font_size, 30.0);
}

#[test]
fn a_detached_target_restyles_its_own_tree() {
    let mut e = engine();
    let detached = e.create_native_element(ElementTag::Div).unwrap();
    let leaf = child(&mut e, detached);
    animate_color(&mut e, detached, FillMode::Both);
    e.record_selector_restyles();
    e.set_animation_time(500.0).unwrap();
    assert_eq!(restyled(&mut e), vec![detached, leaf]);
    assert_eq!(
        e.computed_style(leaf).unwrap().color,
        e.computed_style(detached).unwrap().color
    );
}
