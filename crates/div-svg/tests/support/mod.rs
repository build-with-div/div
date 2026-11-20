//! Markup helpers shared by the corpus tests: a start-tag scanner for the
//! corpora's generated markup, and the shapes, viewports and stroke styles
//! it describes. These stand in for the engine's parser and cascade, which
//! the corpora do not test.

#![allow(dead_code)]

pub mod pixels;

use div_svg::geometry::{path_from_data, LineCap, LineJoin, StrokeStyle};
use div_svg::length::{Axis, Length, LengthContext};
use div_svg::shapes::Shape;
use div_svg::transform::Matrix;
use div_svg::viewport::{parse_view_box, view_box_transform, PreserveAspectRatio};
use div_svg::{shapes, PathData};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Element {
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
}

/// The start tags of the corpus's generated markup, in document order.
pub fn elements(markup: &str) -> Vec<Element> {
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

pub fn style_map(style: &str) -> BTreeMap<String, String> {
    style
        .split(';')
        .filter_map(|decl| decl.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

pub struct Viewport {
    pub ctm: Matrix,
    pub width: f32,
    pub height: f32,
}

/// The coordinate system an `svg` element establishes.
pub fn viewport(svg: &Element, parent: Option<&Viewport>) -> Viewport {
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

pub fn length_context(width: f32, height: f32) -> LengthContext {
    LengthContext {
        viewport_width: width,
        viewport_height: height,
        font_size: 16.0,
        root_font_size: 16.0,
        x_height: 8.0,
    }
}

/// A probe element's shape.
pub fn shape(element: &Element, vp: &Viewport) -> Shape {
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
    let mut out = match element.tag.as_str() {
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
    };
    out.path_length = element.attrs.get("pathLength").map(|v| v.parse().unwrap());
    out
}

pub fn stroke_style(style: &BTreeMap<String, String>) -> StrokeStyle {
    stroke_style_in(style, &length_context(0.0, 0.0))
}

/// The stroke, with `stroke-dasharray` and `stroke-dashoffset` lengths
/// resolved in `context` (percentages against the normalized diagonal).
pub fn stroke_style_in(style: &BTreeMap<String, String>, context: &LengthContext) -> StrokeStyle {
    let mut out = StrokeStyle::default();
    if let Some(w) = style.get("stroke-width") {
        // A percentage is of the normalized diagonal.
        out.width = Length::parse(w).unwrap().resolve(Axis::Other, context);
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
            .map(|s| css_length(s).resolve(Axis::Other, context))
            .collect();
    }
    if let Some(o) = style.get("stroke-dashoffset") {
        out.dash_offset = css_length(o).resolve(Axis::Other, context);
    }
    if style.get("stroke").map(String::as_str) == Some("none") {
        out.width = 0.0;
    }
    out
}

/// A length in a CSS property value. CSS reads the number in double
/// precision and stores it as a float, unlike `div-svg`'s attribute scanner,
/// which accumulates in single precision: `dash/pathlength-path`'s
/// `0.05 0.03` dashes differ by three pixels with the scanner's values.
pub fn css_length(text: &str) -> Length {
    let mut length = Length::parse(text).unwrap_or_else(|| panic!("length {text}"));
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(text.len());
    length.value = text[..end].parse::<f64>().unwrap() as f32;
    length
}
