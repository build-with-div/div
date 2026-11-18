//! `div_svg::text` layout against Chromium 147's SVG text DOM, for the text
//! corpus (`tools/svg/text_corpus.py`), each OS against its own capture.
//!
//! For every `<text>` of every case, at device scales 1 and 2, the test lays
//! the text out with `div-svg` and `div-text` (Geist and Inter, the corpus's
//! web fonts, registered as web fonts) and compares, as f32 and exactly:
//! the number of characters, `getComputedTextLength()`, and per character
//! `getStartPositionOfChar`, `getEndPositionOfChar`, `getExtentOfChar` and
//! `getRotationOfChar`. The text's bounding box (`getBBox`) needs glyph ink
//! bounds and is not compared (V2b).
//!
//! A case that differs must be listed in `known-gaps.txt` as `<os> <scale>x
//! <case> <differing numbers>` (`os` is `linux` or `windows`) under a
//! comment giving the cause and owner. The test fails on a new difference,
//! on a listed one whose count changes, and on a listed case that becomes
//! exact, so the list stays true.
//!
//! The pixel test paints the same cases, and the pixel-only `paint/` cases,
//! with `div_svg::text::paint` as Div's renderer rasterizes a page (white
//! 256-px tiles stepping by 254), and compares each case's slot of
//! Chromium's sheet (`pixels-<os>-<scale>x.png`) byte for byte. Its gaps are
//! in `pixel-gaps.txt`, as `<os> <scale>x <case> <differing pixels> <largest
//! channel difference>`, kept true the same way.

#![cfg(any(target_os = "linux", windows))]

mod support;

div_text::initialize_skia_at_load!();

use div_geometry::{RasterConfiguration, RasterPixelGeometry};
use div_style::{FontFamilyList, FontStyleEnum};
use div_svg::length::{LengthContext, Unit};
use div_svg::paint::{to_sk_matrix, SvgPaintStyle};
use div_svg::text::{
    self, parse_length_list, parse_number_list, Baseline, BaselineShift, TextAnchor, TextChild,
    TextFont, TextLayout, TextSpan, TextStyle, WhiteSpace,
};
use div_svg::transform::{parse_transform_list, Matrix};
use div_text::{Font, FontAxisRange, FontCollection, FontFaceDescriptor};
use serde_json::Value;
use skia_safe::{Canvas, PixelGeometry, SurfaceProps, SurfacePropsFlags, M44};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use support::pixels::{self, cascade, paint_style, Root};
use support::{elements, style_map, viewport, Element, Viewport};

/// The oracle's platform, its file, and the OS column of `known-gaps.txt`.
#[cfg(target_os = "linux")]
const ORACLE: (&str, &str, &str) = ("linux", "positions-linux", "linux");
#[cfg(windows)]
const ORACLE: (&str, &str, &str) = ("win32", "positions-windows", "windows");
const KNOWN_GAPS: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-text-v1/known-gaps.txt");
const PIXEL_GAPS: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-text-v1/pixel-gaps.txt");
#[cfg(target_os = "linux")]
const PIXELS: &str = "pixels-linux";
#[cfg(windows)]
const PIXELS: &str = "pixels-windows";
/// The inherited text properties the corpus sets.
const INHERITED: [&str; 9] = [
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "letter-spacing",
    "word-spacing",
    "text-anchor",
    "dominant-baseline",
    "white-space",
];

/// The raster configuration of Div's Chromium-matching renderer for real
/// fonts (`div-paint`'s, and the components fidelity harness's).
const RASTER: RasterConfiguration = RasterConfiguration::chromium_linux_lcd();

