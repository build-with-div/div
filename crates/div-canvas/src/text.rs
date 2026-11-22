//! Canvas text: the text attributes, `font` parsing and serialization,
//! segmentation into words, shaping, ink bounds and `TextMetrics`.
//!
//! Ported from Chromium 147.0.7727.50 (BSD licences):
//!
//! - `modules/canvas/canvas2d/base_rendering_context_2d.cc` (`font`,
//!   `setFont`, `DrawTextInternal`, `measureText` and the text setters) and
//!   `canvas_rendering_context_2d_state.cc` (`SetFont`, the spacing setters,
//!   `StringToNumWithUnit`);
//! - `core/html/canvas/text_metrics.cc` (`GetFontBaseline`, `Update`,
//!   `MeasureRuns`);
//! - `platform/fonts/plain_text_node.cc` (space normalization, bidi runs,
//!   `SegmentWord`, `NextWordEndIndex`, `Shape`, `AccumulateInlineSize`);
//! - `platform/fonts/shaping/shape_result.cc` (`ComputeInkBounds`),
//!   `glyph_bounds_accumulator.h`, `shape_result_spacing.cc` and
//!   `skia/skia_text_metrics.cc` (`SkFontGetBoundsForGlyphs`);
//! - `platform/fonts/simple_font_data.cc` (`PlatformInit`, the normalized
//!   typo metrics) and `font_metrics.cc` (`AscentDescentWithHacks`).
//!
//! Shaping, font matching and glyph rasterization are div-text's, used as
//! they are. The differences from Chromium that remain are listed in
//! `docs/progress/2026-11-21-canvas-text.md`.

use div_style::{
    FontFamily, FontFamilyList, FontKerning, FontShorthand, FontStretch, FontStyleEnum,
    FontVariantCaps, FontWeight, GenericFontFamily, TextRendering,
};
use std::sync::Arc;

use div_text::{
    BidiParagraph, FontDescription, FontPlatformData, ShapeResult, ShapeResultRun, TextDirection,
};
use skia_safe::Rect;

/// `kDefaultFont`.
pub(crate) const DEFAULT_FONT: &str = "10px sans-serif";

macro_rules! keyword_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant,)*
        }

        impl $name {
            /// The IDL enum value, or `None` (an assignment the binding ignores).
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($text => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)*
                }
            }
        }
    };
}

keyword_enum!(
    /// `CanvasTextAlign`.
    TextAlign {
        Start = "start",
        End = "end",
        Left = "left",
        Right = "right",
        Center = "center",
    }
);

keyword_enum!(
    /// `CanvasTextBaseline`.
    TextBaseline {
        Top = "top",
        Hanging = "hanging",
        Middle = "middle",
        Alphabetic = "alphabetic",
        Ideographic = "ideographic",
        Bottom = "bottom",
    }
);

keyword_enum!(
    /// `CanvasDirection`.
    CanvasDirection {
        Ltr = "ltr",
        Rtl = "rtl",
        Inherit = "inherit",
    }
);

keyword_enum!(
    /// `CanvasFontKerning`.
    CanvasFontKerning {
        Auto = "auto",
        Normal = "normal",
        None = "none",
    }
);

keyword_enum!(
    /// `CanvasFontStretch`.
    CanvasFontStretch {
        UltraCondensed = "ultra-condensed",
        ExtraCondensed = "extra-condensed",
        Condensed = "condensed",
        SemiCondensed = "semi-condensed",
        Normal = "normal",
        SemiExpanded = "semi-expanded",
        Expanded = "expanded",
        ExtraExpanded = "extra-expanded",
        UltraExpanded = "ultra-expanded",
    }
);

keyword_enum!(
    /// `CanvasFontVariantCaps`.
    CanvasFontVariantCaps {
        Normal = "normal",
        SmallCaps = "small-caps",
        AllSmallCaps = "all-small-caps",
        PetiteCaps = "petite-caps",
        AllPetiteCaps = "all-petite-caps",
        Unicase = "unicase",
        TitlingCaps = "titling-caps",
    }
);

keyword_enum!(
    /// `CanvasTextRendering`.
    CanvasTextRendering {
        Auto = "auto",
        OptimizeSpeed = "optimizeSpeed",
        OptimizeLegibility = "optimizeLegibility",
        GeometricPrecision = "geometricPrecision",
    }
);

impl CanvasFontKerning {
    pub(crate) fn to_style(self) -> FontKerning {
        match self {
            Self::Auto => FontKerning::Auto,
            Self::Normal => FontKerning::Normal,
            Self::None => FontKerning::None,
        }
    }
}

impl CanvasFontStretch {
    /// `CanvasFontStretchToSelectionValue`.
    pub(crate) fn to_style(self) -> FontStretch {
        FontStretch(match self {
            Self::UltraCondensed => 50.0,
            Self::ExtraCondensed => 62.5,
            Self::Condensed => 75.0,
            Self::SemiCondensed => 87.5,
            Self::Normal => 100.0,
            Self::SemiExpanded => 112.5,
            Self::Expanded => 125.0,
            Self::ExtraExpanded => 150.0,
            Self::UltraExpanded => 200.0,
        })
    }

