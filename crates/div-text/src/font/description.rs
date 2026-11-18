//! FontDescription — what CSS wants the font to look like.
//!
//! Mirrors Blink's `FontDescription` (platform/fonts/font_description.h).
//! This is the input to font resolution: it carries all CSS font properties
//! needed to select and configure a typeface.

use div_geometry::RasterConfiguration;
use div_style::{
    FontFamilyList, FontFeature, FontKerning, FontLanguageOverride, FontOpticalSizing,
    FontOrientation, FontPalette, FontSizeAdjust, FontSmoothing, FontStretch, FontStyleEnum,
    FontSynthesis, FontVariantAlternates, FontVariantCaps, FontVariantEastAsian, FontVariantEmoji,
    FontVariantLigatures, FontVariantNumeric, FontVariantPosition, FontVariation, FontWeight,
    TextAutospace, TextRendering, TextSizeAdjust, TextSpacingTrim,
};

/// Complete description of desired font properties, derived from CSS.
///
/// Mirrors Blink's `FontDescription`. Used as the key for font resolution:
/// the `FontCache` maps a `FontDescription` to a resolved `FontPlatformData`.
#[derive(Debug, Clone)]
pub struct FontDescription {
    /// Immutable raster policy participating in font-instance identity.
    pub raster_configuration: RasterConfiguration,
    /// Device scale selects physical hinting/strike size, never layout size.
    pub device_scale_factor: f64,
    /// Ordered list of font families (CSS `font-family`).
    pub family: FontFamilyList,
    /// Computed font size in pixels (CSS `font-size`, after cascade/inheritance).
    pub size: f32,
    /// Specified font size before min-font-size clamping.
    pub specified_size: f32,
    /// Font weight: 100–900 (CSS `font-weight`).
    pub weight: FontWeight,
    /// Font stretch as percentage (CSS `font-stretch`).
    pub stretch: FontStretch,
    /// Font style: normal, italic, or oblique (CSS `font-style`).
    pub style: FontStyleEnum,
    /// Kerning control (CSS `font-kerning`).
    pub kerning: FontKerning,
    /// Small-caps and other variant caps (CSS `font-variant-caps`).
    pub variant_caps: FontVariantCaps,
    /// Ligature control (CSS `font-variant-ligatures`).
    pub variant_ligatures: FontVariantLigatures,
    /// Numeric glyph variants (CSS `font-variant-numeric`).
    pub variant_numeric: FontVariantNumeric,
    /// East Asian glyph variants (CSS `font-variant-east-asian`).
    pub variant_east_asian: FontVariantEastAsian,
    /// Sub/superscript glyph variants (CSS `font-variant-position`).
    pub variant_position: FontVariantPosition,
    /// Alternate glyph forms (CSS `font-variant-alternates`).
    pub variant_alternates: FontVariantAlternates,
    /// Emoji presentation preference (CSS `font-variant-emoji`).
    pub variant_emoji: FontVariantEmoji,
    /// Apparent-size preservation (CSS `font-size-adjust`).
    pub size_adjust: FontSizeAdjust,
    /// Aspect captured from the first available face for
    /// `font-size-adjust: from-font`; used for later fallback faces.
    pub(crate) resolved_from_font_aspect: Option<f32>,
    /// Extra spacing between characters in pixels (CSS `letter-spacing`).
    pub letter_spacing: f32,
    /// Extra spacing at word boundaries in pixels (CSS `word-spacing`).
    pub word_spacing: f32,
    /// Automatic spacing at ideograph/alphanumeric boundaries.
    pub text_autospace: TextAutospace,
    /// Full-width punctuation spacing policy, retained for line-edge shaping.
    pub text_spacing_trim: TextSpacingTrim,
    /// BCP47 locale for language-specific shaping.
    pub locale: Option<String>,
    /// Font smoothing mode (CSS `-webkit-font-smoothing`).
    pub font_smoothing: FontSmoothing,
    /// Text rendering hint (CSS `text-rendering`).
    pub text_rendering: TextRendering,
    /// OpenType feature settings (CSS `font-feature-settings`).
    pub feature_settings: Vec<FontFeature>,
    /// Variable font variation axes (CSS `font-variation-settings`).
    pub variation_settings: Vec<FontVariation>,
    /// Whether to synthesize bold (CSS `font-synthesis-weight`).
    pub font_synthesis_weight: FontSynthesis,
    /// Whether to synthesize italic (CSS `font-synthesis-style`).
    pub font_synthesis_style: FontSynthesis,
    /// Whether missing small-cap glyphs may be synthesized.
    pub font_synthesis_small_caps: FontSynthesis,
    /// Whether missing super/subscript glyphs may be synthesized.
    pub font_synthesis_position: FontSynthesis,
    /// Optical sizing mode (CSS `font-optical-sizing`).
    pub font_optical_sizing: FontOpticalSizing,
    /// OpenType language-system override.
    pub language_override: FontLanguageOverride,
    /// Color-font palette selection.
    pub palette: FontPalette,
    /// Resolved font orientation for vertical text layout.
    ///
    /// Derived from `writing-mode` + `text-orientation`. Controls whether
    /// glyphs are rendered upright, rotated, or in mixed mode. Defaults to
    /// `Horizontal` (standard left-to-right flow).
    pub orientation: FontOrientation,
    /// Whether this text belongs to a platform-native control label.
    pub native_control_text: bool,
    /// Whether this run belongs to an independently styled embedded document.
    /// Chromium retains linear/subpixel advances for a fallback face in that
    /// context even when the outer deterministic author profile is aliased.
    pub embedded_document_text: bool,
    /// Whether native HTML-button Ahem advance compatibility is active.
    pub native_button_text_metrics: bool,
}

