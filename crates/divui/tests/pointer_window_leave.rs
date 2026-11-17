//! A pointer that leaves the window leaves its whole hover path, as Chromium
//! does when the mouse leaves the page (RFC 0001, state sources).

use div::prelude::*;
use div::{PointerInput, PointerInputKind};
use std::cell::RefCell;
use std::rc::Rc;

fn block(document: &Document, parent: &Element) -> Element {
    let element = Element::create(document, "div").unwrap();
    element
        .set_property(StyleProperty::Height, LengthValue::px(40.0).into())
        .unwrap();
    parent.append_child(&element).unwrap();
    element
}

fn record(log: &Rc<RefCell<Vec<String>>>, element: &Element, name: &'static str) {
    for event in [
        "mouseleave",
        "mouseenter",
        "pointerleave",
        "pointerenter",
        "pointercancel",
        "mousecancel",
        "click",
    ] {
        let log = log.clone();
        element
            .on(event, move |_| {
                log.borrow_mut().push(format!("{event} {name}"))
            })
            .unwrap();
    }
}

#[test]
fn leaving_the_window_leaves_the_hover_path_innermost_first() {
    let document = Document::new(100, 80).unwrap();
    let outer = block(&document, &document.body());
    let inner = block(&document, &outer);
    document.update_all().unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    record(&log, &outer, "outer");
    record(&log, &inner, "inner");

    let mouse = |kind| {
        document
            .dispatch_pointer_input(PointerInput::mouse(kind, 10.0, 10.0))
            .unwrap()
    };
    mouse(PointerInputKind::Move);
    assert_eq!(
        *log.borrow(),
        [
            "pointerenter outer",
            "pointerenter inner",
            "mouseenter outer",
            "mouseenter inner"
        ]
    );
    log.borrow_mut().clear();

    mouse(PointerInputKind::Leave);
    assert_eq!(
        *log.borrow(),
        [
            "pointerleave inner",
            "pointerleave outer",
            "mouseleave inner",
            "mouseleave outer"
        ]
    );
    assert!(!inner.is_hovered().unwrap());
    assert!(!outer.is_hovered().unwrap());
    log.borrow_mut().clear();

    // Nothing is hovered, so a second leave dispatches nothing.
    mouse(PointerInputKind::Leave);
    assert!(log.borrow().is_empty());
}

#[test]
fn a_press_survives_leaving_and_coming_back() {
    let document = Document::new(100, 80).unwrap();
    let outer = block(&document, &document.body());
    let inner = block(&document, &outer);
    document.update_all().unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    record(&log, &inner, "inner");

    let mouse = |kind| {
        document
            .dispatch_pointer_input(PointerInput::mouse(kind, 10.0, 10.0))
            .unwrap()
    };
    mouse(PointerInputKind::Down);
    assert!(inner.is_active().unwrap());
    mouse(PointerInputKind::Leave);
    assert!(inner.is_active().unwrap());
    mouse(PointerInputKind::Move);
    mouse(PointerInputKind::Up);
    assert_eq!(
        *log.borrow(),
        [
            "pointerenter inner",
            "mouseenter inner",
            "pointerleave inner",
            "mouseleave inner",
            "pointerenter inner",
            "mouseenter inner",
            "click inner"
        ]
    );
}