    /// `V8CanvasFontStretch::Create(FontDescription::ToString(stretch))`.
    /// Chromium reaches `NOTREACHED` for a percentage that names no keyword;
    /// Div keeps `normal`.
    pub(crate) fn from_style(stretch: FontStretch) -> Self {
        [
            Self::UltraCondensed,
            Self::ExtraCondensed,
            Self::Condensed,
            Self::SemiCondensed,
            Self::Normal,
            Self::SemiExpanded,
            Self::Expanded,
            Self::ExtraExpanded,
            Self::UltraExpanded,
        ]
        .into_iter()
        .find(|k| k.to_style().0 == stretch.0)
        .unwrap_or(Self::Normal)
    }
}

impl CanvasFontVariantCaps {
    pub(crate) fn to_style(self) -> FontVariantCaps {
        match self {
            Self::Normal => FontVariantCaps::Normal,
            Self::SmallCaps => FontVariantCaps::SmallCaps,
            Self::AllSmallCaps => FontVariantCaps::AllSmallCaps,
            Self::PetiteCaps => FontVariantCaps::PetiteCaps,
            Self::AllPetiteCaps => FontVariantCaps::AllPetiteCaps,
            Self::Unicase => FontVariantCaps::Unicase,
            Self::TitlingCaps => FontVariantCaps::TitlingCaps,
        }
    }

    pub(crate) fn from_style(caps: FontVariantCaps) -> Self {
        match caps {
            FontVariantCaps::Normal => Self::Normal,
            FontVariantCaps::SmallCaps => Self::SmallCaps,
            FontVariantCaps::AllSmallCaps => Self::AllSmallCaps,
            FontVariantCaps::PetiteCaps => Self::PetiteCaps,
            FontVariantCaps::AllPetiteCaps => Self::AllPetiteCaps,
            FontVariantCaps::Unicase => Self::Unicase,
            FontVariantCaps::TitlingCaps => Self::TitlingCaps,
        }
    }
}

impl CanvasTextRendering {
    pub(crate) fn to_style(self) -> TextRendering {
        match self {
            Self::Auto => TextRendering::Auto,
            Self::OptimizeSpeed => TextRendering::OptimizeSpeed,
            Self::OptimizeLegibility => TextRendering::OptimizeLegibility,
            Self::GeometricPrecision => TextRendering::GeometricPrecision,
        }
    }
}

/// A `letterSpacing` or `wordSpacing` unit: the CSS length units whose
/// size does not depend on the viewport, a container or the font's glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpacingUnit {
    Px,
    Em,
    Rem,
    Cm,
    Mm,
    Q,
    In,
    Pt,
    Pc,
}

impl SpacingUnit {
    fn parse(unit: &str) -> Option<Self> {
        Some(match unit.to_ascii_lowercase().as_str() {
            "px" => Self::Px,
            "em" => Self::Em,
            "rem" => Self::Rem,
            "cm" => Self::Cm,
            "mm" => Self::Mm,
            "q" => Self::Q,
            "in" => Self::In,
            "pt" => Self::Pt,
            "pc" => Self::Pc,
            _ => return None,
        })
    }

    /// `CSSPrimitiveValue::UnitTypeToString`.
    fn name(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Em => "em",
            Self::Rem => "rem",
            Self::Cm => "cm",
            Self::Mm => "mm",
            Self::Q => "q",
            Self::In => "in",
            Self::Pt => "pt",
            Self::Pc => "pc",
        }
    }

    /// `CSSToLengthConversionData::ZoomedComputedPixels` with the font
    /// sizes `SetFont` gives it (em and rem are both the computed size) and
    /// zoom 1.
    fn to_px(self, value: f32, font_size: f32) -> f32 {
        let value = value as f64;
        (match self {
            Self::Px => value,
            Self::Em | Self::Rem => value * font_size as f64,
            Self::Cm => value * (96.0 / 2.54),
            Self::Mm => value * (96.0 / 25.4),
            Self::Q => value * (96.0 / 101.6),
            Self::In => value * 96.0,
            Self::Pt => value * (96.0 / 72.0),
            Self::Pc => value * 16.0,
        }) as f32
    }
}

/// `letterSpacing` or `wordSpacing` state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spacing {
    pub value: f32,
    pub unit: SpacingUnit,
    pub parsed: String,
    pub is_set: bool,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            value: 0.0,
            unit: SpacingUnit::Px,
            parsed: "0px".into(),
            is_set: false,
        }
    }
}

impl Spacing {
    /// `SetLetterSpacing` / `SetWordSpacing` up to the font rebuild: returns
    /// whether the value changed.
    pub fn set(&mut self, input: &str) -> bool {
        self.is_set = true;
        if self.parsed == input {
            return false;
        }
        let Some((value, unit)) = string_to_num_with_unit(input) else {
            return false;
        };
        if unit == self.unit && value == self.value {
            return false;
        }
        self.unit = unit;
        self.value = value;
        self.parsed = format!("{}{}", append_number(value), unit.name());
        true
    }

    /// The spacing in pixels for a font of `font_size`, as the shaper adds
    /// it: `TextRunLayoutUnit(float)` truncates to 1/65536 px.
    pub fn px(&self, font_size: f32) -> f32 {
        text_run_layout_unit(self.unit.to_px(self.value, font_size))
    }
}

