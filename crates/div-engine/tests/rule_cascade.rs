//! The rule cascade: CSS Cascade 5 ordering of adopted style rules, and
//! incremental restyles that equal a full re-cascade.

use div_dom::ElementTag;
use div_engine::{
    AnimationTimeline, Engine, FocusOrigin, NodeHandle, PointerEventKind, StyleSheetError,
    ViewportMetrics, MOUSE_POINTER_ID,
};
use div_style::selector_parser::parse_selector_list;
use div_style::stylesheet::{CascadeOrigin, StyleRule, StyleSheet};
use div_style::{
    parse_literal, AnimationOptions, Color, FillMode, Keyframes, LengthValue, PropertyKeyframes,
    Style, StyleProperty,
};
use std::collections::HashSet;

fn engine() -> Engine {
    Engine::new(ViewportMetrics::from_logical_size(200.0, 200.0, 1.0).unwrap()).unwrap()
}

fn element(engine: &mut Engine, parent: NodeHandle, tag: ElementTag) -> NodeHandle {
    let element = engine.create_native_element(tag).unwrap();
    engine.append_child(parent, element).unwrap();
    element
}

fn rule(selectors: &str, declarations: Style) -> StyleRule {
    StyleRule::new(parse_selector_list(selectors, None).unwrap(), declarations)
}

fn opacity(value: f32) -> Style {
    Style::default().opacity(value)
}

fn opacity_of(engine: &Engine, element: NodeHandle) -> f32 {
    engine.computed_style(element).unwrap().opacity
}

#[test]
fn specificity_then_order_of_appearance() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "class", "a b").unwrap();
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule(".a", opacity(0.3)))
        .with_rule(rule("div", opacity(0.4)))
        .with_rule(rule(".b", opacity(0.5)));
    engine.adopt_style_sheet(&sheet).unwrap();
    // `.a` and `.b` outrank `div`; `.b` comes later.
    assert_eq!(opacity_of(&engine, target), 0.5);
}

#[test]
fn a_selector_list_ranks_by_its_most_specific_matching_selector() {
    let mut engine = engine();
    let root = engine.root();
    let with_id = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(with_id, "id", "x").unwrap();
    let plain = element(&mut engine, root, ElementTag::Div);
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("div, #x", opacity(0.2)))
        .with_rule(rule("div.never, div:first-child", opacity(0.6)));
    engine.adopt_style_sheet(&sheet).unwrap();
    // On `#x` the list has specificity (1, 0, 0) and beats the later
    // `div:first-child` (0, 1, 1); on the other element it has (0, 0, 1).
    assert_eq!(opacity_of(&engine, with_id), 0.2);
    assert_eq!(opacity_of(&engine, plain), 0.2);
    let first = engine.children(root).unwrap()[0];
    assert_eq!(first, with_id);
}

#[test]
fn origins_importance_and_the_style_attribute() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "id", "t").unwrap();
    let user_agent = StyleSheet::new(CascadeOrigin::UserAgent).with_rule(rule("#t", opacity(0.1)));
    engine.adopt_style_sheet(&user_agent).unwrap();
    assert_eq!(opacity_of(&engine, target), 0.1);

    // Author normal beats user-agent normal, whatever the specificity.
    let author = engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author).with_rule(rule("div", opacity(0.2))),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.2);

    // The element's own declarations rank as the style attribute.
    engine
        .set_property(target, StyleProperty::Opacity, 0.3_f32.into())
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.3);

    // Author `!important` beats the style attribute.
    let important = engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author)
                .with_rule(rule("div", Style::default()).with_important(opacity(0.4))),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.4);

    // User-agent `!important` beats author `!important`.
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::UserAgent)
                .with_rule(rule("div", Style::default()).with_important(opacity(0.5))),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.5);

    engine.remove_style_sheet(important).unwrap();
    engine.remove_style_sheet(author).unwrap();
    assert_eq!(opacity_of(&engine, target), 0.5);
    assert_eq!(
        engine.remove_style_sheet(author),
        Err(StyleSheetError::UnknownSheet)
    );
}

