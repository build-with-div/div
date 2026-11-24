//! The shaping caches end with the last font client.
//!
//! The caches and the client count are process-wide, and other tests in this
//! binary keep them alive. Each test here therefore runs its body in a child
//! copy of this binary that runs only that test.

use std::ffi::OsStr;
use std::process::Command;
use std::sync::{Arc, Barrier};

use crate::font::{Font, FontDescription};
use crate::shaping::{TextDirection, TextShaper};

/// Names the one test a child process runs.
const CHILD: &str = "DIV_TEXT_FONT_LIFETIME_CHILD";

/// Runs `body` in a child process that runs only `test`. Fails unless the
/// child runs exactly that test, and it passes.
fn in_own_process(test: &str, body: impl FnOnce()) {
    if std::env::var_os(CHILD).as_deref() == Some(OsStr::new(test)) {
        body();
        return;
    }
    let output = Command::new(std::env::current_exe().expect("test binary path"))
        .args(["--exact", test])
        .env(CHILD, test)
        .output()
        .expect("run the test binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed;"),
        "child run of {test} failed ({}):\n{stdout}\n{stderr}",
        output.status
    );
}

const SHAPING_TEXT: &str = "Shaping text";

fn shaped_width(shaper: &TextShaper, font: &Font) -> f32 {
    shaper.shape(SHAPING_TEXT, font, TextDirection::Ltr).width()
}

#[test]
fn the_last_font_client_releases_the_shaping_parent_fonts() {
    in_own_process(
        "shaping::lifetime_tests::the_last_font_client_releases_the_shaping_parent_fonts",
        || {
            use super::harfbuzz::retained_parent_font_count;

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
            let result = shaper.shape(SHAPING_TEXT, &second, TextDirection::Ltr);
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
        },
    );
}

/// Only Linux draws web fonts through the outline cache.
#[cfg(target_os = "linux")]
mod web_font_outlines {
    use std::sync::{Arc, Barrier};

    use div_geometry::RasterConfiguration;
    use div_style::{FontFamily, FontFamilyList};

    use super::in_own_process;
    use crate::font::{Font, FontCollection, FontDescription, FontFaceDescriptor};
    use crate::shaping::web_font_outlines::retained_web_font_outline_count;
    use crate::shaping::{ShapeResult, TextDirection, TextRasterPolicy, TextShaper};

    const DEJAVU: &[u8] = include_bytes!("../../fonts/DejaVuSans.ttf");
    const TEXT: &str = "Hamburgefonstiv";

    fn web_font() -> Font {
        let collection = FontCollection::system();
        collection
            .register(Arc::from(DEJAVU), FontFaceDescriptor::new("Web Sans"))
            .unwrap();
        let mut description = FontDescription::default();
        description.raster_configuration = RasterConfiguration::chromium_linux_lcd();
        description.family =
            FontFamilyList::new([FontFamily::Named("Web Sans".to_owned())]).unwrap();
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
        in_own_process(
            "shaping::lifetime_tests::web_font_outlines::the_last_font_client_releases_the_web_font_outlines",
            || {
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
            },
        );
    }
}
