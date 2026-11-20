//! `div_svg::paint` against Chromium 147's pixels for the paint corpus
//! (`tools/svg/paint_corpus.py`), each OS against its own capture. The
//! corpus has three sets with their own fixtures and gap lists: `v1` (fills,
//! strokes, transforms, viewports, Lucide icons), `v2` (dashes,
//! `pathLength`, `paint-order`, `shape-rendering`, `vector-effect`, chart
//! compositions), `v3` (`clipPath`), `v4` (paint servers: gradients and
//! patterns), `v5` (`mask`), `v6` (`use` and `symbol`) and `v7` (markers).
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
use div_svg::marker::{marker_positions, MarkerAttributes, MarkerKind, MarkerUnits, Orient};
use div_svg::mask::{mask_content, Mask, MaskAttributes, MaskType};
use div_svg::paint::{clip_viewport, paint_shape, to_sk_matrix, ShapeRendering};
use div_svg::paint_server::{
    gradient_stops, parse_stop_offset, GradientAttributes, GradientKind, PaintServer,
    PatternAttributes, SpreadMethod, Units,
};
use div_svg::path_data::{PathData, Segment};
use div_svg::shapes::parse_points;
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::use_element::UseAttributes;
use div_svg::viewport::{
    is_empty_view_box, parse_view_box, NestedViewport, PreserveAspectRatio, ViewportAttributes,
};
use div_svg::Rect;
use serde_json::Value;
use skia_safe::{Canvas, Color4f, Picture, PictureRecorder, Rect as SkRect, M44};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::pixels::{self, cascade, color, paint_style_with, Root, INK};
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
const KNOWN_GAPS_V6: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v6/known-gaps.txt");
const KNOWN_GAPS_V7: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v7/known-gaps.txt");
fn fixtures(set: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/fidelity/fixtures/{set}"))
}

#[derive(Clone)]
struct Node {
    element: Element,
    children: Vec<Node>,
}

/// `node` with every `use` given its instance as its only child
/// (`div_svg::use_element`): a copy of the target, without ids, inside which
/// each `use` is instantiated in turn. `ancestors` are the ids of `node`'s
/// ancestors, each copy counting as its original; a `use` whose target is
/// among them, or is the `use` itself, gets no instance.
fn instantiate(root: &Node, node: &Node, ancestors: &mut Vec<String>, copy: bool) -> Node {
    let id = node.element.attrs.get("id").cloned();
    if let Some(id) = &id {
        ancestors.push(id.clone());
    }
    let mut element = node.element.clone();
    if copy {
        element.attrs.remove("id");
    }
    let children = if element.tag == "use" {
        href(&node.element)
            .filter(|target| !ancestors.iter().any(|a| a == target))
            .and_then(|target| find(root, target))
            .map(|target| vec![instantiate(root, target, ancestors, true)])
            .unwrap_or_default()
    } else {
        node.children
            .iter()
            .map(|child| instantiate(root, child, ancestors, copy))
            .collect()
    };
    if id.is_some() {
        ancestors.pop();
    }
    Node { element, children }
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
    bounds(node, vp, props, Bounds::Visual)
}

/// An element's fill box (`getBBox`) in its own user space: a shape's
/// geometry, or the union of a container's rendered children.
fn fill_box(node: &Node, vp: &Viewport) -> Option<Rect> {
    bounds(node, vp, &BTreeMap::new(), Bounds::Fill)
}

#[derive(Clone, Copy, PartialEq)]
enum Bounds {
    /// `getBBox`.
    Fill,
    /// The visual rect: strokes outset by half their width.
    Visual,
    /// What cc bounds an opacity layer with (`PaintChunksToCcLayer`'s
    /// `UpdateEffectBounds`): the union of the drawn shapes' visual rects,
    /// each enclosed in integers in its own user space.
    Drawable,
}