#[test]
fn layers_rank_below_unlayered_rules_and_reverse_for_important() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "id", "t").unwrap();
    let mut sheet = StyleSheet::new(CascadeOrigin::Author);
    sheet.declare_layers(["base", "theme"]);
    sheet.push(rule("#t", opacity(0.1)).in_layer("theme"));
    sheet.push(rule("#t", opacity(0.2)).in_layer("base"));
    sheet.push(rule("div", opacity(0.3)));
    engine.adopt_style_sheet(&sheet).unwrap();
    // The unlayered `div` beats both layered `#t` rules.
    assert_eq!(opacity_of(&engine, target), 0.3);

    let mut sheet = StyleSheet::new(CascadeOrigin::Author);
    sheet.declare_layers(["base", "theme"]);
    sheet.push(rule("#t", opacity(0.1)).in_layer("theme"));
    sheet.push(rule("#t", opacity(0.2)).in_layer("base"));
    let mut engine2 = engine_with(&sheet);
    let target2 = engine2.1;
    // Among layers, the later-declared `theme` wins although its rule comes
    // first.
    assert_eq!(opacity_of(&engine2.0, target2), 0.1);

    let mut sheet = StyleSheet::new(CascadeOrigin::Author);
    sheet.declare_layers(["base", "theme"]);
    sheet.push(
        rule("div", Style::default())
            .with_important(opacity(0.6))
            .in_layer("theme"),
    );
    sheet.push(
        rule("div", Style::default())
            .with_important(opacity(0.7))
            .in_layer("base"),
    );
    sheet.push(rule("#t", Style::default()).with_important(opacity(0.8)));
    engine2 = engine_with(&sheet);
    // For `!important`, the earliest layer wins over later layers and over
    // unlayered rules.
    assert_eq!(opacity_of(&engine2.0, engine2.1), 0.7);
}

fn engine_with(sheet: &StyleSheet) -> (Engine, NodeHandle) {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "id", "t").unwrap();
    engine.adopt_style_sheet(sheet).unwrap();
    (engine, target)
}

#[test]
fn presentational_hints_rank_between_user_agent_and_author_rules() {
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::UserAgent).with_rule(rule("div", opacity(0.1))),
        )
        .unwrap();
    engine
        .set_presentational_hint(target, StyleProperty::Opacity, 0.2_f32.into())
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.2);
    let author = engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author).with_rule(rule("*", opacity(0.3))),
        )
        .unwrap();
    assert_eq!(opacity_of(&engine, target), 0.3);
    engine.remove_style_sheet(author).unwrap();
    assert!(engine
        .remove_presentational_hint(target, StyleProperty::Opacity)
        .unwrap());
    assert_eq!(opacity_of(&engine, target), 0.1);
}

#[test]
fn matched_declarations_inherit() {
    let mut engine = engine();
    let root = engine.root();
    let parent = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(parent, "class", "warm").unwrap();
    let child = element(&mut engine, parent, ElementTag::Span);
    engine
        .adopt_style_sheet(
            &StyleSheet::new(CascadeOrigin::Author)
                .with_rule(rule(".warm", Style::default().color(Color::RED))),
        )
        .unwrap();
    assert_eq!(engine.computed_style(child).unwrap().color, Color::RED);
    engine.set_attribute(parent, "class", "cool").unwrap();
    assert_eq!(
        engine.computed_style(child).unwrap().color,
        engine.computed_style(root).unwrap().color
    );
}

#[test]
fn sheets_with_unmatched_state_or_bad_values_are_rejected() {
    let mut engine = engine();
    let sheet =
        StyleSheet::new(CascadeOrigin::Author).with_rule(rule("div:not(:invalid)", opacity(0.5)));
    assert_eq!(
        engine.adopt_style_sheet(&sheet),
        Err(StyleSheetError::UnsupportedPseudoClass("invalid"))
    );
}