/// `TextRunLayoutUnit(float)`: 16 fractional bits, truncated.
fn text_run_layout_unit(v: f32) -> f32 {
    let raw = (v * 65536.0).trunc();
    if !raw.is_finite() {
        return 0.0;
    }
    let raw = raw.clamp(i32::MIN as f32, i32::MAX as f32) as i64;
    (raw as f64 / 65536.0) as f32
}

/// `StringToNumWithUnit`: exactly one dimension token whose unit is a
/// length unit. The number follows the CSS tokenizer's grammar.
fn string_to_num_with_unit(input: &str) -> Option<(f32, SpacingUnit)> {
    let bytes = input.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i > int_start;
    if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        digits = true;
    }
    if !digits {
        return None;
    }
    // An exponent only when digits follow (`1e3px`; `2em` is 2 and `em`).
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let number: f64 = input[..i].parse().ok()?;
    // The unit is the rest, which must be one identifier.
    let unit = &input[i..];
    let mut chars = unit.chars();
    let first = chars.next()?;
    let starts_ident = first.is_ascii_alphabetic() || first == '_' || !first.is_ascii();
    if !starts_ident {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()) {
        return None;
    }
    Some((number as f32, SpacingUnit::parse(unit)?))
}

/// `StringBuilder::AppendNumber(float)`: six significant digits, trailing
/// zeros removed (`NumberToFixedPrecisionString`).
pub(crate) fn append_number(v: f32) -> String {
    let v = v as f64;
    if v == 0.0 {
        return "0".into();
    }
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".into()
        } else if v > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let sci = format!("{v:.5e}");
    let (mantissa, exponent) = sci.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let trim = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    // Exponential form outside six leading zeros and six integer digits, as
    // double-conversion's `ToPrecision` writes it (`1.23457e+6`).
    if !(-6..6).contains(&exponent) {
        let sign = if exponent < 0 { "-" } else { "+" };
        return format!("{}e{sign}{}", trim(mantissa), exponent.abs());
    }
    let decimals = (5 - exponent).max(0) as usize;
    trim(&format!("{v:.decimals$}"))
}

/// The canvas text state of one entry of the state stack.
#[derive(Debug, Clone)]
pub(crate) struct TextState {
    /// The realized font's description (`HasRealizedFont`), size and
    /// family as specified.
    pub font: Option<FontDescription>,
    pub unparsed_font: String,
    pub align: TextAlign,
    pub baseline: TextBaseline,
    pub direction: CanvasDirection,
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,
    pub kerning: CanvasFontKerning,
    pub stretch: CanvasFontStretch,
    pub variant_caps: CanvasFontVariantCaps,
    pub text_rendering: CanvasTextRendering,
    pub lang: String,
    pub lang_is_dirty: bool,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font: None,
            unparsed_font: DEFAULT_FONT.into(),
            align: TextAlign::Start,
            baseline: TextBaseline::Alphabetic,
            direction: CanvasDirection::Inherit,
            letter_spacing: Spacing::default(),
            word_spacing: Spacing::default(),
            kerning: CanvasFontKerning::Auto,
            stretch: CanvasFontStretch::Normal,
            variant_caps: CanvasFontVariantCaps::Normal,
            text_rendering: CanvasTextRendering::Auto,
            lang: "inherit".into(),
            lang_is_dirty: false,
        }
    }
}

impl TextState {
    /// `CanvasRenderingContext2DState::SetFont`: the context's text
    /// properties applied to a newly parsed font.
    pub fn set_font(&mut self, mut description: FontDescription) {
        let size = description.size;
        if self.word_spacing.is_set {
            description.word_spacing = self.word_spacing.px(size);
        }
        if self.letter_spacing.is_set {
            description.letter_spacing = self.letter_spacing.px(size);
        }
        description.kerning = self.kerning.to_style();
        description.text_rendering = self.text_rendering.to_style();
        self.variant_caps = CanvasFontVariantCaps::from_style(description.variant_caps);
        self.stretch = CanvasFontStretch::from_style(description.stretch);
        self.font = Some(description);
        self.lang_is_dirty = false;
    }

    /// The realized description, rebuilt by `f` (`SetFontInternal`).
    pub fn update_font(&mut self, f: impl FnOnce(&mut FontDescription)) {
        if let Some(description) = self.font.as_mut() {
            f(description);
        }
        self.lang_is_dirty = false;
    }

    /// `BaseRenderingContext2D::font()`.
    pub fn serialize_font(&self) -> String {
        let Some(d) = &self.font else {
            return DEFAULT_FONT.into();
        };
        let mut s = String::new();
        // `Style() == kItalicSlopeValue`: italic and the default oblique
        // angle are the same slope (measured: `oblique` reads back as
        // `italic`).
        if matches!(d.style, FontStyleEnum::Italic)
            || matches!(d.style, FontStyleEnum::Oblique(angle) if angle == 14.0)
        {
            s.push_str("italic ");
        }
        if d.weight == FontWeight::BOLD {
            s.push_str("bold ");
        } else if d.weight != FontWeight::NORMAL {
            s.push_str(&format!("{} ", d.weight.0 as i32));
        }
        if d.variant_caps == FontVariantCaps::SmallCaps {
            s.push_str("small-caps ");
        }
        s.push_str(&append_number(d.specified_size));
        s.push_str("px ");
        s.push_str(&serialize_family(&d.family));
        s
    }
}

