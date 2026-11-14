//! `Document::request_animation_frame` (RFC 0007, section 2): callbacks run
//! once, in request order, at the start of the next frame.

use div::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

fn document() -> Document {
    Document::new(64, 64).unwrap()
}

type Log = Rc<RefCell<Vec<String>>>;

fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

fn request(document: &Document, log: &Log, name: &'static str) -> AnimationFrameHandle {
    let log = log.clone();
    document
        .request_animation_frame(move |time_ms| log.borrow_mut().push(format!("{name}@{time_ms}")))
}

#[test]
fn callbacks_run_once_in_request_order_with_the_frame_time() {
    let document = document();
    let log = log();
    request(&document, &log, "a");
    request(&document, &log, "b");
    assert!(document.has_animation_frame_callbacks());
    assert!(log.borrow().is_empty(), "nothing runs before the frame");

    document.begin_frame(16.0).unwrap();
    assert_eq!(*log.borrow(), ["a@16", "b@16"]);
    assert!(!document.has_animation_frame_callbacks());

    document.begin_frame(32.0).unwrap();
    assert_eq!(log.borrow().len(), 2, "each callback runs once");
}

#[test]
fn a_callback_requested_during_the_frame_waits_for_the_next() {
    let document = document();
    let log = log();
    // A loop: each callback requests the next, so it runs once per frame.
    fn step(document: Document, log: Log, remaining: u32) {
        let next = document.clone();
        document.request_animation_frame(move |time_ms| {
            log.borrow_mut().push(format!("step@{time_ms}"));
            if remaining > 1 {
                step(next, log, remaining - 1);
            }
        });
    }
    step(document.clone(), log.clone(), 3);
    for time_ms in [10.0, 20.0, 30.0, 40.0] {
        document.begin_frame(time_ms).unwrap();
    }
    assert_eq!(*log.borrow(), ["step@10", "step@20", "step@30"]);
    assert!(!document.has_animation_frame_callbacks());
}

#[test]
fn cancel_stops_a_callback_before_or_during_its_frame() {
    let document = document();
    let log = log();
    let cancelled = request(&document, &log, "cancelled");
    document.cancel_animation_frame(cancelled);

    // `first` cancels `second`, which is due later in the same frame.
    let second = Rc::new(RefCell::new(None));
    {
        let (document, log, second) = (document.clone(), log.clone(), second.clone());
        document.clone().request_animation_frame(move |time_ms| {
            log.borrow_mut().push(format!("first@{time_ms}"));
            document.cancel_animation_frame(second.borrow().unwrap());
        });
    }
    *second.borrow_mut() = Some(request(&document, &log, "second"));
    request(&document, &log, "third");
    document.begin_frame(5.0).unwrap();
    assert_eq!(*log.borrow(), ["first@5", "third@5"]);

    // Stale handles and handles from another document are ignored.
    document.cancel_animation_frame(cancelled);
    let other = self::document();
    let foreign = request(&other, &log, "other");
    document.cancel_animation_frame(foreign);
    other.begin_frame(1.0).unwrap();
    assert_eq!(log.borrow().last().unwrap(), "other@1");
}

#[test]
fn callbacks_run_after_animations_advance_with_their_document_current() {
    let document = document();
    let marker = Element::create(&document, "div").unwrap();
    marker.set_attribute("id", "marker").unwrap();
    document.body().append_child(&marker).unwrap();
    let seen = Rc::new(RefCell::new(None));
    {
        let seen = seen.clone();
        document.request_animation_frame(move |time_ms| {
            let current = current_document();
            let same = current.element_by_id("marker").unwrap().is_some();
            *seen.borrow_mut() = Some((time_ms, current.get_time().unwrap(), same));
        });
    }
    document.begin_frame(48.0).unwrap();
    assert_eq!(*seen.borrow(), Some((48.0, 48.0, true)));
}

#[test]
fn callbacks_do_not_subscribe_the_effect_that_runs_the_frame() {
    let document = document();
    let source = create_signal(0);
    let runs = Rc::new(RefCell::new(0));
    {
        let reads = source;
        document.request_animation_frame(move |_| {
            reads.get();
        });
    }
    {
        let (document, runs) = (document.clone(), runs.clone());
        create_effect(move || {
            *runs.borrow_mut() += 1;
            document.begin_frame(1.0).unwrap();
        });
    }
    assert_eq!(*runs.borrow(), 1);
    source.set(1);
    assert_eq!(
        *runs.borrow(),
        1,
        "the callback's read did not subscribe the effect"
    );
}

#[test]
fn headless_frames_advance_from_the_document_time() {
    let app =
        HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap();
    let times = Rc::new(RefCell::new(Vec::new()));
    fn every_frame(document: Document, times: Rc<RefCell<Vec<f64>>>) {
        let next = document.clone();
        document.request_animation_frame(move |time_ms| {
            times.borrow_mut().push(time_ms);
            every_frame(next, times);
        });
    }
    every_frame(app.document().clone(), times.clone());
    app.advance_frames(3, 16.0).unwrap();
    app.advance_frame(8.0).unwrap();
    assert_eq!(*times.borrow(), [16.0, 32.0, 48.0, 56.0]);
    assert_eq!(app.document().get_time().unwrap(), 56.0);
    assert!(app.advance_frame(-1.0).is_err());
    assert!(app.advance_frame(f64::NAN).is_err());
}

#[cfg(feature = "testing")]
#[test]
fn test_app_steps_frames_and_renders_at_the_last_frame() {
    use div::testing::TestApp;
    let width = create_signal(10.0_f32);
    let mut app = TestApp::new(LogicalSize::new(64.0, 64.0)).mount(move || {
        let document = current_document();
        document.request_animation_frame(move |time_ms| width.set(10.0 + time_ms as f32));
        view! { <div id="bar" style:width={LengthValue::px(width.get())} style:height="4px"></div> }
    });
    app.advance_frame(20.0);
    assert_eq!(width.get(), 30.0);
    assert_eq!(
        app.by_id("bar").bounding_rect().unwrap().unwrap().width,
        30.0
    );
    let at_frame = app.render_png();
    assert_eq!(
        app.document().get_time().unwrap(),
        20.0,
        "rendering keeps the frame time"
    );
    assert_eq!(at_frame, app.render_png());
}
