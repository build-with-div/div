//! A renderer snapshot (`Engine::set_renderer_style`) freezes the node's
//! native source and its own declarations (`set_property`). Values from
//! matched stylesheet rules, presentational hints and the `style` attribute
//! stay live: they cascade above the snapshot, and when their rule stops
//! matching they leave nothing behind.
use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle, ViewportMetrics};
use div_geometry::Length;
use div_style::selector_parser::parse_selector_list;
use div_style::stylesheet::{CascadeOrigin, StyleRule, StyleSheet};
use div_style::{Display, Position, RendererStyleValue, Style, StyleProperty};

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap()).unwrap()
}

fn snapshot(engine: &mut Engine, node: NodeHandle, value: RendererStyleValue) {
    engine.set_renderer_style(node, value).unwrap();
}

fn opacity(engine: &Engine, node: NodeHandle) -> f32 {
    engine.computed_style(node).unwrap().opacity
}

/// A `<dialog>` whose snapshot is written while it is closed, before or
/// after it is connected, as the WPT porter's fixtures write it.
fn snapshotted_dialog(engine: &mut Engine, append_first: bool) -> NodeHandle {
    let root = engine.root();
    let dialog = engine
        .create_native_element_named(ElementTag::Div, "dialog")
        .unwrap();
    if append_first {
        engine.append_child(root, dialog).unwrap();
    }
    snapshot(engine, dialog, RendererStyleValue::Display(Display::Block));
    snapshot(engine, dialog, RendererStyleValue::Width(Length::px(120.0)));
    snapshot(engine, dialog, RendererStyleValue::Opacity(0.5));
    if !append_first {
        engine.append_child(root, dialog).unwrap();
    }
    engine.update().unwrap();
    // Closed: the user agent's `dialog:not([open])` rule hides it.
    assert_eq!(
        engine.computed_style(dialog).unwrap().display,
        Display::None
    );
    dialog
}

#[test]
fn a_closed_dialogs_display_none_does_not_outlive_its_rule() {
    for append_first in [false, true] {
        let mut engine = engine();
        let dialog = snapshotted_dialog(&mut engine, append_first);
        engine.show_modal(dialog).unwrap();
        engine.update().unwrap();
        let style = engine.computed_style(dialog).unwrap();
        assert_eq!(
            style.display,
            Display::Block,
            "append first: {append_first}"
        );
        assert_eq!(style.position, Position::Fixed);
        // The snapshot keeps what no rule sets; the user agent's `width:
        // fit-content` still cascades above it, as user agent rules do
        // above every snapshot (an author width goes through set_property).
        assert_eq!(style.opacity, 0.5);
        assert_eq!(style.width, Length::fit_content());
        assert!(engine.bounds(dialog).unwrap().is_some());

        // Closing applies the rule again; showing removes it again.
        engine.close_dialog(dialog, None).unwrap();
        engine.update().unwrap();
        assert_eq!(
            engine.computed_style(dialog).unwrap().display,
            Display::None
        );
        engine.show_dialog(dialog).unwrap();
        engine.update().unwrap();
        let style = engine.computed_style(dialog).unwrap();
        assert_eq!(style.display, Display::Block);
        assert_eq!(style.position, Position::Absolute);
    }
}

#[test]
fn author_rule_values_stay_live_above_a_snapshot() {
    let mut engine = engine();
    let root = engine.root();
    let target = engine.create_native_element(ElementTag::Div).unwrap();
    engine.append_child(root, target).unwrap();
    engine.set_attribute(target, "class", "a").unwrap();
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author).with_rule(StyleRule::new(
                parse_selector_list(".a", None).unwrap(),
                Style::default().opacity(0.25),
            )),
        )
        .unwrap();
    assert_eq!(opacity(&engine, target), 0.25);

    // A snapshot of another property does not capture the rule's opacity.
    snapshot(
        &mut engine,
        target,
        RendererStyleValue::Width(Length::px(50.0)),
    );
    assert_eq!(opacity(&engine, target), 0.25);
    engine.remove_attribute(target, "class").unwrap();
    assert_eq!(opacity(&engine, target), 1.0);
    assert_eq!(
        engine.computed_style(target).unwrap().width,
        Length::px(50.0)
    );

    // A snapshot of the same property sits below the rule while it matches
    // and shows through once it stops matching.
    engine.set_attribute(target, "class", "a").unwrap();
    snapshot(&mut engine, target, RendererStyleValue::Opacity(0.5));
    assert_eq!(opacity(&engine, target), 0.25);
    engine.remove_attribute(target, "class").unwrap();
    assert_eq!(opacity(&engine, target), 0.5);
}

#[test]
fn the_style_attribute_stays_live_and_own_declarations_are_frozen() {
    let mut engine = engine();
    let root = engine.root();
    let target = engine.create_native_element(ElementTag::Div).unwrap();
    engine.append_child(root, target).unwrap();

    engine
        .set_attribute(target, "style", "opacity: 0.3")
        .unwrap();
    snapshot(
        &mut engine,
        target,
        RendererStyleValue::Width(Length::px(40.0)),
    );
    assert_eq!(opacity(&engine, target), 0.3);
    engine.remove_attribute(target, "style").unwrap();
    assert_eq!(opacity(&engine, target), 1.0);

    // The element's own declarations are what a snapshot freezes.
    engine
        .set_property(target, StyleProperty::Opacity, 0.4_f32.into())
        .unwrap();
    snapshot(
        &mut engine,
        target,
        RendererStyleValue::Width(Length::px(60.0)),
    );
    assert_eq!(opacity(&engine, target), 0.4);
    let style = engine.computed_style(target).unwrap();
    assert_eq!(style.width, Length::px(60.0));
}