/// The font description `setFont` builds from a parsed `font` value. The
/// line height is ignored.
pub(crate) fn description_from_shorthand(font: &FontShorthand) -> FontDescription {
    let mut d = FontDescription::with_family_and_size(font.family.clone(), font.size_px);
    d.style = font.style;
    d.weight = font.weight;
    d.stretch = font.stretch;
    d.variant_caps = font.variant_caps;
    d
}

/// `ComputedStyleUtils::ValueForFontFamily(...)->CssText()`.
fn serialize_family(list: &FontFamilyList) -> String {
    list.families
        .iter()
        .map(|family| match family {
            FontFamily::Generic(generic) => generic_name(*generic).to_string(),
            FontFamily::Named(name) => {
                if is_css_identifier(name) {
                    name.clone()
                } else {
                    serialize_string(name)
                }
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn generic_name(generic: GenericFontFamily) -> &'static str {
    match generic {
        GenericFontFamily::None => "",
        GenericFontFamily::Serif => "serif",
        GenericFontFamily::SansSerif => "sans-serif",
        GenericFontFamily::Monospace => "monospace",
        GenericFontFamily::Cursive => "cursive",
        GenericFontFamily::Fantasy => "fantasy",
        GenericFontFamily::SystemUi => "system-ui",
        GenericFontFamily::Math => "math",
        GenericFontFamily::Emoji => "emoji",
        GenericFontFamily::FangSong => "fangsong",
        GenericFontFamily::UiSerif => "ui-serif",
        GenericFontFamily::UiSansSerif => "ui-sans-serif",
        GenericFontFamily::UiMonospace => "ui-monospace",
        GenericFontFamily::UiRounded => "ui-rounded",
    }
}

/// Whether `s` is a single CSS identifier token (no escapes).
fn is_css_identifier(s: &str) -> bool {
    let name_start = |c: char| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii();
    let name = |c: char| name_start(c) || c.is_ascii_digit() || c == '-';
    let mut chars = s.chars().peekable();
    match chars.next() {
        Some('-') => match chars.peek() {
            Some(&c) if name_start(c) || c == '-' => {}
            _ => return false,
        },
        Some(c) if name_start(c) => {}
        _ => return false,
    }
    chars.all(name)
}

/// `SerializeString`.
fn serialize_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{1}'..='\u{1f}' | '\u{7f}' => out.push_str(&format!("\\{:x} ", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Font metrics.

/// `LayoutUnit::FromFloatRound` as its raw 1/64 value.
fn layout_unit_round(v: f32) -> i32 {
    let raw = (v * 64.0).round();
    if raw.is_nan() {
        0
    } else {
        raw.clamp(i32::MIN as f32, i32::MAX as f32) as i32
    }
}

fn layout_unit_to_f32(raw: i32) -> f32 {
    raw as f32 / 64.0
}

/// The primary font's metrics that canvas text reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PrimaryMetrics {
    /// `FloatAscent()` / `FloatDescent()` after `AscentDescentWithHacks`.
    pub ascent: f32,
    pub descent: f32,
    /// `NormalizedTypoAscentAndDescent()`, raw 1/64 units.
    pub typo_ascent: i32,
    pub typo_descent: i32,
}

impl PrimaryMetrics {
    /// `SimpleFontData::PlatformInit` (ascent and descent) and
    /// `ComputeNormalizedTypoAscentAndDescent`, for canvas fonts. Canvas
    /// asks for subpixel ascent and descent (`SetSubpixelAscentDescent(true)`),
    /// but only the font cache's system fonts honour it: a web font's
    /// `SimpleFontData` is built with the default, false, so a `FontFace`
    /// font rounds its ascent and descent at every size.
    pub fn of(font_data: &FontPlatformData) -> Self {
        let sk_font = font_data.sk_font();
        let size = sk_font.size();
        if size == 0.0 {
            return Self {
                ascent: 0.0,
                descent: 0.0,
                typo_ascent: 0,
                typo_descent: 0,
            };
        }
        let (_, metrics) = sk_font.metrics();
        let web_font = is_web_font(font_data);
        // The Linux borrow reads the render style's subpixel positioning,
        // which `gfx::GetFontRenderParams` turns on only above a device
        // scale of 1 (div-text's page metrics follow the same rule). Canvas
        // fonts are resolved at scale 1. Skia still positions their glyphs
        // at subpixel offsets: Blink forces `SkFont::setSubpixel` unless the
        // hinting is full.
        let (ascent, descent) =
            ascent_descent_with_hacks(metrics.ascent, metrics.descent, !web_font, false);
        let (typo_ascent, typo_descent) = typo_ascender_and_descender(font_data)
            .filter(|&(a, _)| a > 0)
            .and_then(|(a, d)| normalized_typo(a as f32, d as f32, size))
            .or_else(|| normalized_typo(ascent, descent, size))
            .unwrap_or((0, 0));
        Self {
            ascent,
            descent,
            typo_ascent,
            typo_descent,
        }
    }

    /// `TextMetrics::GetFontBaseline` (no OpenType BASE values: see the
    /// gaps in the progress record).
    pub fn baseline(&self, baseline: TextBaseline) -> f32 {
        match baseline {
            TextBaseline::Top => layout_unit_to_f32(self.typo_ascent),
            TextBaseline::Hanging => ((self.ascent * 80.0) as f64 / 100.0) as f32,
            TextBaseline::Ideographic => -self.descent,
            TextBaseline::Bottom => -layout_unit_to_f32(self.typo_descent),
            TextBaseline::Middle => {
                (layout_unit_to_f32(self.typo_ascent) - layout_unit_to_f32(self.typo_descent)) / 2.0
            }
            TextBaseline::Alphabetic => 0.0,
        }
    }
}

/// Whether Chromium loads the face as a web font (a `FontFace` or
/// `@font-face` source), not from the system.
pub(crate) fn is_web_font(font_data: &FontPlatformData) -> bool {
    font_data
        .source_info()
        .application_face()
        .is_some_and(|face| face.is_web_font())
}

/// `FontMetrics::AscentDescentWithHacks`. The Linux VDMX branch applies only
/// to full or normal hinting, which Chromium's canvas fonts do not use; Div
/// follows the rounding branch then.
fn ascent_descent_with_hacks(
    sk_ascent: f32,
    sk_descent: f32,
    subpixel_ascent_descent: bool,
    subpixel_positioning: bool,
) -> (f32, f32) {
    if subpixel_ascent_descent && (-sk_ascent < 3.0 || -sk_ascent + sk_descent < 2.0) {
        return (-sk_ascent, sk_descent);
    }
    // SkScalarRoundToScalar.
    let mut ascent = (-sk_ascent + 0.5).floor();
    let mut descent = (sk_descent + 0.5).floor();
    if cfg!(not(target_os = "windows"))
        && descent < sk_descent
        && subpixel_positioning
        && ascent >= 1.0
    {
        descent += 1.0;
        ascent -= 1.0;
    }
    (ascent, descent)
}

/// `TypoAscenderAndDescender`: OS/2 sTypoAscender and -sTypoDescender.
fn typo_ascender_and_descender(font_data: &FontPlatformData) -> Option<(i32, i32)> {
    let tag = u32::from_be_bytes(*b"OS/2");
    let table = font_data.typeface().copy_table_data(tag)?;
    let bytes = table.as_bytes();
    if bytes.len() < 72 {
        return Some((0, 0));
    }
    let ascender = i16::from_be_bytes([bytes[68], bytes[69]]) as i32;
    let descender = i16::from_be_bytes([bytes[70], bytes[71]]) as i32;
    Some((ascender as i16 as i32, (-descender) as i16 as i32))
}

/// `TrySetNormalizedTypoAscentAndDescent`.
fn normalized_typo(ascent: f32, descent: f32, em_height: f32) -> Option<(i32, i32)> {
    let height = ascent + descent;
    if height <= 0.0 || ascent < 0.0 || ascent > height {
        return None;
    }
    let normalized_ascent = layout_unit_round(ascent * em_height / height);
    Some((
        normalized_ascent,
        layout_unit_round(em_height).saturating_sub(normalized_ascent),
    ))
}

// ---------------------------------------------------------------------------
// gfx::RectF, for the ink bounds.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    /// `gfx::RectF(x, y, w, h)`: `SizeF` clamps negative sizes to 0.
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn is_empty(&self) -> bool {
        self.width == 0.0 || self.height == 0.0
    }

    /// `RectF::Union`.
    fn union(&mut self, other: &RectF) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = *other;
            return;
        }
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        *self = RectF::new(left, top, right - left, bottom - top);
    }
}

