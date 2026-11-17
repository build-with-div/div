//! Content and backgrounds under rounded clips against pinned Chromium 147.
//!
//! `tests/fidelity/fixtures/rounded-clip-v1/pages.txt` describes five
//! pages: a circular overflow clip around a green block that overflows it,
//! the same clip around a green `<img>` (css_backgrounds/border-radius-clip-001,
//! `#bg` and `#img-contain`), a rounded overflow clip inside a solid
//! border, a background color clipped to a rounded padding box
//! (background-clip-padding-box-with-border-radius) and a solid gradient
//! clipped to a rounded content box (background-rounded-image-clip-001).
//! The fixture holds Chromium's frames at device scales 1, 1.25, 1.5
//! and 2, captured through the census oracle (headless Chromium at real zoom
//! above scale 1, RFC 0010); regenerate them with
//! `tools/qualification/capture_rounded_clip_oracle.py`.
//!
//! This test builds the same pages through the public API with the default
//! raster configuration and compares each frame pixel for pixel. A frame
//! that is not exact is listed in `GAPS` with its own pixel count, so any
//! change, better or worse, fails.
#![cfg(all(target_os = "linux", feature = "testing"))]

use div::typed_style::{
    parse_literal, BackgroundClip, BackgroundLayer, Color, CssImage, CssLinearGradient,
    GradientColorSpace, GradientStop, GradientStopPosition, RendererStyleValue, StyleColor,
    StyleProperty, StyleValue,
};
use div::{Document, Element, EngineOptions, RasterConfiguration, ViewportMetrics};

const FIXTURE: &str = "../../tests/fidelity/fixtures/rounded-clip-v1";
const SPEC: &str = include_str!("../../../tests/fidelity/fixtures/rounded-clip-v1/pages.txt");
const MANIFEST: &str =
    include_str!("../../../tests/fidelity/fixtures/rounded-clip-v1/manifest.json");
const IMAGE: &[u8] =
    include_bytes!("../../../tests/fidelity/fixtures/rounded-clip-v1/green-100.png");
const WIDTH: f64 = 160.0;
const HEIGHT: f64 = 140.0;
const SCALES: [&str; 4] = ["1", "1.25", "1.5", "2"];

// Frames that are not exact: (page, scale, differing pixels).
//
// - `rrect` at 1.25x: all 23 pixels lie in the bottom-left corner, where
//   Div's black border ring already differs from Chromium's on its outer
//   curve (by up to 7). Owner: fidelity (border raster at 1.25x).
// - `padding-clip` at 1.25x: the padding box's edges at CSS x 30 and 130
//   fall on device 37.5 and 162.5. Chromium ends them on whole device
//   pixels; Div lays out in CSS px and anti-aliases them. Owner: layout in
//   device pixels (engine, RFC 0011).
// - `gradient-clip` at 1.25x: green 128 against 129 on the clip's curve,
//   where Chromium's dithered gradient blend rounds differently at this
//   scale. Owner: fidelity (gradient dither at fractional scales).
const GAPS: &[(&str, &str, usize)] = &[
    ("rrect", "1.25", 23),
    ("padding-clip", "1.25", 457),
    ("gradient-clip", "1.25", 36),
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

// The page's one background image, `linear-gradient(green, green)`, as the
// computed layer Div's CSS parser produces for it, clipped like the page's
// `background-clip`.
fn green_gradient_layer(clip: BackgroundClip) -> StyleValue {
    let green = || GradientStop {
        color: StyleColor::Resolved(Color::from_rgba8(0, 128, 0, 255)),
        position: GradientStopPosition::Auto,
    };
    let mut layer = BackgroundLayer::new(CssImage::LinearGradient(CssLinearGradient {
        angle_degrees: 180.0,
        corner_direction: None,
        repeating: false,
        color_space: GradientColorSpace::Srgb,
        stops: vec![green(), green()],
    }));
    layer.clip = clip;
    StyleValue::Renderer(RendererStyleValue::BackgroundLayers(vec![layer]))
}

fn style(element: &Element, declarations: &str) {
    let declarations: Vec<(&str, &str)> = declarations
        .split(';')
        .filter(|d| !d.trim().is_empty())
        .map(|declaration| {
            let (name, value) = declaration.split_once(':').unwrap();
            (name.trim(), value.trim())
        })
        .collect();
    for &(name, value) in &declarations {
        if name == "background-image" {
            assert_eq!(value, "linear-gradient(green, green)");
            let clip = match declarations
                .iter()
                .find(|(name, _)| *name == "background-clip")
            {
                Some((_, "content-box")) => BackgroundClip::ContentBox,
                Some((_, "padding-box")) => BackgroundClip::PaddingBox,
                _ => BackgroundClip::BorderBox,
            };
            element
                .set_property(StyleProperty::BackgroundLayers, green_gradient_layer(clip))
                .unwrap();
            continue;
        }
        if name == "background-clip" {
            let clip = match value {
                "content-box" => BackgroundClip::ContentBox,
                "padding-box" => BackgroundClip::PaddingBox,
                _ => panic!("background-clip: {value}"),
            };
            element
                .set_property(
                    StyleProperty::BackgroundClip,
                    StyleValue::Renderer(RendererStyleValue::BackgroundClip(clip)),
                )
                .unwrap();
            continue;
        }
        // A corner radius is set per axis, in px.
        if name.starts_with("border-") && name.ends_with("-radius") {
            for axis in ["x", "y"] {
                let property = StyleProperty::from_css_name(&format!("{name}-{axis}")).unwrap();
                element
                    .set_property(property, parse_literal(property, value).unwrap())
                    .unwrap();
            }
            continue;
        }
        // Border widths are integer CSS px in Div's typed literals.
        let value = if name.starts_with("border-") && name.ends_with("-width") {
            value.trim_end_matches("px")
        } else {
            value
        };
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
    // propagates to the viewport, so nothing adds a scrollbar.
    style(
        &body,
        "margin: 0; overflow: hidden; background-color: white",
    );
    // An img box shows the fixture's green-100.png, registered once.
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
    let image_sha256 = manifest["images"]["green-100.png"].as_str().unwrap();
    let image = document
        .register_image_resource("green-100.png", "image/png", image_sha256, IMAGE.to_vec())
        .unwrap();
    // `open[n]` is the page's latest box at depth n.
    let mut open: Vec<Element> = Vec::new();
    for (depth, tag, declarations) in boxes {
        let element = Element::create(&document, tag).unwrap();
        if *tag == "img" {
            element
                .set_image_resource(image, Some((100.0, 100.0)))
                .unwrap();
        }
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
fn rounded_clip_content_matches_pinned_chromium() {
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
        "content under rounded clips changed against pinned Chromium: (page, scale, differing pixels)"
    );
}
