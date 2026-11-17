//! Div gives two font sizes one font exactly when pinned Chromium 147 does.
//!
//! A font is made at its device-pixel size truncated to 1/100 px, after
//! `size-adjust` and `font-size-adjust` (`div_text::font::font_cache_size`).
//! A document's font cache finds it by the device size truncated twice
//! (`font_cache_key_size`), so on one page a later size can take the font
//! made for an earlier one.
//!
//! Oracle: `tests/fidelity/fixtures/font-size-precision-v1/chromium-windows.json`,
//! captured by `tools/qualification/capture_font_size_precision_oracle.py`.
//! Chromium shaped two cases with one font when both their SVG text length
//! and their span width are equal. A `page` or `history` run is one document,
//! met in order; each case of an `alone` run is a document of its own.

use std::path::Path;
use std::sync::Arc;

use div_style::{FontFamilyList, FontSizeAdjust};
use div_text::{Font, FontCollection, FontDescription, FontFaceDescriptor};
use serde_json::Value;

const ORACLE: &str =
    include_str!("../../../tests/fidelity/fixtures/font-size-precision-v1/chromium-windows.json");

/// Named gap: on the `font-size-adjust: 0.6` pages, Div keys each size by
/// its unadjusted size, and these pairs share a font in only one of Div and
/// Chromium. The pattern fits a Chromium key taken after the adjustment,
/// which is not confirmed. (font, earlier size on the page, later size)
const FONT_SIZE_ADJUST_GAPS: &[(&str, &str, &str)] = &[
    ("DejaVu Sans", "15.999", "16.011"),
    ("DejaVu Sans", "16", "16.011"),
    ("DejaVu Sans", "16.001", "16.011"),
    ("DejaVu Sans", "16.004", "16.011"),
    ("DejaVu Sans", "16.005", "16.011"),
    ("DejaVu Sans", "16.009", "16.011"),
    ("DejaVu Sans", "16.011", "16.019"),
    ("DejaVu Sans", "25.001", "25.009"),
    ("DejaVu Sans", "25.291", "25.294"),
    ("DejaVu Sans", "25.291", "25.295"),
    ("DejaVu Sans", "25.291", "25.298"),
    ("Geist", "13.001", "13.009"),
    ("Geist", "13.33", "13.339"),
    ("Geist", "13.331", "13.339"),
    ("Geist", "13.334", "13.339"),
    ("Geist", "13.335", "13.339"),
    ("Geist", "15.999", "16.009"),
    ("Geist", "16", "16.009"),
    ("Geist", "16.001", "16.009"),
    ("Geist", "16.004", "16.009"),
    ("Geist", "16.005", "16.009"),
    ("Geist", "16.009", "16.011"),
    ("Geist", "16.009", "16.019"),
    ("Geist", "17.001", "17.009"),
    ("Geist", "25.001", "25.009"),
];

fn oracle() -> Value {
    serde_json::from_str(ORACLE).expect("oracle JSON")
}

fn collection(oracle: &Value, font: &str, size_adjust: Option<f32>) -> Arc<FontCollection> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(oracle["fonts"][font].as_str().expect("font path"));
    let bytes = std::fs::read(&path).expect("font bytes");
    let collection = FontCollection::system();
    let mut descriptor = FontFaceDescriptor::new("Probe");
    descriptor.size_adjust = size_adjust;
    collection
        .register(Arc::from(bytes), descriptor)
        .expect("register the probe face");
    collection
}

/// The size, in CSS pixels, of the font Div makes for `size`.
fn div_font_size(
    collection: &Arc<FontCollection>,
    size: &str,
    scale: f64,
    font_size_adjust: Option<f32>,
) -> f32 {
    let size: f32 = size.parse().expect("size");
    let mut description =
        FontDescription::with_family_and_size(FontFamilyList::single("Probe"), size);
    description.device_scale_factor = scale;
    if let Some(aspect) = font_size_adjust {
        description.size_adjust = FontSizeAdjust::ExHeight(aspect);
    }
    let font = Font::new_in_collection(description, Arc::clone(collection));
    font.primary_font().expect("the probe face").size()
}

fn widths(case: &Value) -> (f64, f64) {
    (
        case["svg"].as_f64().unwrap(),
        case["html"].as_f64().unwrap(),
    )
}

fn size(case: &Value) -> &str {
    case["size"].as_str().unwrap()
}

fn run_key(run: &Value) -> (String, f64, Option<f32>, Option<f32>) {
    (
        run["font"].as_str().unwrap().to_owned(),
        run["scale"].as_f64().unwrap(),
        run["size_adjust"].as_f64().map(|value| value as f32),
        run["font_size_adjust"].as_f64().map(|value| value as f32),
    )
}

#[test]
fn sizes_share_a_font_exactly_when_chromium_shares_one() {
    let oracle = oracle();
    let mut differences = Vec::new();
    let mut gaps_still_differing = Vec::new();
    let mut compared = 0;
    for run in oracle["runs"].as_array().unwrap() {
        let (font, scale, size_adjust, font_size_adjust) = run_key(run);
        let cases = run["cases"].as_array().unwrap();
        let mut document = collection(&oracle, &font, size_adjust);
        let fonts: Vec<f32> = cases
            .iter()
            .map(|case| {
                if run["kind"] == "alone" {
                    document = collection(&oracle, &font, size_adjust);
                }
                div_font_size(&document, size(case), scale, font_size_adjust)
            })
            .collect();
        for i in 0..cases.len() {
            for j in i + 1..cases.len() {
                let chromium = widths(&cases[i]) == widths(&cases[j]);
                let div = fonts[i] == fonts[j];
                compared += 1;
                if chromium == div {
                    continue;
                }
                let pair = (font.as_str(), size(&cases[i]), size(&cases[j]));
                let gap = FONT_SIZE_ADJUST_GAPS.iter().find(|gap| **gap == pair);
                if let (Some(gap), Some(_)) = (gap, font_size_adjust) {
                    gaps_still_differing.push(*gap);
                    continue;
                }
                differences.push(format!(
                    "{} {font} {scale}x size-adjust {size_adjust:?} font-size-adjust \
                     {font_size_adjust:?}: {} and {}: Chromium {}, Div {} ({} and {})",
                    run["kind"].as_str().unwrap(),
                    size(&cases[i]),
                    size(&cases[j]),
                    if chromium { "shares" } else { "separates" },
                    if div { "shares" } else { "separates" },
                    fonts[i],
                    fonts[j],
                ));
            }
        }
    }
    assert!(compared > 4_000_000, "only {compared} comparisons");
    assert!(
        differences.is_empty(),
        "{} of {compared} differ:\n{}",
        differences.len(),
        differences.join("\n")
    );
    for gap in FONT_SIZE_ADJUST_GAPS {
        assert!(
            gaps_still_differing.contains(gap),
            "{gap:?} is listed as a gap but now matches Chromium: remove it"
        );
    }
}

#[test]
fn a_later_size_takes_the_font_of_an_earlier_one_with_its_key() {
    let oracle = oracle();
    // At 2x, 16.001px and 16.009px have the key 32.00. Alone, 16.009px is
    // made at 32.01 device pixels; after 16.001px it uses the 32.00 font.
    let alone = collection(&oracle, "Geist", None);
    assert_eq!(div_font_size(&alone, "16.009", 2.0, None), 16.005);
    let page = collection(&oracle, "Geist", None);
    assert_eq!(div_font_size(&page, "16.001", 2.0, None), 16.0);
    assert_eq!(div_font_size(&page, "16.009", 2.0, None), 16.0);
}