#[test]
fn animations_rank_above_normal_and_below_important_declarations() {
    let animated = |sheet: StyleSheet| {
        let (mut engine, target) = engine_with(&sheet);
        engine
            .animate(
                target,
                PropertyKeyframes::typed(
                    StyleProperty::Opacity,
                    Keyframes::from_values(0.25_f32, 0.25_f32),
                )
                .unwrap(),
                AnimationOptions {
                    duration_ms: 1000.0,
                    fill: FillMode::Both,
                    ..AnimationOptions::default()
                },
                AnimationTimeline::Document,
            )
            .unwrap();
        engine.update().unwrap();
        opacity_of(&engine, target)
    };
    assert_eq!(
        animated(StyleSheet::new(CascadeOrigin::Author).with_rule(rule("#t", opacity(0.5)))),
        0.25
    );
    assert_eq!(
        animated(
            StyleSheet::new(CascadeOrigin::Author)
                .with_rule(rule("#t", Style::default()).with_important(opacity(0.5)))
        ),
        0.5
    );
}

/// Every element's computed style, as text.
fn snapshot(engine: &Engine, elements: &[NodeHandle]) -> Vec<String> {
    elements
        .iter()
        .map(|element| format!("{:?}", engine.computed_style(*element).unwrap()))
        .collect()
}

/// Restyle the document from scratch by adopting the sheet again.
fn full_cascade(engine: &mut Engine, sheet: &StyleSheet, id: &mut div_engine::StyleSheetId) {
    engine.remove_style_sheet(*id).unwrap();
    *id = engine.adopt_style_sheet(sheet).unwrap();
    engine.update().unwrap();
}

#[test]
fn incremental_restyles_equal_a_full_cascade() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule(".on > span", Style::default().color(Color::RED)))
        .with_rule(rule("div + div", opacity(0.9)))
        .with_rule(rule("div ~ .late", opacity(0.8)))
        .with_rule(rule("span:first-child", opacity(0.7)))
        .with_rule(rule("div:nth-child(2n of .on)", opacity(0.6)))
        .with_rule(rule("#hot", Style::default().color(Color::BLUE)))
        .with_rule(rule(
            "[data-mode=dark] span",
            Style::default().color(Color::GREEN),
        ))
        .with_rule(rule("div:has(> .flag)", opacity(0.5)))
        .with_rule(rule("div:empty", opacity(0.4)))
        .with_rule(rule("input:checked", opacity(0.3)))
        .with_rule(rule("div:focus-within", opacity(0.2)))
        .with_rule(rule("input:focus", opacity(0.15)));
    let mut engine = engine();
    let root = engine.root();
    let mut elements = Vec::new();
    for _ in 0..4 {
        let div = element(&mut engine, root, ElementTag::Div);
        elements.push(div);
        for _ in 0..2 {
            elements.push(element(&mut engine, div, ElementTag::Span));
        }
    }
    let input = element(&mut engine, elements[0], ElementTag::Input);
    engine.set_attribute(input, "type", "checkbox").unwrap();
    elements.push(input);
    let mut id = engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();

    let divs: Vec<_> = elements
        .iter()
        .copied()
        .filter(|e| engine.computed_style(*e).is_ok())
        .collect();
    let (d0, s0, d1, s2, d2) = (divs[0], divs[1], divs[3], divs[4], divs[6]);
    let mutations: Vec<Box<dyn Fn(&mut Engine)>> = vec![
        Box::new(move |e| e.set_attribute(d1, "class", "on").unwrap()),
        Box::new(move |e| e.set_attribute(d2, "class", "on late").unwrap()),
        Box::new(move |e| e.set_attribute(s2, "id", "hot").unwrap()),
        Box::new(move |e| e.set_attribute(d0, "data-mode", "dark").unwrap()),
        Box::new(move |e| {
            e.remove_attribute(d1, "class").unwrap();
        }),
        Box::new(move |e| e.set_attribute(s0, "class", "flag").unwrap()),
        Box::new(move |e| {
            let first = e.children(d2).unwrap()[0];
            e.detach(first).unwrap();
        }),
        Box::new(move |e| {
            let span = e.create_native_element(ElementTag::Span).unwrap();
            let before = e.children(d1).unwrap()[0];
            e.insert_before(d1, span, before).unwrap();
        }),
        Box::new(move |e| {
            for child in e.children(d2).unwrap() {
                e.remove(child).unwrap();
            }
        }),
        Box::new(move |e| e.append_or_move_child(d1, s0).unwrap()),
        Box::new(move |e| e.set_checked(input, true).unwrap()),
        Box::new(move |e| e.focus(input).unwrap()),
        Box::new(move |e| e.blur(input).unwrap()),
        Box::new(move |e| e.set_attribute(d0, "data-mode", "light").unwrap()),
    ];
    for (step, mutate) in mutations.iter().enumerate() {
        mutate(&mut engine);
        engine.update().unwrap();
        let live: Vec<_> = elements
            .iter()
            .copied()
            .filter(|e| engine.computed_style(*e).is_ok())
            .collect();
        let incremental = snapshot(&engine, &live);
        full_cascade(&mut engine, &sheet, &mut id);
        assert_eq!(incremental, snapshot(&engine, &live), "mutation {step}");
    }
}

