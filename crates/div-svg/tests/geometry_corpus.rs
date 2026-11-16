//! `div-svg` against Chromium 147's answers for the geometry corpus
//! (`tools/svg/geometry_corpus.py`, captured on Linux; identical on Windows).
//!
//! Every value is compared exactly. A field of a case that differs must be
//! listed in `known-gaps.txt` next to the oracle, as `<case> <field>`, under
//! a comment giving the cause; a gap is never a tolerance, and the other
//! fields of that case stay exact. The test fails both on a new difference
//! and on a listed gap that no longer differs, so the list stays true.

use div_svg::geometry::{
    contains_in_fill, contains_in_stroke, path_from_data, point_at_length, total_length, FillRule,
    LineCap, LineJoin, StrokeStyle,
};
use div_svg::length::{Axis, Length, LengthContext};
use div_svg::shapes::Shape;
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::viewport::{parse_view_box, view_box_transform, PreserveAspectRatio};
use div_svg::{shapes, PathData, Point};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-geometry-v1/chromium-linux.json");
const KNOWN_GAPS: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-geometry-v1/known-gaps.txt");

struct Element {
    tag: String,
    attrs: BTreeMap<String, String>,
}

/// The start tags of the corpus's generated markup, in document order.
fn elements(markup: &str) -> Vec<Element> {
    let mut out = Vec::new();
    for chunk in markup.split('<').skip(1) {
        if chunk.starts_with('/') {
            continue;
        }
        let body = chunk.split('>').next().unwrap().trim_end_matches('/');
        let tag: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let mut attrs = BTreeMap::new();
        let mut rest = &body[tag.len()..];
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            let name_end = rest
                .find(|c: char| c == '=' || c.is_whitespace())
                .unwrap_or(rest.len());
            let name = rest[..name_end].to_owned();
            rest = &rest[name_end..];
            if let Some(after) = rest.strip_prefix("=\"") {
                let end = after.find('"').unwrap();
                attrs.insert(name, after[..end].to_owned());
                rest = &after[end + 1..];
            } else {
                attrs.insert(name, String::new());
            }
        }
        out.push(Element { tag, attrs });
    }
    out
}

