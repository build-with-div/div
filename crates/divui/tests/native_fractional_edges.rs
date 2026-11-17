//! Box edges at fractional device positions against pinned Chromium 147.
//!
//! `tests/fidelity/fixtures/fractional-edges-v1/pages.txt` describes five
//! pages: a single box, two abutting boxes, a block split across two
//! columns, a table caption split across two columns and an absolutely
//! positioned box whose child overflows into the next column. Each has an
//! edge at CSS x 45 and a top edge at CSS y 21. The
//! fixture holds Chromium's frames at device scales 1, 1.25, 1.5 and 2,
//! captured through the census oracle (headless Chromium at real zoom above
//! scale 1, RFC 0010); regenerate them with
//! `tools/qualification/capture_fractional_edges_oracle.py`.
//!
//! In those frames Chromium lays out in device pixels, so every box edge
//! lands on a whole device pixel: no edge is anti-aliased and abutting boxes
//! leave no seam. This test builds the same pages through the
//! public API with the default raster configuration (the census's
//! configuration for these rows) and compares each frame pixel for pixel. A
//! frame that is not exact is listed in `GAPS` with its own pixel count, so
//! any change, better or worse, fails.
#![cfg(all(target_os = "linux", feature = "testing"))]

use div::typed_style::{parse_literal, StyleProperty};
use div::{Document, Element, EngineOptions, RasterConfiguration, ViewportMetrics};

const FIXTURE: &str = "../../tests/fidelity/fixtures/fractional-edges-v1";
const SPEC: &str = include_str!("../../../tests/fidelity/fixtures/fractional-edges-v1/pages.txt");
const WIDTH: f64 = 120.0;
const HEIGHT: f64 = 80.0;
const SCALES: [&str; 4] = ["1", "1.25", "1.5", "2"];

// Frames that are not exact: (page, scale, differing pixels).
//
// Every page at 1.25 and 1.5. The edges at CSS x 45 and y 21 fall on device
// 56.25 and 26.25 at 1.25x (67.5 and 31.5 at 1.5x). Chromium ends and
// starts them on whole device pixels (56 and 26; 68 and 32). Div lays out in
// CSS px and rasterizes the fractional edge with partial coverage, and in
// `single`, `abutting` and `multicol` every differing pixel is one of those
// anti-aliased edge pixels. In `caption` and `overflow`, 31 (1.25x) and 37
// (1.5x) more pixels are fully painted in Div and white in Chromium: Div
// draws the box whole under a fragmentainer clip snapped outward to device
// pixels. Owner: layout in device pixels and edge snapping (engine,
// RFC 0011).
const GAPS: &[(&str, &str, usize)] = &[
    ("single", "1.25", 113),
    ("single", "1.5", 135),
    ("abutting", "1.25", 224),
    ("abutting", "1.5", 209),
    ("multicol", "1.25", 224),
    ("multicol", "1.5", 209),
    ("caption", "1.25", 169),
    ("caption", "1.5", 172),
    ("overflow", "1.25", 194),
    ("overflow", "1.5", 172),
];

// (depth, tag, declarations) of each box, page by page in spec order.
type Boxes = Vec<(usize, &'static str, &'static str)>;

fn pages() -> Vec<(&'static str, Boxes)> {
    let mut result: Vec<(&str, Boxes)> = Vec::new();
    for line in SPEC.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, '|').map(str::trim).collect();
        let entry = (parts[1].parse().unwrap(), parts[2], parts[3]);
        match result.iter_mut().find(|(name, _)| *name == parts[0]) {
            Some((_, boxes)) => boxes.push(entry),
            None => result.push((parts[0], vec![entry])),
        }
    }
    result
}

fn style(element: &Element, declarations: &str) {
    for declaration in declarations.split(';').filter(|d| !d.trim().is_empty()) {
        let (name, value) = declaration.split_once(':').unwrap();
        let (name, value) = (name.trim(), value.trim());
        let property = StyleProperty::from_css_name(name).unwrap();
        element
            .set_property(
                property,
                parse_literal(property, value).unwrap_or_else(|error| panic!("{name}: {error}")),
            )
            .unwrap();
    }
}

// Div's frame as RGB, composited over white like a page's canvas: the
// bitmap is premultiplied RGBA.
fn div_frame(boxes: &Boxes, scale: f64) -> (usize, usize, Vec<[u8; 3]>) {
    let document = Document::with_options(
        ViewportMetrics::from_logical_size(WIDTH, HEIGHT, scale).unwrap(),
        EngineOptions {
            raster_configuration: RasterConfiguration::default(),
        },
    )
    .unwrap();
    let body = document.body();
    // The capture's `html, body { overflow: hidden }`: body overflow
    // propagates to the viewport, so overflowing columns add no scrollbar.
    style(
        &body,
        "margin: 0; overflow: hidden; background-color: white",
    );
    // `open[n]` is the page's latest box at depth n.
    let mut open: Vec<Element> = Vec::new();
    for (depth, tag, declarations) in boxes {
        let element = Element::create(&document, tag).unwrap();
        style(&element, declarations);
        open.truncate(*depth);
        open.last().unwrap_or(&body).append_child(&element).unwrap();
        open.push(element);
    }
    let bitmap = document.render_to_bitmap().unwrap();
    let (width, height) = (bitmap.width() as usize, bitmap.height() as usize);
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        let row = &bitmap.pixels()[y * bitmap.stride()..];
        for x in 0..width {
            let p = &row[x * 4..x * 4 + 4];
            let over_white = |c: u8| (c as u16 + 255 - p[3] as u16).min(255) as u8;
            pixels.push([over_white(p[0]), over_white(p[1]), over_white(p[2])]);
        }
    }
    (width, height, pixels)
}

fn chromium_frame(page: &str, scale: &str) -> (usize, usize, Vec<[u8; 3]>) {
    let path = format!(
        "{}/{FIXTURE}/{page}-{scale}.png",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut decoder =
        png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    let channels = info.color_type.samples();
    let pixels = buffer[..info.buffer_size()]
        .chunks(channels)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    (info.width as usize, info.height as usize, pixels)
}

// With `PIXEL_OUT` set to a directory, the test writes Div's frames there.
fn write_png(path: &std::path::Path, width: usize, height: usize, pixels: &[[u8; 3]]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = pixels.iter().flatten().copied().collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
}

#[test]
fn fractional_box_edges_match_pinned_chromium() {
    let mut measured = Vec::new();
    for (page, boxes) in pages() {
        for scale in SCALES {
            let (width, height, expected) = chromium_frame(page, scale);
            let (div_width, div_height, actual) = div_frame(&boxes, scale.parse().unwrap());
            assert_eq!(
                (div_width, div_height),
                (width, height),
                "{page} frame size at {scale}x"
            );
            if let Some(directory) = std::env::var_os("PIXEL_OUT") {
                write_png(
                    &std::path::Path::new(&directory).join(format!("div-{page}-{scale}.png")),
                    width,
                    height,
                    &actual,
                );
            }
            let differing = expected
                .iter()
                .zip(&actual)
                .filter(|(expected, actual)| expected != actual)
                .count();
            println!("{page} {scale}x: {differing} differing pixels");
            if differing != 0 {
                measured.push((page, scale, differing));
            }
        }
    }
    assert_eq!(
        measured, GAPS,
        "fractional box edges changed against pinned Chromium: (page, scale, differing pixels)"
    );
}
