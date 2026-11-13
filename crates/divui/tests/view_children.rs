//! `{expr}` children in `view!`: views render as themselves, text as text.

use div::prelude::*;

fn headless() -> HeadlessApp {
    HeadlessApp::new(ViewportMetrics::from_logical_size(64.0, 64.0, 1.0).unwrap()).unwrap()
}

fn root(app: &HeadlessApp) -> Element {
    app.document().body().first_child().unwrap().unwrap()
}

fn text(element: &Element) -> String {
    element.text_content().unwrap()
}

/// Children passed as a `Vec<ViewNode>` prop, rendered with `{children}`.
#[component]
fn Card(children: Vec<ViewNode>) -> ViewNode {
    view! { <section>"[" {children} "]"</section> }
}

#[test]
fn for_and_show_render_as_children() {
    let mut app = headless();
    let items = create_signal(vec![1, 2]);
    let open = create_signal(false);
    app.mount(move || {
        view! {
            <div>
                "items:"
                {For(move || items.get(), |item| *item, |item| view! { <span>{item}</span> })}
                {Show(move || open.get(), || "closed", || "open")}
            </div>
        }
    })
    .unwrap();
    let root = root(&app);
    assert_eq!(text(&root), "items:12closed");

    items.set(vec![3, 1, 4]);
    open.set(true);
    assert_eq!(text(&root), "items:314open");
}

#[test]
fn a_top_level_view_expression_is_returned_as_itself() {
    let mut app = headless();
    let items = create_signal(vec!["a", "b"]);
    app.mount(move || {
        view! { {For(move || items.get(), |item| *item, |item| view! { <p>{item}</p> })} }
    })
    .unwrap();
    // For's container is the body's only child.
    assert_eq!(text(&root(&app)), "ab");
    items.set(vec!["c"]);
    assert_eq!(text(&root(&app)), "c");
}

struct State {
    name: String,
}

#[test]
fn text_expressions_stay_text_and_reactive_ones_update() {
    let mut app = headless();
    let count = create_signal(1);
    app.mount(move || {
        let label = String::from("label");
        let path = std::path::PathBuf::from("dir");
        let state = std::rc::Rc::new(State {
            name: String::from("rc"),
        });
        let owned = State {
            name: String::from("field"),
        };
        view! {
            <p>
                {count.get()} " " {label} " " {path.display()} " "
                {state.name} " " {owned.name} " " {format!("x{}", count.get() * 10)}
            </p>
        }
    })
    .unwrap();
    let root = root(&app);
    assert_eq!(text(&root), "1 label dir rc field x10");
    count.set(2);
    assert_eq!(text(&root), "2 label dir rc field x20");
    // Text nodes, no wrapper elements.
    assert!(root.first_child().unwrap().is_none());
}

#[test]
fn view_values_and_borrowing_views_render() {
    let mut app = headless();
    app.mount(|| {
        let names = [String::from("x"), String::from("y")];
        // A view expression may borrow a local.
        let names = &names;
        let node = view! { <span>"node"</span> };
        view! {
            <div>
                {node}
                {names.iter().map(|name| { let name = name.clone(); view! { <span>{name}</span> } }).collect::<Vec<_>>()}
                {Some(view! { <span>"some"</span> })}
                <Card><span>"in card"</span></Card>
            </div>
        }
    })
    .unwrap();
    assert_eq!(text(&root(&app)), "nodexysome[in card]");
}