/// `InlineLayoutUnit`: 16 fractional bits in 64 bits.
fn inline_layout_unit_ceil(v: f32) -> i64 {
    (v as f64 * 65536.0).ceil() as i64
}

/// A glyph advance as the shaper's `TextRunLayoutUnit`. Div's advances are
/// floats (T8 makes them HarfBuzz's 16.16 values).
fn advance_units(v: f32) -> i64 {
    (v as f64 * 65536.0).round() as i64
}

/// `ShapeResult::ComputeInkBounds` for a horizontal result, with
/// `GlyphBoundsAccumulator` and `SkFontGetBoundsForGlyphs`.
fn compute_ink_bounds(result: &ShapeResult) -> RectF {
    let mut ink = RectF::default();
    let mut run_advance = 0.0f32;
    for run in &result.runs {
        let sk_font = run.font_data.sk_font();
        let mut bounds = vec![Rect::default(); run.glyphs.len()];
        sk_font.get_bounds(&run.glyphs, &mut bounds, None);
        if !sk_font.is_subpixel() {
            for b in &mut bounds {
                *b = Rect::new(
                    b.left.floor(),
                    b.top.floor(),
                    b.right.ceil(),
                    b.bottom.ceil(),
                );
            }
        }
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut origin = inline_layout_unit_ceil(run_advance);
        for (i, b) in bounds.iter().enumerate() {
            let glyph = RectF::new(b.left, b.top, b.right - b.left, b.bottom - b.top);
            if !glyph.is_empty() {
                let (dx, dy) = run.offsets.get(i).copied().unwrap_or((0.0, 0.0));
                let x = glyph.x + (origin as f64 / 65536.0) as f32 + dx;
                let y = glyph.y + dy;
                let right = x + glyph.width;
                let bottom = y + glyph.height;
                if min_x == 0.0 && max_x == 0.0 {
                    (min_x, max_x, min_y, max_y) = (x, right, y, bottom);
                } else {
                    min_x = min_x.min(x);
                    max_x = max_x.max(right);
                    min_y = min_y.min(y);
                    max_y = max_y.max(bottom);
                }
            }
            origin += advance_units(run.advances.get(i).copied().unwrap_or(0.0));
        }
        ink.union(&RectF::new(min_x, min_y, max_x - min_x, max_y - min_y));
        run_advance += run.advances.iter().sum::<f32>();
    }
    ink
}