/// An element's bounds of the given kind in its own user space.
fn bounds(
    node: &Node,
    vp: &Viewport,
    props: &BTreeMap<String, String>,
    kind: Bounds,
) -> Option<Rect> {
    let props = cascade(&node.element, props);
    // The children's, each in this element's user space, then mapped.
    let children = |node: &Node, vp: &Viewport, props: &BTreeMap<String, String>| {
        node.children
            .iter()
            .filter(|c| c.element.attrs.get("display").map(String::as_str) != Some("none"))
            .filter_map(|c| {
                bounds(c, vp, props, kind).map(|b| map_rect(&local_transform(c, vp), b))
            })
            .reduce(union)
    };
    let viewport = |node: &Node, viewport: NestedViewport, props: &BTreeMap<String, String>| {
        let (width, height) = viewport.content_size();
        let inner = Viewport {
            ctm: Matrix::IDENTITY,
            width,
            height,
        };
        children(node, &inner, props).map(|b| map_rect(&viewport.content_transform(), b))
    };
    match node.element.tag.as_str() {
        tag if is_resource(tag) => None,
        "use" => match node.children.first() {
            Some(instance) if is_viewport(&instance.element.tag) => viewport(
                instance,
                nested_viewport(instance, Some(&node.element), vp)?,
                &cascade(&instance.element, &props),
            ),
            _ => children(node, vp, &props),
        },
        "g" => children(node, vp, &props),
        "svg" => viewport(node, nested_viewport(node, None, vp)?, &props),
        _ => {
            let bbox = shape(&node.element, vp).bbox;
            if kind == Bounds::Fill {
                return Some(bbox);
            }
            if kind == Bounds::Drawable && draws(&props) == 0 {
                return None;
            }
            let stroked = props.get("stroke").is_some_and(|s| s != "none");
            let half = if stroked {
                props
                    .get("stroke-width")
                    .and_then(|w| Length::parse(w))
                    .map_or(1.0, |w| {
                        w.resolve(Axis::Other, &length_context(vp.width, vp.height))
                    })
                    / 2.0
            } else {
                0.0
            };
            let mut rect = Rect::new(
                bbox.x - half,
                bbox.y - half,
                bbox.width + 2.0 * half,
                bbox.height + 2.0 * half,
            );
            // A shape's visual rect includes its markers'.
            if let Some(markers) = CASE_ROOT.with(|r| {
                r.borrow()
                    .as_ref()
                    .and_then(|root| marker_bounds(root, &node.element, vp, &props))
            }) {
                rect = union(rect, markers);
            }
            if kind != Bounds::Drawable {
                return Some(rect);
            }
            let (x, y) = (rect.x.floor(), rect.y.floor());
            Some(Rect::new(
                x,
                y,
                (rect.x + rect.width).ceil() - x,
                (rect.y + rect.height).ceil() - y,
            ))
        }
    }
}

/// How many drawing operations a shape with properties `props` records: a
/// fill and a stroke, each when it paints.
fn draws(props: &BTreeMap<String, String>) -> usize {
    if props.get("visibility").map(String::as_str) == Some("hidden") {
        return 0;
    }
    let fill = props.get("fill").is_none_or(|f| f != "none");
    let stroke = props.get("stroke").is_some_and(|s| s != "none")
        && props
            .get("stroke-width")
            .is_none_or(|w| w.trim_end_matches("px").parse::<f32>().unwrap() > 0.0);
    usize::from(fill) + usize::from(stroke)
}

/// The drawing operations an element under an opacity effect records in the
/// effect's own transform space, or `None` when it records anything else
/// (a transform, a viewport, a nested effect, a paint server's shader).
fn single_space_draws(node: &Node, props: &BTreeMap<String, String>) -> Option<usize> {
    let props = cascade(&node.element, props);
    match node.element.tag.as_str() {
        tag if is_resource(tag) => Some(0),
        "g" | "use" => node
            .children
            .iter()
            .filter(|c| c.element.attrs.get("display").map(String::as_str) != Some("none"))
            .map(|c| {
                let attrs = &c.element.attrs;
                let plain = !attrs.contains_key("transform")
                    && !(c.element.tag == "use"
                        && (attrs.contains_key("x") || attrs.contains_key("y")))
                    && ["opacity", "clip-path", "mask"]
                        .iter()
                        .all(|name| url_or_value(&c.element, name).is_none());
                if plain {
                    single_space_draws(c, &props)
                } else {
                    None
                }
            })
            .sum(),
        tag if is_shape(tag) => {
            let server = |name: &str| props.get(name).is_some_and(|v| v.starts_with("url("));
            // Markers draw a record of their own, each in its own space.
            let markers = marker_segments(&node.element).is_some()
                && ["marker-start", "marker-mid", "marker-end"]
                    .iter()
                    .any(|name| props.get(*name).is_some_and(|v| v.starts_with("url(")));
            (!server("fill") && !server("stroke") && !markers).then(|| draws(&props))
        }
        _ => None,
    }
}

