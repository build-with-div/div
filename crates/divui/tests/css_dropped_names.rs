//! The twelve CSS names Div stopped accepting because pinned Chromium rejects
//! them. CSS text cannot reach them; their typed constants, setters and
//! animations are unchanged.
//!
//! The oracle comes from
//! `tools/qualification/capture_css_dropped_names_oracle.py`.

use div::prelude::*;
use div::typed_style::{parse_literal, AnimationOptions, Easing, FillMode, Keyframes};
use div_style::{
    parse_style_attribute, value_from_computed, INTERNAL_CSS_NAMES, PROPERTY_METADATA,
};
use serde_json::Value;
use std::collections::BTreeSet;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/css-dropped-names-v1/chromium-windows.json");

const DROPPED: [&str; 12] = [
    "block-ellipsis",
    "filter-blur",
    "filter-grayscale",
    "font-synthesis-position",
    "hanging-punctuation",
    "ruby-overhang",
    "scroll-snap-align-block",
    "scroll-snap-align-inline",
    "scroll-snap-axis",
    "scroll-snap-strictness",
    "scrollbar-thumb-color",
    "scrollbar-track-color",
];

const ROUTES: [&str; 4] = ["supports", "set_property", "attribute", "sheet"];
const LONG: &str = "Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima";
const AXIS_Y: &str = "scroll-snap-axis: y; scroll-snap-strictness: mandatory";
const AXIS_X: &str = "scroll-snap-axis: x; scroll-snap-strictness: mandatory";
const START: &str = "scroll-snap-align: start";

#[derive(Clone, Copy)]
enum Scene {
    Text,
    Colour,
    Bars,
    SnapY,
    SnapX,
}

fn property(name: &str) -> StyleProperty {
    PROPERTY_METADATA
        .iter()
        .find(|row| row.css_name == name)
        .unwrap_or_else(|| panic!("no schema row for {name}"))
        .property
}

/// Set declarations through the typed API, which keeps every schema row.
fn set(element: &Element, declarations: &str) {
    for declaration in declarations.split(';').filter(|d| !d.trim().is_empty()) {
        let (name, value) = declaration.split_once(':').unwrap();
        let property = property(name.trim());
        let value = parse_literal(property, value.trim())
            .unwrap_or_else(|error| panic!("{declaration}: {error}"));
        element.set_property(property, value).unwrap();
    }
}

fn child(document: &Document, parent: &Element, declarations: &str) -> Element {
    let element = Element::create(document, "div").unwrap();
    set(&element, declarations);
    parent.append_child(&element).unwrap();
    element
}

fn new_document() -> Document {
    let document = Document::with_font_collection(
        ViewportMetrics::from_logical_size(320.0, 240.0, 1.0).unwrap(),
        FontCollection::deterministic_test(),
    )
    .unwrap();
    set(&document.body(), "margin: 0; background-color: white");
    document
}

/// The scene's pixels. `attribute` is a `style` attribute on the target and,
/// in the snap scenes, on each snap area.
fn frame(scene: Scene, outer: &str, item: &str, attribute: Option<&str>) -> Vec<u8> {
    let document = new_document();
    let body = document.body();
    let mark = |element: &Element| {
        if let Some(text) = attribute {
            element.set_attribute("style", text).unwrap();
        }
    };
    match scene {
        Scene::Text => {
            let target = child(&document, &body, "width: 110px; font-size: 16px");
            set(&target, outer);
            mark(&target);
            target.set_text(LONG).unwrap();
        }
        Scene::Colour => {
            let target = child(
                &document,
                &body,
                "width: 100px; height: 60px; margin-left: 20px; margin-top: 20px; \
                 background-color: #c82828",
            );
            set(&target, outer);
            mark(&target);
        }
        Scene::Bars => {
            let target = child(
                &document,
                &body,
                "width: 120px; height: 120px; overflow: scroll",
            );
            set(&target, outer);
            mark(&target);
            child(
                &document,
                &target,
                "width: 400px; height: 400px; background-color: #e0e0e0",
            );
        }
        Scene::SnapY | Scene::SnapX => {
            let horizontal = matches!(scene, Scene::SnapX);
            let layout = if horizontal { "display: flex; " } else { "" };
            let target = child(
                &document,
                &body,
                &format!("{layout}width: 100px; height: 100px; overflow: scroll"),
            );
            set(&target, outer);
            mark(&target);
            let size = if horizontal {
                "flex-shrink: 0; width: 70px; height: 70px"
            } else {
                "height: 70px"
            };
            for index in 0..6 {
                let colour = if index % 2 == 0 { "#c82828" } else { "#2828c8" };
                let area = child(
                    &document,
                    &target,
                    &format!("{size}; background-color: {colour}"),
                );
                set(&area, item);
                mark(&area);
            }
            let (x, y) = if horizontal { (33.0, 0.0) } else { (0.0, 33.0) };
            target.scroll_to(x, y).unwrap();
        }
    }
    document.render_to_bitmap().unwrap().pixels().to_vec()
}

