//! The text half of `BaseRenderingContext2D` (`base_rendering_context_2d.cc`,
//! Chromium BSD): the text attributes, `fillText`, `strokeText` and
//! `measureText`. The algorithms are in [`crate::text`].

use div_style::{parse_literal, StyleProperty, StyleValue, TextAutospace, TypographyValue};
use div_text::shaping::TextRasterPolicy;
use std::sync::Arc;

use div_geometry::TextEdging;
use div_text::{Font, FontDescription, TextDirection};

use super::CanvasRenderingContext2D;
use crate::math::clamp_to_f32;
use crate::state::PaintType;
use crate::text::{
    description_from_shorthand, is_web_font, CanvasDirection, CanvasFontKerning, CanvasFontStretch,
    CanvasFontVariantCaps, CanvasTextRendering, PrimaryMetrics, TextAlign, TextBaseline, TextItem,
    TextMetrics, TextNode, TextState,
};

/// `CanvasFontCache::ParseFont` for a canvas without a computed style: the
/// `font` shorthand, with no CSS-wide keyword.
fn parse_font(input: &str) -> Option<FontDescription> {
    match parse_literal(StyleProperty::Font, input) {
        Ok(StyleValue::Typography(TypographyValue::Font(font))) => {
            Some(description_from_shorthand(&font))
        }
        _ => None,
    }
}

impl CanvasRenderingContext2D {
    fn text_state(&self) -> &TextState {
        &self.state().text
    }

    fn text_state_mut(&mut self) -> &mut TextState {
        &mut self.state_mut().text
    }

    // ---- font ------------------------------------------------------------

    pub fn font(&self) -> String {
        self.text_state().serialize_font()
    }

    pub fn set_font(&mut self, font: &str) {
        let state = self.text_state();
        // CurrentFontResolvedAndUpToDate.
        if font == state.unparsed_font && state.font.is_some() && !state.lang_is_dirty {
            return;
        }
        let Some(description) = parse_font(font) else {
            return;
        };
        let state = self.text_state_mut();
        state.set_font(description);
        state.unparsed_font = font.to_string();
    }

    /// `if (!state.HasRealizedFont()) setFont(font())`.
    fn realize_font(&mut self) {
        if self.text_state().font.is_none() {
            let font = self.font();
            self.set_font(&font);
        }
    }

    // ---- text attributes -------------------------------------------------

