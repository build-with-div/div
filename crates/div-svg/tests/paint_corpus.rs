//! `div_svg::paint` against Chromium 147's pixels for the paint corpus
//! (`tools/svg/paint_corpus.py`), each OS against its own capture. The
//! corpus has three sets with their own fixtures and gap lists: `v1` (fills,
//! strokes, transforms, viewports, Lucide icons), `v2` (dashes,
//! `pathLength`, `paint-order`, `shape-rendering`, `vector-effect`, chart
//! compositions) and `v3` (`clipPath`).
//!
//! Chromium renders every case of a sheet in its own slot of one page, at
//! scale 1 and, for a subset, at 2 (real zoom). This test paints
//! each case with `div-svg` the way Div's renderer rasterizes a page: on
//! white `raster_n32_premul` tiles of 256 px that step by 254
//! (`div-paint`'s `render.rs`, after cc's tiling), each case in the device
//! space of the tile that shows it, and compares it with the slot, byte for
//! byte, with the helpers in `support/pixels.rs`. A case that differs must
//! be listed in
//! `known-gaps.txt` as `<scale>x <case> <differing pixels> <largest channel
//! difference>` under a comment giving the cause. The test fails on a new
//! difference, on a listed one whose numbers change, and on a listed case
//! that becomes exact, so the list stays true.

#![cfg(any(target_os = "linux", windows))]

mod support;

use div_svg::clip::{clip_content, ClipChild, ClipContent, ClipPath, ClipPathUnits, ClipReference};
use div_svg::geometry::FillRule;
use div_svg::length::Axis;
use div_svg::paint::{
    clip_viewport, paint_shape, to_sk_matrix, PaintOrder, ShapeRendering, SvgPaintStyle,
};
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::viewport::parse_view_box;
use div_svg::Rect;
use serde_json::Value;
use skia_safe::{Canvas, M44};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use support::pixels::{self, cascade, paint_style, view_box, Root};
use support::{elements, length_context, shape, stroke_style_in, style_map, Element, Viewport};

#[cfg(target_os = "linux")]
const ORACLE: (&str, &str) = ("linux", "chromium-linux");
#[cfg(windows)]
const ORACLE: (&str, &str) = ("win32", "chromium-windows");
const KNOWN_GAPS_V1: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v1/known-gaps.txt");
const KNOWN_GAPS_V2: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v2/known-gaps.txt");
const KNOWN_GAPS_V3: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v3/known-gaps.txt");
fn fixtures(set: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/fidelity/fixtures/{set}"))
}

struct Node {
    element: Element,
    children: Vec<Node>,
}

/// The element tree of a case's generated markup.
fn tree(markup: &str) -> Node {
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    for chunk in markup.split('<').skip(1) {
        let body = chunk.split('>').next().unwrap();
        if body.starts_with('/') {
            let node = stack.pop().unwrap();
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => root = Some(node),
            }
            continue;
        }
        let element = elements(&format!("<{body}>")).pop().unwrap();
        let node = Node {
            element,
            children: Vec::new(),
        };
        if body.trim_end().ends_with('/') {
            stack.last_mut().unwrap().children.push(node);
        } else {
            stack.push(node);
        }
    }
    root.unwrap()
}

fn transform_of(element: &Element) -> Matrix {
    element
        .attrs
        .get("transform")
        .and_then(|t| parse_transform_list(t))
        .unwrap_or(Matrix::IDENTITY)
}

fn find<'a>(node: &'a Node, id: &str) -> Option<&'a Node> {
    if node.element.attrs.get("id").map(String::as_str) == Some(id) {
        return Some(node);
    }
    node.children.iter().find_map(|child| find(child, id))
}

/// The `#id` of an element's `clip-path: url(#id)`, from its `style`
/// attribute or its presentation attribute.
fn clip_reference(element: &Element) -> Option<String> {
    let style = style_map(element.attrs.get("style").map_or("", |s| s));
    let value = style
        .get("clip-path")
        .or_else(|| element.attrs.get("clip-path"))?;
    let id = value.trim().strip_prefix("url(#")?.strip_suffix(')')?;
    Some(id.to_owned())
}