impl FontDescription {
    /// Create a description with CSS initial values.
    pub fn new() -> Self {
        Self {
            raster_configuration: RasterConfiguration::default(),
            device_scale_factor: 1.0,
            family: FontFamilyList::default(),
            size: 16.0,
            specified_size: 16.0,
            weight: FontWeight::NORMAL,
            stretch: FontStretch::NORMAL,
            style: FontStyleEnum::Normal,
            kerning: FontKerning::Auto,
            variant_caps: FontVariantCaps::Normal,
            variant_ligatures: FontVariantLigatures::default(),
            variant_numeric: FontVariantNumeric::default(),
            variant_east_asian: FontVariantEastAsian::default(),
            variant_position: FontVariantPosition::Normal,
            variant_alternates: FontVariantAlternates::Normal,
            variant_emoji: FontVariantEmoji::Normal,
            size_adjust: FontSizeAdjust::None,
            resolved_from_font_aspect: None,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            text_autospace: TextAutospace::Normal,
            text_spacing_trim: TextSpacingTrim::Normal,
            locale: None,
            font_smoothing: FontSmoothing::Auto,
            text_rendering: TextRendering::Auto,
            feature_settings: Vec::new(),
            variation_settings: Vec::new(),
            font_synthesis_weight: FontSynthesis::Auto,
            font_synthesis_style: FontSynthesis::Auto,
            font_synthesis_small_caps: FontSynthesis::Auto,
            font_synthesis_position: FontSynthesis::Auto,
            font_optical_sizing: FontOpticalSizing::Auto,
            language_override: FontLanguageOverride::NORMAL,
            palette: FontPalette::Normal,
            orientation: FontOrientation::Horizontal,
            native_control_text: false,
            embedded_document_text: false,
            native_button_text_metrics: false,
        }
    }

    /// Create a description for a specific family and size.
    pub fn with_family_and_size(family: FontFamilyList, size: f32) -> Self {
        let mut desc = Self::new();
        desc.family = family;
        desc.size = size;
        desc.specified_size = size;
        desc
    }

    /// Build the complete font selection input from a computed style.
    ///
    /// Keeping this conversion in the font subsystem prevents layout, paint,
    /// and font-relative length resolution from drifting apart.
    pub fn from_computed_style(style: &div_style::ComputedStyle) -> Self {
        let adjusted_size = match style.text_size_adjust {
            TextSizeAdjust::Percentage(percent) if percent.is_finite() && percent >= 0.0 => {
                style.font_size * percent / 100.0
            }
            TextSizeAdjust::Auto | TextSizeAdjust::None | TextSizeAdjust::Percentage(_) => {
                style.font_size
            }
        };
        Self {
            raster_configuration: style.raster_configuration,
            device_scale_factor: style.device_scale_factor,
            family: style.font_family.clone(),
            size: adjusted_size,
            specified_size: style.font_size,
            weight: style.font_weight,
            stretch: style.font_stretch,
            style: style.font_style,
            kerning: style.font_kerning,
            variant_caps: style.font_variant_caps,
            variant_ligatures: style.font_variant_ligatures,
            variant_numeric: style.font_variant_numeric,
            variant_east_asian: style.font_variant_east_asian,
            variant_position: style.font_variant_position,
            variant_alternates: style.font_variant_alternates,
            variant_emoji: style.font_variant_emoji,
            size_adjust: style.font_size_adjust,
            resolved_from_font_aspect: None,
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
            text_autospace: style.text_autospace,
            text_spacing_trim: style.text_spacing_trim,
            locale: style.locale.clone(),
            font_smoothing: style.font_smoothing,
            text_rendering: style.text_rendering,
            feature_settings: style.font_feature_settings.clone(),
            variation_settings: style.font_variation_settings.clone(),
            font_synthesis_weight: style.font_synthesis_weight,
            font_synthesis_style: style.font_synthesis_style,
            font_synthesis_small_caps: style.font_synthesis_small_caps,
            font_synthesis_position: style.font_synthesis_position,
            font_optical_sizing: style.font_optical_sizing,
            language_override: style.font_language_override,
            palette: style.font_palette.clone(),
            orientation: div_style::font_orientation(style.writing_mode, style.text_orientation),
            native_control_text: style.native_control_text,
            embedded_document_text: style.embedded_document_text,
            native_button_text_metrics: style.native_button_text_metrics,
        }
    }
}