// ---------------------------------------------------------------------------
// PlainTextNode.

/// `Character::IsNormalizedCanvasSpaceCharacter`.
fn is_normalized_canvas_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r')
}

/// `Character::TreatAsSpace`.
fn treat_as_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{a0}')
}

/// Characters that become a zero-width space for the shaper
/// (`TreatAsZeroWidthSpaceInComplexScriptLegacy`), approximated by the
/// controls and the bidi and zero-width formatting characters.
fn treat_as_zero_width_space(c: char) -> bool {
    let c = c as u32;
    c < 0x20
        || (0x7f..0xa0).contains(&c)
        || c == 0x200b
        || c == 0x200e
        || c == 0x200f
        || (0x202a..=0x202e).contains(&c)
        || (0x2066..=0x2069).contains(&c)
        || c == 0xfeff
}

/// Characters that may need bidi reordering (`Character::MaybeBidiRtl`),
/// approximated from above: bidi analysis then decides.
fn maybe_bidi_rtl(c: char) -> bool {
    c as u32 >= 0x0590
}

/// CJK ideographs, kana, CJK symbols and emoji, approximated
/// (`Character::IsCJKIdeographOrSymbol`).
fn is_cjk_ideograph_or_symbol(c: char) -> bool {
    let c = c as u32;
    (0x2e80..=0x9fff).contains(&c)
        || (0xf900..=0xfaff).contains(&c)
        || (0xfe30..=0xfe4f).contains(&c)
        || (0xff00..=0xffef).contains(&c)
        || (0x1f000..=0x1faff).contains(&c)
        || (0x20000..=0x3ffff).contains(&c)
        || (0x2600..=0x27bf).contains(&c)
}

/// Characters that never start a new CJK or emoji word: combining marks,
/// joiners, variation selectors and emoji modifiers.
fn continues_cjk_word(c: char) -> bool {
    let c = c as u32;
    (0x0300..=0x036f).contains(&c)
        || (0x3099..=0x309a).contains(&c)
        || c == 0x200d
        || (0xfe00..=0xfe0f).contains(&c)
        || (0xe0100..=0xe01ef).contains(&c)
        || (0x1f3fb..=0x1f3ff).contains(&c)
        || (0xe0020..=0xe007f).contains(&c)
}

fn is_word_delimiter(c: char, split_by_zws: bool) -> bool {
    c == ' ' || c == '\t' || (split_by_zws && c == '\u{200b}')
}

/// `NextWordEndIndex` over characters.
fn next_word_end(text: &[char], start: usize, is_8bit: bool) -> usize {
    let length = text.len();
    if start >= length {
        return 0;
    }
    if start + 1 == length || is_word_delimiter(text[start], true) {
        return start + 1;
    }
    if is_8bit {
        let mut i = start + 1;
        while i < length && !is_word_delimiter(text[i], false) {
            i += 1;
        }
        return i;
    }
    if !is_cjk_ideograph_or_symbol(text[start]) {
        let mut end = start + 1;
        while end < length {
            let c = text[end];
            if is_word_delimiter(c, true) || is_cjk_ideograph_or_symbol(c) {
                return end;
            }
            end += 1;
        }
        return length;
    }
    let mut end = start + 1;
    while end < length && continues_cjk_word(text[end]) {
        end += 1;
    }
    end
}

/// One word, space or CJK character of a `PlainTextNode`, shaped.
/// A `PlainTextItem`: one word (or space) of one bidi run, shaped, with
/// its spacing applied and its ink bounds.
pub(crate) struct TextItem {
    pub text: String,
    pub direction: TextDirection,
    pub result: ShapeResult,
    pub ink_bounds: RectF,
}

/// A `PlainTextNode` built for canvas text: normalized, split into bidi
/// runs and words, shaped word by word. Items are shared with the shape
/// cache (`crate::shape_cache`).
pub(crate) struct TextNode {
    pub items: Vec<Arc<TextItem>>,
    pub base_rtl: bool,
}

pub(crate) struct Segment {
    pub text: String,
    pub start: usize,
    pub direction: TextDirection,
}

