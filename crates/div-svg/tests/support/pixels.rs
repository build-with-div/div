//! Pixel helpers shared by the corpus tests that compare with Chromium's
//! screenshots: the paint properties the corpora set, the outer `svg`'s
//! box at a device scale, Div's tiled rasterization, and the sheet
//! comparison.

use super::{length_context, stroke_style_in, style_map, Element, Viewport};
use div_svg::geometry::FillRule;
use div_svg::paint::{
    clip_viewport, root_transform, PaintOrder, RootBox, ShapeRendering, SvgPaint, SvgPaintStyle,
};
use div_svg::transform::Matrix;
use div_svg::viewport::{parse_view_box, view_box_transform, PreserveAspectRatio};
use div_svg::Rect;
use skia_safe::{surfaces, AlphaType, Canvas, Color, Color4f, ColorType, ImageInfo, SurfaceProps};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The page's `color`, which `currentColor` resolves to.
pub const INK: &str = "#09090b";
/// The inherited paint properties the corpora set.
pub const PROPERTIES: [&str; 14] = [
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

/// Inherited properties: presentation attributes, then the `style`
/// attribute, over the parent's values.
pub fn cascade(element: &Element, parent: &BTreeMap<String, String>) -> BTreeMap<String, String> {
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

pub fn color(text: &str, current: &str) -> Color4f {
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

pub fn paint_style(props: &BTreeMap<String, String>, vp: &Viewport) -> SvgPaintStyle {
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

pub fn view_box(element: &Element, width: f32, height: f32) -> Option<Matrix> {
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

/// LayoutUnit rounding: to the nearest 1/64, then half up to an integer.
pub fn snap(v: f64) -> i32 {
    let fixed = (v * 64.0).round() as i64;
    ((fixed + 32) >> 6) as i32
}

/// The outer `svg` of a case, as Div's renderer places it in a tile.
pub struct Root {
    /// The transform from the root's user space to the tile's pixels.
    pub ctm: Matrix,
    /// The root's viewport, for lengths.
    pub viewport: Viewport,
    /// From the root's user space to the host space of
    /// `vector-effect: non-scaling-stroke`: the root's coordinates before
    /// the paint offset and zoom.
    pub to_host: Matrix,
}

/// Chromium 147 rasterizes a layer larger than 512 px in 256 px tiles that
/// overlap by two texels (cc's tiling, which `div-paint`'s `render.rs`
/// reproduces): tile `i` is drawn translated by `−254·i` and supplies the
/// pixels from `254·i + 1` (from 0 for the first) to `254·i + 255`. Stroke
/// outlines and transformed points are computed in single precision in that
/// tile's device space, so the result depends on it.
pub const TILE_SIZE: i32 = 256;
const TILE_STEP: i32 = TILE_SIZE - 2;

/// One raster tile with the case's root `svg` (`root`, `width` × `height`
/// CSS px, moved by `offset`) at `at`, the device position of its slot
/// within the tile: RGBA, `TILE_SIZE` square. `paint` paints its content.
/// `props` are the tile surface's text properties (Div's LCD surfaces).
pub fn render(
    root: &Element,
    size: (f64, f64),
    offset: (f64, f64),
    scale: f64,
    at: (i32, i32),
    props: Option<&SurfaceProps>,
    paint: &dyn Fn(&Canvas, &Root),
) -> Vec<u8> {
    let (width, height) = size;
    let tile = (TILE_SIZE, TILE_SIZE);
    let mut surface = match props {
        Some(props) => surfaces::raster(&ImageInfo::new_n32_premul(tile, None), None, Some(props)),
        None => surfaces::raster_n32_premul(tile),
    }
    .unwrap();
    let canvas = surface.canvas();
    canvas.clear(Color::WHITE);

    // Layout is in device pixels at real zoom: the root's box and its
    // `viewBox` fit are computed at the zoomed size.
    let (zw, zh) = (width * scale, height * scale);
    let (zx, zy) = (
        offset.0 * scale + f64::from(at.0),
        offset.1 * scale + f64::from(at.1),
    );
    let snapped = (
        snap(zx),
        snap(zy),
        snap(zx + zw) - snap(zx),
        snap(zy + zh) - snap(zy),
    );
    let root_box = RootBox {
        snapped,
        size: (zw as f32, zh as f32),
        is_document_element: false,
    };
    if !root_box.renders() {
        return pixels(&mut surface, tile);
    }
    let (local, viewport) = match view_box(root, zw as f32, zh as f32) {
        Some(vbt) => {
            let vb = parse_view_box(&root.attrs["viewBox"]).unwrap();
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
                width: width as f32,
                height: height as f32,
            },
        ),
    };
    let ctm = root_transform(&root_box, &local);
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
    let to_host = Matrix::translate(f64::from(snapped.0), f64::from(snapped.1))
        .then_apply(&Matrix::scale(scale, scale))
        .inverse()
        .unwrap();
    paint(
        canvas,
        &Root {
            ctm,
            viewport,
            to_host,
        },
    );
    canvas.restore();
    pixels(&mut surface, tile)
}

fn pixels(surface: &mut skia_safe::Surface, frame: (i32, i32)) -> Vec<u8> {
    let info = ImageInfo::new(frame, ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut out = vec![0u8; (frame.0 * frame.1 * 4) as usize];
    assert!(surface.read_pixels(&info, &mut out, frame.0 as usize * 4, (0, 0)));
    out
}

/// The `frame` of device pixels at `slot` on the sheet, as Div's renderer
/// produces them: the case is painted once per tile it touches, by
/// `render_at(at)` with its slot's position in that tile, and each pixel is
/// taken from the tile that supplies it.
pub fn render_tiled(
    frame: (i32, i32),
    slot: (usize, usize),
    render_at: &dyn Fn((i32, i32)) -> Vec<u8>,
) -> Vec<u8> {
    let owner = |v: i32| (v - 1).max(0) / TILE_STEP;
    let origin = |i: i32| i * TILE_STEP;
    let (sx, sy) = (slot.0 as i32, slot.1 as i32);
    let mut out = vec![0u8; (frame.0 * frame.1 * 4) as usize];
    for ty in owner(sy)..=owner(sy + frame.1 - 1) {
        for tx in owner(sx)..=owner(sx + frame.0 - 1) {
            let at = (sx - origin(tx), sy - origin(ty));
            let tile = render_at(at);
            for y in 0..frame.1 {
                for x in 0..frame.0 {
                    if owner(sx + x) != tx || owner(sy + y) != ty {
                        continue;
                    }
                    let src = (((at.1 + y) * TILE_SIZE + at.0 + x) * 4) as usize;
                    let dst = ((y * frame.0 + x) * 4) as usize;
                    out[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
                }
            }
        }
    }
    out
}

pub struct Sheet {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub fn decode(path: &Path) -> Sheet {
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
pub fn diff(
    actual: &[u8],
    sheet: &Sheet,
    origin: (usize, usize),
    frame: (i32, i32),
) -> (usize, u8) {
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

pub fn write_png(path: &Path, frame: (i32, i32), rgba: &[u8]) {
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

/// Checks `results` (`"<scale>x <case>"` to differing pixels and largest
/// channel difference) against a gap list of `[<os>] <scale>x <case>
/// <pixels> <max>` lines, where a line without an OS applies to both. Fails
/// on a new difference, a changed one, and a listed case that is exact.
pub fn check_gaps(name: &str, total: usize, results: &BTreeMap<String, (usize, u8)>, gaps: &str) {
    let mut listed = BTreeMap::new();
    for line in gaps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 5 {
            let os = parts.remove(0);
            assert!(["linux", "windows"].contains(&os), "gap OS: {line}");
            if os != std::env::consts::OS {
                continue;
            }
        }
        assert_eq!(parts.len(), 4, "malformed gap line: {line}");
        let key = format!("{} {}", parts[0], parts[1]);
        let stats = (parts[2].parse().unwrap(), parts[3].parse().unwrap());
        assert!(listed.insert(key, stats).is_none(), "duplicate gap: {line}");
    }

    let exact = total - results.len();
    eprintln!("{name}: {exact} of {total} captures exact");
    let mut problems = Vec::new();
    for (key, (count, max)) in results {
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
