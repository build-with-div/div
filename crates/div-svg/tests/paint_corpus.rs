//! `div_svg::paint` against Chromium 147's pixels for the paint corpus
//! (`tools/svg/paint_corpus.py`), each OS against its own capture. The
//! corpus has three sets with their own fixtures and gap lists: `v1` (fills,
//! strokes, transforms, viewports, Lucide icons), `v2` (dashes,
//! `pathLength`, `paint-order`, `shape-rendering`, `vector-effect`, chart
//! compositions), `v3` (`clipPath`) and `v4` (paint servers: gradients and
//! patterns).
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

div_text::initialize_skia_at_load!();

use div_svg::clip::{clip_content, ClipChild, ClipContent, ClipPath, ClipPathUnits, ClipReference};
use div_svg::geometry::FillRule;
use div_svg::length::{Axis, Length};
use div_svg::mask::{mask_content, Mask, MaskAttributes, MaskType};
use div_svg::paint::{clip_viewport, paint_shape, to_sk_matrix, ShapeRendering};
use div_svg::paint_server::{
    gradient_stops, parse_stop_offset, GradientAttributes, GradientKind, PaintServer,
    PatternAttributes, SpreadMethod, Units,
};
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::viewport::{parse_view_box, PreserveAspectRatio};
use div_svg::Rect;
use serde_json::Value;
use skia_safe::{Canvas, Color4f, Picture, PictureRecorder, Rect as SkRect, M44};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::pixels::{self, cascade, color, paint_style_with, view_box, Root, INK};
use support::{elements, length_context, shape, style_map, Element, Viewport};

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
const KNOWN_GAPS_V4: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v4/known-gaps.txt");
const KNOWN_GAPS_V5: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v5/known-gaps.txt");
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
    url_reference(element, "clip-path")
}

/// The `#id` of an element's `<property>: url(#id)`.
fn url_reference(element: &Element, property: &str) -> Option<String> {
    let style = style_map(element.attrs.get("style").map_or("", |s| s));
    let value = style
        .get(property)
        .or_else(|| element.attrs.get(property))?;
    let id = value.trim().strip_prefix("url(#")?.strip_suffix(')')?;
    Some(id.to_owned())
}