/// Each pointer path that changes `:hover` or `:active` restyles: hover
/// enter and leave through routing, press, release and cancel, the pointer
/// leaving the window, and a forgotten pointer. Every step reads the
/// computed style after `update()`, so a path that changes the state
/// without noting it leaves the previous value and fails here.
#[test]
fn pointer_state_changes_restyle() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("#t:hover", opacity(0.5)))
        .with_rule(rule("#t:active", opacity(0.25)));
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(target, "id", "t").unwrap();
    for property in [StyleProperty::Width, StyleProperty::Height] {
        let value = parse_literal(property, "100px").unwrap();
        engine.set_property(target, property, value).unwrap();
    }
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, target), 1.0);

    let step = |engine: &mut Engine, what: &str, expected: f32| {
        engine.update().unwrap();
        assert_eq!(opacity_of(engine, target), expected, "after {what}");
    };
    let mouse = MOUSE_POINTER_ID;
    let (inside, outside) = ((10.0, 10.0), (150.0, 150.0));

    // Hover enter and leave, through the routing.
    engine
        .pointer_event(mouse, PointerEventKind::Move, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "moving in", 0.5);
    engine
        .pointer_event(mouse, PointerEventKind::Move, outside.0, outside.1)
        .unwrap();
    step(&mut engine, "moving out", 1.0);

    // Press and release over the same element: the hover path does not
    // change, so only `:active` restyles.
    engine
        .pointer_event(mouse, PointerEventKind::Move, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "moving in again", 0.5);
    engine
        .pointer_event(mouse, PointerEventKind::Down, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "pressing", 0.25);
    engine
        .pointer_event(mouse, PointerEventKind::Up, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "releasing", 0.5);

    // Cancel ends `:active` and forgets the hover path.
    engine
        .pointer_event(mouse, PointerEventKind::Down, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "pressing again", 0.25);
    engine
        .pointer_event(mouse, PointerEventKind::Cancel, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "cancelling", 1.0);

    // A press captured to the element and released outside it: the
    // release ends `:active` while capture keeps `:hover`, and the rehover
    // after capture ends leaves `:hover`.
    engine
        .pointer_event(mouse, PointerEventKind::Move, inside.0, inside.1)
        .unwrap();
    engine
        .pointer_event(mouse, PointerEventKind::Down, inside.0, inside.1)
        .unwrap();
    engine.set_pointer_capture(mouse, target).unwrap();
    engine
        .pointer_event_with_buttons(mouse, PointerEventKind::Move, outside.0, outside.1, 1)
        .unwrap();
    step(&mut engine, "moving out while captured", 0.25);
    engine
        .pointer_event(mouse, PointerEventKind::Up, outside.0, outside.1)
        .unwrap();
    step(&mut engine, "releasing outside while captured", 0.5);
    let _ = engine.end_pointer_capture(mouse);
    engine.pointer_rehover(mouse, outside.0, outside.1).unwrap();
    step(&mut engine, "rehovering after capture ends", 1.0);

    // The pointer leaves the window.
    engine
        .pointer_event(mouse, PointerEventKind::Move, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "moving in before leaving", 0.5);
    let _ = engine.pointer_leave(mouse);
    step(&mut engine, "leaving the window", 1.0);

    // A forgotten pointer loses its hover path and its press.
    engine
        .pointer_event(mouse, PointerEventKind::Move, inside.0, inside.1)
        .unwrap();
    engine
        .pointer_event(mouse, PointerEventKind::Down, inside.0, inside.1)
        .unwrap();
    step(&mut engine, "pressing before forgetting", 0.25);
    engine.forget_pointer(mouse);
    step(&mut engine, "forgetting the pointer", 1.0);
}

