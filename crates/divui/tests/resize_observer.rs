//! `ResizeObserver` (RFC 0007, section 3): observed targets are delivered
//! after layout in `begin_frame`, once when first observed and then when the
//! observed box changes size, deeper targets first within a frame.

use div::prelude::*;
use div::typed_style::parse_literal;
use std::cell::RefCell;
use std::rc::Rc;

fn document() -> Document {
    Document::new(320, 240).unwrap()
}

fn style(element: &Element, styles: &[(StyleProperty, &str)]) {
    for &(property, input) in styles {
        element
            .set_property(property, parse_literal(property, input).unwrap())
            .unwrap();
    }
}

fn block(document: &Document, parent: &Element, id: &str, width: &str, height: &str) -> Element {
    let element = Element::create(document, "div").unwrap();
    element.set_attribute("id", id).unwrap();
    parent.append_child(&element).unwrap();
    style(
        &element,
        &[
            (StyleProperty::BoxSizing, "content-box"),
            (StyleProperty::Width, width),
            (StyleProperty::Height, height),
        ],
    );
    element
}

fn size(inline_size: f64, block_size: f64) -> ResizeObserverSize {
    ResizeObserverSize {
        inline_size,
        block_size,
    }
}

type Log = Rc<RefCell<Vec<String>>>;

/// An observer that logs `name:id=inline x block` for each entry's
/// observed-box size.
fn logging_observer(document: &Document, log: &Log, name: &'static str) -> ResizeObserver {
    let log = log.clone();
    ResizeObserver::new(document, move |entries, _| {
        for entry in entries {
            let id = entry
                .target
                .get_attribute("id")
                .unwrap()
                .unwrap_or_default();
            let size = entry.content_box_size;
            log.borrow_mut().push(format!(
                "{name}:{id}={}x{}",
                size.inline_size, size.block_size
            ));
        }
    })
}

#[test]
fn a_target_is_delivered_when_first_observed_and_when_it_resizes() {
    let document = document();
    let target = block(&document, &document.body(), "a", "100px", "50px");
    style(
        &target,
        &[
            (StyleProperty::Padding, "10px 6px 4px 3px"),
            (StyleProperty::Border, "2px solid black"),
        ],
    );
    let entries = Rc::new(RefCell::new(Vec::new()));
    let observer = {
        let entries = entries.clone();
        ResizeObserver::new(&document, move |delivered, _| {
            entries.borrow_mut().extend_from_slice(delivered);
        })
    };
    observer.observe(&target, ResizeObserverBox::ContentBox);
    assert!(document.has_pending_resize_observations());
    assert!(
        entries.borrow().is_empty(),
        "nothing is delivered before a frame"
    );

    document.begin_frame(16.0).unwrap();
    assert!(!document.has_pending_resize_observations());
    {
        let entries = entries.borrow();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert!(entry.target.is_same_node(&target).unwrap());
        assert_eq!(entry.content_box_size, size(100.0, 50.0));
        assert_eq!(entry.border_box_size, size(113.0, 68.0));
        // RFC 0007 as amended: every entry carries the device-pixel size, as
        // Chromium 147's `ResizeObserverEntry` constructor fills it whatever
        // box was observed. This observation is of the content box only.
        assert_eq!(entry.device_pixel_content_box_size, size(100.0, 50.0));
        assert_eq!(
            entry.content_rect,
            Rect {
                x: 3.0,
                y: 10.0,
                width: 100.0,
                height: 50.0,
            }
        );
    }

    document.begin_frame(32.0).unwrap();
    assert_eq!(
        entries.borrow().len(),
        1,
        "an unchanged size is not delivered"
    );

    style(&target, &[(StyleProperty::Width, "120px")]);
    document.begin_frame(48.0).unwrap();
    assert_eq!(entries.borrow().len(), 2);
    assert_eq!(entries.borrow()[1].content_box_size, size(120.0, 50.0));
}

#[test]
fn only_a_change_in_the_observed_box_is_delivered() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let target = block(&document, &document.body(), "a", "40px", "20px");
    let content = logging_observer(&document, &log, "content");
    let border = logging_observer(&document, &log, "border");
    content.observe(&target, ResizeObserverBox::ContentBox);
    border.observe(&target, ResizeObserverBox::BorderBox);
    document.begin_frame(1.0).unwrap();
    assert_eq!(*log.borrow(), ["content:a=40x20", "border:a=40x20"]);

    // Padding grows the border box only.
    style(&target, &[(StyleProperty::Padding, "5px")]);
    document.begin_frame(2.0).unwrap();
    assert_eq!(log.borrow().len(), 3);
    assert_eq!(log.borrow()[2], "border:a=40x20");

    // Observing the same target with another box delivers it afresh.
    content.observe(&target, ResizeObserverBox::DevicePixelContentBox);
    document.begin_frame(3.0).unwrap();
    assert_eq!(log.borrow()[3..], ["content:a=40x20"]);
}

