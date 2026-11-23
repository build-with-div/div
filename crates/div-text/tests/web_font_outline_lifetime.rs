//! The web-font outline cache ends with the last font client.
//!
//! This integration test runs in its own process because the cache and the
//! client count are process-wide. Only Linux draws web fonts through this
//! cache.

#![cfg(target_os = "linux")]

use std::sync::{Arc, Barrier};

use div_geometry::RasterConfiguration;
use div_style::{FontFamily, FontFamilyList};
use div_text::font::{Font, FontCollection, FontDescription, FontFaceDescriptor};
use div_text::shaping::{
    retained_web_font_outline_count, ShapeResult, TextRasterPolicy, TextShaper,
};
use div_text::TextDirection;

const DEJAVU: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
const TEXT: &str = "Hamburgefonstiv";

fn web_font() -> Font {
    let collection = FontCollection::system();
    collection
        .register(Arc::from(DEJAVU), FontFaceDescriptor::new("Web Sans"))
        .unwrap();
    let mut description = FontDescription::default();
    description.raster_configuration = RasterConfiguration::chromium_linux_lcd();
    description.family = FontFamilyList::new([FontFamily::Named("Web Sans".to_owned())]).unwrap();
    description.size = 16.0;
    description.specified_size = 16.0;
    Font::new_in_collection(description, collection)
}

fn text_blob(shaped: &ShapeResult) -> skia_safe::TextBlob {
    shaped
        .to_text_blob_with_raster_policy_at_scale(Some(4.0), TextRasterPolicy::Skia, 1.0)
        .unwrap()
}

fn render(blob: &skia_safe::TextBlob) -> Vec<u8> {
    let mut surface = skia_safe::surfaces::raster_n32_premul((180, 40)).unwrap();
    surface.canvas().clear(skia_safe::Color::WHITE);
    let mut paint = skia_safe::Paint::default();
    paint.set_color(skia_safe::Color::BLACK);
    surface.canvas().draw_text_blob(blob, (4.0, 28.0), &paint);
    let info = surface.image_info();
    let mut pixels = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0)));
    pixels
}

#[test]
fn the_last_font_client_releases_the_web_font_outlines() {
    assert_eq!(retained_web_font_outline_count(), 0);
    let shaper = TextShaper::new();
    let font = web_font();
    let shaped = shaper.shape(TEXT, &font, TextDirection::Ltr);
    let blob = text_blob(&shaped);
    let pixels = render(&blob);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != [255; 4]));
    assert_eq!(retained_web_font_outline_count(), 1);

    // A shape result owns its font data, so it is a client too.
    drop(font);
    assert_eq!(retained_web_font_outline_count(), 1);
    assert_eq!(render(&text_blob(&shaped)), pixels);
    drop(shaped);
    assert_eq!(retained_web_font_outline_count(), 0);
    assert_eq!(skia_safe::graphics::font_cache_count_used(), 0);

    // A blob built before the release keeps its own typeface.
    assert_eq!(render(&blob), pixels);
    drop(blob);

    // Drawing after a release rebuilds the entry and gives the same pixels.
    for _ in 0..3 {
        let font = web_font();
        let shaped = shaper.shape(TEXT, &font, TextDirection::Ltr);
        assert_eq!(render(&text_blob(&shaped)), pixels);
        assert_eq!(retained_web_font_outline_count(), 1);
        drop(font);
        drop(shaped);
        assert_eq!(retained_web_font_outline_count(), 0);
    }

    // Clients start on other threads while the last one is being retired.
    let start = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let start = start.clone();
            let pixels = &pixels;
            scope.spawn(move || {
                let shaper = TextShaper::new();
                start.wait();
                for _ in 0..8 {
                    let font = web_font();
                    let shaped = shaper.shape(TEXT, &font, TextDirection::Ltr);
                    assert_eq!(&render(&text_blob(&shaped)), pixels);
                    drop(font);
                    drop(shaped);
                    std::thread::yield_now();
                }
            });
        }
    });
    assert_eq!(retained_web_font_outline_count(), 0);
    assert_eq!(skia_safe::graphics::font_cache_count_used(), 0);
}
