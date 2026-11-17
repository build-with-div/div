//! Web-font text against pinned Chromium 147 on Linux.
//!
//! Chromium on Linux rasterizes every `@font-face` typeface with Skia's
//! Fontations (skrifa) scaler. The lines in
//! `tests/fidelity/fixtures/webfont-fontations-v1/lines.txt` cover a
//! monospaced font (Geist Mono), a proportional one (Inter, whose optical
//! size axis follows the font size) and a non-default variable instance
//! (Geist at weight 650), at 13px and 16px. The fixture holds Chromium's
//! frames at device scales 1, 1.25, 1.5 and 2; regenerate them with
//! `tools/qualification/capture_webfont_oracle.py`.
//!
//! This test registers the same fonts through the public API, builds the
//! same page with `RasterConfiguration::chromium_linux_lcd` (the census
//! harness's real-font Fontconfig profile) and compares each 24px row with
//! Chromium's, pixel for pixel. A row that is not exact is listed in
//! `GAPS` with its own pixel count, so any change, better or worse, fails.
#![cfg(all(target_os = "linux", feature = "testing"))]

use div::typed_style::{parse_literal, StyleProperty};
use div::{
    Document, Element, EngineOptions, FontAxisRange, FontFaceDescriptor, FontStyleRange,
    RasterConfiguration, ViewportMetrics,
};

const FIXTURE: &str = "../../tests/fidelity/fixtures/webfont-fontations-v1";
const SPEC: &str = include_str!("../../../tests/fidelity/fixtures/webfont-fontations-v1/lines.txt");
const GEIST_MONO: &[u8] = include_bytes!(
    "../../../tests/fidelity/components/assets/fonts/geist/GeistMono-Variable.woff2"
);
const GEIST: &[u8] =
    include_bytes!("../../../tests/fidelity/components/assets/fonts/geist/Geist-Variable.woff2");
const INTER: &[u8] =
    include_bytes!("../../../tests/fidelity/components/assets/fonts/inter/InterVariable.woff2");

const WIDTH: f64 = 400.0;
const ROW: f64 = 24.0;
const LEFT: f64 = 4.0;
const SCALES: [&str; 4] = ["1", "1.25", "1.5", "2"];

// Rows that are not exact: (scale, row index, differing pixels).
//
// The frames above scale 1 are real zoom (`--force-device-scale-factor`,
// RFC 0010): Blink lays the line boxes out in device pixels. Div lays out
// in CSS px and replays paint at the scale, so its baselines can land one
// device pixel away from Chromium's. In 11 of the 14 rows the glyph raster
// is identical and the whole row is offset by one device pixel: Div's ink
// is 1 device px higher (Geist Mono and Geist 650) or 1 device px lower
// (Inter 16px), and shifting Div's row by that pixel leaves 0 differing
// pixels. Owner: line-box layout in device pixels (engine, RFC 0011).
//
// Row 2, Inter 13px at 1.25, 1.5 and 2, was not exact under the emulated
// oracle either: Div's first ink row is faint, as from a baseline at a
// fractional device y, while Chromium's sits on a whole device pixel. A
// one-pixel shift still leaves 1,339, 2,423 and 4,153 pixels. Chromium
// uses `opsz` 14 (13px clamped to Inter's minimum) at every scale; Div gives
// the same counts with `'opsz' 14` set explicitly, so optical sizing is not
// the cause.
const GAPS: &[(&str, usize, usize)] = &[
    ("1.25", 1, 3125),
    ("1.25", 2, 2729),
    ("1.25", 3, 3596),
    ("1.25", 5, 3710),
    ("1.5", 0, 3058),
    ("1.5", 2, 3503),
    ("1.5", 3, 4405),
    ("1.5", 4, 3648),
    ("2", 0, 4122),
    ("2", 1, 5296),
    ("2", 2, 4263),
    ("2", 3, 6315),
    ("2", 4, 4804),
    ("2", 5, 6083),
];

struct Line {
    font: &'static str,
    family: &'static str,
    weight: &'static str,
    size: &'static str,
    text: &'static str,
}