fn differing_pixels(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

fn oracle() -> Value {
    serde_json::from_str(ORACLE).unwrap()
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap()
}

#[test]
fn chromium_rejects_each_name_and_div_drops_it() {
    let oracle = oracle();
    for row in oracle["controls"].as_array().unwrap() {
        for route in ROUTES {
            assert_eq!(row[route], true, "control {} {route}", text(row, "name"));
        }
    }
    let rows = oracle["dropped"].as_array().unwrap();
    assert_eq!(rows.len(), 42);
    let names: BTreeSet<&str> = rows.iter().map(|row| text(row, "name")).collect();
    assert_eq!(names, DROPPED.into_iter().collect());
    for row in rows {
        let (name, value) = (text(row, "name"), text(row, "value"));
        for route in ROUTES {
            assert_eq!(row[route], false, "Chromium {route} {name}: {value}");
        }
        assert!(INTERNAL_CSS_NAMES.contains(&name), "{name}");
        assert_eq!(StyleProperty::from_css_name(name), None, "{name}");
        let attribute = parse_style_attribute(&format!("{name}: {value}"));
        assert!(attribute.declarations.is_empty(), "{name}: {value}");
        assert_eq!(attribute.dropped.len(), 1, "{name}: {value}");
    }
}

#[test]
fn a_dropped_name_in_a_style_attribute_changes_no_pixel() {
    // One value per name, in the scene where a typed setter shows its effect.
    // `font-synthesis-position`, `hanging-punctuation` and `ruby-overhang`
    // change no pixel even through their typed setters.
    let cases: [(&str, &str, Scene, &str, &str); 12] = [
        ("block-ellipsis", "\"x\"", Scene::Text, "line-clamp: 2", ""),
        ("filter-blur", "2", Scene::Colour, "", ""),
        ("filter-grayscale", "1", Scene::Colour, "", ""),
        ("font-synthesis-position", "none", Scene::Text, "", ""),
        ("hanging-punctuation", "first", Scene::Text, "", ""),
        ("ruby-overhang", "none", Scene::Text, "", ""),
        ("scroll-snap-align-block", "start", Scene::SnapY, AXIS_Y, ""),
        (
            "scroll-snap-align-inline",
            "start",
            Scene::SnapX,
            AXIS_X,
            "",
        ),
        (
            "scroll-snap-axis",
            "y",
            Scene::SnapY,
            "scroll-snap-strictness: mandatory",
            START,
        ),
        (
            "scroll-snap-strictness",
            "proximity",
            Scene::SnapY,
            "scroll-snap-axis: y",
            START,
        ),
        ("scrollbar-thumb-color", "red", Scene::Bars, "", ""),
        ("scrollbar-track-color", "red", Scene::Bars, "", ""),
    ];
    let oracle = oracle();
    let rows = oracle["dropped"].as_array().unwrap();
    for (name, value, scene, outer, item) in cases {
        assert!(
            rows.iter()
                .any(|row| text(row, "name") == name && text(row, "value") == value),
            "{name}: {value} is not in the oracle"
        );
        let without = frame(scene, outer, item, None);
        let with = frame(scene, outer, item, Some(&format!("{name}: {value}")));
        assert_eq!(
            differing_pixels(&without, &with),
            0,
            "style=\"{name}: {value}\""
        );
    }
}

#[test]
fn typed_setters_still_change_the_frame() {
    // (name, scene, without: outer and item, with: outer and item)
    let cases: [(&str, Scene, &str, &str, &str, &str); 8] = [
        (
            "block-ellipsis",
            Scene::Text,
            "line-clamp: 2",
            "",
            "line-clamp: 2; block-ellipsis: \"x\"",
            "",
        ),
        ("filter-blur", Scene::Colour, "", "", "filter-blur: 2", ""),
        (
            "filter-grayscale",
            Scene::Colour,
            "",
            "",
            "filter-grayscale: 1",
            "",
        ),
        (
            "scrollbar-track-color",
            Scene::Bars,
            "",
            "",
            "scrollbar-track-color: red",
            "",
        ),
        (
            "scrollbar-thumb-color",
            Scene::Bars,
            "",
            "",
            "scrollbar-thumb-color: red",
            "",
        ),
        (
            "scroll-snap-axis",
            Scene::SnapY,
            "scroll-snap-strictness: mandatory",
            START,
            AXIS_Y,
            START,
        ),
        (
            "scroll-snap-align-block",
            Scene::SnapY,
            AXIS_Y,
            "",
            AXIS_Y,
            "scroll-snap-align-block: start",
        ),
        (
            "scroll-snap-align-inline",
            Scene::SnapX,
            AXIS_X,
            "",
            AXIS_X,
            "scroll-snap-align-inline: start",
        ),
    ];
    for (name, scene, outer, item, typed_outer, typed_item) in cases {
        let without = frame(scene, outer, item, None);
        let with = frame(scene, typed_outer, typed_item, None);
        assert!(differing_pixels(&without, &with) > 0, "{name}");
    }
}

fn linear() -> AnimationOptions {
    AnimationOptions {
        duration_ms: 1000.0,
        fill: FillMode::Both,
        easing: Easing::Linear,
        ..AnimationOptions::default()
    }
}

#[test]
fn the_animation_api_still_reaches_every_typed_property() {
    let cases: [(&str, &str, &str); 12] = [
        ("block-ellipsis", "no-ellipsis", "\"x\""),
        ("filter-blur", "0", "4"),
        ("filter-grayscale", "0", "1"),
        ("font-synthesis-position", "auto", "none"),
        ("hanging-punctuation", "none", "first"),
        ("ruby-overhang", "auto", "none"),
        ("scroll-snap-align-block", "none", "start"),
        ("scroll-snap-align-inline", "none", "start"),
        ("scroll-snap-axis", "x", "y"),
        ("scroll-snap-strictness", "proximity", "mandatory"),
        ("scrollbar-thumb-color", "red", "blue"),
        ("scrollbar-track-color", "red", "blue"),
    ];
    assert_eq!(
        cases
            .map(|case| case.0)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        DROPPED.into_iter().collect()
    );
    for (name, from, to) in cases {
        let property = property(name);
        let parse = |literal| parse_literal(property, literal).unwrap();
        let document = new_document();
        // What the typed setter stores, to compare the animated value with.
        let expected = |literal| {
            let probe = child(&document, &document.body(), "");
            probe.set_property(property, parse(literal)).unwrap();
            value_from_computed(&probe.computed_style().unwrap(), property)
        };
        let (start, end) = (expected(from), expected(to));
        assert_ne!(start, end, "{name}");
        let node = child(&document, &document.body(), "");
        node.animate(
            property,
            Keyframes::from_values(parse(from), parse(to)),
            linear(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        document.advance_time(0.0).unwrap();
        let at_start = value_from_computed(&node.computed_style().unwrap(), property);
        assert_eq!(at_start, start, "{name} at 0 ms");
        document.advance_time(1000.0).unwrap();
        let at_end = value_from_computed(&node.computed_style().unwrap(), property);
        assert_eq!(at_end, end, "{name} at 1000 ms");
    }
}

#[test]
fn filter_parts_interpolate_through_the_animation_api() {
    for (name, to, half) in [
        ("filter-blur", 4.0_f32, 2.0),
        ("filter-grayscale", 1.0, 0.5),
    ] {
        let document = new_document();
        let node = child(&document, &document.body(), "");
        node.animate(
            property(name),
            Keyframes::from_values(0.0_f32, to),
            linear(),
        )
        .unwrap();
        document.advance_time(500.0).unwrap();
        let computed = node.computed_style().unwrap();
        let value = if name == "filter-blur" {
            computed.filter_blur
        } else {
            computed.filter_grayscale
        };
        assert_eq!(value, half, "{name}");
    }
}