/// The tile surface's text properties for [`RASTER`], as `div-paint`'s
/// `raster_surface_properties` builds them.
fn surface_props() -> SurfaceProps {
    assert_eq!(RASTER.pixel_geometry, RasterPixelGeometry::RgbHorizontal);
    SurfaceProps::new_with_text_properties(
        SurfacePropsFlags::default(),
        PixelGeometry::RGBH,
        RASTER.contrast(),
        RASTER.gamma(),
    )
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The corpus's web fonts, registered as `@font-face` registers them.
fn collection() -> Arc<FontCollection> {
    let fonts = root().join("tests/fidelity/components/assets/fonts");
    let collection = FontCollection::system();
    for (family, file) in [
        ("Geist", "geist/Geist-Variable.woff2"),
        ("Inter", "inter/InterVariable.woff2"),
    ] {
        let mut descriptor = FontFaceDescriptor::new(family);
        descriptor.weight = FontAxisRange::new(100.0, 900.0);
        let bytes = std::fs::read(fonts.join(file)).expect("read font");
        collection
            .register(Arc::from(bytes), descriptor)
            .expect("register font");
    }
    collection
}

// ─── Markup ─────────────────────────────────────────────────────────────

enum Child {
    Element(Node),
    Text(String),
}

struct Node {
    tag: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Child>,
}

/// The element tree of a case's generated markup, with text content.
fn tree(markup: &str) -> Node {
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        if open > 0 {
            if let Some(top) = stack.last_mut() {
                top.children.push(Child::Text(rest[..open].to_owned()));
            }
        }
        let close = rest[open..].find('>').unwrap() + open;
        let body = &rest[open + 1..close];
        rest = &rest[close + 1..];
        if body.starts_with('/') {
            let node = stack.pop().unwrap();
            match stack.last_mut() {
                Some(parent) => parent.children.push(Child::Element(node)),
                None => root = Some(node),
            }
            continue;
        }
        let element = elements(&format!("<{body}>")).pop().unwrap();
        let node = Node {
            tag: element.tag,
            attrs: element.attrs,
            children: Vec::new(),
        };
        if body.ends_with('/') {
            stack
                .last_mut()
                .unwrap()
                .children
                .push(Child::Element(node));
        } else {
            stack.push(node);
        }
    }
    root.expect("one root element")
}

/// An element's specified properties: presentation attributes, then the
/// `style` attribute.
fn declared(node: &Node) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = node
        .attrs
        .iter()
        .filter(|(k, _)| *k != "style")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(style) = node.attrs.get("style") {
        out.extend(style_map(style));
    }
    out
}

fn css_px(text: &str, font_size: f32) -> f32 {
    let length = parse_length_list(text).pop().expect("a length");
    match length.unit {
        Unit::Em => length.value * font_size,
        Unit::Number | Unit::Px => length.value,
        other => panic!("unsupported unit {other:?}"),
    }
}

struct Context<'a> {
    collection: &'a Arc<FontCollection>,
    viewport: (f32, f32),
}

