//! The rule cascade: CSS Cascade 5 ordering of adopted style rules, and
//! incremental restyles that equal a full re-cascade.

use div_dom::ElementTag;
use div_engine::{AnimationTimeline, Engine, NodeHandle, StyleSheetError, ViewportMetrics};
use div_style::selector_parser::parse_selector_list;
use div_style::stylesheet::{CascadeOrigin, StyleRule, StyleSheet};
use div_style::{
    AnimationOptions, Color, FillMode, Keyframes, PropertyKeyframes, Style, StyleProperty,
};

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