/// Resolves `#id` to a `clipPath`. `None` when it names nothing, names
/// another element, or closes a cycle through `chain`, the references
/// being followed.
fn resolve_clip(root: &Node, id: &str, vp: &Viewport, chain: &mut Vec<String>) -> Option<ClipPath> {
    if chain.iter().any(|seen| seen == id) {
        return None;
    }
    let node = find(root, id)?;
    if node.element.tag != "clipPath" {
        return None;
    }
    chain.push(id.to_owned());
    let own = &node.element.attrs;
    let clip_path = clip_reference(&node.element).map(|r| reference(root, &r, vp, chain));
    let mut children = Vec::new();
    for child in &node.children {
        let element = &child.element;
        // Inherited properties: the child's own, else the clipPath's.
        let inherited = |name: &str| element.attrs.get(name).or_else(|| own.get(name));
        if element.attrs.get("display").map(String::as_str) == Some("none")
            || inherited("visibility").map(String::as_str) == Some("hidden")
        {
            continue;
        }
        let content = match element.tag.as_str() {
            "text" | "use" => ClipContent::Unsupported,
            _ => {
                let shape = shape(element, vp);
                ClipContent::Shape {
                    path: if shape.empty {
                        skia_safe::Path::new()
                    } else {
                        shape.path.clone()
                    },
                    geometry: shape.geometry,
                    clip_rule: match inherited("clip-rule").map(String::as_str) {
                        Some("evenodd") => FillRule::EvenOdd,
                        _ => FillRule::NonZero,
                    },
                    anti_alias: inherited("shape-rendering")
                        .and_then(|v| ShapeRendering::parse(v))
                        .unwrap_or_default()
                        .anti_alias(),
                }
            }
        };
        let clip_path = clip_reference(element).map(|r| reference(root, &r, vp, chain));
        children.push(ClipChild {
            content,
            transform: transform_of(element),
            clip_path,
        });
    }
    chain.pop();
    Some(ClipPath {
        units: own
            .get("clipPathUnits")
            .map_or(ClipPathUnits::UserSpaceOnUse, |v| ClipPathUnits::parse(v)),
        transform: transform_of(&node.element),
        children,
        clip_path,
    })
}

/// A `clip-path` reference inside a `clipPath`: resolved, or present but
/// naming nothing usable.
fn reference(root: &Node, id: &str, vp: &Viewport, chain: &mut Vec<String>) -> ClipReference {
    resolve_clip(root, id, vp, chain).map_or(ClipReference::Unresolved, |clip| {
        ClipReference::Resolved(Box::new(clip))
    })
}