/// A key press after pointer focus turns on `:focus-visible`, as Blink's
/// heuristic does, and restyles.
#[test]
fn keyboard_input_restyles_focus_visible() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("button:focus-visible", opacity(0.5)));
    let mut engine = engine();
    let root = engine.root();
    let button = element(&mut engine, root, ElementTag::Button);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine
        .focus_with_origin(button, FocusOrigin::Pointer)
        .unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, button), 1.0, "after pointer focus");
    engine.note_keyboard_event();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, button), 0.5, "after a key press");
}

/// `#grand > div > target`: the grandparent sits above the former parent,
/// which the child-change restyle does not reach, so only the state hook
/// restyles it.
fn grandparent_page(sheet: &StyleSheet, tag: ElementTag) -> (Engine, NodeHandle, NodeHandle) {
    let mut engine = engine();
    let root = engine.root();
    let grand = element(&mut engine, root, ElementTag::Div);
    engine.set_attribute(grand, "id", "grand").unwrap();
    let parent = element(&mut engine, grand, ElementTag::Div);
    let target = element(&mut engine, parent, tag);
    for property in [StyleProperty::Width, StyleProperty::Height] {
        let value = parse_literal(property, "100px").unwrap();
        engine.set_property(target, property, value).unwrap();
    }
    engine.adopt_style_sheet(sheet).unwrap();
    engine.update().unwrap();
    (engine, grand, target)
}

/// Removing the focused node ends `:focus-within` on every ancestor.
#[test]
fn removing_the_focused_node_restyles_its_ancestors() {
    let sheet =
        StyleSheet::new(CascadeOrigin::Author).with_rule(rule("#grand:focus-within", opacity(0.5)));
    let (mut engine, grand, button) = grandparent_page(&sheet, ElementTag::Button);
    engine.focus(button).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 0.5, "after focusing");
    engine.detach(button).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 1.0, "after removing it");
}

/// Removing the pressed node hands `:active` to its former parent, so its
/// ancestors keep it until the release, as pinned Chromium 147 does
/// (`tests/fidelity/fixtures/active-chain-removal-v1`). The removed node
/// itself loses it, and inserting it again does not give it back.
#[test]
fn removing_the_pressed_node_keeps_its_ancestors_active_until_the_release() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("#grand:active", opacity(0.25)))
        .with_rule(rule("#pressed:active", opacity(0.5)));
    let (mut engine, grand, pressed) = grandparent_page(&sheet, ElementTag::Div);
    engine.set_attribute(pressed, "id", "pressed").unwrap();
    engine.update().unwrap();
    engine
        .pointer_event(MOUSE_POINTER_ID, PointerEventKind::Down, 10.0, 10.0)
        .unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 0.25, "after pressing");
    assert_eq!(opacity_of(&engine, pressed), 0.5, "the pressed node");
    engine.detach(pressed).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 0.25, "after removing it");
    let root = engine.root();
    engine.append_child(root, pressed).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, pressed), 1.0, "reinserted");
    engine
        .pointer_event(MOUSE_POINTER_ID, PointerEventKind::Up, 10.0, 10.0)
        .unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 1.0, "after the release");
}

