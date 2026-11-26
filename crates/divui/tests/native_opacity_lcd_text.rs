//! Text inside CSS opacity groups against pinned Chromium 147 on Linux.
//!
//! cc decides once per layer whether its opacity groups keep LCD text
//! (`PaintOpBuffer::Playback`, 147.0.7727.50): when the layer's raster can
//! use LCD text, its display list draws text and it holds no `SaveLayerOp`
//! (the op Blink emits for a filter, a blend mode or a mask), every
//! `SaveLayerAlphaOp` opens with `kPreserveLCDText | kInitWithPrevious`. The
//! group then starts as a copy of the page below it, so its text stays LCD.
//!
//! `tests/fidelity/fixtures/opacity-lcd-text-v1/pages.txt` describes four
//! pages: text in a 50% group over the page, a disabled-button-like box, the
//! first page plus a filtered box elsewhere in the layer (not eligible, so
//! Chromium's text is grayscale), and the text without a group. The fixture
//! holds Chromium's frames at device scales 1, 1.25, 1.5 and 2, captured
//! through the census oracle with the real-font Fontconfig profile (real zoom
//! above scale 1, RFC 0010); regenerate them with
//! `tools/qualification/capture_opacity_lcd_oracle.py`.
//!
//! This test builds the same pages through the public API with
//! `RasterConfiguration::chromium_linux_lcd` and compares each frame pixel
//! for pixel. A frame that is not exact is listed in `GAPS` with its own
//! pixel count, so any change, better or worse, fails.
#![cfg(all(target_os = "linux", feature = "testing"))]

use div::typed_style::{parse_literal, StyleProperty};
use div::{
    Document, Element, EngineOptions, FontAxisRange, FontFaceDescriptor, FontStyleRange,
    RasterConfiguration, ViewportMetrics,
};

const FIXTURE: &str = "../../tests/fidelity/fixtures/opacity-lcd-text-v1";
const SPEC: &str = include_str!("../../../tests/fidelity/fixtures/opacity-lcd-text-v1/pages.txt");
const INTER: &[u8] =
    include_bytes!("../../../tests/fidelity/components/assets/fonts/inter/InterVariable.woff2");
const WIDTH: f64 = 240.0;
const HEIGHT: f64 = 80.0;
const SCALES: [&str; 4] = ["1", "1.25", "1.5", "2"];

// Frames that are not exact: (page, scale, differing pixels).
//
// - Above scale 1, every page with text: real-zoom Chromium lays the text
//   out in device pixels, so its rows land one device pixel away from
//   Div's, which lays out in CSS px (`no-group` shows the same counts
//   without any group). The colors match: each frame's LCD-colored pixel
//   count equals Chromium's. Owner: engine (device-pixel layout, RFC 0011).
// - `filter-elsewhere`, every scale: both draw the grouped text in
//   grayscale, as cc decides, but Chromium's grayscale glyph raster for this
//   text differs from Div's. Every differing pixel is in the text rows; the
//   blurred box is exact at every scale since Skia is compiled with
//   Chromium's `SK_AVOID_SLOW_RASTER_PIPELINE_BLURS` (it was 700, 880 and
//   1,100 pixels at 1x, 1.25x and 1.5x without it). Owner: fidelity
//   (grayscale raster of LCD-configured text).
const GAPS: &[(&str, &str, usize)] = &[
    ("on-page", "1.25", 1545),
    ("on-page", "1.5", 1906),
    ("on-page", "2", 2690),
    ("on-box", "1.25", 722),
    ("on-box", "1.5", 906),
    ("on-box", "2", 1170),
    ("filter-elsewhere", "1", 1271),
    ("filter-elsewhere", "1.25", 2021),
    ("filter-elsewhere", "1.5", 2490),
    ("filter-elsewhere", "2", 3538),
    ("no-group", "1.25", 1545),
    ("no-group", "1.5", 1906),
    ("no-group", "2", 2690),
];

// (depth, tag, declarations, text) of each box, page by page in spec order.
type Boxes = Vec<(usize, &'static str, &'static str, &'static str)>;

fn pages() -> Vec<(&'static str, Boxes)> {
    let mut result: Vec<(&str, Boxes)> = Vec::new();
    for line in SPEC.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(5, '|').map(str::trim).collect();
        let entry = (parts[1].parse().unwrap(), parts[2], parts[3], parts[4]);
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
        // The public API sets the supported filter functions one by one.
        if name == "filter" {
            let radius = value
                .strip_prefix("blur(")
                .and_then(|rest| rest.strip_suffix("px)"))
                .unwrap_or_else(|| panic!("unsupported filter {value}"));
            element.set_filter_blur(radius.parse().unwrap()).unwrap();
            continue;
        }
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
            raster_configuration: RasterConfiguration::chromium_linux_lcd(),
        },
    )
    .unwrap();
    let mut descriptor = FontFaceDescriptor::new("Inter");
    descriptor.style = FontStyleRange::Normal;
    descriptor.weight = FontAxisRange::new(100.0, 900.0);
    document
        .register_font_face(INTER.to_vec(), descriptor)
        .unwrap();
    let body = document.body();
    // The capture's `html, body` rule; body overflow propagates to the
    // viewport, so nothing adds a scrollbar.
    style(
        &body,
        "margin: 0; overflow: hidden; background-color: white; color: black; \
         font-family: 'Inter'; font-size: 16px; line-height: 24px; white-space: pre",
    );
    // `open[n]` is the page's latest box at depth n.
    let mut open: Vec<Element> = Vec::new();
    for (depth, tag, declarations, text) in boxes {
        let element = Element::create(&document, tag).unwrap();
        style(&element, declarations);
        if !text.is_empty() {
            element.set_text(text).unwrap();
        }
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

// Pixels whose channels differ by more than 8: LCD text's color fringes.
fn colored(pixels: &[[u8; 3]]) -> usize {
    pixels
        .iter()
        .filter(|p| p.iter().max().unwrap() - p.iter().min().unwrap() > 8)
        .count()
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
fn opacity_group_text_matches_pinned_chromium() {
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
            println!(
                "{page} {scale}x: {differing} differing pixels; colored: Chromium {}, Div {}",
                colored(&expected),
                colored(&actual)
            );
            if differing != 0 {
                measured.push((page, scale, differing));
            }
        }
    }
    assert_eq!(
        measured, GAPS,
        "text in opacity groups changed against pinned Chromium: (page, scale, differing pixels)"
    );
}
