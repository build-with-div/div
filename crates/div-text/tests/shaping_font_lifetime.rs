//! The shaper's parent-font cache ends with the last font client.
//!
//! This integration test runs in its own process because the cache and the
//! client count are process-wide, and other text tests would keep them alive.

use std::sync::{Arc, Barrier};

use div_text::font::{Font, FontDescription};
use div_text::shaping::{retained_parent_font_count, TextShaper};
use div_text::TextDirection;

const TEXT: &str = "Shaping text";

fn shaped_width(shaper: &TextShaper, font: &Font) -> f32 {
    shaper.shape(TEXT, font, TextDirection::Ltr).width()
}

#[test]
fn the_last_font_client_releases_the_shaping_parent_fonts() {
    assert_eq!(retained_parent_font_count(), 0);
    let shaper = TextShaper::new();
    let first = Font::new(FontDescription::new());
    let width = shaped_width(&shaper, &first);
    assert!(width > 0.0);
    assert!(retained_parent_font_count() > 0);

    // Another client keeps the cache after the first one goes.
    let second = Font::new(FontDescription::new());
    drop(first);
    assert!(retained_parent_font_count() > 0);

    // A shape result owns its font data, so it is a client too.
    let result = shaper.shape(TEXT, &second, TextDirection::Ltr);
    drop(second);
    assert!(retained_parent_font_count() > 0);
    assert_eq!(result.width(), width);
    drop(result);
    assert_eq!(retained_parent_font_count(), 0);
    assert_eq!(skia_safe::graphics::font_cache_count_used(), 0);

    // Shaping after a release rebuilds the cache and gives the same width.
    for _ in 0..3 {
        let font = Font::new(FontDescription::new());
        assert_eq!(shaped_width(&shaper, &font), width);
        assert!(retained_parent_font_count() > 0);
        drop(font);
        assert_eq!(retained_parent_font_count(), 0);
    }

    // Clients start on other threads while the last one is being retired.
    let start = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let start = start.clone();
            scope.spawn(move || {
                let shaper = TextShaper::new();
                start.wait();
                for _ in 0..32 {
                    let font = Font::new(FontDescription::new());
                    assert_eq!(shaped_width(&shaper, &font), width);
                    drop(font);
                    std::thread::yield_now();
                }
            });
        }
    });
    assert_eq!(retained_parent_font_count(), 0);
    assert_eq!(skia_safe::graphics::font_cache_count_used(), 0);
}