/// The axis-aligned bounds of `rect` mapped by `m`.
fn map_rect(m: &Matrix, rect: Rect) -> Rect {
    let corners = [
        (rect.x, rect.y),
        (rect.x + rect.width, rect.y),
        (rect.x, rect.y + rect.height),
        (rect.x + rect.width, rect.y + rect.height),
    ]
    .map(|(x, y)| m.map(div_svg::Point::new(x, y)));
    let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// An element's fill box (`getBBox`) in its own user space: a shape's
/// geometry, or the union of a group's rendered children.
fn fill_box(node: &Node, vp: &Viewport) -> Option<Rect> {
    match node.element.tag.as_str() {
        "defs" | "clipPath" => None,
        "g" => node
            .children
            .iter()
            .filter(|c| c.element.attrs.get("display").map(String::as_str) != Some("none"))
            .filter_map(|c| fill_box(c, vp).map(|b| map_rect(&transform_of(&c.element), b)))
            .reduce(|a, b| {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Rect::new(
                    x,
                    y,
                    (a.x + a.width).max(b.x + b.width) - x,
                    (a.y + a.height).max(b.y + b.height) - y,
                )
            }),
        _ => Some(shape(&node.element, vp).bbox),
    }
}

/// Paints an element with its group opacity and `clip-path`. `ctm` maps
/// its user space to the device.
fn with_effects(
    canvas: &Canvas,
    root: &Node,
    node: &Node,
    ctm: &Matrix,
    vp: &Viewport,
    paint: impl FnOnce(&Canvas),
) {
    let opacity = node
        .element
        .attrs
        .get("opacity")
        .map_or(1.0, |v| v.parse::<f32>().unwrap());
    let clip =
        clip_reference(&node.element).and_then(|id| resolve_clip(root, &id, vp, &mut Vec::new()));
    // The group opacity is a layer inside the clip: Chromium clips the
    // opacity effect's output (measured: `edge/opacity`).
    let paint = |canvas: &Canvas| {
        if opacity < 1.0 {
            canvas.save_layer_alpha_f(None, opacity);
            paint(canvas);
            canvas.restore();
        } else {
            paint(canvas);
        }
    };
    match clip {
        Some(clip) => {
            canvas.save();
            canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
            let reference_box = fill_box(node, vp).unwrap_or_default();
            clip_content(canvas, &clip, reference_box, paint);
            canvas.restore();
        }
        None => paint(canvas),
    }
}

/// Paints the children of a container whose user space is `ctm`.
fn paint_children(
    canvas: &Canvas,
    root: &Node,
    node: &Node,
    ctm: &Matrix,
    vp: &Viewport,
    props: &BTreeMap<String, String>,
    to_host: &Matrix,
) {
    for child in &node.children {
        let element = &child.element;
        let props = cascade(element, props);
        match element.tag.as_str() {
            "defs" | "clipPath" => {}
            "g" => {
                let ctm = ctm.then_apply(&transform_of(element));
                with_effects(canvas, root, child, &ctm, vp, |canvas| {
                    paint_children(canvas, root, child, &ctm, vp, &props, to_host)
                });
            }
            "svg" => {
                // A nested `svg` clips to its viewport (overflow is hidden),
                // then establishes its own user space.
                let ctx = length_context(vp.width, vp.height);
                let len = |name: &str, axis: Axis, default: f32| {
                    element
                        .attrs
                        .get(name)
                        .and_then(|v| div_svg::length::Length::parse(v))
                        .map_or(default, |l| l.resolve(axis, &ctx))
                };
                let x = len("x", Axis::Horizontal, 0.0);
                let y = len("y", Axis::Vertical, 0.0);
                let width = len("width", Axis::Horizontal, vp.width);
                let height = len("height", Axis::Vertical, vp.height);
                canvas.save();
                canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
                clip_viewport(canvas, Rect::new(x, y, width, height), true);
                let mut inner = ctm.then_apply(&Matrix::translate(f64::from(x), f64::from(y)));
                let mut inner_vp = Viewport {
                    ctm: Matrix::IDENTITY,
                    width,
                    height,
                };
                if let Some(vbt) = view_box(element, width, height) {
                    inner = inner.then_apply(&vbt);
                    let vb = parse_view_box(&element.attrs["viewBox"]).unwrap();
                    inner_vp.width = vb.width;
                    inner_vp.height = vb.height;
                }
                paint_children(canvas, root, child, &inner, &inner_vp, &props, to_host);
                canvas.restore();
            }
            _ => {
                let ctm = ctm.then_apply(&transform_of(element));
                with_effects(canvas, root, child, &ctm, vp, |canvas| {
                    canvas.save();
                    canvas.set_matrix(&M44::from(to_sk_matrix(&ctm)));
                    paint_shape(
                        canvas,
                        &shape(element, vp),
                        &paint_style(&props, vp),
                        Some(&to_host.then_apply(&ctm)),
                    );
                    canvas.restore();
                });
            }
        }
    }
}

#[test]
fn paint_v1_matches_chromium_147() {
    check("svg-paint-v1", KNOWN_GAPS_V1);
}

#[test]
fn paint_v2_matches_chromium_147() {
    check("svg-paint-v2", KNOWN_GAPS_V2);
}

#[test]
fn paint_v3_matches_chromium_147() {
    check("svg-paint-v3", KNOWN_GAPS_V3);
}

fn check(set: &str, known_gaps: &str) {
    let (platform, prefix) = ORACLE;
    let dir = fixtures(set);
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("{prefix}.json"))).expect("read oracle"),
    )
    .unwrap();
    assert_eq!(
        document["platform"], platform,
        "oracle captured on another OS"
    );
    assert_eq!(
        document["pinned"], true,
        "oracle is not Chromium 147.0.7727.50"
    );

    // `DIV_SVG_PAINT_DUMP=<dir>` writes Div's pixels for each differing case.
    let dump = std::env::var_os("DIV_SVG_PAINT_DUMP").map(PathBuf::from);
    let mut results = BTreeMap::new();
    let mut total = 0;
    for sheet_meta in document["sheets"].as_array().unwrap() {
        let scale = sheet_meta["scale"].as_f64().unwrap();
        let sheet = pixels::decode(&dir.join(sheet_meta["png"].as_str().unwrap()));
        assert_eq!(
            (sheet.width, sheet.height),
            (
                (sheet_meta["width"].as_f64().unwrap() * scale) as usize,
                (sheet_meta["height"].as_f64().unwrap() * scale) as usize
            ),
            "sheet size at {scale}x"
        );
        for case in sheet_meta["cases"].as_array().unwrap() {
            total += 1;
            let id = case["id"].as_str().unwrap();
            // The slot's frame: the case's box plus one CSS pixel, which
            // holds a root moved by snapping.
            let frame = (
                ((case["width"].as_f64().unwrap() + 1.0) * scale) as i32,
                ((case["height"].as_f64().unwrap() + 1.0) * scale) as i32,
            );
            let origin = (
                (case["x"].as_f64().unwrap() * scale) as usize,
                (case["y"].as_f64().unwrap() * scale) as usize,
            );
            let svg = tree(case["svg"].as_str().unwrap());
            let size = (
                case["width"].as_f64().unwrap(),
                case["height"].as_f64().unwrap(),
            );
            let offset = case["offset"].as_array().unwrap();
            let offset = (offset[0].as_f64().unwrap(), offset[1].as_f64().unwrap());
            let props = cascade(&svg.element, &BTreeMap::new());
            let paint = |canvas: &Canvas, root: &Root| {
                paint_children(
                    canvas,
                    &svg,
                    &svg,
                    &root.ctm,
                    &root.viewport,
                    &props,
                    &root.to_host,
                );
            };
            let actual = pixels::render_tiled(frame, origin, &|at| {
                pixels::render(&svg.element, size, offset, scale, at, None, &paint)
            });
            let (count, max) = pixels::diff(&actual, &sheet, origin, frame);
            if count > 0 {
                if let Some(dir) = &dump {
                    std::fs::create_dir_all(dir).unwrap();
                    let name = format!("{set}-{scale}x-{}.png", id.replace('/', "-"));
                    pixels::write_png(&dir.join(name), frame, &actual);
                }
                results.insert(format!("{scale}x {id}"), (count, max));
            }
        }
    }

    pixels::check_gaps(set, total, &results, known_gaps);
}
