//! `div_svg::paint` against Chromium 147's pixels for the paint corpus
//! (`tools/svg/paint_corpus.py`), each OS against its own capture. The
//! corpus has two sets with their own fixtures and gap lists: `v1` (fills,
//! strokes, transforms, viewports, Lucide icons) and `v2` (dashes,
//! `pathLength`, `paint-order`, `shape-rendering`, `vector-effect`, chart
//! compositions).
//!
//! Chromium renders every case of a sheet in its own slot of one page, at
//! scale 1 and, for a subset, at 2 (real zoom). This test paints
//! each case with `div-svg` the way Div's renderer rasterizes a page: on
//! white `raster_n32_premul` tiles of 256 px that step by 254
//! (`div-paint`'s `render.rs`, after cc's tiling), each case in the device
//! space of the tile that shows it, and compares it with the slot, byte for
//! byte. A case that differs must be listed in
//! `known-gaps.txt` as `<scale>x <case> <differing pixels> <largest channel
//! difference>` under a comment giving the cause. The test fails on a new
//! difference, on a listed one whose numbers change, and on a listed case
//! that becomes exact, so the list stays true.

#![cfg(any(target_os = "linux", windows))]

mod support;

use div_svg::geometry::FillRule;
use div_svg::length::Axis;
use div_svg::paint::{
    clip_viewport, paint_shape, root_transform, to_sk_matrix, PaintOrder, RootBox, ShapeRendering,
    SvgPaint, SvgPaintStyle,
};
use div_svg::transform::{parse_transform_list, Matrix};
use div_svg::viewport::{parse_view_box, view_box_transform, PreserveAspectRatio};
use div_svg::Rect;
use serde_json::Value;
use skia_safe::{surfaces, AlphaType, Color, Color4f, ColorType, ImageInfo, M44};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use support::{elements, length_context, shape, stroke_style_in, style_map, Element, Viewport};

#[cfg(target_os = "linux")]
const ORACLE: (&str, &str) = ("linux", "chromium-linux");
#[cfg(windows)]
const ORACLE: (&str, &str) = ("win32", "chromium-windows");
const KNOWN_GAPS_V1: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v1/known-gaps.txt");
const KNOWN_GAPS_V2: &str =
    include_str!("../../../tests/fidelity/fixtures/svg-paint-v2/known-gaps.txt");
/// The page's `color`, which `currentColor` resolves to.
const INK: &str = "#09090b";
/// The SVG properties the corpus sets, all inherited.
const PROPERTIES: [&str; 14] = [
    "color",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "paint-order",
    "shape-rendering",
];

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

/// Inherited properties: presentation attributes, then the `style`
/// attribute, over the parent's values.
fn cascade(element: &Element, parent: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = parent.clone();
    // `vector-effect` is not inherited.
    out.remove("vector-effect");
    if let Some(value) = element.attrs.get("vector-effect") {
        out.insert("vector-effect".to_owned(), value.clone());
    }
    for name in PROPERTIES {
        if let Some(value) = element.attrs.get(name) {
            out.insert(name.to_owned(), value.clone());
        }
    }
    out.extend(style_map(element.attrs.get("style").map_or("", |s| s)));
    out
}

fn color(text: &str, current: &str) -> Color4f {
    let text = if text == "currentColor" {
        current
    } else {
        text
    };
    let hex = text
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("colour {text}"));
    let hex = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_owned()
    };
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    Color4f::from(Color::from_rgb(channel(0), channel(2), channel(4)))
}

fn paint_value(value: Option<&String>, initial: SvgPaint, current: &str) -> SvgPaint {
    match value.map(String::as_str) {
        None => initial,
        Some("none") => SvgPaint::None,
        Some(text) => SvgPaint::Color(color(text, current)),
    }
}

fn paint_style(props: &BTreeMap<String, String>, vp: &Viewport) -> SvgPaintStyle {
    let current = props.get("color").map_or(INK, |s| s);
    let opacity = |name: &str| props.get(name).map_or(1.0, |v| v.parse::<f32>().unwrap());
    let initial = SvgPaintStyle::default();
    SvgPaintStyle {
        fill: paint_value(props.get("fill"), initial.fill, current),
        fill_opacity: opacity("fill-opacity"),
        fill_rule: match props.get("fill-rule").map(String::as_str) {
            Some("evenodd") => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        },
        stroke: paint_value(props.get("stroke"), SvgPaint::None, current),
        stroke_opacity: opacity("stroke-opacity"),
        stroke_style: stroke_style_in(props, &length_context(vp.width, vp.height)),
        // An invalid value is ignored, leaving the inherited one; the
        // corpus sets these on single elements, so that is the initial one.
        paint_order: props
            .get("paint-order")
            .and_then(|v| PaintOrder::parse(v))
            .unwrap_or_default(),
        shape_rendering: props
            .get("shape-rendering")
            .and_then(|v| ShapeRendering::parse(v))
            .unwrap_or_default(),
        non_scaling_stroke: props.get("vector-effect").map(String::as_str)
            == Some("non-scaling-stroke"),
    }
}

