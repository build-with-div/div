//! `on:` attributes on a component tag reach the element it renders.

use std::cell::RefCell;
use std::rc::Rc;

use div::prelude::*;

/// A button with a label and no handler prop, like `div-components`' Button.
#[component]
fn LabelButton(label: String) -> ViewNode {
    view! { <button id="button"><span id="label">{label.clone()}</span></button> }
}

/// Two sibling elements and a text node.
#[component]
fn Pair() -> ViewNode {
    view! {
        <p id="first">"first"</p>
        "text"
        <p id="second">"second"</p>
    }
}

/// Content shown by `Show`, which renders a `display: contents` container.
#[component]
fn Later() -> ViewNode {
    Show(
        || true,
        || ViewNode::Empty,
        || view! { <p id="later">"later"</p> },
    )
}

/// Only text, so no element.
#[component]
fn Caption() -> ViewNode {
    view! { "caption" }
}

fn headless() -> HeadlessApp {
    HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap()
}

fn by_id(app: &HeadlessApp, id: &str) -> Element {
    app.document().element_by_id(id).unwrap().unwrap()
}

#[test]
fn a_listener_on_a_component_reaches_its_root_element() {
    let mut app = headless();
    let clicks = Rc::new(RefCell::new(Vec::<String>::new()));
    let log = clicks.clone();
    app.mount(move || {
        let log = log.clone();
        let inner = log.clone();
        view! {
            <div>
                <LabelButton label="Go" on:click={move |event: &Event| {
                    log.borrow_mut().push(format!("click {}", event.event_type));
                }} on:keydown={move |_: &Event| inner.borrow_mut().push("keydown".into())} />
            </div>
        }
    })
    .unwrap();

    by_id(&app, "button").click().unwrap();
    // A click on the label inside bubbles to the forwarded listener.
    by_id(&app, "label").click().unwrap();
    assert_eq!(*clicks.borrow(), ["click click", "click click"]);
}

#[test]
fn a_listener_on_a_fragment_component_reaches_each_top_level_element() {
    let mut app = headless();
    let clicks = Rc::new(RefCell::new(Vec::<String>::new()));
    let log = clicks.clone();
    app.mount(move || {
        let log = log.clone();
        view! {
            <div>
                <Pair on:click=move |event| {
                    let target = event.current_target().unwrap();
                    log.borrow_mut().push(target.get_attribute("id").unwrap().unwrap());
                } />
            </div>
        }
    })
    .unwrap();

    by_id(&app, "first").click().unwrap();
    by_id(&app, "second").click().unwrap();
    assert_eq!(*clicks.borrow(), ["first", "second"]);
}

#[test]
fn a_listener_on_a_shown_component_catches_its_contents_clicks() {
    let mut app = headless();
    let clicks = Rc::new(RefCell::new(0));
    let count = clicks.clone();
    app.mount(move || {
        let count = count.clone();
        view! { <div><Later on:click=move |_| *count.borrow_mut() += 1 /></div> }
    })
    .unwrap();

    by_id(&app, "later").click().unwrap();
    assert_eq!(*clicks.borrow(), 1);
}

#[test]
#[should_panic(expected = "`<Caption on:click>`: `Caption` returns no element")]
fn a_listener_on_a_component_without_an_element_panics() {
    let mut app = headless();
    let _ = app.mount(|| view! { <div><Caption on:click=|_| {} /></div> });
}