/// A property's value from `style` or its presentation attribute.
fn url_or_value<'a>(element: &'a Element, name: &str) -> Option<String> {
    let style = style_map(element.attrs.get("style").map_or("", |s| s));
    style.get(name).or_else(|| element.attrs.get(name)).cloned()
}

thread_local! {
    /// The alpha of an opacity effect folded into the one draw under it.
    static FOLDED_ALPHA: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
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
        // A `use` contributes its target's geometry when the target is a
        // shape, and nothing otherwise.
        let (element, transform, used) = if child.element.tag == "use" {
            match child.children.first() {
                Some(instance) if is_shape(&instance.element.tag) => (
                    &instance.element,
                    local_transform(child, vp).then_apply(&transform_of(&instance.element)),
                    Some(&child.element),
                ),
                _ => continue,
            }
        } else {
            (&child.element, transform_of(&child.element), None)
        };
        // Inherited properties: the child's own, else the `use`'s, else
        // the clipPath's.
        let inherited = |name: &str| {
            element
                .attrs
                .get(name)
                .or_else(|| used.and_then(|u| u.attrs.get(name)))
                .or_else(|| own.get(name))
        };
        let display_none = |e: &Element| e.attrs.get("display").map(String::as_str) == Some("none");
        if display_none(element)
            || used.is_some_and(display_none)
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
        let clip_path = clip_reference(&child.element).map(|r| reference(root, &r, vp, chain));
        children.push(ClipChild {
            content,
            transform,
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
        "defs"
            | "clipPath"
            | "mask"
            | "linearGradient"
            | "radialGradient"
            | "pattern"
            | "stop"
            | "symbol"
            | "marker"
    )
}

fn is_shape(tag: &str) -> bool {
    matches!(
        tag,
        "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path"
    )
}

/// An `svg`, or a `symbol` instance: a nested viewport.
fn is_viewport(tag: &str) -> bool {
    tag == "svg" || tag == "symbol"
}

fn use_attributes(element: &Element) -> UseAttributes {
    let len = |name: &str| element.attrs.get(name).and_then(|v| Length::parse(v));
    UseAttributes {
        x: len("x"),
        y: len("y"),
        width: len("width"),
        height: len("height"),
    }
}

fn viewport_attributes(element: &Element) -> ViewportAttributes {
    let attrs = &element.attrs;
    let len = |name: &str| attrs.get(name).and_then(|v| Length::parse(v));
    let view_box = attrs.get("viewBox");
    ViewportAttributes {
        x: len("x"),
        y: len("y"),
        width: len("width"),
        height: len("height"),
        view_box: view_box.and_then(|v| parse_view_box(v)),
        empty_view_box: view_box.is_some_and(|v| is_empty_view_box(v)),
        preserve_aspect_ratio: attrs
            .get("preserveAspectRatio")
            .and_then(|v| PreserveAspectRatio::parse(v))
            .unwrap_or_default(),
    }
}

/// The viewport of a nested `svg`, or of the instance of `use_element`;
/// `None` when it is disabled.
fn nested_viewport(
    node: &Node,
    use_element: Option<&Element>,
    vp: &Viewport,
) -> Option<NestedViewport> {
    let context = length_context(vp.width, vp.height);
    let attributes = viewport_attributes(&node.element);
    match use_element {
        Some(element) => use_attributes(element).instance_viewport(&attributes, &context),
        None => attributes.resolve(&context),
    }
}

/// The transform from an element's user space to its parent's: its
/// `transform`, and for a `use` then its translation.
fn local_transform(node: &Node, vp: &Viewport) -> Matrix {
    let transform = transform_of(&node.element);
    if node.element.tag != "use" {
        return transform;
    }
    let context = length_context(vp.width, vp.height);
    transform.then_apply(&use_attributes(&node.element).translation(&context))
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
    let mask = resolve_mask(root, &node.element, vp, servers);
    let masked = matches!(mask, MaskReference::Mask(_));
    let paint = |canvas: &Canvas| match mask {
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
    // opacity effect's output (measured: `edge/opacity`). As cc plays it
    // back, the layer is bounded by the drawable bounds of its content, and
    // a layer holding a single drawing operation is folded into it, the
    // operation's float alpha multiplied (`PlaybackFoldingIterator`; Div's
    // renderer does the same in `div-paint`'s `paint_record.rs`). The fold
    // is modelled for colour paints, by the fill or stroke opacity.
    let paint = |canvas: &Canvas| {
        if opacity >= 1.0 {
            paint(canvas);
        } else if !masked && single_space_draws(node, props) == Some(1) {
            FOLDED_ALPHA.with(|alpha| alpha.set(Some(opacity)));
            paint(canvas);
            FOLDED_ALPHA.with(|alpha| alpha.set(None));
        } else {
            canvas.save();
            canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
            let bounds = bounds(node, vp, props, Bounds::Drawable)
                .map(|r| SkRect::from_xywh(r.x, r.y, r.width, r.height));
            canvas.save_layer_alpha_f(bounds, opacity);
            paint(canvas);
            canvas.restore();
            canvas.restore();
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
            "use" => {
                let ctm = ctm.then_apply(&local_transform(child, vp));
                with_effects(
                    canvas,
                    root,
                    child,
                    &ctm,
                    vp,
                    parent,
                    servers,
                    |canvas| match child.children.first() {
                        Some(instance) if is_viewport(&instance.element.tag) => {
                            if instance.element.attrs.get("display").map(String::as_str)
                                == Some("none")
                            {
                                return;
                            }
                            if let Some(viewport) = nested_viewport(instance, Some(element), vp) {
                                let props = cascade(&instance.element, &props);
                                paint_viewport(
                                    canvas, root, instance, &ctm, &viewport, &props, to_host,
                                    servers,
                                );
                            }
                        }
                        _ => {
                            paint_children(canvas, root, child, &ctm, vp, &props, to_host, servers)
                        }
                    },
                );
            }
            "svg" => {
                if let Some(viewport) = nested_viewport(child, None, vp) {
                    paint_viewport(
                        canvas, root, child, ctm, &viewport, &props, to_host, servers,
                    );
                }
            }
            _ => {
                if props.get("visibility").map(String::as_str) == Some("hidden") {
                    continue;
                }
                let ctm = ctm.then_apply(&transform_of(element));
                with_effects(canvas, root, child, &ctm, vp, parent, servers, |canvas| {
                    let folded = FOLDED_ALPHA.with(|alpha| alpha.take());
                    canvas.save();
                    canvas.set_matrix(&M44::from(to_sk_matrix(&ctm)));
                    let props = with_context_paint(&props);
                    let mut style =
                        paint_style_with(&props, vp, &|id| resolve_server(root, id, vp, servers));
                    if let Some(alpha) = folded {
                        style.fill_opacity *= alpha;
                        style.stroke_opacity *= alpha;
                    }
                    paint_shape(
                        canvas,
                        &shape(element, vp),
                        &style,
                        Some(&to_host.then_apply(&ctm)),
                        &mut |canvas| {
                            paint_markers(canvas, root, element, &ctm, vp, &props, to_host, servers)
                        },
                    );
                    canvas.restore();
                });
            }
        }
    }
}

thread_local! {
    /// The markers being painted, outermost first: a marker that refers to
    /// one of them, itself included, is not painted.
    static MARKERS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The fill and stroke of the shape whose markers are being painted,
    /// which `context-fill` and `context-stroke` take.
    static CONTEXT_PAINT: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    /// The case being painted, for the markers a shape's bounds include.
    static CASE_ROOT: RefCell<Option<Node>> = const { RefCell::new(None) };
}

/// `props` with `context-fill` and `context-stroke` replaced by the context
/// element's paints, or by `none` outside a marker.
fn with_context_paint(props: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let context = CONTEXT_PAINT.with(|c| c.borrow().clone());
    let mut out = props.clone();
    for name in ["fill", "stroke"] {
        let value = match props.get(name).map(String::as_str) {
            Some("context-fill") => context.as_ref().map_or("none".to_owned(), |c| c.0.clone()),
            Some("context-stroke") => context.as_ref().map_or("none".to_owned(), |c| c.1.clone()),
            _ => continue,
        };
        out.insert(name.to_owned(), value);
    }
    out
}

/// The properties of `target` as it inherits from its ancestors in `node`.
fn cascade_to(
    node: &Node,
    target: &Node,
    parent: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let props = cascade(&node.element, parent);
    if std::ptr::eq(node, target) {
        return Some(props);
    }
    node.children
        .iter()
        .find_map(|child| cascade_to(child, target, &props))
}

/// The segments markers are placed on, for the shapes that take markers.
fn marker_segments(element: &Element) -> Option<Vec<Segment>> {
    let attr = |name: &str| element.attrs.get(name).map_or("", String::as_str);
    let num = |name: &str| attr(name).parse::<f32>().unwrap_or(0.0);
    let points = |closed: bool| {
        let list = parse_points(attr("points"));
        let mut segments: Vec<Segment> = list
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                if i == 0 {
                    Segment::MoveTo(p)
                } else {
                    Segment::LineTo(p)
                }
            })
            .collect();
        if closed && !segments.is_empty() {
            segments.push(Segment::Close);
        }
        segments
    };
    Some(match element.tag.as_str() {
        "path" => PathData::parse(attr("d")).segments,
        "line" => vec![
            Segment::MoveTo(div_svg::Point::new(num("x1"), num("y1"))),
            Segment::LineTo(div_svg::Point::new(num("x2"), num("y2"))),
        ],
        "polyline" => points(false),
        "polygon" => points(true),
        _ => return None,
    })
}

