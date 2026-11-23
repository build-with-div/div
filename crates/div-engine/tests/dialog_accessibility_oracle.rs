//! The `dialog-accessibility-v1` oracle replayed through the engine.
//!
//! `tools/qualification/capture_dialog_accessibility_oracle.py` recorded the
//! accessibility tree of pinned Chromium 147 for a `<dialog>` in each state.
//! This test builds the same page and, for each case, compares which nodes
//! are exposed and the dialog's role, name, description and `modal` flag.
//!
//! A field listed in [`PENDING`] must still differ, so the ledger can only
//! shrink: when the owner's work lands, the case fails until its row goes.
use accesskit::{Node, NodeId, Role};
use div_dom::ElementTag;
use div_engine::{Engine, NodeHandle};
use div_geometry::ViewportMetrics;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const ORACLE: &str = include_str!(
    "../../../tests/fidelity/fixtures/dialog-accessibility-v1/chromium-linux-x11.json"
);

const NAMES: &str = "engine: the `aria-labelledby`, `aria-describedby` and `title` attributes";
const ROLES: &str = "engine: the `role` attribute";

/// The dialog fields that do not match yet, with the work that closes them.
const PENDING: &[(&str, &[&str], &str)] = &[
    ("show_labelledby", &["name"], NAMES),
    ("modal_title", &["name"], NAMES),
    ("modal_labelledby_inside", &["name"], NAMES),
    ("show_describedby", &["description"], NAMES),
    ("modal_describedby_inside", &["description"], NAMES),
    ("show_role_alertdialog", &["role"], ROLES),
    ("modal_role_alertdialog", &["role"], ROLES),
    ("modal_role_button", &["role", "name", "modal"], ROLES),
];

struct Page {
    engine: Engine,
    body: NodeHandle,
    d: NodeHandle,
    second: NodeHandle,
    nodes: HashMap<&'static str, NodeHandle>,
}

fn element(engine: &mut Engine, parent: NodeHandle, tag: ElementTag, text: &str) -> NodeHandle {
    let node = engine.create_native_element(tag).unwrap();
    let content = engine.create_text(text).unwrap();
    engine.append_child(node, content).unwrap();
    engine.append_child(parent, node).unwrap();
    node
}

fn dialog(engine: &mut Engine, parent: NodeHandle) -> NodeHandle {
    let node = engine
        .create_native_element_named(ElementTag::Div, "dialog")
        .unwrap();
    engine.append_child(parent, node).unwrap();
    node
}

fn page() -> Page {
    let metrics = ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap();
    let mut engine = Engine::new(metrics).unwrap();
    let body = engine.root();
    let outside = element(&mut engine, body, ElementTag::Button, "out");
    let d = dialog(&mut engine, body);
    let inside = element(&mut engine, d, ElementTag::Button, "in");
    let text = element(&mut engine, d, ElementTag::Div, "body text");
    let second = dialog(&mut engine, body);
    element(&mut engine, second, ElementTag::Button, "two");
    let heading = element(&mut engine, body, ElementTag::Div, "Heading name");
    element(&mut engine, body, ElementTag::Div, "Description text");
    let nodes = HashMap::from([
        ("d", d),
        ("inside", inside),
        ("text", text),
        ("second", second),
        ("outside", outside),
        ("heading", heading),
    ]);
    Page {
        engine,
        body,
        d,
        second,
        nodes,
    }
}

fn run(page: &mut Page, step: &Value) {
    if let Some(parts) = step.as_array() {
        let name = parts[1].as_str().unwrap();
        let value = parts[2].as_str().unwrap();
        page.engine.set_attribute(page.d, name, value).unwrap();
        return;
    }
    let (d, second, body) = (page.d, page.second, page.body);
    let engine = &mut page.engine;
    match step.as_str().unwrap() {
        "show" => engine.show_dialog(d).unwrap(),
        "show-modal" => engine.show_modal(d).unwrap(),
        "close" => {
            engine.close_dialog(d, None).unwrap();
        }
        "remove-open" => {
            engine.remove_attribute(d, "open").unwrap();
        }
        "set-open" => engine.set_attribute(d, "open", "").unwrap(),
        "move" => engine.append_or_move_child(body, d).unwrap(),
        "second-show" => engine.show_dialog(second).unwrap(),
        "second-show-modal" => engine.show_modal(second).unwrap(),
        other => panic!("unknown step {other}"),
    }
}

/// The nodes a user of the tree can reach: from the root, through nodes that
/// are not hidden.
fn exposed(engine: &mut Engine) -> (HashMap<NodeId, Node>, HashSet<NodeId>) {
    let root = engine.accessibility_node_id(engine.root()).unwrap();
    let nodes: HashMap<_, _> = engine
        .accessibility_snapshot()
        .unwrap()
        .into_iter()
        .collect();
    let mut reached = HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = nodes.get(&id) else { continue };
        if node.is_hidden() || !reached.insert(id) {
            continue;
        }
        stack.extend(node.children().iter().copied());
    }
    (nodes, reached)
}

