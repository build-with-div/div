//! `ShapeResult::to_positioned_text_blob`: glyphs at caller-given positions
//! rasterize exactly as the advance-positioned blob does.

use div_text::font::{Font, FontDescription};
use div_text::shaping::{ShapeResult, TextDirection, TextRasterPolicy, TextShaper};
use skia_safe::{
    surfaces, Color, ImageInfo, Paint, PixelGeometry, SurfaceProps, SurfacePropsFlags, TextBlob,
};

const POLICIES: [TextRasterPolicy; 6] = [
    TextRasterPolicy::Skia,
    TextRasterPolicy::ChromiumNativeControl,
    TextRasterPolicy::ChromiumEmbeddedDocument,
    TextRasterPolicy::ChromiumAuthorLcd,
    TextRasterPolicy::ChromiumAliased,
    TextRasterPolicy::ChromiumAuthorAliased,
];

fn shape(text: &str, size: f32) -> ShapeResult {
    let mut description = FontDescription::new();
    description.size = size;
    description.specified_size = size;
    TextShaper::new().shape(text, &Font::new(description), TextDirection::Ltr)
}

/// Each glyph's origin as the advance-positioned blob places it: the
/// accumulated advance plus the glyph offset.
fn advance_placements(result: &ShapeResult) -> Vec<(f32, f32)> {
    let mut placements = Vec::new();
    let mut x = 0.0;
    for run in &result.runs {
        for glyph in 0..run.num_glyphs {
            placements.push((x + run.offsets[glyph].0, run.offsets[glyph].1));
            x += run.advances[glyph];
        }
    }
    placements
}

/// The blob drawn on an LCD (RGB horizontal) surface, so that subpixel
/// policies paint their subpixel masks.
fn pixels(blob: &TextBlob) -> Vec<u8> {
    let props = SurfaceProps::new(SurfacePropsFlags::default(), PixelGeometry::RGBH);
    let info = ImageInfo::new_n32_premul((400, 80), None);
    let mut surface = surfaces::raster(&info, None, Some(&props)).expect("LCD surface");
    let canvas = surface.canvas();
    canvas.clear(Color::WHITE);
    let mut paint = Paint::default();
    paint.set_color(Color::BLACK).set_anti_alias(true);
    canvas.draw_text_blob(blob, (7.3, 40.0), &paint);
    let mut out = vec![0u8; 400 * 80 * 4];
    assert!(surface.read_pixels(&info, &mut out, 400 * 4, (0, 0)));
    out
}

#[test]
fn placements_equal_to_the_advances_give_the_advance_blob() {
    for size in [10.0, 13.33, 16.0] {
        let result = shape("Hello, world", size);
        let placements = advance_placements(&result);
        for policy in POLICIES {
            for scale in [1.0, 2.0] {
                for origin in [None, Some(7.3)] {
                    let expected = result
                        .to_text_blob_with_raster_policy_at_scale(origin, policy, scale)
                        .unwrap();
                    let actual = result
                        .to_positioned_text_blob(
                            0..result.num_characters,
                            &placements,
                            policy,
                            scale,
                            origin,
                        )
                        .unwrap();
                    assert_eq!(
                        expected.bounds(),
                        actual.bounds(),
                        "{size}px {policy:?} at {scale}x, origin {origin:?}"
                    );
                    assert!(
                        pixels(&expected) == pixels(&actual),
                        "{size}px {policy:?} at {scale}x, origin {origin:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_character_range_takes_only_its_glyphs() {
    let result = shape("Hello, world", 16.0);
    // "world" on its own, positioned from 0, equals the sub-range's blob.
    let sub = result.sub_range(7, 12);
    let placements = advance_placements(&sub);
    for policy in POLICIES {
        let expected = sub.to_text_blob_with_raster_policy(None, policy).unwrap();
        let actual = result
            .to_positioned_text_blob(7..12, &placements, policy, 1.0, None)
            .unwrap();
        assert!(pixels(&expected) == pixels(&actual), "{policy:?}");
    }
}

#[test]
fn moved_glyphs_paint_where_they_are_placed() {
    let result = shape("AB", 16.0);
    let placements = advance_placements(&result);
    let moved: Vec<_> = placements
        .iter()
        .map(|&(x, y)| (x + 40.0, y + 9.0))
        .collect();
    let blob = result
        .to_positioned_text_blob(0..2, &moved, TextRasterPolicy::Skia, 1.0, None)
        .unwrap();
    let reference = result
        .to_positioned_text_blob(0..2, &placements, TextRasterPolicy::Skia, 1.0, None)
        .unwrap();
    let (a, b) = (blob.bounds(), reference.bounds());
    assert_eq!((a.left - b.left, a.top - b.top), (40.0, 9.0));
}

#[test]
fn ranges_and_placements_that_do_not_fit_give_none() {
    let result = shape("Hello", 16.0);
    let placements = advance_placements(&result);
    let policy = TextRasterPolicy::Skia;
    assert!(result
        .to_positioned_text_blob(2..2, &[], policy, 1.0, None)
        .is_none());
    assert!(result
        .to_positioned_text_blob(0..5, &placements[..4], policy, 1.0, None)
        .is_none());
    assert!(result
        .to_positioned_text_blob(0..2, &placements, policy, 1.0, None)
        .is_none());

    // A base and its combining mark form one cluster: a range ending
    // between them splits it.
    let marked = shape("e\u{301}x", 16.0);
    let run = &marked.runs[0];
    assert!(
        run.clusters.iter().all(|&cluster| cluster != 1),
        "the mark joins its base's cluster"
    );
    let all = advance_placements(&marked);
    assert!(marked
        .to_positioned_text_blob(0..1, &all[..1], policy, 1.0, None)
        .is_none());
    let whole = run.clusters.iter().filter(|&&cluster| cluster < 2).count();
    assert!(marked
        .to_positioned_text_blob(0..2, &all[..whole], policy, 1.0, None)
        .is_some());
}
