use div::prelude::*;
use std::{cell::Cell, rc::Rc};

#[path = "support/real_zoom.rs"]
mod real_zoom;
use real_zoom::{Gap, RealZoomGaps};

// The expected widths are pinned Chromium 147's at scale 1, measured by
// `tools/qualification/probe_windows_text_oracle.py` on Linux and Windows
// (build-with-div/div-fidelity-data `archive/docs/progress/evidence/`
// `windows-text-oracle-v2.json` and `linux-text-oracle-v2.json`).
// Div keeps them at every scale. Above scale 1, real-zoom Chromium lays the
// units out in device pixels and differs where `FONT_UNITS_REAL_ZOOM_GAPS`
// lists it: (scale, case, before or after, Chromium's width). The
// application retains the unit; the callback changes only the font, without
// computing or setting a pixel width.
#[cfg(windows)]
const FONT_UNITS_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "before", 46.0),
    (1.25, 1, "before", 38.0),
    (1.25, 3, "before", 59.42499923706055),
    (1.25, 1, "after", 40.0),
    (1.25, 2, "after", 61.0625),
    (1.25, 3, "after", 64.6624984741211),
    (1.5, 0, "before", 46.66666793823242),
    (1.5, 1, "before", 36.66666793823242),
    (1.5, 2, "before", 56.145835876464844),
    (1.5, 3, "before", 59.427085876464844),
    (1.5, 0, "after", 51.66666793823242),
    (1.5, 1, "after", 40.01041793823242),
    (1.5, 2, "after", 61.09375),
    (1.5, 3, "after", 64.66667175292969),
    (2.0, 0, "before", 46.25),
    (2.0, 1, "before", 37.4765625),
    (2.0, 2, "before", 56.1484375),
    (2.0, 3, "before", 59.4296875),
    (2.0, 0, "after", 51.25),
    (2.0, 1, "after", 41.2421875),
    (2.0, 2, "after", 61.09375),
    (3.0, 0, "before", 46.66666793823242),
    (3.0, 1, "before", 37.47916793823242),
    (3.0, 2, "before", 56.145835876464844),
    (3.0, 3, "before", 59.43229293823242),
    (3.0, 0, "after", 50.833335876464844),
    (3.0, 1, "after", 40.83854293823242),
    (3.0, 2, "after", 61.10416793823242),
];
#[cfg(not(windows))]
const FONT_UNITS_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "before", 46.0),
    (1.25, 1, "before", 37.4375),
    (1.25, 3, "before", 59.42499923706055),
    (1.25, 1, "after", 40.72500228881836),
    (1.25, 2, "after", 61.0625),
    (1.25, 3, "after", 64.6624984741211),
    (1.5, 0, "before", 46.66666793823242),
    (1.5, 1, "before", 37.41666793823242),
    (1.5, 2, "before", 56.145835876464844),
    (1.5, 3, "before", 59.427085876464844),
    (1.5, 0, "after", 51.66666793823242),
    (1.5, 1, "after", 40.72916793823242),
    (1.5, 2, "after", 61.09375),
    (1.5, 3, "after", 64.66667175292969),
    (2.0, 0, "before", 46.25),
    (2.0, 1, "before", 37.4296875),
    (2.0, 2, "before", 56.1484375),
    (2.0, 3, "before", 59.4296875),
    (2.0, 0, "after", 51.25),
    (2.0, 2, "after", 61.09375),
    (3.0, 0, "before", 46.66666793823242),
    (3.0, 1, "before", 37.43229293823242),
    (3.0, 2, "before", 56.145835876464844),
    (3.0, 3, "before", 59.43229293823242),
    (3.0, 0, "after", 50.833335876464844),
    (3.0, 1, "after", 40.739585876464844),
    (3.0, 2, "after", 61.10416793823242),
];