fn role_name(role: Role) -> String {
    match role {
        Role::Dialog => "dialog".to_owned(),
        Role::AlertDialog => "alertdialog".to_owned(),
        Role::Button => "button".to_owned(),
        other => format!("{other:?}"),
    }
}

/// Chromium's dialog fields and Div's, by field name.
fn dialog_fields(expected: &Value, node: &Node) -> Vec<(&'static str, String, String)> {
    let text = |value: &Value| value.as_str().unwrap_or("").to_owned();
    vec![
        ("role", text(&expected["role"]), role_name(node.role())),
        (
            "name",
            text(&expected["name"]),
            node.label().unwrap_or("").to_owned(),
        ),
        (
            "description",
            text(&expected["description"]),
            node.description().unwrap_or("").to_owned(),
        ),
        (
            "modal",
            expected["modal"].as_bool().unwrap_or(false).to_string(),
            node.is_modal().to_string(),
        ),
    ]
}

#[test]
fn the_accessibility_tree_of_a_dialog_matches_chromium() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    let cases = oracle["cases"].as_object().unwrap();
    assert_eq!(cases.len(), 35);
    for (name, _, _) in PENDING {
        assert!(cases.contains_key(*name), "{name} is not in the oracle");
    }
    let mut failures = Vec::new();
    for (case, record) in cases {
        let mut page = page();
        for step in record["steps"].as_array().unwrap() {
            run(&mut page, step);
        }
        let handles = page.nodes.clone();
        let (nodes, reached) = exposed(&mut page.engine);
        let expected = &record["nodes"];
        for (label, handle) in &handles {
            let id = page.engine.accessibility_node_id(*handle).unwrap();
            let chromium = !expected[*label]["ignored"].as_bool().unwrap();
            if reached.contains(&id) != chromium {
                failures.push(format!(
                    "{case}: {label} exposed: Chromium {chromium}, Div {}",
                    reached.contains(&id)
                ));
            }
        }
        let pending = PENDING
            .iter()
            .find(|(name, _, _)| name == case)
            .map_or(&[][..], |(_, fields, _)| *fields);
        if expected["d"]["ignored"].as_bool().unwrap() {
            assert!(pending.is_empty(), "{case}: pending on an ignored dialog");
            continue;
        }
        let id = page.engine.accessibility_node_id(page.d).unwrap();
        let Some(node) = nodes.get(&id) else {
            failures.push(format!("{case}: the dialog is not in the tree"));
            continue;
        };
        for (field, chromium, div) in dialog_fields(&expected["d"], node) {
            let is_pending = pending.contains(&field);
            if is_pending && chromium == div {
                failures.push(format!(
                    "{case}: {field} now matches ({div:?}); remove it from PENDING"
                ));
            } else if !is_pending && chromium != div {
                failures.push(format!(
                    "{case}: {field}: Chromium {chromium:?}, Div {div:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn modal_flag(engine: &mut Engine, handle: NodeHandle) -> bool {
    let id = engine.accessibility_node_id(handle).unwrap();
    let nodes: HashMap<_, _> = engine
        .accessibility_snapshot()
        .unwrap()
        .into_iter()
        .collect();
    nodes[&id].is_modal()
}

#[test]
fn an_authored_role_decides_whether_a_dialog_has_the_modal_flag() {
    let mut page = page();
    let d = page.d;
    page.engine.show_modal(d).unwrap();
    page.engine
        .set_accessibility_role(d, Role::AlertDialog)
        .unwrap();
    assert!(modal_flag(&mut page.engine, d));
    page.engine.set_accessibility_role(d, Role::Button).unwrap();
    assert!(!modal_flag(&mut page.engine, d));
    page.engine.clear_accessibility_role(d).unwrap();
    assert!(modal_flag(&mut page.engine, d));
}

#[test]
fn aria_modal_is_read_only_on_a_dialog() {
    let metrics = ViewportMetrics::from_logical_size(480.0, 400.0, 1.0).unwrap();
    let mut engine = Engine::new(metrics).unwrap();
    let root = engine.root();
    let plain = element(&mut engine, root, ElementTag::Div, "plain");
    engine.set_attribute(plain, "aria-modal", "true").unwrap();
    assert!(!modal_flag(&mut engine, plain));
    // A modal root that is not a dialog keeps its flag, whatever the attribute.
    engine.set_attribute(plain, "aria-modal", "false").unwrap();
    engine.set_modal_root(Some(plain)).unwrap();
    assert!(modal_flag(&mut engine, plain));
}