fn transform_of(element: &Element) -> Matrix {
    element
        .attrs
        .get("transform")
        .and_then(|t| parse_transform_list(t))
        .unwrap_or(Matrix::IDENTITY)
}

fn view_box(element: &Element, width: f32, height: f32) -> Option<Matrix> {
    let vb = element
        .attrs
        .get("viewBox")
        .and_then(|v| parse_view_box(v))?;
    let par = element
        .attrs
        .get("preserveAspectRatio")
        .and_then(|v| PreserveAspectRatio::parse(v))
        .unwrap_or_default();
    Some(view_box_transform(vb, par, width, height))
}

/// Paints the children of a container whose user space is `ctm`.
fn paint_children(
    canvas: &skia_safe::Canvas,
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
            "g" => {
                let ctm = ctm.then_apply(&transform_of(element));
                paint_children(canvas, child, &ctm, vp, &props, to_host);
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
                paint_children(canvas, child, &inner, &inner_vp, &props, to_host);
                canvas.restore();
            }
            _ => {
                let ctm = ctm.then_apply(&transform_of(element));
                canvas.save();
                canvas.set_matrix(&M44::from(to_sk_matrix(&ctm)));
                paint_shape(
                    canvas,
                    &shape(element, vp),
                    &paint_style(&props, vp),
                    Some(&to_host.then_apply(&ctm)),
                );
                canvas.restore();
            }
        }
    }
}

/// LayoutUnit rounding: to the nearest 1/64, then half up to an integer.
fn snap(v: f64) -> i32 {
    let fixed = (v * 64.0).round() as i64;
    ((fixed + 32) >> 6) as i32
}

/// One raster tile with the case painted at `at`, the device position of
/// its slot within the tile: RGBA, `TILE_SIZE` square.
fn render(case: &Value, scale: f64, at: (i32, i32)) -> Vec<u8> {
    let width = case["width"].as_f64().unwrap();
    let height = case["height"].as_f64().unwrap();
    let offset = case["offset"].as_array().unwrap();
    let (ox, oy) = (offset[0].as_f64().unwrap(), offset[1].as_f64().unwrap());
    let size = (TILE_SIZE, TILE_SIZE);
    let mut surface = surfaces::raster_n32_premul(size).unwrap();
    let canvas = surface.canvas();
    canvas.clear(Color::WHITE);

    // Layout is in device pixels at real zoom: the root's box and its
    // `viewBox` fit are computed at the zoomed size.
    let (zw, zh) = (width * scale, height * scale);
    let (zx, zy) = (ox * scale + f64::from(at.0), oy * scale + f64::from(at.1));
    let snapped = (
        snap(zx),
        snap(zy),
        snap(zx + zw) - snap(zx),
        snap(zy + zh) - snap(zy),
    );
    let root = RootBox {
        snapped,
        size: (zw as f32, zh as f32),
        is_document_element: false,
    };
    if !root.renders() {
        return pixels(&mut surface, size);
    }
    let markup = case["svg"].as_str().unwrap();
    let svg = tree(markup);
    let user_w = width as f32;
    let user_h = height as f32;
    let (local, vp) = match view_box(&svg.element, zw as f32, zh as f32) {
        Some(vbt) => {
            let vb = parse_view_box(&svg.element.attrs["viewBox"]).unwrap();
            (
                vbt,
                Viewport {
                    ctm: Matrix::IDENTITY,
                    width: vb.width,
                    height: vb.height,
                },
            )
        }
        None => (
            Matrix::scale(scale, scale),
            Viewport {
                ctm: Matrix::IDENTITY,
                width: user_w,
                height: user_h,
            },
        ),
    };
    let ctm = root_transform(&root, &local);
    // The replaced content clip: the snapped content box.
    canvas.save();
    clip_viewport(
        canvas,
        Rect::new(
            snapped.0 as f32,
            snapped.1 as f32,
            snapped.2 as f32,
            snapped.3 as f32,
        ),
        false,
    );
    let props = cascade(&svg.element, &BTreeMap::new());
    // `vector-effect`'s host space: the root's coordinates before the
    // paint offset and zoom.
    let to_host = Matrix::translate(f64::from(snapped.0), f64::from(snapped.1))
        .then_apply(&Matrix::scale(scale, scale))
        .inverse()
        .unwrap();
    paint_children(canvas, &svg, &ctm, &vp, &props, &to_host);
    canvas.restore();
    pixels(&mut surface, size)
}