thread_local! {
    /// The masks being resolved, outermost first, to break reference
    /// cycles at the closing reference.
    static MASK_CHAIN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// What an element's `mask` reference does.
enum MaskReference {
    /// No `mask`, one that does not resolve to a `mask` element, or one
    /// that closes a cycle: the element is painted unmasked
    /// (`HasSingleInvalidSVGMaskLayer`; measured for the cycle:
    /// `edge/self`).
    None,
    Mask(Mask<Picture>),
}

/// Resolves an element's `mask: url(#id)`, recording the mask's content.
fn resolve_mask(
    root: &Node,
    element: &Element,
    vp: &Viewport,
    servers: &[String],
) -> MaskReference {
    let Some(id) = url_reference(element, "mask") else {
        return MaskReference::None;
    };
    if MASK_CHAIN.with(|chain| chain.borrow().contains(&id)) {
        return MaskReference::None;
    }
    let Some(node) = find(root, &id).filter(|n| n.element.tag == "mask") else {
        return MaskReference::None;
    };
    let attrs = &node.element.attrs;
    let style = style_map(attrs.get("style").map_or("", |s| s));
    let get = |name: &str| style.get(name).or_else(|| attrs.get(name));
    let attributes = MaskAttributes {
        units: get("maskUnits").and_then(|v| Units::parse(v)),
        content_units: get("maskContentUnits").and_then(|v| Units::parse(v)),
        x: get("x").and_then(|v| Length::parse(v)),
        y: get("y").and_then(|v| Length::parse(v)),
        width: get("width").and_then(|v| Length::parse(v)),
        height: get("height").and_then(|v| Length::parse(v)),
        mask_type: get("mask-type").and_then(|v| MaskType::parse(v)),
    };
    MASK_CHAIN.with(|chain| chain.borrow_mut().push(id));
    let content = record_content(root, node, vp, servers);
    MASK_CHAIN.with(|chain| chain.borrow_mut().pop());
    MaskReference::Mask(attributes.resolve(&length_context(vp.width, vp.height), content))
}

/// An element's visual rect in its own user space: its fill box, outset
/// by half its stroke width when it is stroked, or its children's united.
fn visual_rect(node: &Node, vp: &Viewport, props: &BTreeMap<String, String>) -> Option<Rect> {
    let props = cascade(&node.element, props);
    match node.element.tag.as_str() {
        tag if is_resource(tag) => None,
        "g" => node
            .children
            .iter()
            .filter(|c| c.element.attrs.get("display").map(String::as_str) != Some("none"))
            .filter_map(|c| {
                visual_rect(c, vp, &props).map(|b| map_rect(&transform_of(&c.element), b))
            })
            .reduce(union),
        _ => {
            let bbox = shape(&node.element, vp).bbox;
            let stroked = props.get("stroke").is_some_and(|s| s != "none");
            let half = if stroked {
                props
                    .get("stroke-width")
                    .map_or(1.0, |w| w.trim_end_matches("px").parse::<f32>().unwrap())
                    / 2.0
            } else {
                0.0
            };
            Some(Rect::new(
                bbox.x - half,
                bbox.y - half,
                bbox.width + 2.0 * half,
                bbox.height + 2.0 * half,
            ))
        }
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.width).max(b.x + b.width) - x,
        (a.y + a.height).max(b.y + b.height) - y,
    )
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

/// Elements that never render where they are.
fn is_resource(tag: &str) -> bool {
    matches!(
        tag,
        "defs" | "clipPath" | "mask" | "linearGradient" | "radialGradient" | "pattern" | "stop"
    )
}

/// The `#id` an element's `href` (or `xlink:href`) names.
fn href(element: &Element) -> Option<&str> {
    element
        .attrs
        .get("href")
        .or_else(|| element.attrs.get("xlink:href"))?
        .strip_prefix('#')
}

/// A gradient element's own attributes and `stop` children.
fn gradient_attributes(node: &Node) -> GradientAttributes {
    let attrs = &node.element.attrs;
    let kind = if node.element.tag == "linearGradient" {
        GradientKind::Linear
    } else {
        GradientKind::Radial
    };
    let len = |name: &str| attrs.get(name).and_then(|v| Length::parse(v));
    let stops: Vec<&Node> = node
        .children
        .iter()
        .filter(|c| c.element.tag == "stop")
        .collect();
    // `color` is inherited from the gradient into its stops.
    let inherited_color = attrs.get("color").map_or(INK, |s| s);
    let stops = (!stops.is_empty()).then(|| {
        gradient_stops(stops.iter().map(|stop| {
            let element = &stop.element;
            let style = style_map(element.attrs.get("style").map_or("", |s| s));
            let get = |name: &str| style.get(name).or_else(|| element.attrs.get(name));
            let current = get("color").map_or(inherited_color, |s| s);
            let c =
                get("stop-color").map_or(Color4f::new(0.0, 0.0, 0.0, 1.0), |v| color(v, current));
            let opacity = get("stop-opacity").map_or(1.0, |v| v.parse::<f32>().unwrap());
            let offset = element
                .attrs
                .get("offset")
                .map_or(0.0, |v| parse_stop_offset(v));
            (offset, Color4f::new(c.r, c.g, c.b, c.a * opacity))
        }))
    });
    GradientAttributes {
        units: attrs.get("gradientUnits").and_then(|v| Units::parse(v)),
        transform: attrs
            .get("gradientTransform")
            .and_then(|t| parse_transform_list(t)),
        spread: attrs
            .get("spreadMethod")
            .and_then(|v| SpreadMethod::parse(v)),
        stops,
        x1: len("x1"),
        y1: len("y1"),
        x2: len("x2"),
        y2: len("y2"),
        cx: len("cx"),
        cy: len("cy"),
        r: len("r"),
        fx: len("fx"),
        fy: len("fy"),
        fr: len("fr"),
        ..GradientAttributes::new(kind)
    }
}

/// A pattern element's own attributes; its content is the element itself
/// when it has children.
fn pattern_attributes(node: &Node) -> PatternAttributes<&Node> {
    let attrs = &node.element.attrs;
    let len = |name: &str| attrs.get(name).and_then(|v| Length::parse(v));
    PatternAttributes {
        units: attrs.get("patternUnits").and_then(|v| Units::parse(v)),
        content_units: attrs
            .get("patternContentUnits")
            .and_then(|v| Units::parse(v)),
        transform: attrs
            .get("patternTransform")
            .and_then(|t| parse_transform_list(t)),
        x: len("x"),
        y: len("y"),
        width: len("width"),
        height: len("height"),
        view_box: attrs.get("viewBox").and_then(|v| parse_view_box(v)),
        preserve_aspect_ratio: attrs
            .get("preserveAspectRatio")
            .and_then(|v| PreserveAspectRatio::parse(v)),
        content: (!node.children.is_empty()).then_some(node),
    }
}

fn is_gradient(tag: &str) -> bool {
    tag == "linearGradient" || tag == "radialGradient"
}

/// A gradient's attributes after `href` inheritance. `chain` holds the ids
/// already followed; a reference back into it, or to an element that is
/// not a gradient, is not followed.
fn gradient_chain(root: &Node, node: &Node, chain: &mut Vec<String>) -> GradientAttributes {
    let mut attrs = gradient_attributes(node);
    if let Some(id) = href(&node.element) {
        if !chain.iter().any(|seen| seen == id) {
            if let Some(target) = find(root, id).filter(|t| is_gradient(&t.element.tag)) {
                chain.push(id.to_owned());
                attrs.inherit(&gradient_chain(root, target, chain));
            }
        }
    }
    attrs
}

/// A pattern's attributes after `href` inheritance, as [`gradient_chain`].
fn pattern_chain<'a>(
    root: &'a Node,
    node: &'a Node,
    chain: &mut Vec<String>,
) -> PatternAttributes<&'a Node> {
    let mut attrs = pattern_attributes(node);
    if let Some(id) = href(&node.element) {
        if !chain.iter().any(|seen| seen == id) {
            if let Some(target) = find(root, id).filter(|t| t.element.tag == "pattern") {
                chain.push(id.to_owned());
                attrs.inherit(&pattern_chain(root, target, chain));
            }
        }
    }
    attrs
}

