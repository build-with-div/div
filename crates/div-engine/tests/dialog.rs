//! `<dialog>` and the top layer (`dialog.rs`, RFC 0015): the user agent
//! rules, the state methods, `:modal` and `:open`, inertness and the
//! focusing steps. `dialog_oracle.rs` replays pinned Chromium.
use div_dom::ElementTag;
use div_engine::{Engine, EngineError, NodeHandle, PointerEventKind};
use div_geometry::ViewportMetrics;
use div_style::selector_parser::parse_selector_list;
use div_style::{Display, LengthValue, Position, StyleProperty, StyleValue};

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap()).unwrap()
}

fn set(
    engine: &mut Engine,
    node: NodeHandle,
    property: StyleProperty,
    value: impl Into<StyleValue>,
) {
    engine.set_property(node, property, value.into()).unwrap();
}

fn sized(engine: &mut Engine, node: NodeHandle, width: f32, height: f32) {
    set(engine, node, StyleProperty::Width, LengthValue::px(width));
    set(engine, node, StyleProperty::Height, LengthValue::px(height));
}

fn dialog(engine: &mut Engine) -> NodeHandle {
    let root = engine.root();
    let dialog = engine
        .create_native_element_named(ElementTag::Div, "dialog")
        .unwrap();
    engine.append_child(root, dialog).unwrap();
    dialog
}

fn button(engine: &mut Engine, parent: NodeHandle) -> NodeHandle {
    let button = engine.create_native_element(ElementTag::Button).unwrap();
    sized(engine, button, 60.0, 24.0);
    engine.append_child(parent, button).unwrap();
    button
}

fn matches(engine: &Engine, node: NodeHandle, selector: &str) -> bool {
    let list = parse_selector_list(selector, None).unwrap();
    engine.matches_selector_list(node, &list).unwrap()
}

#[test]
fn the_user_agent_rules_hide_a_closed_dialog_and_center_a_modal_one() {
    let mut engine = engine();
    let dialog = dialog(&mut engine);
    let content = engine.create_native_element(ElementTag::Div).unwrap();
    set(&mut engine, content, StyleProperty::Display, Display::Block);
    sized(&mut engine, content, 100.0, 40.0);
    engine.append_child(dialog, content).unwrap();
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

    engine.close_dialog(dialog, None).unwrap();
    engine.show_modal(dialog).unwrap();
    engine.update().unwrap();
    assert_eq!(
        engine.computed_style(dialog).unwrap().position,
        Position::Fixed
    );
    // 100x40 content, 1em (16px) padding and 3px (medium) borders, centered
    // in the 480x400 viewport.
    let bounds = engine.bounds(dialog).unwrap().unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (171.0, 161.0, 138.0, 78.0)
    );
}

#[test]
fn show_modal_opens_a_modal_dialog_in_the_top_layer() {
    let mut engine = engine();
    let first = dialog(&mut engine);
    let second = dialog(&mut engine);
    assert!(!matches(&engine, first, ":open"));
    engine.show_modal(first).unwrap();
    engine.show_modal(second).unwrap();
    assert_eq!(engine.top_layer(), &[first, second]);
    assert_eq!(engine.active_modal_dialog(), Some(second));
    assert!(matches(&engine, first, "dialog:modal:open"));
    assert!(
        engine.is_inert(first),
        "the lower dialog is outside the topmost"
    );
    assert!(!engine.is_inert(second));

    assert!(engine.close_dialog(second, Some("done")).unwrap());
    assert_eq!(engine.dialog_return_value(second).unwrap(), "done");
    assert!(!matches(&engine, second, ":modal"));
    assert!(!matches(&engine, second, ":open"));
    assert_eq!(engine.top_layer(), &[first]);
    // Closing a closed dialog does nothing.
    assert!(!engine.close_dialog(second, Some("again")).unwrap());
    assert_eq!(engine.dialog_return_value(second).unwrap(), "done");
}

