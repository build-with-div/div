//! Listener handles and options (sample-editor gap 29), as the DOM's
//! `removeEventListener` and `AddEventListenerOptions` work in Chromium 147:
//! `EventTarget::FireEventListeners` and `SetDefaultAddEventListenerOptions`
//! in `third_party/blink/renderer/core/dom/events/event_target.cc`.

use div::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<&'static str>>>;

fn mounted(document: &Document) -> Element {
    let element = Element::create(document, "div").unwrap();
    document.body().append_child(&element).unwrap();
    element
}

fn logger(log: &Log, name: &'static str) -> impl Fn(&Event) + 'static {
    let log = log.clone();
    move |_| log.borrow_mut().push(name)
}

fn fire(document: &Document, target: &Element, event_type: &str) -> bool {
    document
        .dispatch_event(target, &Event::new(event_type, true, true))
        .unwrap()
}

#[test]
fn a_handle_removes_exactly_its_listener() {
    let document = Document::new(100, 80).unwrap();
    let node = mounted(&document);
    let log = Log::default();
    let first = node.on("custom", logger(&log, "first")).unwrap();
    let _second = node.on("custom", logger(&log, "second")).unwrap();
    let capture = node.on_capture("custom", logger(&log, "capture")).unwrap();
    fire(&document, &node, "custom");
    assert_eq!(*log.borrow(), ["capture", "first", "second"]);

    log.borrow_mut().clear();
    first.remove().unwrap();
    capture.remove().unwrap();
    fire(&document, &node, "custom");
    assert_eq!(
        *log.borrow(),
        ["second"],
        "only the handles' listeners left"
    );

    // Removing again, or after the node is gone, does nothing.
    first.remove().unwrap();
    node.remove().unwrap();
    capture.remove().unwrap();
}

#[test]
fn dropping_a_handle_keeps_the_listener() {
    let document = Document::new(100, 80).unwrap();
    let node = mounted(&document);
    let log = Log::default();
    {
        let _handle = node.on("custom", logger(&log, "kept")).unwrap();
    }
    node.on("custom", logger(&log, "discarded")).unwrap();
    fire(&document, &node, "custom");
    assert_eq!(*log.borrow(), ["kept", "discarded"]);
}

#[test]
fn a_listener_removed_during_dispatch_is_not_called_and_one_added_waits() {
    let document = Document::new(100, 80).unwrap();
    let node = mounted(&document);
    let log = Log::default();
    let later: Rc<RefCell<Option<ListenerHandle>>> = Rc::default();
    {
        let later = later.clone();
        let log = log.clone();
        let target = node.clone();
        node.on("custom", move |_| {
            log.borrow_mut().push("first");
            if let Some(handle) = later.borrow_mut().take() {
                handle.remove().unwrap();
            }
            target.on("custom", logger(&log, "added")).unwrap();
        })
        .unwrap();
    }
    *later.borrow_mut() = Some(node.on("custom", logger(&log, "removed")).unwrap());
    fire(&document, &node, "custom");
    assert_eq!(*log.borrow(), ["first"]);
}

#[test]
fn a_once_listener_runs_once_and_is_removed_before_it_is_called() {
    let document = Document::new(100, 80).unwrap();
    let node = mounted(&document);
    let log = Log::default();
    let once = ListenerOptions {
        once: true,
        ..ListenerOptions::default()
    };
    {
        let log = log.clone();
        let target = node.clone();
        let nested = document.clone();
        node.on_with("custom", once, move |_| {
            log.borrow_mut().push("once");
            // A nested dispatch from inside the listener doesn't reach it
            // again: it was removed before it was called.
            fire(&nested, &target, "custom");
        })
        .unwrap();
    }
    node.on("custom", logger(&log, "always")).unwrap();
    fire(&document, &node, "custom");
    assert_eq!(*log.borrow(), ["once", "always", "always"]);
    log.borrow_mut().clear();
    fire(&document, &node, "custom");
    assert_eq!(*log.borrow(), ["always"]);
}

#[test]
fn prevent_default_in_a_passive_listener_does_nothing() {
    let document = Document::new(100, 80).unwrap();
    let node = mounted(&document);
    let passive = ListenerOptions {
        passive: Some(true),
        ..ListenerOptions::default()
    };
    let seen = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        node.on_with("custom", passive, move |event| {
            event.prevent_default();
            seen.borrow_mut().push(event.default_prevented());
        })
        .unwrap();
    }
    assert!(
        fire(&document, &node, "custom"),
        "a passive listener cannot cancel"
    );
    {
        let seen = seen.clone();
        node.on("custom", move |event| {
            event.prevent_default();
            seen.borrow_mut().push(event.default_prevented());
        })
        .unwrap();
    }
    assert!(
        !fire(&document, &node, "custom"),
        "a later active listener can"
    );
    assert_eq!(*seen.borrow(), [false, false, true]);
}

#[test]
fn scroll_blocking_listeners_on_the_root_default_to_passive() {
    // Chromium 147 `SetDefaultAddEventListenerOptions`: touchstart,
    // touchmove, wheel and mousewheel listeners without `passive` on a
    // top-level node (the window, the document, its element or body) are
    // passive. Div's native root stands for all of them.
    let document = Document::new(100, 80).unwrap();
    let root = document.body();
    let node = mounted(&document);
    let prevent = |event: &Event| event.prevent_default();
    for event_type in ["wheel", "mousewheel", "touchstart", "touchmove"] {
        let handle = root.on(event_type, prevent).unwrap();
        assert!(
            fire(&document, &node, event_type),
            "{event_type} on the root is passive"
        );
        handle.remove().unwrap();

        let handle = root
            .on_with(
                event_type,
                ListenerOptions {
                    passive: Some(false),
                    ..ListenerOptions::default()
                },
                prevent,
            )
            .unwrap();
        assert!(
            !fire(&document, &node, event_type),
            "explicit passive: false wins"
        );
        handle.remove().unwrap();

        let handle = node.on(event_type, prevent).unwrap();
        assert!(
            !fire(&document, &node, event_type),
            "{event_type} elsewhere is active"
        );
        handle.remove().unwrap();
    }
    let handle = root.on("click", prevent).unwrap();
    assert!(
        !fire(&document, &node, "click"),
        "other types on the root are active"
    );
    handle.remove().unwrap();
}

#[test]
fn capture_in_options_listens_in_the_capture_phase() {
    let document = Document::new(100, 80).unwrap();
    let parent = mounted(&document);
    let child = Element::create(&document, "div").unwrap();
    parent.append_child(&child).unwrap();
    let log = Log::default();
    parent.on("custom", logger(&log, "bubble")).unwrap();
    parent
        .on_with(
            "custom",
            ListenerOptions {
                capture: true,
                ..ListenerOptions::default()
            },
            logger(&log, "capture"),
        )
        .unwrap();
    child.on("custom", logger(&log, "target")).unwrap();
    fire(&document, &child, "custom");
    assert_eq!(*log.borrow(), ["capture", "target", "bubble"]);
}