    pub fn text_align(&self) -> &'static str {
        self.text_state().align.name()
    }

    pub fn set_text_align(&mut self, value: &str) {
        if let Some(align) = TextAlign::parse(value) {
            self.text_state_mut().align = align;
        }
    }

    pub fn text_baseline(&self) -> &'static str {
        self.text_state().baseline.name()
    }

    pub fn set_text_baseline(&mut self, value: &str) {
        if let Some(baseline) = TextBaseline::parse(value) {
            self.text_state_mut().baseline = baseline;
        }
    }

    /// `inherit` resolves to the canvas element's direction, which is `ltr`
    /// for a canvas without a document.
    pub fn direction(&self) -> &'static str {
        match self.text_state().direction {
            CanvasDirection::Rtl => "rtl",
            CanvasDirection::Ltr | CanvasDirection::Inherit => "ltr",
        }
    }

    pub fn set_direction(&mut self, value: &str) {
        if let Some(direction) = CanvasDirection::parse(value) {
            self.text_state_mut().direction = direction;
        }
    }

    fn text_direction(&self) -> TextDirection {
        match self.text_state().direction {
            CanvasDirection::Rtl => TextDirection::Rtl,
            CanvasDirection::Ltr | CanvasDirection::Inherit => TextDirection::Ltr,
        }
    }

    pub fn letter_spacing(&self) -> String {
        self.text_state().letter_spacing.parsed.clone()
    }

    pub fn set_letter_spacing(&mut self, value: &str) {
        self.realize_font();
        let state = self.text_state_mut();
        if state.letter_spacing.set(value) {
            let size = state.font.as_ref().map_or(0.0, |d| d.size);
            let px = state.letter_spacing.px(size);
            state.update_font(|d| d.letter_spacing = px);
        }
    }

    pub fn word_spacing(&self) -> String {
        self.text_state().word_spacing.parsed.clone()
    }

    pub fn set_word_spacing(&mut self, value: &str) {
        self.realize_font();
        let state = self.text_state_mut();
        if state.word_spacing.set(value) {
            let size = state.font.as_ref().map_or(0.0, |d| d.size);
            let px = state.word_spacing.px(size);
            state.update_font(|d| d.word_spacing = px);
        }
    }

    pub fn font_kerning(&self) -> &'static str {
        self.text_state().kerning.name()
    }

    pub fn set_font_kerning(&mut self, value: &str) {
        let Some(kerning) = CanvasFontKerning::parse(value) else {
            return;
        };
        self.realize_font();
        let state = self.text_state_mut();
        state.kerning = kerning;
        state.update_font(|d| d.kerning = kerning.to_style());
    }

    pub fn font_stretch(&self) -> &'static str {
        self.text_state().stretch.name()
    }

    pub fn set_font_stretch(&mut self, value: &str) {
        let Some(stretch) = CanvasFontStretch::parse(value) else {
            return;
        };
        self.realize_font();
        let state = self.text_state_mut();
        if state.stretch == stretch {
            return;
        }
        state.stretch = stretch;
        state.update_font(|d| d.stretch = stretch.to_style());
    }

    pub fn font_variant_caps(&self) -> &'static str {
        self.text_state().variant_caps.name()
    }

    pub fn set_font_variant_caps(&mut self, value: &str) {
        let Some(caps) = CanvasFontVariantCaps::parse(value) else {
            return;
        };
        self.realize_font();
        let state = self.text_state_mut();
        state.variant_caps = caps;
        state.update_font(|d| d.variant_caps = caps.to_style());
    }

    pub fn text_rendering(&self) -> &'static str {
        self.text_state().text_rendering.name()
    }

    pub fn set_text_rendering(&mut self, value: &str) {
        let Some(rendering) = CanvasTextRendering::parse(value) else {
            return;
        };
        self.realize_font();
        let state = self.text_state_mut();
        if state.text_rendering == rendering {
            return;
        }
        state.text_rendering = rendering;
        state.update_font(|d| d.text_rendering = rendering.to_style());
    }

    pub fn lang(&self) -> String {
        self.text_state().lang.clone()
    }

    pub fn set_lang(&mut self, value: &str) {
        if self.text_state().lang == value {
            return;
        }
        let state = self.text_state_mut();
        state.lang = value.to_string();
        state.lang_is_dirty = true;
        if self.text_state().font.is_some() {
            let font = self.font();
            self.set_font(&font);
        }
    }

    // ---- drawing and measuring -------------------------------------------

    /// `AccessFont`: the realized font, resolved against the canvas's fonts
    /// with the platform's text raster and the `lang` locale.
    fn access_font(&mut self) -> Font {
        self.realize_font();
        let state = self.text_state();
        let mut description = state.font.clone().expect("realized font");
        description.raster_configuration = self.raster;
        description.device_scale_factor = 1.0;
        // Canvas text has no `text-autospace`: Blink applies it in layout.
        description.text_autospace = TextAutospace::NoAutospace;
        description.locale = (state.lang != "inherit").then(|| state.lang.clone());
        let font = Font::new_in_collection(description.clone(), self.fonts.clone());
        // TEMPORARY: compensates for div-text. A canvas surface has no known
        // pixel geometry, so Skia draws an LCD request as grayscale coverage
        // built from an LCD mask (`kGenA8FromLCD`). Linux Chromium draws a
        // web font with the Fontations scaler, whose `onFilterRec` drops that
        // flag: its glyphs are plain grayscale coverage, and so are their
        // bounds. div-text draws a web font through a custom typeface that
        // keeps the flag, so canvas asks for grayscale directly. Windows
        // Chromium draws web fonts with DirectWrite, which keeps it.
        // Removed by the engine lane's fix: div-text's web-font path gives
        // plain grayscale on a surface without pixel geometry. A run that
        // mixes web and system fonts stays approximate until then. See
        // "Temporary compensations" in
        // docs/progress/canvas-text-2026-11-21.md.
        if cfg!(not(target_os = "windows")) && primary_is_web_font(&font) {
            description.raster_configuration.author_text.edging = TextEdging::AntiAlias;
            return Font::new_in_collection(description, self.fonts.clone());
        }
        font
    }

    /// The text's `PlainTextNode`, from the canvas's shape caches.
    fn text_node(&mut self, font: &Font, text: &str, direction: TextDirection) -> Arc<TextNode> {
        let description_key = format!("{:?}", font.description());
        let shaper = &self.shaper;
        self.shape_caches
            .node(&description_key, text, direction, |segment| {
                TextItem::shape(segment, |word, dir, starts_text| {
                    if starts_text {
                        shaper.shape_at_text_start(word, font, dir)
                    } else {
                        shaper.shape(word, font, dir)
                    }
                })
            })
    }

    /// Ends an animation frame for the text caches
    /// (`CanvasRenderingContextHost::NotifyCachesOfSwitchingFrame`): words
    /// and texts not used in the frame that ends are shaped again later.
    pub fn did_switch_frame(&mut self) {
        self.shape_caches.did_switch_frame();
    }

    pub fn fill_text(&mut self, text: &str, x: f64, y: f64, max_width: Option<f64>) {
        self.draw_text(text, x, y, max_width, PaintType::Fill);
    }

    pub fn stroke_text(&mut self, text: &str, x: f64, y: f64, max_width: Option<f64>) {
        self.draw_text(text, x, y, max_width, PaintType::Stroke);
    }

    /// `DrawTextInternal`.
    fn draw_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        max_width: Option<f64>,
        paint_type: PaintType,
    ) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if max_width.is_some_and(|m| !m.is_finite() || m <= 0.0) {
            return;
        }
        let font = self.access_font();
        let Some(primary) = font.primary_font().cloned() else {
            return;
        };
        let metrics = PrimaryMetrics::of(&primary);
        let direction = self.text_direction();
        let node = self.text_node(&font, text, direction);

        let mut location = (clamp_to_f32(x), clamp_to_f32(y));
        let (font_width, ink) = node.accumulate_inline_size();
        let font_width = font_width as f64;
        let use_max_width = max_width.is_some_and(|m| m < font_width);
        let width = if use_max_width {
            max_width.unwrap()
        } else {
            font_width
        };
        let rtl = direction.is_rtl();
        let align = match self.text_state().align {
            TextAlign::Start if rtl => TextAlign::Right,
            TextAlign::Start => TextAlign::Left,
            TextAlign::End if rtl => TextAlign::Left,
            TextAlign::End => TextAlign::Right,
            align => align,
        };
        match align {
            TextAlign::Center => location.0 = (location.0 as f64 - width / 2.0) as f32,
            TextAlign::Right => location.0 = (location.0 as f64 - width) as f32,
            _ => {}
        }
        location.1 += metrics.baseline(self.text_state().baseline);

        let mut bounds = (
            ink.x + location.0,
            ink.y + location.1,
            ink.width,
            ink.height,
        );
        if paint_type == PaintType::Stroke {
            bounds = self.inflate_stroke_rect(bounds);
        }

        let result = node.to_result();
        if use_max_width {
            let scale = clamp_to_f32(width / font_width);
            let canvas = self.canvas();
            canvas.save();
            canvas.scale((scale, 1.0));
            location.0 /= scale;
        }
        // Drawn even when there is no glyph, so `copy` still clears.
        self.draw(bounds, paint_type, |canvas, paint| {
            let device = canvas.local_to_device_as_3x3();
            if let Some(blob) =
                result.to_text_blob_under(None, TextRasterPolicy::Skia, 1.0, Some(&device))
            {
                canvas.draw_text_blob(&blob, location, paint);
            }
        });
        if use_max_width {
            self.canvas().restore();
        }
    }

    /// `measureText`.
    pub fn measure_text(&mut self, text: &str) -> TextMetrics {
        let font = self.access_font();
        let Some(primary) = font.primary_font().cloned() else {
            return TextMetrics::default();
        };
        let metrics = PrimaryMetrics::of(&primary);
        let direction = self.text_direction();
        let node = self.text_node(&font, text, direction);
        let state = self.text_state();
        TextMetrics::new(&node, &metrics, direction, state.align, state.baseline)
    }
}

/// Whether the font's primary face is a web font.
fn primary_is_web_font(font: &Font) -> bool {
    font.primary_font()
        .is_some_and(|primary| is_web_font(primary))
}