/// Blurring ends `:focus-within` on every ancestor.
#[test]
fn blurring_restyles_the_ancestors() {
    let sheet =
        StyleSheet::new(CascadeOrigin::Author).with_rule(rule("#grand:focus-within", opacity(0.5)));
    let (mut engine, grand, button) = grandparent_page(&sheet, ElementTag::Button);
    engine.focus(button).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 0.5, "after focusing");
    engine.blur(button).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, grand), 1.0, "after blurring");
}

// Restyle scope (0.1 PR 4): which elements a change resolves again, and
// which invalidation classes it marks.

fn sized(engine: &mut Engine, parent: NodeHandle, width: &str, height: &str) -> NodeHandle {
    let element = element(engine, parent, ElementTag::Div);
    for (property, value) in [
        (StyleProperty::Width, width),
        (StyleProperty::Height, height),
    ] {
        let value = parse_literal(property, value).unwrap();
        engine.set_property(element, property, value).unwrap();
    }
    element
}

/// `.list`, holding three 200x30 `.item`s stacked from y 0, each holding a
/// `.label` span.
struct List {
    list: NodeHandle,
    items: Vec<NodeHandle>,
    labels: Vec<NodeHandle>,
}

impl List {
    fn new(engine: &mut Engine) -> Self {
        let root = engine.root();
        let list = element(engine, root, ElementTag::Div);
        engine.set_attribute(list, "class", "list").unwrap();
        let (mut items, mut labels) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            let item = sized(engine, list, "200px", "30px");
            engine.set_attribute(item, "class", "item").unwrap();
            let label = element(engine, item, ElementTag::Span);
            engine.set_attribute(label, "class", "label").unwrap();
            items.push(item);
            labels.push(label);
        }
        Self {
            list,
            items,
            labels,
        }
    }

    fn all(&self) -> Vec<NodeHandle> {
        let mut all = vec![self.list];
        all.extend(&self.items);
        all.extend(&self.labels);
        all
    }

    /// The y coordinate inside item `index`.
    fn y(index: usize) -> f32 {
        index as f32 * 30.0 + 10.0
    }
}

fn set(handles: Vec<NodeHandle>) -> HashSet<NodeHandle> {
    handles.into_iter().collect()
}

fn hover(engine: &mut Engine, y: f32) {
    engine
        .pointer_event(MOUSE_POINTER_ID, PointerEventKind::Move, 10.0, y)
        .unwrap();
}

/// Layout's generations: the tree, intrinsic sizes and layout.
fn layout_generations(engine: &Engine) -> (u64, u64, u64) {
    let generations = engine.dirty_generations();
    (generations.tree, generations.intrinsic, generations.layout)
}

/// A subject `:hover` restyles the elements whose hover state changed. An
/// element whose own style does not change keeps its descendants; one whose
/// style changes takes them along. A color change repaints without layout.
#[test]
fn subject_hover_restyles_the_hover_path_and_repaints_only() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule(".item:hover", Style::default().color(Color::RED)));
    let mut engine = engine();
    let list = List::new(&mut engine);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();

    hover(&mut engine, List::y(1));
    let (before, paint) = (
        layout_generations(&engine),
        engine.dirty_generations().paint,
    );
    engine.update().unwrap();
    // The hover path is item 1 and the list; the list matches nothing new.
    assert_eq!(
        set(engine.take_selector_restyles()),
        set(vec![list.list, list.items[1], list.labels[1]])
    );
    assert_eq!(
        *engine.computed_style(list.labels[1]).unwrap().color(),
        Color::RED
    );
    assert_eq!(layout_generations(&engine), before);
    assert!(engine.dirty_generations().paint > paint);

    // Moving to item 2 restyles items 1 and 2; the list stays hovered.
    hover(&mut engine, List::y(2));
    engine.update().unwrap();
    assert_eq!(
        set(engine.take_selector_restyles()),
        set(vec![
            list.items[1],
            list.labels[1],
            list.items[2],
            list.labels[2]
        ])
    );
}

/// A property layout reads marks layout.
#[test]
fn a_hover_padding_change_marks_layout() {
    let sheet = StyleSheet::new(CascadeOrigin::Author).with_rule(rule(
        ".item:hover",
        Style::default().padding_left(LengthValue::px(5.0)),
    ));
    let mut engine = engine();
    List::new(&mut engine);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    hover(&mut engine, List::y(0));
    let before = layout_generations(&engine);
    engine.update().unwrap();
    assert_ne!(layout_generations(&engine), before);
}