fn marker_attributes(element: &Element) -> MarkerAttributes {
    let attrs = &element.attrs;
    let len = |name: &str| attrs.get(name).and_then(|v| Length::parse(v));
    let view_box = attrs.get("viewBox");
    MarkerAttributes {
        marker_width: len("markerWidth"),
        marker_height: len("markerHeight"),
        ref_x: len("refX"),
        ref_y: len("refY"),
        units: match attrs.get("markerUnits").map(String::as_str) {
            Some("userSpaceOnUse") => MarkerUnits::UserSpaceOnUse,
            _ => MarkerUnits::StrokeWidth,
        },
        orient: attrs
            .get("orient")
            .and_then(|v| Orient::parse(v))
            .unwrap_or_default(),
        view_box: view_box.and_then(|v| parse_view_box(v)),
        empty_view_box: view_box.is_some_and(|v| is_empty_view_box(v)),
        preserve_aspect_ratio: attrs
            .get("preserveAspectRatio")
            .and_then(|v| PreserveAspectRatio::parse(v))
            .unwrap_or_default(),
    }
}

/// The `marker` element a shape's `marker-start`, `marker-mid` or
/// `marker-end` refers to, unless it is being painted already.
fn marker_reference<'a>(
    root: &'a Node,
    props: &BTreeMap<String, String>,
    kind: MarkerKind,
) -> Option<(String, &'a Node)> {
    let name = match kind {
        MarkerKind::Start => "marker-start",
        MarkerKind::Mid => "marker-mid",
        MarkerKind::End => "marker-end",
    };
    let id = props
        .get(name)?
        .trim()
        .strip_prefix("url(#")?
        .strip_suffix(')')?
        .to_owned();
    if MARKERS.with(|m| m.borrow().contains(&id)) {
        return None;
    }
    let node = find(root, &id).filter(|n| n.element.tag == "marker")?;
    Some((id, node))
}

