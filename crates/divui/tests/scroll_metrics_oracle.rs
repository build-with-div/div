//! `examples/c/scroll_metrics.c` against pinned Chromium 147 at real zoom.
//!
//! `tests/fidelity/fixtures/scroll-metrics-v1/chromium-windows.json` comes
//! from `tools/qualification/capture_scroll_metrics_oracle.py`, which launches
//! Chrome with `--force-device-scale-factor` at 1x, 1.25x, 1.5x, 2x and 3x
//! (RFC 0010). This test runs the C example's `viewport`, `nested` and
//! `integer_api` scenarios through `divui` and compares every value the web
//! can observe: integer client and scroll sizes, scroll offsets and bounding
//! rectangles. A value listed in `GAPS` must still differ from Chromium; every
//! other value must equal it.
use div::prelude::*;
use serde_json::{json, Value};

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/scroll-metrics-v1/chromium-windows.json");

/// Values Div does not yet match, by scale and path. All three come from
/// Chromium laying out in device pixels at fractional scales, which Div does
/// not do (RFC 0011, engine).
const GAPS: &[(&str, &str, &[&str])] = &[
    // Scroll offsets end on whole device pixels: a 22px range at 1.25x is
    // 27.5 device px, which ends at 28, or 22.4px.
    (
        "device-scale-scroll-offset",
        "1.25",
        &[
            "viewport/original_scrolled/0",
            "viewport/original_scrolled/1",
            "viewport/hidden_scrolled/0",
            "viewport/hidden_scrolled/1",
            "nested/scrolled/0",
            "nested/rtl_scrolled/0",
        ],
    ),
    (
        "device-scale-scroll-offset",
        "1.5",
        &[
            "viewport/original_scrolled/0",
            "viewport/original_scrolled/1",
            "viewport/hidden_scrolled/0",
        ],
    ),
    // Zoomed border widths are floored to whole device pixels
    // (`StyleBuilderConverter::ConvertBorderWidth`): 3px is 2.4px at 1.25x and
    // 2.667px at 1.5x, 2px is 1.6px at 1.25x. The nested child's x at 1.25x
    // also moves with the snapped scroll offset.
    (
        "device-scale-border-width",
        "1.25",
        &[
            "nested/child_bounds/0",
            "nested/child_bounds/1",
            "integer_api/transformed_bounds/0",
            "integer_api/transformed_bounds/1",
            "integer_api/transformed_bounds/2",
            "integer_api/transformed_bounds/3",
        ],
    ),
    (
        "device-scale-border-width",
        "1.5",
        &["nested/child_bounds/0", "nested/child_bounds/1"],
    ),
    // The classic scrollbar is round(15 x scale) device pixels: 19 at 1.25x
    // (15.2px) and 23 at 1.5x (15.333px). The 100.5px scroller's client width
    // is then 85.3px or 85.167px, which `clientWidth` reports as 85.
    (
        "device-scale-scrollbar-thickness",
        "1.25",
        &[
            "integer_api/scroll/client/0",
            "integer_api/scroll/client/1",
            "integer_api/transformed/client/0",
            "integer_api/transformed/client/1",
        ],
    ),
    (
        "device-scale-scrollbar-thickness",
        "1.5",
        &[
            "integer_api/scroll/client/0",
            "integer_api/scroll/client/1",
            "integer_api/transformed/client/0",
            "integer_api/transformed/client/1",
        ],
    ),
];

fn document(scale: f64) -> Document {
    Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(320.0, 240.0, scale).unwrap(),
    )
    .unwrap()
}

fn sized(element: &Element, width: f32, height: f32) {
    element.set_width(LengthValue::px(width)).unwrap();
    element.set_height(LengthValue::px(height)).unwrap();
}

fn border(element: &Element, width: f32) {
    element
        .set_border(Border {
            width,
            style: BorderStyle::Solid,
            color: Color::BLACK,
        })
        .unwrap();
}