#[test]
fn retained_font_units_update_after_native_rust_callbacks() {
    let mut gaps = RealZoomGaps::new(FONT_UNITS_REAL_ZOOM_GAPS);
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        for (case, (unit, line_height, before_width, after_width)) in [
            (LengthValue::Ch(2.5), LineHeight::Normal, 47.5, 50.0),
            (
                LengthValue::Ex(2.5),
                LineHeight::Normal,
                if cfg!(windows) { 37.484375 } else { 37.40625 },
                if cfg!(windows) { 39.984375 } else { 40.734375 },
            ),
            (
                LengthValue::Lh(2.5),
                LineHeight::Number(1.2),
                56.125,
                61.046875,
            ),
            (
                LengthValue::Lh(2.5),
                LineHeight::Percentage(127.5),
                59.421875,
                64.671875,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let document = Document::with_font_collection_and_options(
                ViewportMetrics::from_logical_size(800.0, 600.0, scale).unwrap(),
                FontCollection::deterministic_test(),
                EngineOptions {
                    raster_configuration: RasterConfiguration::deterministic_aliased(false),
                },
            )
            .unwrap();
            let parent = document.body();
            parent
                .set_font_family(FontFamilyList::single("Ahem"))
                .unwrap();
            parent.set_font_size(LengthValue::px(18.72)).unwrap();
            let element = Element::create(&document, "div").unwrap();
            parent.append_child(&element).unwrap();
            element.set_position(Position::Absolute).unwrap();
            element.set_left(Length::px(0.0)).unwrap();
            element.set_top(Length::px(0.0)).unwrap();
            element.set_height(LengthValue::px(10.0)).unwrap();
            // Declare the width before the final line-height context.
            element.set_width(unit).unwrap();
            element.set_line_height(line_height).unwrap();
            let owned_style = element.computed_style().unwrap();
            let owned_bounds = element.bounding_rect().unwrap().unwrap();
            assert_eq!(owned_bounds.width, before_width, "{unit:?} at {scale}");
            gaps.check(scale, case, "before", owned_bounds.width);
            let weak = element.downgrade();
            let target = parent.downgrade();
            let calls = Rc::new(Cell::new(0));
            let called = calls.clone();
            element
                .on("click", move |_| {
                    target
                        .upgrade()
                        .unwrap()
                        .set_font_size(LengthValue::px(20.37))
                        .unwrap();
                    called.set(called.get() + 1);
                })
                .unwrap();
            element.click().unwrap();
            let after = element.bounding_rect().unwrap().unwrap().width;
            assert_eq!(after, after_width, "{unit:?} at {scale}");
            gaps.check(scale, case, "after", after);
            assert_eq!(calls.get(), 1);
            assert_eq!(owned_style.font_size, 18.72);
            assert_eq!(owned_bounds.width, before_width);
            assert_eq!(
                document.render_to_png_buffer().unwrap(),
                document.render_to_png_buffer().unwrap()
            );
            drop(element);
            drop(parent);
            drop(document);
            assert!(weak.upgrade().is_none());
        }
    }
    gaps.finish();
}

#[test]
fn retained_font_units_refresh_after_shared_font_registration() {
    let fonts = FontCollection::deterministic_test();
    let document = Document::with_font_collection_and_options(
        ViewportMetrics::from_logical_size(800.0, 600.0, 1.0).unwrap(),
        fonts.clone(),
        EngineOptions {
            raster_configuration: RasterConfiguration::deterministic_aliased(false),
        },
    )
    .unwrap();
    let element = Element::create(&document, "div").unwrap();
    document.body().append_child(&element).unwrap();
    element
        .set_font_family(FontFamilyList::single("Retained unit font"))
        .unwrap();
    element.set_font_size(LengthValue::px(20.0)).unwrap();
    element.set_width(LengthValue::Ch(2.0)).unwrap();
    let before = element.computed_style().unwrap();
    // Register through the shared collection rather than this Document.
    let face = fonts
        .register(
            std::sync::Arc::from(include_bytes!("../../div-text/fonts/Ahem.ttf").as_slice()),
            FontFaceDescriptor::new("Retained unit font"),
        )
        .unwrap();
    assert_eq!(element.computed_style().unwrap().width.value(), 40.0);
    assert_eq!(element.bounding_rect().unwrap().unwrap().width, 40.0);
    assert_ne!(before.width.value(), 40.0);
    fonts.unregister(face).unwrap();
    assert_eq!(element.computed_style().unwrap().width, before.width);
}

// The aliased raster configuration models Chromium text whose Fontconfig
// disables antialiasing: Linux Chromium then resolves full hinting and
// rounds each HarfBuzz advance to a whole pixel, so nine 16px Ahem glyphs
// span exactly 144px (docs/progress/aliased-text-whole-pixel-advances-2026-10-25.md).
// The earlier pinned observation of 144.015625 (native-general-font-chromium-v3824)
// loaded Ahem through @font-face, which Blink renders with the default
// antialiased style instead. The word's bounds are pinned Chromium 147's at
// scale 1 on Linux and Windows (build-with-div/div-fidelity-data `archive/docs/progress/evidence/`
// `windows-text-oracle-v2.json` and `linux-text-oracle-v2.json`),
// and Div keeps them at every scale. Above scale 1, real-zoom Chromium
// places the word on a device-pixel line box and its y differs where
// `ATOMIC_WORD_REAL_ZOOM_GAPS` lists it. The fixed-width float neighbor also
// checks that text rounding does not inflate unrelated atomic contents.
#[cfg(windows)]
const ATOMIC_WORD_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "y", 485.8000183105469),
    (1.5, 0, "y", 486.3333435058594),
    (2.0, 0, "y", 486.0),
];
#[cfg(not(windows))]
const ATOMIC_WORD_REAL_ZOOM_GAPS: &[Gap] = &[
    (1.25, 0, "y", 487.3999938964844),
    (3.0, 0, "y", 487.3333435058594),
];