/// `PlainTextNode::SegmentText` (canvas: spaces normalized, bidi
/// supported, no directional override).
pub(crate) fn segment(text: &str, direction: TextDirection) -> (Vec<Segment>, bool) {
    let original: Vec<char> = text.chars().collect();
    let mut changed = false;
    let mut maybe_bidi = false;
    let normalized: Vec<char> = original
        .iter()
        .map(|&c| {
            let n = if is_normalized_canvas_space(c) || (treat_as_space(c) && c != '\u{a0}') {
                ' '
            } else if treat_as_zero_width_space(c) {
                '\u{200b}'
            } else {
                c
            };
            changed |= n != c;
            maybe_bidi |= maybe_bidi_rtl(c);
            n
        })
        .collect();
    // A normalized string is a new 16-bit buffer.
    let is_8bit = !changed && original.iter().all(|&c| (c as u32) <= 0xff);

    let mut base_rtl = direction.is_rtl();
    let mut runs: Vec<(usize, usize, TextDirection)> = Vec::new();
    if maybe_bidi || direction.is_rtl() {
        let paragraph = BidiParagraph::new(text, Some(direction));
        base_rtl = paragraph.base_direction().is_rtl();
        let unidirectional_ltr = !base_rtl && paragraph.levels().iter().all(|l| l.number() == 0);
        if !unidirectional_ltr {
            // Byte offsets to character offsets.
            let mut char_at_byte = vec![0usize; text.len() + 1];
            for (index, (byte, _)) in text.char_indices().enumerate() {
                char_at_byte[byte] = index;
            }
            char_at_byte[text.len()] = original.len();
            for run in paragraph.visual_runs() {
                let start = char_at_byte[run.start];
                let end = char_at_byte[run.end];
                runs.push((start, end - start, run.direction));
            }
        }
    }
    if runs.is_empty() {
        runs.push((
            0,
            normalized.len(),
            if base_rtl {
                TextDirection::Rtl
            } else {
                TextDirection::Ltr
            },
        ));
    }

    let mut segments = Vec::new();
    for (start, length, dir) in runs {
        let words = &normalized[start..start + length];
        let insertion = segments.len();
        let mut index = 0;
        while index < length {
            let end = next_word_end(words, index, is_8bit);
            let segment = Segment {
                text: words[index..end].iter().collect(),
                start: start + index,
                direction: dir,
            };
            if dir.is_ltr() {
                segments.push(segment);
            } else {
                segments.insert(insertion, segment);
            }
            index = end;
        }
    }
    (segments, base_rtl)
}

impl TextItem {
    /// One item of `PlainTextNode::Shape`: shapes `segment` with `shape`,
    /// whose font carries the spacing. Chromium adds no word spacing to a
    /// space at the start of the text (`ShapeResultSpacing::ComputeSpacing`).
    pub fn shape(
        segment: Segment,
        word_spacing: f32,
        shape: impl FnOnce(&str, TextDirection) -> ShapeResult,
    ) -> Self {
        let mut result = shape(&segment.text, segment.direction);
        if segment.start == 0 && word_spacing != 0.0 && segment.text.starts_with(' ') {
            remove_leading_word_spacing(&mut result, word_spacing);
        }
        let mut ink_bounds = compute_ink_bounds(&result);
        if result.width < 0.0 {
            // Negative spacing can move glyphs left of the origin.
            let left = result.width.min(ink_bounds.width);
            if left < ink_bounds.x {
                let grow = ink_bounds.x - left;
                ink_bounds = RectF::new(
                    ink_bounds.x - grow,
                    ink_bounds.y,
                    ink_bounds.width + grow,
                    ink_bounds.height,
                );
            }
        }
        Self {
            text: segment.text,
            direction: segment.direction,
            result,
            ink_bounds,
        }
    }
}

impl TextNode {
    /// `PlainTextNode::AccumulateInlineSize`.
    pub fn accumulate_inline_size(&self) -> (f32, RectF) {
        let mut inline_size = 0.0f32;
        let mut bounds = RectF::default();
        for item in &self.items {
            if self.base_rtl {
                inline_size -= item.result.width;
            }
            let mut adjusted = item.ink_bounds;
            adjusted.x += inline_size;
            bounds.union(&adjusted);
            if !self.base_rtl {
                inline_size += item.result.width;
            }
        }
        if self.base_rtl {
            inline_size = -inline_size;
            bounds.x += inline_size;
        }
        (inline_size, bounds)
    }

    /// `TextMetrics::MeasureRuns`.
    pub fn measure_runs(&self) -> (f32, RectF) {
        let mut bounds = RectF::default();
        let mut xpos = 0.0f32;
        for item in &self.items {
            let mut run = item.ink_bounds;
            run.x += xpos;
            bounds.union(&run);
            xpos += item.result.width;
        }
        (xpos, bounds)
    }

    /// Every item's runs, in visual order, as one result to draw (Blink
    /// draws a node as one text blob).
    pub fn to_result(&self) -> ShapeResult {
        let mut combined = ShapeResult::empty(if self.base_rtl {
            TextDirection::Rtl
        } else {
            TextDirection::Ltr
        });
        for item in &self.items {
            combined.width += item.result.width;
            combined.num_characters += item.result.num_characters;
            combined.runs.extend(item.result.runs.iter().map(clone_run));
        }
        combined
    }
}