/// `clientWidth`, `clientHeight`, `scrollWidth` and `scrollHeight`.
fn dims(element: &Element) -> Value {
    json!({
        "client": [element.client_width().unwrap(), element.client_height().unwrap()],
        "scroll": [element.scroll_width().unwrap(), element.scroll_height().unwrap()],
    })
}

fn offset(element: &Element) -> Value {
    json!([
        element.scroll_left().unwrap(),
        element.scroll_top().unwrap()
    ])
}

fn rect(element: &Element) -> Value {
    let rect = element.bounding_rect().unwrap().unwrap();
    json!([rect.x, rect.y, rect.width, rect.height])
}

fn viewport(scale: f64) -> Value {
    let document = document(scale);
    let root = document.body();
    let content = Element::create(&document, "div").unwrap();
    root.append_child(&content).unwrap();
    content.set_position(Position::Absolute).unwrap();
    content.set_left(Length::px(27.0)).unwrap();
    content.set_top(Length::px(22.0)).unwrap();
    sized(&content, 300.0, 224.0);
    root.set_overflow_x(Overflow::Auto).unwrap();
    root.set_overflow_y(Overflow::Auto).unwrap();
    let mut steps = serde_json::Map::new();
    steps.insert("original".into(), dims(&root));
    root.scroll_to(1000.0, 1000.0).unwrap();
    steps.insert("original_scrolled".into(), offset(&root));
    sized(&content, 80.0, 40.0);
    steps.insert("small_offset".into(), offset(&root));
    steps.insert("small".into(), dims(&root));
    root.set_overflow_x(Overflow::Scroll).unwrap();
    root.set_overflow_y(Overflow::Hidden).unwrap();
    steps.insert("asymmetric".into(), dims(&root));
    root.set_overflow_y(Overflow::Scroll).unwrap();
    steps.insert("forced".into(), dims(&root));
    root.set_overflow_x(Overflow::Hidden).unwrap();
    root.set_overflow_y(Overflow::Hidden).unwrap();
    sized(&content, 300.0, 224.0);
    steps.insert("hidden".into(), dims(&root));
    root.scroll_to(1000.0, 1000.0).unwrap();
    steps.insert("hidden_scrolled".into(), offset(&root));
    document
        .set_viewport(ViewportMetrics::from_logical_size(360.0, 260.0, scale).unwrap())
        .unwrap();
    steps.insert("resized_offset".into(), offset(&root));
    steps.insert("resized".into(), dims(&root));
    Value::Object(steps)
}

fn nested(scale: f64) -> Value {
    let document = document(scale);
    let port = Element::create(&document, "div").unwrap();
    let child = Element::create(&document, "div").unwrap();
    document.body().append_child(&port).unwrap();
    port.append_child(&child).unwrap();
    port.set_position(Position::Absolute).unwrap();
    port.set_left(Length::px(40.0)).unwrap();
    port.set_top(Length::px(40.0)).unwrap();
    sized(&port, 80.0, 80.0);
    port.set_padding(Edges::all(LengthValue::px(10.0))).unwrap();
    border(&port, 3.0);
    port.set_overflow(Overflow::Hidden).unwrap();
    port.set_scrollbar_width(ScrollbarWidth::None).unwrap();
    sized(&child, 130.0, 120.0);
    let mut steps = serde_json::Map::new();
    steps.insert("original".into(), dims(&port));
    port.scroll_to(1000.0, 1000.0).unwrap();
    steps.insert("scrolled".into(), offset(&port));
    steps.insert("child_bounds".into(), rect(&child));
    port.set_direction(Direction::Rtl).unwrap();
    port.scroll_to(-1000.0, 1000.0).unwrap();
    steps.insert("rtl_scrolled".into(), offset(&port));
    sized(&child, 30.0, 20.0);
    steps.insert("small_offset".into(), offset(&port));
    steps.insert("small".into(), dims(&port));
    sized(&port, 140.0, 140.0);
    steps.insert("resized".into(), dims(&port));
    Value::Object(steps)
}