/// Builds a `<text>` or `<tspan>` from its element and the inherited
/// properties.
fn span(node: &Node, inherited: &BTreeMap<String, String>, cx: &Context<'_>) -> TextSpan {
    let own = declared(node);
    let mut props: BTreeMap<String, String> = inherited
        .iter()
        .filter(|(k, _)| INHERITED.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let parent_size: f32 = props.get("font-size").map_or(16.0, |v| css_px(v, 16.0));
    for (k, v) in &own {
        props.insert(k.clone(), v.clone());
    }
    let size = props
        .get("font-size")
        .map_or(16.0, |v| css_px(v, parent_size));
    // Computed values: font-size in px, spacing in px.
    props.insert("font-size".into(), format!("{size}"));
    for name in ["letter-spacing", "word-spacing"] {
        if let Some(v) = props.get(name).cloned() {
            props.insert(name.into(), format!("{}", css_px(&v, size)));
        }
    }
    let font = TextFont {
        family: FontFamilyList::single(props.get("font-family").map_or("Geist", |v| v.as_str())),
        size,
        weight: props
            .get("font-weight")
            .map_or(400.0, |v| v.parse().unwrap()),
        style: match props.get("font-style").map(|v| v.as_str()) {
            Some("italic") => FontStyleEnum::Italic,
            _ => FontStyleEnum::Normal,
        },
        letter_spacing: props
            .get("letter-spacing")
            .map_or(0.0, |v| v.parse().unwrap()),
        word_spacing: props
            .get("word-spacing")
            .map_or(0.0, |v| v.parse().unwrap()),
        raster_configuration: RASTER,
    };
    let x_height = {
        let unscaled = Font::new_in_collection(font.description(1.0), Arc::clone(cx.collection));
        unscaled.font_metrics().map_or(0.0, |m| m.x_height)
    };
    let style = TextStyle {
        font,
        text_anchor: props
            .get("text-anchor")
            .and_then(|v| TextAnchor::parse(v))
            .unwrap_or_default(),
        dominant_baseline: props
            .get("dominant-baseline")
            .and_then(|v| Baseline::parse(v))
            .unwrap_or_default(),
        alignment_baseline: own
            .get("alignment-baseline")
            .and_then(|v| Baseline::parse_alignment(v))
            .unwrap_or_default(),
        baseline_shift: own
            .get("baseline-shift")
            .and_then(|v| BaselineShift::parse(v))
            .unwrap_or_default(),
        white_space: match props.get("white-space").map(|v| v.as_str()) {
            Some("pre") => WhiteSpace::Preserve,
            _ => WhiteSpace::Collapse,
        },
    };
    let lengths = LengthContext {
        viewport_width: cx.viewport.0,
        viewport_height: cx.viewport.1,
        font_size: size,
        root_font_size: 16.0,
        x_height,
    };
    let list = |name: &str| {
        node.attrs
            .get(name)
            .map_or(Vec::new(), |v| parse_length_list(v))
    };
    let children = node
        .children
        .iter()
        .map(|child| match child {
            Child::Text(text) => TextChild::Text(text.clone()),
            Child::Element(e) if e.tag == "tspan" => TextChild::Span(span(e, &props, cx)),
            Child::Element(e) => panic!("unexpected <{}> in text", e.tag),
        })
        .collect();
    TextSpan {
        style,
        x: list("x"),
        y: list("y"),
        dx: list("dx"),
        dy: list("dy"),
        rotate: node
            .attrs
            .get("rotate")
            .map_or(Vec::new(), |v| parse_number_list(v)),
        lengths,
        children,
    }
}

fn transform_of(node: &Node) -> Matrix {
    node.attrs
        .get("transform")
        .and_then(|t| parse_transform_list(t))
        .unwrap_or(Matrix::IDENTITY)
}

/// Every `<text>` under `node`, in document order, laid out.
fn layouts(
    node: &Node,
    ctm: &Matrix,
    props: &BTreeMap<String, String>,
    cx: &Context<'_>,
    scale: f64,
    out: &mut Vec<TextLayout>,
) {
    for child in &node.children {
        let Child::Element(element) = child else {
            continue;
        };
        let inherited = props;
        let mut props = props.clone();
        props.extend(declared(element));
        let ctm = ctm.then_apply(&transform_of(element));
        match element.tag.as_str() {
            "g" => layouts(element, &ctm, &props, cx, scale, out),
            "text" => {
                let text = span(element, inherited, cx);
                let to_device = Matrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0).then_apply(&ctm);
                out.push(text::layout(&text, cx.collection, &to_device));
            }
            other => panic!("unexpected <{other}>"),
        }
    }
}

// ─── Painting ───────────────────────────────────────────────────────────

fn element(node: &Node) -> Element {
    Element {
        tag: node.tag.clone(),
        attrs: node.attrs.clone(),
    }
}