fn clone_run(run: &ShapeResultRun) -> ShapeResultRun {
    ShapeResultRun {
        font_data: run.font_data.clone(),
        glyphs: run.glyphs.clone(),
        advances: run.advances.clone(),
        offsets: run.offsets.clone(),
        clusters: run.clusters.clone(),
        start_index: run.start_index,
        num_characters: run.num_characters,
        num_glyphs: run.num_glyphs,
        direction: run.direction,
    }
}

/// Takes the word spacing back off a space at the start of the text
/// (`ShapeResultSpacing::ComputeSpacing` adds it only past index 0).
fn remove_leading_word_spacing(result: &mut ShapeResult, word_spacing: f32) {
    for run in &mut result.runs {
        if run.start_index != 0 {
            continue;
        }
        if let Some(i) = run.clusters.iter().rposition(|&c| c == 0) {
            run.advances[i] -= word_spacing;
            result.width -= word_spacing;
            return;
        }
    }
}

/// `TextMetrics`, with the attributes Chromium 147 exposes to script
/// (`emHeightAscent` and `emHeightDescent` sit behind a runtime flag that is
/// off there).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextMetrics {
    pub width: f64,
    pub actual_bounding_box_left: f64,
    pub actual_bounding_box_right: f64,
    pub font_bounding_box_ascent: f64,
    pub font_bounding_box_descent: f64,
    pub actual_bounding_box_ascent: f64,
    pub actual_bounding_box_descent: f64,
    pub hanging_baseline: f64,
    pub alphabetic_baseline: f64,
    pub ideographic_baseline: f64,
}

impl TextMetrics {
    /// `TextMetrics::Update`.
    pub(crate) fn new(
        node: &TextNode,
        metrics: &PrimaryMetrics,
        direction: TextDirection,
        align: TextAlign,
        baseline: TextBaseline,
    ) -> Self {
        let (xpos, glyph_bounds) = node.measure_runs();
        let real_width = xpos as f64;
        let rtl = direction.is_rtl();
        let text_align_dx: f32 = match align {
            TextAlign::Center => (real_width / 2.0) as f32,
            TextAlign::Right => real_width as f32,
            TextAlign::Start if rtl => real_width as f32,
            TextAlign::End if !rtl => real_width as f32,
            _ => 0.0,
        };
        let ascent = metrics.ascent;
        let descent = metrics.descent;
        let baseline_y = metrics.baseline(baseline);
        Self {
            width: real_width,
            actual_bounding_box_left: (-glyph_bounds.x + text_align_dx) as f64,
            actual_bounding_box_right: (glyph_bounds.right() - text_align_dx) as f64,
            font_bounding_box_ascent: (ascent - baseline_y) as f64,
            font_bounding_box_descent: (descent + baseline_y) as f64,
            actual_bounding_box_ascent: (-glyph_bounds.y - baseline_y) as f64,
            actual_bounding_box_descent: (glyph_bounds.bottom() + baseline_y) as f64,
            hanging_baseline: (ascent * 80.0 / 100.0 - baseline_y) as f64,
            alphabetic_baseline: (-baseline_y) as f64,
            ideographic_baseline: (-descent - baseline_y) as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_with_six_significant_digits() {
        assert_eq!(append_number(10.0), "10");
        assert_eq!(append_number(12.5), "12.5");
        assert_eq!(append_number(10.3), "10.3");
        assert_eq!(append_number(-0.25), "-0.25");
        assert_eq!(append_number(1.0 / 3.0), "0.333333");
    }

    #[test]
    fn spacing_needs_one_length_dimension() {
        assert_eq!(string_to_num_with_unit("2px"), Some((2.0, SpacingUnit::Px)));
        assert_eq!(
            string_to_num_with_unit("-1.5EM"),
            Some((-1.5, SpacingUnit::Em))
        );
        assert_eq!(
            string_to_num_with_unit("1e1px"),
            Some((10.0, SpacingUnit::Px))
        );
        assert_eq!(string_to_num_with_unit("0"), None);
        assert_eq!(string_to_num_with_unit(" 2px"), None);
        assert_eq!(string_to_num_with_unit("2px "), None);
        assert_eq!(string_to_num_with_unit("2deg"), None);
        assert_eq!(
            string_to_num_with_unit(".5px"),
            Some((0.5, SpacingUnit::Px))
        );
    }

    #[test]
    fn words_split_at_spaces() {
        let (segments, rtl) = segment("ab  c\td", TextDirection::Ltr);
        assert!(!rtl);
        let words: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(words, ["ab", " ", " ", "c", " ", "d"]);
    }

    #[test]
    fn family_serialization_quotes_non_identifiers() {
        let list = FontFamilyList::single("Canvas Sans").push_generic(GenericFontFamily::Serif);
        assert_eq!(serialize_family(&list), "\"Canvas Sans\", serif");
        assert_eq!(
            serialize_family(&FontFamilyList::single("CanvasSans")),
            "CanvasSans"
        );
    }
}