fn style_map(style: &str) -> BTreeMap<String, String> {
    style
        .split(';')
        .filter_map(|decl| decl.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

struct Viewport {
    ctm: Matrix,
    width: f32,
    height: f32,
}

/// The coordinate system an `svg` element establishes.
fn viewport(svg: &Element, parent: Option<&Viewport>) -> Viewport {
    let context = parent.map(|p| (p.width, p.height)).unwrap_or((0.0, 0.0));
    let len = |name: &str, axis: Axis, default: f32| {
        svg.attrs
            .get(name)
            .and_then(|v| Length::parse(v))
            .map(|l| l.resolve(axis, &length_context(context.0, context.1)))
            .unwrap_or(default)
    };
    let (default_w, default_h) = if parent.is_some() {
        (context.0, context.1)
    } else {
        (300.0, 150.0)
    };
    let width = len("width", Axis::Horizontal, default_w);
    let height = len("height", Axis::Vertical, default_h);
    let x = len("x", Axis::Horizontal, 0.0);
    let y = len("y", Axis::Vertical, 0.0);
    let par = svg
        .attrs
        .get("preserveAspectRatio")
        .and_then(|v| PreserveAspectRatio::parse(v))
        .unwrap_or_default();
    let view_box = svg.attrs.get("viewBox").and_then(|v| parse_view_box(v));
    let mut ctm = parent.map(|p| p.ctm).unwrap_or(Matrix::IDENTITY);
    if parent.is_some() {
        ctm = ctm.then_apply(&Matrix::translate(f64::from(x), f64::from(y)));
    }
    let (width_inside, height_inside) = match view_box {
        Some(vb) => {
            ctm = ctm.then_apply(&view_box_transform(vb, par, width, height));
            (vb.width, vb.height)
        }
        None => (width, height),
    };
    Viewport {
        ctm,
        width: width_inside,
        height: height_inside,
    }
}

fn length_context(width: f32, height: f32) -> LengthContext {
    LengthContext {
        viewport_width: width,
        viewport_height: height,
        font_size: 16.0,
        root_font_size: 16.0,
        x_height: 8.0,
    }
}

/// A probe element's shape.
fn shape(element: &Element, vp: &Viewport) -> Shape {
    let ctx = length_context(vp.width, vp.height);
    // An attribute that is present but does not parse as a length is used
    // as 0, while an absent `rx`/`ry` is `auto`: the corpus's `rect/rx-auto`
    // (`rx="auto" ry="15"`) has square corners in Chromium 147.
    let num = |name: &str, axis: Axis| {
        element
            .attrs
            .get(name)
            .map(|v| Length::parse(v).map_or(0.0, |l| l.resolve(axis, &ctx)))
    };
    let get = |name: &str, axis: Axis| num(name, axis).unwrap_or(0.0);
    match element.tag.as_str() {
        "path" => Shape::from_path(path_from_data(&PathData::parse(
            element.attrs.get("d").map_or("", |s| s),
        ))),
        "rect" => shapes::rect(
            get("x", Axis::Horizontal),
            get("y", Axis::Vertical),
            get("width", Axis::Horizontal),
            get("height", Axis::Vertical),
            num("rx", Axis::Horizontal),
            num("ry", Axis::Vertical),
        ),
        "circle" => shapes::circle(
            get("cx", Axis::Horizontal),
            get("cy", Axis::Vertical),
            get("r", Axis::Other),
        ),
        "ellipse" => shapes::ellipse(
            get("cx", Axis::Horizontal),
            get("cy", Axis::Vertical),
            get("rx", Axis::Horizontal),
            get("ry", Axis::Vertical),
        ),
        "line" => shapes::line(
            get("x1", Axis::Horizontal),
            get("y1", Axis::Vertical),
            get("x2", Axis::Horizontal),
            get("y2", Axis::Vertical),
        ),
        "polyline" | "polygon" => shapes::poly(
            element.attrs.get("points").map_or("", |s| s),
            element.tag == "polygon",
        ),
        other => panic!("unexpected probe <{other}>"),
    }
}

fn stroke_style(style: &BTreeMap<String, String>) -> StrokeStyle {
    let mut out = StrokeStyle::default();
    if let Some(w) = style.get("stroke-width") {
        out.width = w.trim_end_matches("px").parse().unwrap();
    }
    out.cap = match style.get("stroke-linecap").map(String::as_str) {
        Some("round") => LineCap::Round,
        Some("square") => LineCap::Square,
        _ => LineCap::Butt,
    };
    out.join = match style.get("stroke-linejoin").map(String::as_str) {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        _ => LineJoin::Miter,
    };
    if let Some(m) = style.get("stroke-miterlimit") {
        out.miter_limit = m.parse().unwrap();
    }
    if let Some(d) = style.get("stroke-dasharray") {
        out.dash_array = d
            .split([' ', ','])
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
    }
    if style.get("stroke").map(String::as_str) == Some("none") {
        out.width = 0.0;
    }
    out
}

fn f(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

fn floats(value: &Value) -> Vec<f64> {
    value.as_array().unwrap().iter().map(f).collect()
}

/// Compares one case; returns each differing field with its first
/// difference.
fn compare(case: &Value) -> Vec<(&'static str, String)> {
    let markup = case["svg"].as_str().unwrap();
    let chromium = &case["chromium"];
    let elems = elements(markup);
    let outer = viewport(&elems[0], None);
    // In `viewbox/nested` the probe is the inner `svg` and Chromium answers
    // for the `rect` inside it, which inherits the probe's style.
    let (vp, element, style) = if let Some(inner) = elems
        .iter()
        .position(|e| e.attrs.contains_key("data-inner"))
    {
        let probe = &elems[inner - 1];
        let mut style = style_map(probe.attrs.get("style").map_or("", |s| s));
        style.extend(style_map(elems[inner].attrs.get("style").map_or("", |s| s)));
        (viewport(probe, Some(&outer)), &elems[inner], style)
    } else {
        let probe = elems
            .iter()
            .find(|e| e.attrs.contains_key("data-probe"))
            .unwrap();
        let style = style_map(probe.attrs.get("style").map_or("", |s| s));
        (outer, probe, style)
    };
    let own = element
        .attrs
        .get("transform")
        .and_then(|t| parse_transform_list(t))
        .unwrap_or(Matrix::IDENTITY);
    let ctm = vp.ctm.then_apply(&own);
    let Shape { path, bbox } = shape(element, &vp);

    let mut diffs = Vec::new();
    let mut check = |field: &'static str, ours: Vec<f64>, theirs: Vec<f64>| {
        if ours != theirs {
            diffs.push((field, format!("div {ours:?} chromium {theirs:?}")));
        }
    };
    // `getCTM` is a DOMMatrix of doubles, computed in double precision.
    // (`getScreenCTM` adds the case's page slot, which is layout's answer,
    // not this crate's.)
    check(
        "ctm",
        vec![ctm.a, ctm.b, ctm.c, ctm.d, ctm.e, ctm.f],
        floats(&chromium["ctm"]),
    );
    check(
        "bbox",
        [bbox.x, bbox.y, bbox.width, bbox.height]
            .iter()
            .map(|v| f64::from(*v))
            .collect(),
        floats(&chromium["bbox"]),
    );
    let Some(theirs_length) = chromium.get("total_length") else {
        return diffs;
    };
    let length = total_length(&path);
    check(
        "total_length",
        vec![f64::from(length)],
        vec![f(theirs_length)],
    );
    match chromium["points"].as_array() {
        Some(points) => {
            for (i, p) in points.iter().enumerate() {
                // The capture computes `total * i / 32` in double; the IDL
                // argument is a float.
                let distance = (f64::from(length) * i as f64 / 32.0) as f32;
                let ours = point_at_length(&path, distance).unwrap_or_default();
                let theirs = floats(p);
                let ours = vec![f64::from(ours.x), f64::from(ours.y)];
                if ours != theirs {
                    diffs.push(("points", format!("[{i}] div {ours:?} chromium {theirs:?}")));
                    break;
                }
            }
        }
        // Chromium throws: the path is empty.
        None => check(
            "points",
            vec![f64::from(u8::from(point_at_length(&path, 0.0).is_some()))],
            vec![0.0],
        ),
    }
    let grid = floats(&chromium["grid"]);
    let rule = if style.get("fill-rule").map(String::as_str) == Some("evenodd") {
        FillRule::EvenOdd
    } else {
        FillRule::NonZero
    };
    let stroke = stroke_style(&style);
    let mut fill = String::new();
    let mut in_stroke = String::new();
    for j in 0..17 {
        for i in 0..17 {
            let p = Point::new(
                (grid[0] + grid[2] * (i as f64 + 0.5) / 17.0) as f32,
                (grid[1] + grid[3] * (j as f64 + 0.5) / 17.0) as f32,
            );
            fill.push(if contains_in_fill(&path, p, rule) {
                '1'
            } else {
                '0'
            });
            in_stroke.push(if contains_in_stroke(&path, p, &stroke) {
                '1'
            } else {
                '0'
            });
        }
    }
    for (field, ours, theirs) in [
        ("in_fill", fill, chromium["in_fill"].as_str().unwrap()),
        (
            "in_stroke",
            in_stroke,
            chromium["in_stroke"].as_str().unwrap(),
        ),
    ] {
        if ours != theirs {
            let count = ours
                .chars()
                .zip(theirs.chars())
                .filter(|(a, b)| a != b)
                .count();
            diffs.push((field, format!("{count} of 289 grid points differ")));
        }
    }
    diffs
}

#[test]
fn geometry_matches_chromium_147() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    assert_eq!(oracle["chromium"], "Chrome/147.0.7727.50");
    let known: BTreeSet<String> = KNOWN_GAPS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .collect();
    let mut report = Vec::new();
    let mut differing = BTreeSet::new();
    let mut exact = 0;
    let cases = oracle["cases"].as_array().unwrap();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let diffs = compare(case);
        exact += usize::from(diffs.is_empty());
        for (field, message) in diffs {
            let key = format!("{id} {field}");
            if !known.contains(&key) {
                report.push(format!("{key}: {message}"));
            }
            differing.insert(key);
        }
    }
    eprintln!(
        "{exact}/{} corpus cases exact; {} case fields differ",
        cases.len(),
        differing.len()
    );
    let new: Vec<_> = differing.difference(&known).collect();
    let fixed: Vec<_> = known.difference(&differing).collect();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "new gaps {new:?}; listed gaps now exact {fixed:?}\n\n{}",
        report.join("\n")
    );
}