/// The paint of a `<text>` and its `tspan`s, in pre-order (the order of
/// `TextFragment::span`).
fn span_styles(
    node: &Node,
    parent: &BTreeMap<String, String>,
    vp: &Viewport,
) -> Vec<SvgPaintStyle> {
    fn walk(
        node: &Node,
        parent: &BTreeMap<String, String>,
        vp: &Viewport,
        out: &mut Vec<SvgPaintStyle>,
    ) {
        let props = cascade(&element(node), parent);
        out.push(paint_style(&props, vp));
        for child in &node.children {
            if let Child::Element(e) = child {
                walk(e, &props, vp, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(node, parent, vp, &mut out);
    out
}

/// Paints every `<text>` under `node`, whose user space maps to the tile's
/// pixels by `ctm`. `props` are the inherited text properties and `paints`
/// the inherited paint properties.
fn paint_texts(
    canvas: &Canvas,
    node: &Node,
    ctm: &Matrix,
    props: &BTreeMap<String, String>,
    paints: &BTreeMap<String, String>,
    cx: &Context<'_>,
    root: &Root,
) {
    for child in &node.children {
        let Child::Element(element_node) = child else {
            continue;
        };
        let inherited = props;
        let mut props = props.clone();
        props.extend(declared(element_node));
        let ctm = ctm.then_apply(&transform_of(element_node));
        match element_node.tag.as_str() {
            "g" => {
                let paints = cascade(&element(element_node), paints);
                paint_texts(canvas, element_node, &ctm, &props, &paints, cx, root);
            }
            "text" => {
                let text = span(element_node, inherited, cx);
                let layout = text::layout(&text, cx.collection, &ctm);
                let styles = span_styles(element_node, paints, &root.viewport);
                canvas.save();
                canvas.set_matrix(&M44::from(to_sk_matrix(&ctm)));
                text::paint(canvas, &layout, &|span| styles[span].clone());
                canvas.restore();
            }
            other => panic!("unexpected <{other}>"),
        }
    }
}

// ─── Comparison ─────────────────────────────────────────────────────────

fn numbers(value: &Value) -> Vec<f64> {
    match value {
        Value::Number(n) => vec![n.as_f64().unwrap()],
        Value::Array(items) => items.iter().flat_map(numbers).collect(),
        other => panic!("unexpected {other}"),
    }
}

/// The SVG DOM's answers for one laid-out `<text>`, in the oracle's shape.
fn queries(layout: &TextLayout) -> BTreeMap<&'static str, Vec<f64>> {
    let n = layout.character_fragments.len();
    let mut out: BTreeMap<&'static str, Vec<f64>> = BTreeMap::new();
    out.insert("chars", vec![n as f64]);
    out.insert(
        "length",
        vec![f64::from(text::computed_text_length(layout))],
    );
    let mut start = Vec::new();
    let mut end = Vec::new();
    let mut extent = Vec::new();
    let mut rotation = Vec::new();
    for i in 0..n {
        let p = text::start_position(layout, i);
        start.extend([f64::from(p.x), f64::from(p.y)]);
        let p = text::end_position(layout, i);
        end.extend([f64::from(p.x), f64::from(p.y)]);
        let r = text::extent(layout, i);
        extent.extend([r.x, r.y, r.width, r.height].map(f64::from));
        let fragment = &layout.fragments[layout.character_fragments[i]];
        rotation.push(f64::from(fragment.angle));
    }
    out.insert("start", start);
    out.insert("end", end);
    out.insert("extent", extent);
    out.insert("rotation", rotation);
    out
}

const FIELDS: [&str; 6] = ["chars", "length", "start", "end", "extent", "rotation"];

#[test]
fn text_positions_match_chromium_147() {
    let (platform, prefix, os) = ORACLE;
    let dir = root().join("tests/fidelity/fixtures/svg-text-v1");
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("{prefix}.json"))).expect("read oracle"),
    )
    .expect("parse oracle");
    assert_eq!(document["platform"], platform);
    assert_eq!(document["pinned"], true);
    let collection = collection();
    let debug = std::env::var("DIV_SVG_TEXT_DEBUG").ok();

    let mut differing: BTreeMap<String, usize> = BTreeMap::new();
    let mut compared = 0;
    for case in document["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let svg = tree(case["svg"].as_str().unwrap());
        let root_element = support::Element {
            tag: svg.tag.clone(),
            attrs: svg.attrs.clone(),
        };
        let vp: Viewport = viewport(&root_element, None);
        let cx = Context {
            collection: &collection,
            viewport: (vp.width, vp.height),
        };
        let props = declared(&svg);
        for scale in [1u32, 2] {
            let mut laid_out = Vec::new();
            layouts(&svg, &vp.ctm, &props, &cx, f64::from(scale), &mut laid_out);
            let expected = case["chromium"][scale.to_string()].as_array().unwrap();
            assert_eq!(laid_out.len(), expected.len(), "{id}: number of <text>");
            let mut wrong = 0;
            for (layout, oracle) in laid_out.iter().zip(expected) {
                let ours = queries(layout);
                for field in FIELDS {
                    let theirs = numbers(&oracle[field]);
                    let mine = &ours[field];
                    compared += theirs.len();
                    if mine.len() != theirs.len() {
                        wrong += theirs.len().max(1);
                        continue;
                    }
                    for (k, (a, b)) in mine.iter().zip(&theirs).enumerate() {
                        if a != b {
                            wrong += 1;
                            if debug.as_deref().is_some_and(|d| id.starts_with(d)) {
                                eprintln!("{scale}x {id} {field}[{k}]: div {a} chromium {b}");
                            }
                        }
                    }
                }
            }
            if wrong > 0 {
                differing.insert(format!("{scale}x {id}"), wrong);
            }
        }
    }

    let mut listed: BTreeMap<String, usize> = BTreeMap::new();
    for line in KNOWN_GAPS.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.len(), 4, "known gap line: {line}");
        assert!(
            ["linux", "windows"].contains(&parts[0]),
            "known gap OS: {line}"
        );
        if parts[0] == os {
            listed.insert(
                format!("{} {}", parts[1], parts[2]),
                parts[3].parse().unwrap(),
            );
        }
    }
    let mut problems = Vec::new();
    for (key, count) in &differing {
        match listed.get(key) {
            None => problems.push(format!("{key}: {count} numbers differ (not listed)")),
            Some(n) if n != count => {
                problems.push(format!("{key}: {count} numbers differ (listed {n})"))
            }
            _ => {}
        }
    }
    let exact: BTreeSet<&String> = listed
        .keys()
        .filter(|k| !differing.contains_key(*k))
        .collect();
    for key in exact {
        problems.push(format!("{key}: listed but now exact"));
    }
    eprintln!(
        "text corpus on {platform}: {compared} numbers, {} case-scales differ",
        differing.len()
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn text_pixels_match_chromium_147() {
    let (platform, _, _) = ORACLE;
    let dir = root().join("tests/fidelity/fixtures/svg-text-v1");
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("{PIXELS}.json"))).expect("read oracle"),
    )
    .expect("parse oracle");
    assert_eq!(document["platform"], platform);
    assert_eq!(document["pinned"], true);
    let collection = collection();
    // `DIV_SVG_TEXT_DUMP=<dir>` writes Div's pixels for each differing case.
    let dump = std::env::var_os("DIV_SVG_TEXT_DUMP").map(PathBuf::from);
    let props = surface_props();

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
            let svg = tree(case["svg"].as_str().unwrap());
            let root_element = element(&svg);
            let size = (
                case["width"].as_f64().unwrap(),
                case["height"].as_f64().unwrap(),
            );
            // The slot's frame: the case's box plus one CSS pixel, which
            // holds a root moved by snapping.
            let frame = (
                ((size.0 + 1.0) * scale) as i32,
                ((size.1 + 1.0) * scale) as i32,
            );
            let origin = (
                (case["x"].as_f64().unwrap() * scale) as usize,
                (case["y"].as_f64().unwrap() * scale) as usize,
            );
            let declared_props = declared(&svg);
            let paints = cascade(&root_element, &BTreeMap::new());
            let paint = |canvas: &Canvas, root: &Root| {
                let cx = Context {
                    collection: &collection,
                    viewport: (root.viewport.width, root.viewport.height),
                };
                paint_texts(canvas, &svg, &root.ctm, &declared_props, &paints, &cx, root);
            };
            let actual = pixels::render_tiled(frame, origin, &|at| {
                pixels::render(
                    &root_element,
                    size,
                    (0.0, 0.0),
                    scale,
                    at,
                    Some(&props),
                    &paint,
                )
            });
            let (count, max) = pixels::diff(&actual, &sheet, origin, frame);
            if count > 0 {
                if let Some(dir) = &dump {
                    std::fs::create_dir_all(dir).unwrap();
                    let name = format!("text-{scale}x-{}.png", id.replace('/', "-"));
                    pixels::write_png(&dir.join(name), frame, &actual);
                }
                results.insert(format!("{scale}x {id}"), (count, max));
            }
        }
    }
    pixels::check_gaps("svg-text-v1 pixels", total, &results, PIXEL_GAPS);
}