#[test]
fn the_invalid_state_cases_fail_as_blink_does() {
    let mut engine = engine();
    let dialog = dialog(&mut engine);
    engine.show_dialog(dialog).unwrap();
    assert!(matches!(
        engine.show_modal(dialog),
        Err(EngineError::InvalidState(_))
    ));
    // Showing an open non-modal dialog again does nothing.
    engine.show_dialog(dialog).unwrap();
    engine.close_dialog(dialog, None).unwrap();
    engine.show_modal(dialog).unwrap();
    assert!(matches!(
        engine.show_dialog(dialog),
        Err(EngineError::InvalidState(_))
    ));
    engine.show_modal(dialog).unwrap();

    let detached = engine
        .create_native_element_named(ElementTag::Div, "dialog")
        .unwrap();
    assert!(matches!(
        engine.show_modal(detached),
        Err(EngineError::InvalidState(_))
    ));
    let div = engine.create_native_element(ElementTag::Div).unwrap();
    assert!(matches!(
        engine.show_modal(div),
        Err(EngineError::InvalidInput("not a dialog"))
    ));
}

#[test]
fn removing_open_keeps_the_dialog_modal_and_removal_ends_it() {
    let mut engine = engine();
    let dialog = dialog(&mut engine);
    let inside = button(&mut engine, dialog);
    engine.show_modal(dialog).unwrap();
    engine.remove_attribute(dialog, "open").unwrap();
    assert!(matches(&engine, dialog, ":modal"));
    assert!(!matches(&engine, dialog, ":open"));
    assert_eq!(engine.active_modal_dialog(), Some(dialog));
    // `close()` sees a closed dialog and does nothing.
    assert!(!engine.close_dialog(dialog, None).unwrap());
    assert_eq!(engine.active_modal_dialog(), Some(dialog));

    engine.detach(dialog).unwrap();
    assert_eq!(engine.active_modal_dialog(), None);
    assert!(!engine.is_modal_dialog(dialog).unwrap());
    let _ = inside;
}

#[test]
fn inert_content_cannot_take_focus_or_pointer_input() {
    let mut engine = engine();
    let root = engine.root();
    let outside = button(&mut engine, root);
    let dialog = dialog(&mut engine);
    let inside = button(&mut engine, dialog);
    engine.update().unwrap();
    engine.focus(outside).unwrap();

    engine.show_modal(dialog).unwrap();
    assert_eq!(engine.focused(), Some(inside));
    assert!(engine.is_inert(outside));
    assert_eq!(engine.focus(outside), Err(EngineError::NotFocusable));
    assert_eq!(engine.focused(), Some(inside));
    // The outside button is at the top left; a press there does not hit it.
    let update = engine
        .pointer_event(1, PointerEventKind::Down, 10.0, 10.0)
        .unwrap();
    assert_ne!(update.target, Some(outside));
    engine
        .pointer_event(1, PointerEventKind::Up, 10.0, 10.0)
        .unwrap();
    // Tab stays in the dialog.
    assert_eq!(engine.next_focus_target(1).unwrap(), Some(inside));

    engine.close_dialog(dialog, None).unwrap();
    assert_eq!(engine.focused(), Some(outside), "focus goes back");
    assert!(!engine.is_inert(outside));
}

#[test]
fn the_focusing_steps_prefer_autofocus_skip_nested_dialogs_and_fall_back_to_the_dialog() {
    let mut engine = engine();
    let outer = dialog(&mut engine);
    let first = button(&mut engine, outer);
    let autofocus = button(&mut engine, outer);
    engine.set_attribute(autofocus, "autofocus", "").unwrap();
    engine.show_modal(outer).unwrap();
    assert_eq!(engine.focused(), Some(autofocus));
    engine.close_dialog(outer, None).unwrap();
    engine.remove_attribute(autofocus, "autofocus").unwrap();
    engine.show_modal(outer).unwrap();
    assert_eq!(engine.focused(), Some(first));
    engine.close_dialog(outer, None).unwrap();

    // An open nested dialog is focusable itself, but the steps do not look
    // inside it, so its autofocus button does not win (Blink's
    // `SetFocusForDialogLegacy` skips the children of nested dialogs).
    let host = dialog(&mut engine);
    let nested = engine
        .create_native_element_named(ElementTag::Div, "dialog")
        .unwrap();
    engine.append_child(host, nested).unwrap();
    engine.set_attribute(nested, "open", "").unwrap();
    let in_nested = button(&mut engine, nested);
    engine.set_attribute(in_nested, "autofocus", "").unwrap();
    button(&mut engine, host);
    engine.show_modal(host).unwrap();
    assert_eq!(engine.focused(), Some(nested));
    engine.close_dialog(host, None).unwrap();

    let empty = dialog(&mut engine);
    engine.show_modal(empty).unwrap();
    assert_eq!(engine.focused(), Some(empty));
    // A dialog without `tabindex` is not in the sequential order.
    assert_eq!(engine.next_focus_target(1).unwrap(), None);
}