/// `:hover` in an ancestor compound restyles the hovered element's subtree.
#[test]
fn ancestor_hover_restyles_the_subtree() {
    let sheet =
        StyleSheet::new(CascadeOrigin::Author).with_rule(rule(".list:hover .label", opacity(0.5)));
    let mut engine = engine();
    let list = List::new(&mut engine);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();
    hover(&mut engine, List::y(0));
    engine.update().unwrap();
    assert_eq!(set(engine.take_selector_restyles()), set(list.all()));
    for label in &list.labels {
        assert_eq!(opacity_of(&engine, *label), 0.5);
    }
}

/// An attribute or class no selector reads restyles nothing; one a subject
/// compound reads restyles the element; one an ancestor compound reads
/// restyles the subtree. Reordering classes changes no token.
#[test]
fn attribute_changes_restyle_by_where_selectors_read_them() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule(".a", opacity(0.5)))
        .with_rule(rule("[data-on] span", opacity(0.25)))
        .with_rule(rule("#x", opacity(0.75)));
    let mut engine = engine();
    let root = engine.root();
    let target = element(&mut engine, root, ElementTag::Div);
    let child = element(&mut engine, target, ElementTag::Span);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();
    let step = |engine: &mut Engine, what: &str, expected: Vec<NodeHandle>| {
        assert_eq!(
            set(engine.take_selector_restyles()),
            set(expected),
            "after {what}"
        );
    };

    engine.set_attribute(target, "class", "b").unwrap();
    step(&mut engine, "an unread class", vec![]);
    engine.set_attribute(target, "data-x", "1").unwrap();
    step(&mut engine, "an unread attribute", vec![]);
    engine.set_attribute(target, "id", "y").unwrap();
    step(&mut engine, "an unread id", vec![]);
    engine.set_attribute(target, "class", "b a").unwrap();
    step(&mut engine, "a subject class", vec![target, child]);
    assert_eq!(opacity_of(&engine, target), 0.5);
    engine.set_attribute(target, "class", "a b").unwrap();
    step(&mut engine, "reordered classes", vec![]);
    engine.set_attribute(target, "data-on", "").unwrap();
    step(&mut engine, "an ancestor attribute", vec![target, child]);
    assert_eq!(opacity_of(&engine, child), 0.25);
    engine.set_attribute(target, "id", "x").unwrap();
    step(&mut engine, "a subject id", vec![target, child]);
    assert_eq!(opacity_of(&engine, target), 0.75);
    engine.remove_attribute(target, "data-x").unwrap();
    step(&mut engine, "removing an unread attribute", vec![]);
}

/// A class a sibling compound reads restyles the parent's subtree.
#[test]
fn a_sibling_feature_restyles_the_parent_subtree() {
    let sheet = StyleSheet::new(CascadeOrigin::Author).with_rule(rule(".on + .item", opacity(0.5)));
    let mut engine = engine();
    let list = List::new(&mut engine);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();
    engine
        .set_attribute(list.items[0], "class", "item on")
        .unwrap();
    assert_eq!(set(engine.take_selector_restyles()), set(list.all()));
    assert_eq!(opacity_of(&engine, list.items[1]), 0.5);
}

/// A class `:has()` reads restyles the document.
#[test]
fn a_relational_feature_restyles_the_document() {
    let sheet =
        StyleSheet::new(CascadeOrigin::Author).with_rule(rule(".list:has(.flag)", opacity(0.5)));
    let mut engine = engine();
    let list = List::new(&mut engine);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();
    engine
        .set_attribute(list.labels[2], "class", "label flag")
        .unwrap();
    let restyled = set(engine.take_selector_restyles());
    assert!(restyled.is_superset(&set(list.all())));
    assert_eq!(opacity_of(&engine, list.list), 0.5);
}