/// The union of a shape's markers' rects in its user space: each marker's
/// viewport, or its content's visual rect when `overflow` is visible.
fn marker_bounds(
    root: &Node,
    element: &Element,
    vp: &Viewport,
    props: &BTreeMap<String, String>,
) -> Option<Rect> {
    let segments = marker_segments(element)?;
    let context = length_context(vp.width, vp.height);
    let stroke_width = props
        .get("stroke-width")
        .and_then(|w| Length::parse(w))
        .map_or(1.0, |w| w.resolve(Axis::Other, &context));
    marker_positions(&segments)
        .iter()
        .filter_map(|position| {
            let (_, marker) = marker_reference(root, props, position.kind)?;
            let attributes = marker_attributes(&marker.element);
            let placement = attributes.place(position, stroke_width, &context)?;
            let rect = if marker_overflows(&marker.element) {
                let size = attributes.view_box.unwrap_or(placement.viewport);
                let inner = Viewport {
                    ctm: Matrix::IDENTITY,
                    width: size.width,
                    height: size.height,
                };
                let props = cascade_to(root, marker, &BTreeMap::new()).unwrap_or_default();
                let content = marker
                    .children
                    .iter()
                    .filter_map(|c| {
                        bounds(c, &inner, &props, Bounds::Visual)
                            .map(|b| map_rect(&local_transform(c, &inner), b))
                    })
                    .reduce(union)?;
                map_rect(&placement.content_transform, content)
            } else {
                placement.viewport
            };
            Some(map_rect(&placement.transform, rect))
        })
        .reduce(union)
}

/// Whether a marker's `overflow` lets its content out of its viewport.
fn marker_overflows(element: &Element) -> bool {
    let style = style_map(element.attrs.get("style").map_or("", |s| s));
    let overflow = style
        .get("overflow")
        .or_else(|| element.attrs.get("overflow"))
        .map(String::as_str);
    matches!(overflow, Some("visible" | "auto"))
}