fn pixels(surface: &mut skia_safe::Surface, frame: (i32, i32)) -> Vec<u8> {
    let info = ImageInfo::new(frame, ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut out = vec![0u8; (frame.0 * frame.1 * 4) as usize];
    assert!(surface.read_pixels(&info, &mut out, frame.0 as usize * 4, (0, 0)));
    out
}

/// Chromium 147 rasterizes a layer larger than 512 px in 256 px tiles that
/// overlap by two texels (cc's tiling, which `div-paint`'s `render.rs`
/// reproduces): tile `i` is drawn translated by `−254·i` and supplies the
/// pixels from `254·i + 1` (from 0 for the first) to `254·i + 255`. Stroke
/// outlines and transformed points are computed in single precision in that
/// tile's device space, so the result depends on it. A case is painted once
/// per tile it touches and each pixel is taken from the tile that supplies
/// it.
const TILE_SIZE: i32 = 256;
const TILE_STEP: i32 = TILE_SIZE - 2;

fn render_tiled(case: &Value, scale: f64, frame: (i32, i32), slot: (usize, usize)) -> Vec<u8> {
    let owner = |v: i32| (v - 1).max(0) / TILE_STEP;
    let origin = |i: i32| i * TILE_STEP;
    let (sx, sy) = (slot.0 as i32, slot.1 as i32);
    let mut out = vec![0u8; (frame.0 * frame.1 * 4) as usize];
    for ty in owner(sy)..=owner(sy + frame.1 - 1) {
        for tx in owner(sx)..=owner(sx + frame.0 - 1) {
            let at = (sx - origin(tx), sy - origin(ty));
            let width = TILE_SIZE;
            let tile = render(case, scale, at);
            for y in 0..frame.1 {
                for x in 0..frame.0 {
                    if owner(sx + x) != tx || owner(sy + y) != ty {
                        continue;
                    }
                    let src = (((at.1 + y) * width + at.0 + x) * 4) as usize;
                    let dst = ((y * frame.0 + x) * 4) as usize;
                    out[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
                }
            }
        }
    }
    out
}

struct Sheet {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn decode(path: &Path) -> Sheet {
    let mut decoder = png::Decoder::new(std::fs::File::open(path).expect("open sheet"));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("read sheet header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode sheet");
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let (width, height) = (info.width as usize, info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..width * height * 4].to_vec(),
        png::ColorType::Rgb => buf[..width * height * 3]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("unexpected sheet colour type {other:?}"),
    };
    Sheet {
        width,
        height,
        rgba,
    }
}

/// The differing pixel count and largest channel difference.
fn diff(actual: &[u8], sheet: &Sheet, origin: (usize, usize), frame: (i32, i32)) -> (usize, u8) {
    let (fw, fh) = (frame.0 as usize, frame.1 as usize);
    let mut count = 0;
    let mut max = 0u8;
    for y in 0..fh {
        for x in 0..fw {
            let a = &actual[(y * fw + x) * 4..][..4];
            let e = &sheet.rgba[((origin.1 + y) * sheet.width + origin.0 + x) * 4..][..4];
            let d = (0..4).map(|i| a[i].abs_diff(e[i])).max().unwrap();
            if d > 0 {
                count += 1;
                max = max.max(d);
            }
        }
    }
    (count, max)
}

fn write_png(path: &Path, frame: (i32, i32), rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, frame.0 as u32, frame.1 as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
}

#[test]
fn paint_v1_matches_chromium_147() {
    check("svg-paint-v1", KNOWN_GAPS_V1);
}

#[test]
fn paint_v2_matches_chromium_147() {
    check("svg-paint-v2", KNOWN_GAPS_V2);
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
        let sheet = decode(&dir.join(sheet_meta["png"].as_str().unwrap()));
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
            let actual = render_tiled(case, scale, frame, origin);
            let (count, max) = diff(&actual, &sheet, origin, frame);
            if count > 0 {
                if let Some(dir) = &dump {
                    std::fs::create_dir_all(dir).unwrap();
                    let name = format!("{set}-{scale}x-{}.png", id.replace('/', "-"));
                    write_png(&dir.join(name), frame, &actual);
                }
                results.insert(format!("{scale}x {id}"), (count, max));
            }
        }
    }

    let mut listed = BTreeMap::new();
    for line in known_gaps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.len(), 4, "malformed gap line: {line}");
        let key = format!("{} {}", parts[0], parts[1]);
        let stats = (parts[2].parse().unwrap(), parts[3].parse().unwrap());
        assert!(listed.insert(key, stats).is_none(), "duplicate gap: {line}");
    }

    let exact = total - results.len();
    eprintln!("{set}: {exact} of {total} captures exact");
    let mut problems = Vec::new();
    for (key, (count, max)) in &results {
        match listed.get(key) {
            None => problems.push(format!("new gap: {key} {count} {max}")),
            Some(stats) if stats != &(*count, *max) => problems.push(format!(
                "gap changed: {key} listed {} {}, now {count} {max}",
                stats.0, stats.1
            )),
            _ => {}
        }
    }
    let differing: BTreeSet<_> = results.keys().collect();
    for key in listed.keys() {
        if !differing.contains(key) {
            problems.push(format!("listed gap is now exact: {key}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