#[test]
fn a_target_without_a_box_is_delivered_once_with_zero_sizes() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let target = block(&document, &document.body(), "hidden", "40px", "20px");
    style(&target, &[(StyleProperty::Display, "none")]);
    let observer = logging_observer(&document, &log, "o");
    observer.observe(&target, ResizeObserverBox::ContentBox);
    document.begin_frame(1.0).unwrap();
    document.begin_frame(2.0).unwrap();
    assert_eq!(*log.borrow(), ["o:hidden=0x0"]);

    style(&target, &[(StyleProperty::Display, "block")]);
    document.begin_frame(3.0).unwrap();
    assert_eq!(*log.borrow(), ["o:hidden=0x0", "o:hidden=40x20"]);
}

#[test]
fn entries_follow_observation_order_and_observers_creation_order() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let body = document.body();
    let a = block(&document, &body, "a", "10px", "10px");
    let b = block(&document, &body, "b", "20px", "10px");
    let first = logging_observer(&document, &log, "first");
    let second = logging_observer(&document, &log, "second");
    second.observe(&a, ResizeObserverBox::ContentBox);
    first.observe(&b, ResizeObserverBox::ContentBox);
    first.observe(&a, ResizeObserverBox::ContentBox);
    document.begin_frame(1.0).unwrap();
    assert_eq!(
        *log.borrow(),
        ["first:b=20x10", "first:a=10x10", "second:a=10x10"]
    );
}

#[test]
fn unobserve_disconnect_and_drop_stop_delivery() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let body = document.body();
    let a = block(&document, &body, "a", "10px", "10px");
    let b = block(&document, &body, "b", "10px", "10px");
    let kept = logging_observer(&document, &log, "kept");
    let disconnected = logging_observer(&document, &log, "disconnected");
    let dropped = logging_observer(&document, &log, "dropped");
    for observer in [&kept, &disconnected, &dropped] {
        observer.observe(&a, ResizeObserverBox::ContentBox);
        observer.observe(&b, ResizeObserverBox::ContentBox);
    }
    kept.unobserve(&a);
    kept.unobserve(&a);
    disconnected.disconnect();
    drop(dropped);
    document.begin_frame(1.0).unwrap();
    assert_eq!(*log.borrow(), ["kept:b=10x10"]);
}

#[test]
fn deeper_targets_resized_by_a_callback_are_delivered_in_the_same_frame() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let body = document.body();
    let outer = block(&document, &body, "outer", "100px", "40px");
    let inner = block(&document, &outer, "inner", "10px", "10px");
    let errors = Rc::new(RefCell::new(Vec::new()));
    {
        let errors = errors.clone();
        document.on_error(move |error| errors.borrow_mut().push(error.message.clone()));
    }
    // The observer of `outer` sizes `inner` to half of it, as a layout
    // component would.
    let sizing = {
        let (log, inner) = (log.clone(), inner.clone());
        ResizeObserver::new(&document, move |entries, _| {
            for entry in entries {
                let width = entry.content_box_size.inline_size / 2.0;
                log.borrow_mut().push(format!("sizing:outer={width}"));
                style(&inner, &[(StyleProperty::Width, &format!("{width}px"))]);
            }
        })
    };
    sizing.observe(&outer, ResizeObserverBox::ContentBox);
    let watching = logging_observer(&document, &log, "watching");
    watching.observe(&inner, ResizeObserverBox::ContentBox);
    document.begin_frame(1.0).unwrap();
    assert_eq!(
        *log.borrow(),
        [
            // Depth 0: `inner` (depth 3) is delivered before `sizing`
            // changes it, as it is gathered with `outer` (depth 2).
            "sizing:outer=50",
            "watching:inner=10x10",
            // Deeper than 2: `inner` resized by the callback.
            "watching:inner=50x10",
        ]
    );
    assert!(errors.borrow().is_empty());
}