#[test]
fn atomic_intrinsic_text_keeps_chromium_fraction_after_native_font_alias_change() {
    let mut gaps = RealZoomGaps::new(ATOMIC_WORD_REAL_ZOOM_GAPS);
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let fonts = FontCollection::deterministic_test();
        fonts
            .register(
                std::sync::Arc::from(include_bytes!("../../div-text/fonts/Ahem.ttf").as_slice()),
                FontFaceDescriptor::new("Native Ahem Alias"),
            )
            .unwrap();
        let document = Document::with_font_collection_and_options(
            ViewportMetrics::from_logical_size(800.0, 600.0, scale).unwrap(),
            fonts,
            EngineOptions {
                raster_configuration: RasterConfiguration::deterministic_aliased(false),
            },
        )
        .unwrap();
        let root = document.body();
        let container = Element::create(&document, "div").unwrap();
        root.append_child(&container).unwrap();
        container.set_position(Position::Absolute).unwrap();
        container.set_left(Length::px(10.0)).unwrap();
        container.set_top(Length::px(485.0)).unwrap();
        container.set_width(LengthValue::px(75.0)).unwrap();
        let word = Element::create(&document, "span").unwrap();
        container.append_child(&word).unwrap();
        word.set_display(Display::InlineBlock).unwrap();
        word.set_font_size(LengthValue::px(16.0)).unwrap();
        word.set_font_family(FontFamilyList::single("Ahem"))
            .unwrap();
        word.set_text("ABCDEFGHI").unwrap();
        let owned = word.bounding_rect().unwrap().unwrap();
        let raster_policy = RasterConfiguration::deterministic_aliased(false);
        assert_eq!(
            word.computed_style().unwrap().raster_configuration,
            raster_policy
        );
        assert_eq!(
            (owned.x, owned.y, owned.width, owned.height),
            (10.0, 487.0, 144.0, 16.0),
            "word at {scale}"
        );
        gaps.check(scale, 0, "y", owned.y);
        let float_container = Element::create(&document, "div").unwrap();
        root.append_child(&float_container).unwrap();
        float_container.set_position(Position::Absolute).unwrap();
        float_container.set_left(Length::px(170.0)).unwrap();
        float_container.set_top(Length::px(485.0)).unwrap();
        float_container.set_width(LengthValue::px(75.0)).unwrap();
        let atomic = Element::create(&document, "span").unwrap();
        float_container.append_child(&atomic).unwrap();
        atomic.set_display(Display::InlineBlock).unwrap();
        for _ in 0..2 {
            let child = Element::create(&document, "div").unwrap();
            atomic.append_child(&child).unwrap();
            child.set_float(Float::Left).unwrap();
            child.set_width(LengthValue::px(90.0)).unwrap();
            child.set_height(LengthValue::px(20.0)).unwrap();
        }
        let floats = atomic.bounding_rect().unwrap().unwrap();
        assert_eq!(
            (floats.x, floats.y, floats.width, floats.height),
            (170.0, 485.0, 90.0, 40.0),
            "floats at {scale}"
        );
        let before_alias_pixels = document.render_to_png_buffer().unwrap();
        let target = word.downgrade();
        let calls = Rc::new(Cell::new(0));
        let called = calls.clone();
        root.on("click", move |_| {
            target
                .upgrade()
                .unwrap()
                .set_font_family(FontFamilyList::single("Native Ahem Alias"))
                .unwrap();
            called.set(called.get() + 1);
        })
        .unwrap();
        root.click().unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(word.bounding_rect().unwrap().unwrap(), owned);
        assert_eq!(
            word.computed_style().unwrap().raster_configuration,
            raster_policy
        );
        assert_eq!(atomic.bounding_rect().unwrap().unwrap(), floats);
        // A different native family alias for the same owned face changes
        // neither the explicitly selected raster policy nor its actual pixels.
        // Capture after adding the unchanged neighboring float geometry.
        let after_alias_pixels = document.render_to_png_buffer().unwrap();
        assert_eq!(after_alias_pixels, before_alias_pixels);
        assert_eq!(after_alias_pixels, document.render_to_png_buffer().unwrap());
    }
    gaps.finish();
}