/// An attribute the engine writes itself, here the focus marker, restyles
/// at the next `update()` the rules that read it.
#[test]
fn engine_written_attributes_restyle_at_the_next_update() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("[data-divui-focused]", opacity(0.5)));
    let mut engine = engine();
    let root = engine.root();
    let input = element(&mut engine, root, ElementTag::Input);
    engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    engine.record_selector_restyles();
    engine.focus(input).unwrap();
    assert_eq!(opacity_of(&engine, input), 1.0, "before update()");
    engine.update().unwrap();
    assert!(engine.take_selector_restyles().contains(&input));
    assert_eq!(opacity_of(&engine, input), 0.5);
    engine.blur(input).unwrap();
    engine.update().unwrap();
    assert_eq!(opacity_of(&engine, input), 1.0);
}

/// Pointer and focus state restyles, scoped by the index, equal a full
/// re-cascade after every step.
#[test]
fn scoped_state_restyles_equal_a_full_cascade() {
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule(".item:hover", Style::default().color(Color::RED)))
        .with_rule(rule(".list:hover .label", opacity(0.9)))
        .with_rule(rule(".item:hover + .item", opacity(0.8)))
        .with_rule(rule(".item:hover ~ .item .label", opacity(0.7)))
        .with_rule(rule(".list:has(.item:active)", opacity(0.6)))
        .with_rule(rule(":is(.item, .label):active", opacity(0.5)))
        .with_rule(rule(".item:nth-child(2 of :hover)", opacity(0.4)))
        .with_rule(rule(".list:focus-within .label", opacity(0.3)))
        .with_rule(rule(".item:not(:hover) .label", opacity(0.2)));
    let mut engine = engine();
    let list = List::new(&mut engine);
    let input = element(&mut engine, list.items[2], ElementTag::Input);
    let mut id = engine.adopt_style_sheet(&sheet).unwrap();
    engine.update().unwrap();
    let mut elements = list.all();
    elements.push(input);

    let mouse = MOUSE_POINTER_ID;
    let steps: Vec<Box<dyn Fn(&mut Engine)>> = vec![
        Box::new(|e| hover(e, List::y(0))),
        Box::new(|e| hover(e, List::y(1))),
        Box::new(move |e| {
            e.pointer_event(mouse, PointerEventKind::Down, 10.0, List::y(1))
                .unwrap();
        }),
        Box::new(|e| hover(e, List::y(2))),
        Box::new(move |e| {
            e.pointer_event(mouse, PointerEventKind::Up, 10.0, List::y(2))
                .unwrap();
        }),
        Box::new(move |e| e.focus(input).unwrap()),
        Box::new(|e| hover(e, 150.0)),
        Box::new(move |e| e.blur(input).unwrap()),
        Box::new(move |e| {
            e.pointer_leave(mouse);
        }),
    ];
    for (step, mutate) in steps.iter().enumerate() {
        mutate(&mut engine);
        engine.update().unwrap();
        let incremental = snapshot(&engine, &elements);
        full_cascade(&mut engine, &sheet, &mut id);
        assert_eq!(incremental, snapshot(&engine, &elements), "step {step}");
    }
}

#[test]
fn type_rules_select_an_element_by_its_created_name() {
    let mut engine = engine();
    let root = engine.root();
    let heading = engine
        .create_native_element_named(ElementTag::Div, "h1")
        .unwrap();
    engine.append_child(root, heading).unwrap();
    let plain = element(&mut engine, root, ElementTag::Div);
    let sheet = StyleSheet::new(CascadeOrigin::Author)
        .with_rule(rule("div", opacity(0.4)))
        .with_rule(rule("h1", opacity(0.7)));
    engine.adopt_style_sheet(&sheet).unwrap();
    // `h1` is a `Div` for layout, but only the `h1` rule's bucket holds it.
    assert_eq!(opacity_of(&engine, heading), 0.7);
    assert_eq!(opacity_of(&engine, plain), 0.4);
    assert_eq!(engine.local_name(heading).unwrap(), Some("h1"));
    assert_eq!(engine.local_name(plain).unwrap(), Some("div"));
}