/// Paints a shape's markers (`div_svg::marker`): each on its own saved
/// canvas carrying the marker transform, clipped to the marker viewport
/// unless `overflow` is visible, with the marker's content inheriting from
/// the marker element.
#[allow(clippy::too_many_arguments)]
fn paint_markers(
    canvas: &Canvas,
    root: &Node,
    element: &Element,
    ctm: &Matrix,
    vp: &Viewport,
    props: &BTreeMap<String, String>,
    to_host: &Matrix,
    servers: &[String],
) {
    let Some(segments) = marker_segments(element) else {
        return;
    };
    let context = length_context(vp.width, vp.height);
    let stroke_width = props
        .get("stroke-width")
        .and_then(|w| Length::parse(w))
        .map_or(1.0, |w| w.resolve(Axis::Other, &context));
    for position in marker_positions(&segments) {
        let Some((id, marker)) = marker_reference(root, props, position.kind) else {
            continue;
        };
        let Some(placement) =
            marker_attributes(&marker.element).place(&position, stroke_width, &context)
        else {
            continue;
        };
        canvas.save();
        canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
        canvas.concat(&to_sk_matrix(&placement.transform));
        if !marker_overflows(&marker.element) {
            clip_viewport(canvas, placement.viewport, false);
        }
        canvas.save();
        canvas.concat(&to_sk_matrix(&placement.content_transform));
        let device = canvas.local_to_device_as_3x3();
        canvas.restore();
        let content = Matrix::new(
            f64::from(device.scale_x()),
            f64::from(device.skew_y()),
            f64::from(device.skew_x()),
            f64::from(device.scale_y()),
            f64::from(device.translate_x()),
            f64::from(device.translate_y()),
        );
        let size = marker_attributes(&marker.element)
            .view_box
            .unwrap_or(placement.viewport);
        let inner_vp = Viewport {
            ctm: Matrix::IDENTITY,
            width: size.width,
            height: size.height,
        };
        let marker_props = cascade_to(root, marker, &BTreeMap::new()).unwrap_or_default();
        let saved = CONTEXT_PAINT.with(|c| {
            c.replace(Some((
                props
                    .get("fill")
                    .cloned()
                    .unwrap_or_else(|| "#000000".to_owned()),
                props
                    .get("stroke")
                    .cloned()
                    .unwrap_or_else(|| "none".to_owned()),
            )))
        });
        MARKERS.with(|m| m.borrow_mut().push(id));
        paint_children(
            canvas,
            root,
            marker,
            &content,
            &inner_vp,
            &marker_props,
            to_host,
            servers,
        );
        MARKERS.with(|m| m.borrow_mut().pop());
        CONTEXT_PAINT.with(|c| c.replace(saved));
        canvas.restore();
    }
}

/// Paints a nested viewport's children: clipped to the viewport unless
/// `overflow` is visible, in the user space the viewport establishes.
#[allow(clippy::too_many_arguments)]
fn paint_viewport(
    canvas: &Canvas,
    root: &Node,
    node: &Node,
    ctm: &Matrix,
    viewport: &NestedViewport,
    props: &BTreeMap<String, String>,
    to_host: &Matrix,
    servers: &[String],
) {
    canvas.save();
    canvas.set_matrix(&M44::from(to_sk_matrix(ctm)));
    let overflow = node.element.attrs.get("overflow").map(String::as_str);
    if !matches!(overflow, Some("visible" | "auto")) {
        clip_viewport(canvas, viewport.viewport, true);
    }
    let inner = ctm.then_apply(&viewport.content_transform());
    let (width, height) = viewport.content_size();
    let inner_vp = Viewport {
        ctm: Matrix::IDENTITY,
        width,
        height,
    };
    paint_children(
        canvas, root, node, &inner, &inner_vp, props, to_host, servers,
    );
    canvas.restore();
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

#[test]
fn paint_v6_matches_chromium_147() {
    check("svg-paint-v6", KNOWN_GAPS_V6);
}

#[test]
fn paint_v7_matches_chromium_147() {
    check("svg-paint-v7", KNOWN_GAPS_V7);
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
            let svg = instantiate(&svg, &svg, &mut Vec::new(), false);
            CASE_ROOT.with(|r| r.replace(Some(svg.clone())));
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