fn lines() -> Vec<Line> {
    SPEC.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let parts: Vec<&str> = line.splitn(5, '|').map(str::trim).collect();
            Line {
                font: parts[0],
                family: parts[1],
                weight: parts[2],
                size: parts[3],
                text: parts[4],
            }
        })
        .collect()
}

fn font_bytes(font: &str) -> &'static [u8] {
    match font {
        "geist/GeistMono-Variable.woff2" => GEIST_MONO,
        "geist/Geist-Variable.woff2" => GEIST,
        "inter/InterVariable.woff2" => INTER,
        other => panic!("{other} is not one of the fixture's fonts"),
    }
}

fn style(element: &Element, name: &str, value: &str) {
    let property = StyleProperty::from_css_name(name).unwrap();
    element
        .set_property(
            property,
            parse_literal(property, value).unwrap_or_else(|error| panic!("{name}: {error}")),
        )
        .unwrap();
}

// Div's frame as RGB, composited over white like a page's canvas: the
// bitmap is premultiplied RGBA.
fn div_frame(items: &[Line], scale: f64) -> (usize, usize, Vec<[u8; 3]>) {
    let height = items.len() as f64 * ROW;
    let document = Document::with_options(
        ViewportMetrics::from_logical_size(WIDTH, height, scale).unwrap(),
        EngineOptions {
            raster_configuration: RasterConfiguration::chromium_linux_lcd(),
        },
    )
    .unwrap();
    let mut registered = Vec::new();
    for item in items {
        if registered.contains(&(item.family, item.font)) {
            continue;
        }
        registered.push((item.family, item.font));
        let mut descriptor = FontFaceDescriptor::new(item.family);
        descriptor.style = FontStyleRange::Normal;
        descriptor.weight = FontAxisRange::new(100.0, 900.0);
        document
            .register_font_face(font_bytes(item.font), descriptor)
            .unwrap();
    }
    let body = document.body();
    style(&body, "margin", "0");
    style(&body, "background-color", "white");
    style(&body, "color", "black");
    for (index, item) in items.iter().enumerate() {
        let row = Element::create(&document, "div").unwrap();
        style(&row, "position", "absolute");
        style(&row, "left", &format!("{LEFT}px"));
        style(&row, "top", &format!("{}px", index as f64 * ROW));
        style(&row, "font-family", &format!("'{}'", item.family));
        style(&row, "font-weight", item.weight);
        style(&row, "font-size", &format!("{}px", item.size));
        style(&row, "line-height", &format!("{ROW}px"));
        style(&row, "white-space", "pre");
        row.set_text(item.text).unwrap();
        body.append_child(&row).unwrap();
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

fn chromium_frame(scale: &str) -> (usize, usize, Vec<[u8; 3]>) {
    let path = format!(
        "{}/{FIXTURE}/chromium-linux-{scale}.png",
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
fn web_font_rows_match_pinned_chromium() {
    let items = lines();
    let mut measured = Vec::new();
    for scale in SCALES {
        let factor: f64 = scale.parse().unwrap();
        let (width, height, expected) = chromium_frame(scale);
        let (div_width, div_height, actual) = div_frame(&items, factor);
        assert_eq!(
            (div_width, div_height),
            (width, height),
            "frame size at {scale}x"
        );
        if let Some(directory) = std::env::var_os("PIXEL_OUT") {
            write_png(
                &std::path::Path::new(&directory).join(format!("div-{scale}.png")),
                width,
                height,
                &actual,
            );
        }
        for (index, item) in items.iter().enumerate() {
            let top = (index as f64 * ROW * factor).round() as usize;
            let bottom = (((index + 1) as f64 * ROW * factor).round() as usize).min(height);
            let differing = (top * width..bottom * width)
                .filter(|&offset| expected[offset] != actual[offset])
                .count();
            println!(
                "{scale}x {} {} {}px: {differing} differing pixels",
                item.family, item.weight, item.size
            );
            if differing != 0 {
                measured.push((scale, index, differing));
            }
        }
    }
    assert_eq!(
        measured, GAPS,
        "web-font rows changed against pinned Chromium: (scale, row, differing pixels)"
    );
}