/// Resolves the `#id` of a `url()` paint, for an element in viewport `vp`.
/// `servers` are the patterns whose content is being recorded: a reference
/// back to one of them closes a cycle and resolves to nothing.
fn resolve_server(
    root: &Node,
    id: &str,
    vp: &Viewport,
    servers: &[String],
) -> Option<Arc<PaintServer>> {
    if servers.iter().any(|seen| seen == id) {
        return None;
    }
    let node = find(root, id)?;
    let context = length_context(vp.width, vp.height);
    let server = match node.element.tag.as_str() {
        "linearGradient" | "radialGradient" => {
            let attrs = gradient_chain(root, node, &mut vec![id.to_owned()]);
            PaintServer::Gradient(attrs.resolve(&context))
        }
        "pattern" => {
            let attrs = pattern_chain(root, node, &mut vec![id.to_owned()]);
            let mut inner = servers.to_vec();
            inner.push(id.to_owned());
            PaintServer::Pattern(attrs.resolve(&context, |content: &&Node| {
                record_content(root, content, vp, &inner)
            }))
        }
        _ => return None,
    };
    Some(Arc::new(server))
}

/// A pattern's children painted in their own user space, inheriting from
/// the pattern element that holds them.
fn record_content(root: &Node, content: &Node, vp: &Viewport, servers: &[String]) -> Picture {
    let mut recorder = PictureRecorder::new();
    let canvas = recorder.begin_recording(SkRect::new(-1e5, -1e5, 1e5, 1e5), false);
    let props = cascade(&content.element, &cascade(&root.element, &BTreeMap::new()));
    paint_children(
        canvas,
        root,
        content,
        &Matrix::IDENTITY,
        vp,
        &props,
        &Matrix::IDENTITY,
        servers,
    );
    recorder.finish_recording_as_picture(None).unwrap()
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
        tag if is_resource(tag) => None,
        "g" => node
            .children
            .iter()
            .filter(|c| c.element.attrs.get("display").map(String::as_str) != Some("none"))
            .filter_map(|c| fill_box(c, vp).map(|b| map_rect(&transform_of(&c.element), b)))
            .reduce(union),
        _ => Some(shape(&node.element, vp).bbox),
    }
}

/// Paints an element with its group opacity, `mask` and `clip-path`.
/// `ctm` maps its user space to the device; `props` are its parent's.
#[allow(clippy::too_many_arguments)]
fn with_effects(
    canvas: &Canvas,
    root: &Node,
    node: &Node,
    ctm: &Matrix,
    vp: &Viewport,
    props: &BTreeMap<String, String>,
    servers: &[String],
    paint: impl FnOnce(&Canvas),
) {
    let paint = |canvas: &Canvas| match resolve_mask(root, &node.element, vp, servers) {
        MaskReference::None => paint(canvas),
        MaskReference::Mask(mask) => {
            canvas.save();
            canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
            let reference_box = fill_box(node, vp).unwrap_or_default();
            let visual = visual_rect(node, vp, props).unwrap_or_default();
            mask_content(canvas, &mask, reference_box, visual, paint);
            canvas.restore();
        }
    };
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
    servers: &[String],
) {
    for child in &node.children {
        let element = &child.element;
        if element.attrs.get("display").map(String::as_str) == Some("none") {
            continue;
        }
        let parent = props;
        let props = cascade(element, props);
        match element.tag.as_str() {
            tag if is_resource(tag) => {}
            "g" => {
                let ctm = ctm.then_apply(&transform_of(element));
                with_effects(canvas, root, child, &ctm, vp, parent, servers, |canvas| {
                    paint_children(canvas, root, child, &ctm, vp, &props, to_host, servers)
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
                paint_children(
                    canvas, root, child, &inner, &inner_vp, &props, to_host, servers,
                );
                canvas.restore();
            }
            _ => {
                if props.get("visibility").map(String::as_str) == Some("hidden") {
                    continue;
                }
                let ctm = ctm.then_apply(&transform_of(element));
                with_effects(canvas, root, child, &ctm, vp, parent, servers, |canvas| {
                    canvas.save();
                    canvas.set_matrix(&M44::from(to_sk_matrix(&ctm)));
                    paint_shape(
                        canvas,
                        &shape(element, vp),
                        &paint_style_with(&props, vp, &|id| resolve_server(root, id, vp, servers)),
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

#[test]
fn paint_v4_matches_chromium_147() {
    check("svg-paint-v4", KNOWN_GAPS_V4);
}

#[test]
fn paint_v5_matches_chromium_147() {
    check("svg-paint-v5", KNOWN_GAPS_V5);
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
                    &[],
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