#[test]
fn a_shallower_target_resized_by_a_callback_waits_and_reports_the_loop_error() {
    let document = document();
    let log = Rc::new(RefCell::new(Vec::new()));
    let body = document.body();
    let outer = block(&document, &body, "outer", "100px", "40px");
    let inner = block(&document, &outer, "inner", "10px", "10px");
    let errors = Rc::new(RefCell::new(Vec::new()));
    {
        let errors = errors.clone();
        document.on_error(move |error| errors.borrow_mut().push(error.message.clone()));
    }
    let watching = logging_observer(&document, &log, "watching");
    watching.observe(&outer, ResizeObserverBox::ContentBox);
    document.begin_frame(1.0).unwrap();
    // The observer of `inner` grows `outer`, which is shallower.
    let growing = {
        let (log, outer) = (log.clone(), outer.clone());
        ResizeObserver::new(&document, move |_, _| {
            log.borrow_mut().push("growing:inner".to_owned());
            style(&outer, &[(StyleProperty::Width, "200px")]);
        })
    };
    growing.observe(&inner, ResizeObserverBox::ContentBox);
    assert!(document.has_pending_resize_observations());
    document.begin_frame(2.0).unwrap();
    assert_eq!(
        *log.borrow(),
        ["watching:outer=100x40", "growing:inner"],
        "`outer` is not delivered after a deeper target in the same frame"
    );
    assert_eq!(
        *errors.borrow(),
        ["ResizeObserver loop completed with undelivered notifications."]
    );
    assert!(
        document.has_pending_resize_observations(),
        "the skipped target asks for another frame"
    );

    document.begin_frame(3.0).unwrap();
    assert_eq!(log.borrow()[2..], ["watching:outer=200x40"]);
    assert_eq!(errors.borrow().len(), 1);
}

#[test]
fn callbacks_run_with_the_document_current_and_untracked() {
    let document = document();
    let target = block(&document, &document.body(), "a", "10px", "10px");
    let source = create_signal(0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observer = {
        let seen = seen.clone();
        ResizeObserver::new(&document, move |entries, observer| {
            source.get();
            let current = current_document();
            seen.borrow_mut()
                .push((current.element_by_id("a").unwrap().is_some(), entries.len()));
            // The callback gets its own observer.
            observer.unobserve(&entries[0].target);
        })
    };
    observer.observe(&target, ResizeObserverBox::ContentBox);
    let runs = Rc::new(RefCell::new(0));
    {
        let (document, runs) = (document.clone(), runs.clone());
        create_effect(move || {
            *runs.borrow_mut() += 1;
            document.begin_frame(1.0).unwrap();
        });
    }
    assert_eq!(*seen.borrow(), [(true, 1)]);
    source.set(1);
    assert_eq!(
        *runs.borrow(),
        1,
        "the callback's read did not subscribe the effect"
    );
    style(&target, &[(StyleProperty::Width, "30px")]);
    document.begin_frame(2.0).unwrap();
    assert_eq!(seen.borrow().len(), 1, "the target was unobserved");
}

#[test]
fn a_new_observation_asks_the_window_for_a_frame() {
    let app =
        HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap();
    let document = app.document().clone();
    let target = block(&document, &document.body(), "a", "10px", "10px");
    let delivered = Rc::new(RefCell::new(0));
    let observer = {
        let delivered = delivered.clone();
        ResizeObserver::new(&document, move |_, _| *delivered.borrow_mut() += 1)
    };
    assert!(!document.has_pending_resize_observations());
    observer.observe(&target, ResizeObserverBox::BorderBox);
    assert!(document.has_pending_resize_observations());
    app.advance_frame(16.0).unwrap();
    assert_eq!(*delivered.borrow(), 1);
    assert!(!document.has_pending_resize_observations());
}

#[cfg(feature = "testing")]
#[test]
fn test_app_collects_reported_errors() {
    use div::testing::TestApp;
    let mut app = TestApp::new(LogicalSize::new(200.0, 100.0)).mount(|| {
        view! { <div id="outer" style:width="100px" style:height="20px"><div id="inner" style:width="10px" style:height="10px"></div></div> }
    });
    let outer = app.by_id("outer");
    let grow = ResizeObserver::new(app.document(), move |_, _| {
        style(&outer, &[(StyleProperty::Width, "150px")]);
    });
    let watch = ResizeObserver::new(app.document(), |_, _| {});
    watch.observe(&app.by_id("outer"), ResizeObserverBox::ContentBox);
    app.advance_frame(16.0);
    grow.observe(&app.by_id("inner"), ResizeObserverBox::ContentBox);
    app.advance_frame(16.0);
    let errors = app.take_reported_errors();
    assert_eq!(
        errors,
        [ReportedError {
            message: "ResizeObserver loop completed with undelivered notifications.".to_owned()
        }]
    );
    assert!(app.take_reported_errors().is_empty());
}