fn integer_api(scale: f64) -> Value {
    let document = document(scale);
    let root = document.body();
    let scroller = Element::create(&document, "div").unwrap();
    let content = Element::create(&document, "div").unwrap();
    root.append_child(&scroller).unwrap();
    scroller.append_child(&content).unwrap();
    sized(&scroller, 100.5, 80.5);
    border(&scroller, 2.0);
    scroller.set_overflow(Overflow::Scroll).unwrap();
    scroller.set_scrollbar_width(ScrollbarWidth::Auto).unwrap();
    sized(&content, 200.5, 160.5);
    let inline = Element::create(&document, "span").unwrap();
    root.append_child(&inline).unwrap();
    inline.set_display(Display::InlineBlock).unwrap();
    sized(&inline, 40.5, 20.5);
    let mut steps = serde_json::Map::new();
    steps.insert("root".into(), dims(&root));
    steps.insert("scroll".into(), dims(&scroller));
    scroller
        .set_transform(TransformList(vec![TransformOperation::Scale(2.0, 3.0)]))
        .unwrap();
    steps.insert("transformed".into(), dims(&scroller));
    steps.insert("transformed_bounds".into(), rect(&scroller));
    scroller.set_overflow(Overflow::Hidden).unwrap();
    steps.insert("hidden".into(), dims(&scroller));
    steps.insert("inline_block".into(), dims(&inline));
    Value::Object(steps)
}

/// Every numeric leaf as (path, value).
fn leaves(value: &Value, path: String, out: &mut Vec<(String, f64)>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                leaves(value, format!("{path}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                leaves(value, format!("{path}/{index}"), out);
            }
        }
        Value::Number(number) => out.push((path, number.as_f64().unwrap())),
        other => panic!("{path}: unexpected {other}"),
    }
}

fn gap(scale: &str, path: &str) -> Option<&'static str> {
    GAPS.iter()
        .find(|(_, gap_scale, paths)| *gap_scale == scale && paths.contains(&path))
        .map(|(name, _, _)| *name)
}

#[test]
fn scroll_metrics_match_chromium_at_real_zoom_except_named_gaps() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "147.0.7727.50");
    assert_eq!(oracle["viewport"], json!([320, 240]));
    let scales = oracle["scales"].as_object().unwrap();
    assert_eq!(
        scales.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1.0", "1.25", "1.5", "2.0", "3.0"]
    );
    let mut failures = Vec::new();
    let mut gaps_seen = 0;
    for (key, chromium) in scales {
        let scale: f64 = key.parse().unwrap();
        let div = json!({
            "viewport": viewport(scale),
            "nested": nested(scale),
            "integer_api": integer_api(scale),
        });
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        leaves(chromium, String::new(), &mut expected);
        leaves(&div, String::new(), &mut actual);
        let paths = |leaves: &[(String, f64)]| {
            let mut paths: Vec<_> = leaves.iter().map(|(path, _)| path.clone()).collect();
            paths.sort();
            paths
        };
        assert_eq!(paths(&expected), paths(&actual), "{key}: scenario shape");
        for (path, chromium) in &expected {
            let path = &path[1..];
            let div = actual.iter().find(|(p, _)| &p[1..] == path).unwrap().1;
            // Chromium's offsets and rectangles are floats; compare at that
            // precision.
            let equal = *chromium as f32 == div as f32;
            match gap(key, path) {
                Some(name) => {
                    gaps_seen += 1;
                    if equal {
                        failures.push(format!(
                            "{key} {path}: gap {name} is fixed ({div}); remove it from GAPS"
                        ));
                    }
                }
                None if !equal => {
                    failures.push(format!("{key} {path}: Chromium {chromium}, Div {div}"))
                }
                None => {}
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let listed: usize = GAPS.iter().map(|(_, _, paths)| paths.len()).sum();
    assert_eq!(gaps_seen, listed, "every GAPS path names a captured value");
}