impl Default for FontDescription {
    fn default() -> Self {
        Self::new()
    }
}

/// `size` device pixels truncated to 1/100 px, in f32.
fn truncate_to_hundredths(size: f32) -> f32 {
    (size * 100.0).floor() / 100.0
}

fn device_scale(device_scale_factor: f64) -> f32 {
    if device_scale_factor.is_finite() && device_scale_factor > 0.0 {
        device_scale_factor as f32
    } else {
        1.0
    }
}

/// The size, in CSS pixels, a font is made at for `size` CSS pixels at
/// `device_scale_factor`: the device-pixel size truncated to 1/100 px.
///
/// Pinned Chromium 147 makes a font at its device-pixel size truncated to
/// 1/100 px, after `size-adjust` and `font-size-adjust`. The truncation runs
/// in f32, so a size whose f32 value lies just below its hundredth (16.05 is
/// 16.0499992) drops to the hundredth below. Measured in
/// `tests/fidelity/fixtures/font-size-precision-v1`.
pub fn font_cache_size(size: f32, device_scale_factor: f64) -> f32 {
    if !size.is_finite() || size <= 0.0 {
        return size;
    }
    let scale = device_scale(device_scale_factor);
    truncate_to_hundredths(size * scale) / scale
}

/// The size, in device pixels, by which a document's font cache finds a font
/// for `size` CSS pixels at `device_scale_factor`: the device-pixel size
/// truncated to 1/100 px twice, before `size-adjust` and `font-size-adjust`.
///
/// Sizes with one key share the font made for the first of them that the
/// document meets. Pinned Chromium 147 does the same: at 2x, 16.009px after
/// 16.001px uses the 32.00px font, while 16.009px alone uses a 32.01px one.
/// At 1x, 16.22px (made at 16.21) after 16.21px (made at 16.20) uses the
/// 16.20px font. Measured in `tests/fidelity/fixtures/font-size-precision-v1`.
pub fn font_cache_key_size(size: f32, device_scale_factor: f64) -> f32 {
    if !size.is_finite() || size <= 0.0 {
        return size;
    }
    truncate_to_hundredths(truncate_to_hundredths(
        size * device_scale(device_scale_factor),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_cache_size_truncates_device_pixels_to_hundredths_in_f32() {
        assert_eq!(font_cache_size(16.009, 1.0), 16.0);
        assert_eq!(font_cache_size(25.298, 1.0), 25.29);
        assert_eq!(font_cache_size(13.3333, 1.0), 13.33);
        assert_eq!(font_cache_size(16.01, 1.0), 16.01);
        // f32(16.05) is 16.0499992.
        assert_eq!(font_cache_size(16.05, 1.0), 16.04);
        // 32.018 device pixels are made at 32.01.
        assert_eq!(font_cache_size(16.009, 2.0), 16.005);
        // 20.0085 device pixels are made at 20.00.
        assert_eq!(font_cache_size(13.339, 1.5), 20.0 / 1.5);
        assert_eq!(font_cache_size(0.0, 1.0), 0.0);
    }

    #[test]
    fn font_cache_key_size_truncates_twice() {
        // 16.22 is made at 16.21, whose f32 value 16.2099991 keys at 16.20.
        assert_eq!(font_cache_key_size(16.22, 1.0), 16.2);
        assert_eq!(font_cache_key_size(16.21, 1.0), 16.2);
        assert_eq!(font_cache_key_size(16.23, 1.0), 16.23);
        // At 2x, 32.002 and 32.018 device pixels share the 32.00 key, while
        // 26.002 and 26.018 keep 26.00 and 26.01.
        assert_eq!(font_cache_key_size(16.001, 2.0), 32.0);
        assert_eq!(font_cache_key_size(16.009, 2.0), 32.0);
        assert_eq!(font_cache_key_size(13.009, 2.0), 26.01);
    }

    #[test]
    fn percentage_text_size_adjust_changes_used_not_specified_size() {
        let style = div_style::ComputedStyle::initial().derive(|style| {
            style.font_size = 20.0;
            style.text_size_adjust = TextSizeAdjust::Percentage(150.0);
        });
        let description = FontDescription::from_computed_style(&style);
        assert_eq!(description.specified_size, 20.0);
        assert_eq!(description.size, 30.0);
    }

    #[test]
    fn none_text_size_adjust_preserves_used_size() {
        let style = div_style::ComputedStyle::initial().derive(|style| {
            style.font_size = 20.0;
            style.text_size_adjust = TextSizeAdjust::None;
        });
        let description = FontDescription::from_computed_style(&style);
        assert_eq!(description.size, 20.0);
    }
}
