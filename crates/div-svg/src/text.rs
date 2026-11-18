//! SVG text layout: `<text>` and `<tspan>`, as Chromium 147 lays them out.
//!
//! Ported from Blink's BSD-licensed SVG text code at 147.0.7727.50:
//!
//! - `core/layout/svg/svg_text_layout_attributes_builder.cc`: resolving the
//!   `x`, `y`, `dx`, `dy` and `rotate` lists across nested `tspan`s
//!   ([`resolve_attributes`]);
//! - `core/layout/svg/svg_text_layout_algorithm.cc`: SVG 2's text layout
//!   algorithm as Blink runs it (`dx`/`dy`, `x`/`y`, chunks and
//!   `text-anchor`, and the write-back to fragments);
//! - `core/layout/inline/fragment_item.cc`: the rotation of a character
//!   about the start of its baseline ([`TextFragment::rotation`]);
//! - `core/paint/text_fragment_painter.cc` and `core/paint/text_painter.cc`:
//!   painting a fragment's glyphs ([`paint`]): the origin, the
//!   `scale(1 / sf)` and rotation it is drawn under, fill and stroke in
//!   `paint-order`, and the stroke scaled with the font.
//!
//! Blink's SVG inline layout objects (`layout_svg_inline_text`,
//! `layout_svg_tspan`, `layout_svg_inline`) and its character record
//! (`svg_character_data.h`) are LGPL and were not read. The parts they cover
//! are written from SVG 2 §11, CSS Text 3 and CSS Inline 3, and measured
//! against Chromium as a black box by `tests/text_corpus.rs`
//! (`tests/fidelity/fixtures/svg-text-v1`):
//!
//! - whitespace collapsing and the per-character record;
//! - the scaled-font rule: text is shaped at its size on screen,
//!   `font-size × s` with `s = √((a² + b² + c² + d²) / 2)` of the transform
//!   to device pixels ([`scaling_factor`]), with the `opsz` axis at the
//!   unscaled size, and laid out in that scaled space;
//! - the CSS positions: consecutive text with the same font is shaped as one
//!   run and split per element; each element's text advances the line by
//!   its width rounded up to 1/64 px, and each typographic character starts
//!   at its offset rounded down to 1/64 px and spans to its end rounded up;
//! - baselines: `dominant-baseline` picks the ascent a character's box hangs
//!   from, `alignment-baseline` and `baseline-shift` move a `tspan`'s
//!   baseline, all in 1/64 px.
//!
//! Shaping, fonts, metrics and glyph blobs come from `div-text`: each
//! fragment's glyphs are a positioned blob over its part of the shaped run
//! (`ShapeResult::to_positioned_text_blob`), rasterized with the raster
//! configuration HTML text uses.

use std::ops::Range;
use std::sync::Arc;

use div_geometry::{RasterBackend, RasterConfiguration, TextEdging};
use div_style::{FontFamilyList, FontStyleEnum, FontVariation, FontWeight};
use div_text::shaping::TextRasterPolicy;
use div_text::{
    Font, FontCollection, FontDescription, FontMetrics, FontPlatformData, ShapeResult,
    TextDirection, TextShaper,
};
use skia_safe::{Canvas, Paint};

use crate::length::{Axis, Length, LengthContext, Unit};
use crate::number::Scanner;
use crate::paint::{prepare_paint, setup_stroke, to_sk_matrix, PaintLayer, SvgPaintStyle};
use crate::transform::Matrix;
use crate::{Point, Rect};

/// Layout units per CSS pixel: Blink's `LayoutUnit` is fixed point in 1/64.
const LAYOUT_UNIT: f32 = 64.0;

/// `text-anchor`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAnchor {
    #[default]
    Start,
    Middle,
    End,
}

impl TextAnchor {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "start" => Self::Start,
            "middle" => Self::Middle,
            "end" => Self::End,
            _ => return None,
        })
    }
}

/// The baselines V2a supports, for `dominant-baseline` and
/// `alignment-baseline`. `Auto` is the alphabetic baseline in horizontal
/// text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Baseline {
    #[default]
    Auto,
    Alphabetic,
    Middle,
    Central,
    TextBeforeEdge,
    TextAfterEdge,
}

impl Baseline {
    /// A `dominant-baseline` value. `text-top` and `text-bottom` are the
    /// CSS Inline 3 names of the edges.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "auto" => Self::Auto,
            "alphabetic" => Self::Alphabetic,
            "middle" => Self::Middle,
            "central" => Self::Central,
            "text-before-edge" | "text-top" => Self::TextBeforeEdge,
            "text-after-edge" | "text-bottom" => Self::TextAfterEdge,
            _ => return None,
        })
    }

    /// An `alignment-baseline` value; `baseline` aligns on the dominant
    /// baseline, as `auto` does.
    pub fn parse_alignment(text: &str) -> Option<Self> {
        match text.trim() {
            "baseline" => Some(Self::Auto),
            other => Self::parse(other),
        }
    }
}

/// `baseline-shift`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BaselineShift {
    #[default]
    Baseline,
    Sub,
    Super,
    /// A length; a percentage is of the font size. Positive values raise.
    Length(Length),
}

impl BaselineShift {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "baseline" => Self::Baseline,
            "sub" => Self::Sub,
            "super" => Self::Super,
            other => Self::Length(Length::parse(other)?),
        })
    }
}

/// How white space in the text content is handled (CSS `white-space`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    /// `normal`: runs of spaces, tabs and line feeds collapse to one space,
    /// and spaces at the start and end of the text are removed.
    #[default]
    Collapse,
    /// `pre`: every space is kept. Tabs and line feeds become spaces (not
    /// measured against Chromium).
    Preserve,
}

/// The font properties of a span, in CSS pixels before the scaled-font rule.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFont {
    pub family: FontFamilyList,
    pub size: f32,
    pub weight: f32,
    pub style: FontStyleEnum,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    /// How glyphs are rasterized: the computed style's raster
    /// configuration, as for HTML text.
    pub raster_configuration: RasterConfiguration,
}

impl TextFont {
    /// The description `div-text` shapes with, at `scaling_factor` times
    /// this font's size and spacing.
    pub fn description(&self, scaling_factor: f32) -> FontDescription {
        let mut description = FontDescription::default();
        description.family = self.family.clone();
        description.size = self.size * scaling_factor;
        description.specified_size = description.size;
        description.weight = FontWeight(self.weight);
        description.style = self.style;
        description.letter_spacing = self.letter_spacing * scaling_factor;
        description.word_spacing = self.word_spacing * scaling_factor;
        description.raster_configuration = self.raster_configuration;
        // Optical sizing follows the unscaled size (measured with Inter's
        // `opsz` axis at device scale 2).
        description.variation_settings.push(FontVariation {
            tag: *b"opsz",
            value: self.size,
        });
        description
    }
}

/// The computed style of a `<text>` or `<tspan>` that layout reads.
/// Inherited properties are already inherited; `alignment_baseline` and
/// `baseline_shift` are the element's own (they are ignored on `<text>`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub font: TextFont,
    pub text_anchor: TextAnchor,
    pub dominant_baseline: Baseline,
    pub alignment_baseline: Baseline,
    pub baseline_shift: BaselineShift,
    pub white_space: WhiteSpace,
}

/// A `<text>` or `<tspan>`: its style, positioning attributes and content.
#[derive(Clone, Debug)]
pub struct TextSpan {
    pub style: TextStyle,
    pub x: Vec<Length>,
    pub y: Vec<Length>,
    pub dx: Vec<Length>,
    pub dy: Vec<Length>,
    /// `rotate`, in degrees.
    pub rotate: Vec<f32>,
    /// What this element's lengths resolve against: the viewport, and its
    /// own font size and x-height.
    pub lengths: LengthContext,
    pub children: Vec<TextChild>,
}

#[derive(Clone, Debug)]
pub enum TextChild {
    Text(String),
    Span(TextSpan),
}

/// Parses a `<length>` list such as `x` or `dx`. An invalid list is empty.
/// Each number is read as CSS reads it, in double precision and then
/// rounded to single (measured: `0.355em` differs by an ulp otherwise).
pub fn parse_length_list(text: &str) -> Vec<Length> {
    let mut out = Vec::new();
    for token in text
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|t| !t.is_empty())
    {
        match parse_css_length(token) {
            Some(length) => out.push(length),
            None => return Vec::new(),
        }
    }
    out
}

fn parse_css_length(token: &str) -> Option<Length> {
    let mut length = Length::parse(token)?;
    let digits = token
        .find(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E' || c == '%')
        .map_or(token, |end| &token[..end]);
    let digits = match digits.find(['e', 'E']) {
        // An exponent letter followed by no digits starts the unit (`em`, `ex`).
        Some(e)
            if !digits[e + 1..]
                .starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '-') =>
        {
            &digits[..e]
        }
        _ => digits,
    };
    length.value = digits.parse::<f64>().ok()? as f32;
    Some(length)
}

/// Parses a `<number>` list such as `rotate`. An invalid list is empty.
pub fn parse_number_list(text: &str) -> Vec<f32> {
    let mut scanner = Scanner::new(text);
    let mut out = Vec::new();
    scanner.skip_ws();
    while !scanner.at_end() {
        match scanner.number() {
            Some(n) => out.push(n),
            None => return Vec::new(),
        }
        scanner.skip_comma_ws();
    }
    out
}

/// The scaled-font rule's factor for text whose user space maps to device
/// pixels by `to_device` (the CTM times the device scale):
/// `√((a² + b² + c² + d²) / 2)`. Measured against Chromium; 1 when the
/// transform is degenerate.
pub fn scaling_factor(to_device: &Matrix) -> f32 {
    let Matrix { a, b, c, d, .. } = *to_device;
    let s = ((a * a + b * b + c * c + d * d) / 2.0).sqrt() as f32;
    if s.is_finite() && s > 0.0 {
        s
    } else {
        1.0
    }
}

/// One text item: Blink's `TextFragmentRareData` for an SVG text item, plus
/// the glyphs to paint. An item is a run of one element's text with one
/// font that only its first character positions: a character with its own
/// `x`, `y`, `dx`, `dy` or `rotate`, or one that starts a chunk, starts a
/// new item.
#[derive(Clone, Debug)]
pub struct TextFragment {
    /// The item's box in scaled space (user units times
    /// [`TextLayout::scaling_factor`]): `x` is the start of the advance,
    /// `y` is the baseline minus [`Self::baseline_ascent`].
    pub rect: Rect,
    /// Each character's advance from `rect.x`, in scaled px, rounded out to
    /// 1/64 px: Blink's `FragmentItem::LineLeftAndRightForOffsets`.
    pub character_bounds: Vec<(f32, f32)>,
    /// The ascent above the font's dominant baseline, in scaled px.
    pub baseline_ascent: f32,
    /// The font's integer alphabetic ascent, in scaled px. The glyphs'
    /// baseline is `rect.y + ascent`, and a rotation turns about
    /// `(rect.x, rect.y + ascent)`.
    pub ascent: f32,
    /// `rotate`, in degrees.
    pub angle: f32,
    /// The span the item belongs to, in pre-order (`<text>` is 0).
    pub span: usize,
    /// The item's addressable characters.
    pub characters: Range<usize>,
    /// The font of the fragment's glyphs.
    pub font: Arc<FontPlatformData>,
    pub glyphs: Vec<u16>,
    /// Advances and offsets in scaled px, from the start of the fragment.
    pub advances: Vec<f32>,
    pub offsets: Vec<(f32, f32)>,
    /// The shaped run the glyphs come from, and the fragment's characters
    /// within it. Painting builds the fragment's glyph blob from these.
    pub shape: Arc<ShapeResult>,
    pub shape_characters: Range<usize>,
}

impl TextFragment {
    /// The rotation about the start of the baseline, in scaled space; the
    /// identity when `angle` is 0. Blink: `FragmentItem::
    /// BuildSvgTransformForBoundingBox`, computed in double precision.
    pub fn rotation(&self) -> Matrix {
        if self.angle == 0.0 {
            return Matrix::IDENTITY;
        }
        let x = f64::from(self.rect.x);
        let y = f64::from(self.rect.y + self.ascent);
        Matrix::translate(x, y)
            .then_apply(&Matrix::rotate(f64::from(self.angle)))
            .then_apply(&Matrix::translate(-x, -y))
    }
}

/// A laid-out `<text>`.
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub scaling_factor: f32,
    /// The `<text>`'s raster configuration, which painting follows.
    pub raster_configuration: RasterConfiguration,
    /// Fragments in logical order, one per item.
    pub fragments: Vec<TextFragment>,
    /// For each addressable character, its fragment.
    pub character_fragments: Vec<usize>,
}

/// Lays out `text`, shaping with the fonts of `collection`. `to_device` is
/// the transform from the text's user space to device pixels.
pub fn layout(text: &TextSpan, collection: &Arc<FontCollection>, to_device: &Matrix) -> TextLayout {
    layout_scaled(text, collection, scaling_factor(to_device))
}

/// [`layout`] with the scaled-font factor given.
pub fn layout_scaled(
    text: &TextSpan,
    collection: &Arc<FontCollection>,
    scaling_factor: f32,
) -> TextLayout {
    let mut spans = Vec::new();
    let mut events = Vec::new();
    flatten(text, None, &mut spans, &mut events);
    collapse_white_space(&spans, &mut events);
    let resolved = resolve_attributes(&spans, &events);
    let fonts = SpanFonts::new(&spans, collection, scaling_factor);
    let mut lines = css_positions(&spans, &events, &fonts, &resolved, scaling_factor);
    debug_assert_eq!(lines.result.len(), resolved.len());
    let sf = scaling_factor;

    // 3. Resolve character positioning: copy `rotate` and `anchored chunk`.
    for (info, data) in lines.result.iter_mut().zip(&resolved) {
        if data.rotate.is_some() {
            info.rotate = data.rotate;
        }
        if data.anchored_chunk {
            info.anchored_chunk = true;
        }
    }
    let result = &mut lines.result;
    let css = &lines.css_positions;

    // 4. Adjust positions: dx, dy.
    let mut shift = (0.0f32, 0.0f32);
    for (i, data) in resolved.iter().enumerate() {
        if data.x.is_some() {
            shift.0 = 0.0;
        }
        if data.y.is_some() {
            shift.1 = 0.0;
        }
        shift.0 += data.dx.unwrap_or(0.0);
        shift.1 += data.dy.unwrap_or(0.0);
        result[i].x = clamp_f32(css[i].0 + shift.0 * sf);
        result[i].y = clamp_f32(css[i].1 + shift.1 * sf);
    }

    // 6. Adjust positions: x, y. `css.y` is the baseline shift of the
    // character's inline box, so it is kept.
    let mut shift = (0.0f32, 0.0f32);
    for i in 0..result.len() {
        let data = &resolved[i];
        if let Some(x) = data.x {
            shift.0 = clamp_f32(x * sf - css[i].0 - result[i].text_length_shift_x);
        }
        if let Some(y) = data.y {
            shift.1 = clamp_f32(y * sf - css[i].1 - result[i].text_length_shift_y + css[i].1);
        }
        result[i].x += shift.0;
        result[i].y += shift.1;
        if result[i].middle && result[i].anchored_chunk {
            result[i].anchored_chunk = false;
            if i + 1 < result.len() {
                result[i + 1].anchored_chunk = true;
            }
        }
    }

    // 7. Apply anchoring, chunk by chunk.
    let mut i = 0;
    while i < result.len() {
        let j = result[i + 1..]
            .iter()
            .position(|info| info.anchored_chunk)
            .map_or(result.len() - 1, |offset| i + offset);
        let mut min_position = f32::INFINITY;
        let mut max_position = f32::NEG_INFINITY;
        for info in &result[i..=j] {
            min_position = min_position.min(info.x);
            max_position = max_position.max(info.x + info.inline_size);
        }
        if min_position != f32::INFINITY {
            let anchor = spans[lines.items[result[i].item].span].style.text_anchor;
            let shift = match anchor {
                TextAnchor::Start => result[i].x - min_position,
                TextAnchor::End => result[i].x - max_position,
                TextAnchor::Middle => result[i].x - (min_position + max_position) / 2.0,
            };
            for info in &mut result[i..=j] {
                info.x += shift;
            }
        }
        i = j + 1;
    }

    // Write back to fragments, one per item.
    let mut fragments = Vec::with_capacity(lines.items.len());
    let mut character_fragments = Vec::with_capacity(result.len());
    let mut items = std::mem::take(&mut lines.items).into_iter();
    for info in result.iter() {
        if info.middle {
            character_fragments.push(fragments.len() - 1);
            continue;
        }
        let item = items.next().expect("an item per non-middle character");
        character_fragments.push(fragments.len());
        fragments.push(TextFragment {
            rect: Rect {
                x: clamp_f32(info.x),
                y: clamp_f32(info.y - item.baseline_ascent),
                width: clamp_f32(info.inline_size),
                height: clamp_f32(item.height),
            },
            character_bounds: item.character_bounds,
            baseline_ascent: item.baseline_ascent,
            ascent: item.ascent,
            angle: info.rotate.unwrap_or(0.0),
            span: item.span,
            characters: item.characters,
            font: item.font,
            glyphs: item.glyphs,
            advances: item.advances,
            offsets: item.offsets,
            shape: item.shape,
            shape_characters: item.shape_characters,
        });
    }
    TextLayout {
        scaling_factor: sf,
        raster_configuration: text.style.font.raster_configuration,
        fragments,
        character_fragments,
    }
}

fn clamp_f32(value: f32) -> f32 {
    value.clamp(f32::MIN, f32::MAX)
}

// ─── Painting ───────────────────────────────────────────────────────────

/// Paints a laid-out `<text>`: Blink's `TextFragmentPainter::Paint` for SVG
/// text and `TextPainter::PaintSvgTextFragment` (BSD), fragment by
/// fragment.
///
/// The canvas carries the text's transform to device pixels, the one its
/// scaling factor was computed from. `style` gives each span's paint (by
/// [`TextFragment::span`]). Each fragment is painted under `scale(1 / sf)`
/// and its rotation, at its box's origin truncated to 1/64 px plus the
/// font's integer ascent, with the glyphs at their offsets from the
/// fragment's start (the advances summed in single precision). Fill and
/// stroke are always anti-aliased (`shape-rendering` does not apply to
/// text), in `paint-order`; the stroke's width and dashes are scaled by the
/// scaling factor unless `vector-effect` is `non-scaling-stroke`.
pub fn paint(canvas: &Canvas, layout: &TextLayout, style: &dyn Fn(usize) -> SvgPaintStyle) {
    let sf = layout.scaling_factor;
    // `div-paint`'s choice for author text: Linux Chromium's monochrome
    // profile fits aliased strikes; otherwise the resolved font's edging
    // applies, with the LCD phase taken from the glyph origins.
    let author = layout.raster_configuration.author_text;
    let policy = if layout.raster_configuration.backend == RasterBackend::ChromiumLinux
        && author.edging == TextEdging::Alias
    {
        TextRasterPolicy::ChromiumAuthorAliased
    } else {
        TextRasterPolicy::Skia
    };
    let lcd = author.edging == TextEdging::SubpixelAntiAlias;
    // The text's object bounding box includes its glyphs' ink bounds
    // (`FragmentItem::ObjectBoundingBox`), which come with `getBBox` (V2b).
    // Until then a paint server in `objectBoundingBox` units does not
    // apply to text, and its fallback colour paints.
    let reference_box = None;
    for fragment in &layout.fragments {
        let style = style(fragment.span);
        // `EnsureShaderTransform`: a paint server is drawn under the same
        // `scale(1 / sf)` and rotation as the glyphs, so its shader takes
        // their inverse.
        let shader_transform = fragment
            .rotation()
            .inverse()
            .unwrap_or(Matrix::IDENTITY)
            .then_apply(&Matrix::scale(f64::from(sf), f64::from(sf)));
        let paints = fragment_paints(&style, sf, reference_box, &shader_transform);
        if paints.iter().all(Option::is_none) {
            continue;
        }
        let mut placements = Vec::with_capacity(fragment.advances.len());
        let mut x = 0.0f32;
        for (advance, offset) in fragment.advances.iter().zip(&fragment.offsets) {
            placements.push((x + offset.0, offset.1));
            x += advance;
        }
        // `PhysicalRect::FastAndLossyFromRectF` truncates to layout units.
        let left = from_layout(to_layout_trunc(fragment.rect.x));
        let Some(blob) = fragment.shape.to_positioned_text_blob(
            fragment.shape_characters.clone(),
            &placements,
            policy,
            1.0,
            lcd.then_some(left),
        ) else {
            continue;
        };
        let top = from_layout(to_layout_trunc(fragment.rect.y) + to_layout_round(fragment.ascent));
        canvas.save();
        if sf != 1.0 {
            canvas.scale((1.0 / sf, 1.0 / sf));
        }
        if fragment.angle != 0.0 {
            canvas.concat(&to_sk_matrix(&fragment.rotation()));
        }
        for paint in paints.iter().flatten() {
            canvas.draw_text_blob(&blob, (left, top), paint);
        }
        canvas.restore();
    }
}

/// `PrepareSvgPaints` and `OrderPaints`: the fill and stroke, in
/// `paint-order`.
fn fragment_paints(
    style: &SvgPaintStyle,
    sf: f32,
    reference_box: Option<Rect>,
    shader_transform: &Matrix,
) -> [Option<Paint>; 2] {
    let fill = style
        .has_fill()
        .then(|| {
            prepare_paint(
                &style.fill,
                style.fill_opacity,
                reference_box,
                Some(shader_transform),
            )
        })
        .flatten()
        .map(|mut flags| {
            flags.set_anti_alias(true);
            flags
        });
    let stroke = style
        .has_visible_stroke()
        .then(|| {
            prepare_paint(
                &style.stroke,
                style.stroke_opacity,
                reference_box,
                Some(shader_transform),
            )
        })
        .flatten()
        .map(|mut flags| {
            flags.set_anti_alias(true);
            // `PrepareStrokeGeometry`: the stroke follows the scaled font.
            let scale = if style.non_scaling_stroke { 1.0 } else { sf };
            setup_stroke(&style.stroke_style, scale, &mut flags);
            flags.set_stroke_width(style.stroke_style.width * scale);
            flags
        });
    let stroke_first = style
        .paint_order
        .0
        .iter()
        .find(|layer| **layer != PaintLayer::Markers)
        == Some(&PaintLayer::Stroke);
    if stroke_first {
        [stroke, fill]
    } else {
        [fill, stroke]
    }
}

// ─── Flattening and white space ─────────────────────────────────────────

/// The content of a `<text>` in document order, like Blink's inline items.
enum Event {
    Open(usize),
    Text { span: usize, text: String },
    Close,
}

struct FlatSpan<'a> {
    span: &'a TextSpan,
    parent: Option<usize>,
    style: &'a TextStyle,
}

fn flatten<'a>(
    span: &'a TextSpan,
    parent: Option<usize>,
    spans: &mut Vec<FlatSpan<'a>>,
    events: &mut Vec<Event>,
) {
    let index = spans.len();
    spans.push(FlatSpan {
        span,
        parent,
        style: &span.style,
    });
    events.push(Event::Open(index));
    for child in &span.children {
        match child {
            TextChild::Text(text) => events.push(Event::Text {
                span: index,
                text: text.clone(),
            }),
            TextChild::Span(child) => flatten(child, Some(index), spans, events),
        }
    }
    events.push(Event::Close);
}

fn is_collapsible(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// CSS Text 3 white-space processing for one line: collapsible runs become
/// one space, across element boundaries; a space at the start or end of the
/// text is removed.
fn collapse_white_space(spans: &[FlatSpan<'_>], events: &mut [Event]) {
    let mut after_space = true;
    let mut last_collapsible: Option<usize> = None;
    for (index, event) in events.iter_mut().enumerate() {
        let Event::Text { span, text } = event else {
            continue;
        };
        let collapse = spans[*span].style.white_space == WhiteSpace::Collapse;
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if is_collapsible(c) {
                if collapse {
                    if !after_space {
                        out.push(' ');
                        after_space = true;
                        last_collapsible = Some(index);
                    }
                    continue;
                }
                out.push(' ');
            } else {
                out.push(c);
            }
            after_space = false;
            last_collapsible = None;
        }
        *text = out;
    }
    if let Some(index) = last_collapsible {
        if let Event::Text { text, .. } = &mut events[index] {
            text.pop();
        }
    }
}

// ─── Resolving x, y, dx, dy and rotate (svg_text_layout_attributes_builder) ─

#[derive(Clone, Copy, Debug, Default)]
struct CharacterData {
    x: Option<f32>,
    y: Option<f32>,
    dx: Option<f32>,
    dy: Option<f32>,
    rotate: Option<f32>,
    anchored_chunk: bool,
}

/// One element's position lists and how many characters it has consumed.
struct AttributesIterator {
    x: Vec<f32>,
    y: Vec<f32>,
    dx: Vec<f32>,
    dy: Vec<f32>,
    rotate: Vec<f32>,
    consumed: usize,
}

impl AttributesIterator {
    fn new(span: &TextSpan) -> Self {
        let resolve = |list: &[Length], axis| {
            list.iter()
                .map(|l| l.resolve(axis, &span.lengths))
                .collect::<Vec<f32>>()
        };
        Self {
            x: resolve(&span.x, Axis::Horizontal),
            y: resolve(&span.y, Axis::Vertical),
            dx: resolve(&span.dx, Axis::Horizontal),
            dy: resolve(&span.dy, Axis::Vertical),
            rotate: span.rotate.clone(),
            consumed: 0,
        }
    }

    fn at(list: &[f32], index: usize) -> Option<f32> {
        list.get(index).copied()
    }

    fn matched_or_last_rotate(&self) -> Option<f32> {
        if self.rotate.is_empty() {
            return None;
        }
        Some(self.rotate[self.consumed.min(self.rotate.len() - 1)])
    }
}

/// Step 3 of SVG 2's text layout algorithm, without recursion. Blink:
/// `SvgTextLayoutAttributesBuilder::Build`, without `textPath` and
/// `textLength`.
fn resolve_attributes(spans: &[FlatSpan<'_>], events: &[Event]) -> Vec<CharacterData> {
    let mut stack: Vec<AttributesIterator> = Vec::new();
    let mut out = Vec::new();
    let mut is_first_char = true;
    let innermost = |stack: &[AttributesIterator], pick: fn(&AttributesIterator) -> &Vec<f32>| {
        stack
            .iter()
            .rev()
            .find_map(|it| AttributesIterator::at(pick(it), it.consumed))
    };
    for event in events {
        match event {
            Event::Open(index) => stack.push(AttributesIterator::new(spans[*index].span)),
            Event::Close => {
                stack.pop();
            }
            Event::Text { text, .. } => {
                for _ in text.chars() {
                    let mut data = CharacterData {
                        x: innermost(&stack, |it| &it.x),
                        y: innermost(&stack, |it| &it.y),
                        ..CharacterData::default()
                    };
                    data.anchored_chunk = data.x.is_some() || data.y.is_some();
                    if is_first_char {
                        is_first_char = false;
                        if data.x.is_none() {
                            data.x = Some(0.0);
                        }
                    }
                    data.dx = innermost(&stack, |it| &it.dx);
                    data.dy = innermost(&stack, |it| &it.dy);
                    data.rotate = stack
                        .iter()
                        .rev()
                        .find_map(AttributesIterator::matched_or_last_rotate);
                    out.push(data);
                    for it in &mut stack {
                        it.consumed += 1;
                    }
                }
            }
        }
    }
    out
}

// ─── Fonts, shaping and CSS positions ───────────────────────────────────

struct SpanFonts {
    /// For each span, the index of its font in `fonts`.
    of_span: Vec<usize>,
    fonts: Vec<(TextFont, Font)>,
}

impl SpanFonts {
    fn new(spans: &[FlatSpan<'_>], collection: &Arc<FontCollection>, sf: f32) -> Self {
        let mut fonts: Vec<(TextFont, Font)> = Vec::new();
        let mut of_span = Vec::with_capacity(spans.len());
        for span in spans {
            let font = &span.style.font;
            let index = match fonts.iter().position(|(f, _)| f == font) {
                Some(index) => index,
                None => {
                    let description = font.description(sf);
                    fonts.push((
                        font.clone(),
                        Font::new_in_collection(description, Arc::clone(collection)),
                    ));
                    fonts.len() - 1
                }
            };
            of_span.push(index);
        }
        Self { of_span, fonts }
    }

    fn font(&self, span: usize) -> &Font {
        &self.fonts[self.of_span[span]].1
    }

    fn metrics(&self, span: usize) -> FontMetrics {
        self.font(span)
            .font_metrics()
            .copied()
            .unwrap_or_else(FontMetrics::zero)
    }
}

/// The ascent and descent, in layout units, around `baseline`. Blink's
/// `FontMetrics::FixedAscent/FixedDescent(baseline)`, measured: the edges of
/// the integer ascent and descent, the middle half an x-height above the
/// alphabetic baseline, and the central halfway between the edges.
fn baseline_metrics(metrics: &FontMetrics, baseline: Baseline) -> (i64, i64) {
    let ascent = metrics.int_ascent();
    let descent = metrics.int_descent();
    let height = to_layout_round(ascent + descent);
    match baseline {
        Baseline::Auto | Baseline::Alphabetic => {
            (to_layout_round(ascent), to_layout_round(descent))
        }
        Baseline::Middle => {
            let half_x = metrics.x_height / 2.0;
            (
                to_layout_round(ascent - half_x),
                to_layout_round(descent + half_x),
            )
        }
        Baseline::Central => (height / 2, height - height / 2),
        Baseline::TextBeforeEdge => (0, height),
        Baseline::TextAfterEdge => (height, 0),
    }
}

fn to_layout_round(px: f32) -> i64 {
    (px * LAYOUT_UNIT).round() as i64
}

fn to_layout_floor(px: f32) -> i64 {
    (px * LAYOUT_UNIT).floor() as i64
}

fn to_layout_ceil(px: f32) -> i64 {
    (px * LAYOUT_UNIT).ceil() as i64
}

fn to_layout_trunc(px: f32) -> i64 {
    (px * LAYOUT_UNIT).trunc() as i64
}

fn from_layout(units: i64) -> f32 {
    units as f32 / LAYOUT_UNIT
}

/// How far a `tspan` moves its baseline down, in layout units, from its own
/// `alignment-baseline` and `baseline-shift`.
fn span_shift(span: &FlatSpan<'_>, metrics: &FontMetrics, sf: f32) -> i64 {
    let style = span.style;
    let mut shift = 0;
    if style.alignment_baseline != Baseline::Auto {
        let (dominant, _) = baseline_metrics(metrics, style.dominant_baseline);
        let (aligned, _) = baseline_metrics(metrics, style.alignment_baseline);
        shift += dominant - aligned;
    }
    let half_height = to_layout_round(metrics.int_ascent() + metrics.int_descent()) / 2;
    shift += match style.baseline_shift {
        BaselineShift::Baseline => 0,
        BaselineShift::Sub => half_height,
        BaselineShift::Super => -half_height,
        BaselineShift::Length(length) => {
            let px = match length.unit {
                Unit::Percent => length.value * style.font.size / 100.0,
                _ => length.resolve(Axis::Vertical, &span.span.lengths),
            };
            -to_layout_trunc(px * sf)
        }
    };
    shift
}

/// One text item on the line: a run of one element's text with one font,
/// split before every character that has its own positioning data.
struct Item {
    span: usize,
    characters: Range<usize>,
    /// The ascent above the dominant baseline and the box height, in
    /// scaled px.
    baseline_ascent: f32,
    height: f32,
    /// The integer alphabetic ascent, in scaled px.
    ascent: f32,
    character_bounds: Vec<(f32, f32)>,
    font: Arc<FontPlatformData>,
    glyphs: Vec<u16>,
    advances: Vec<f32>,
    offsets: Vec<(f32, f32)>,
    shape: Arc<ShapeResult>,
    shape_characters: Range<usize>,
}

/// Per-character state of the layout algorithm. Blink:
/// `SvgTextLayoutAlgorithm::SvgPerCharacterInfo`.
#[derive(Clone, Copy, Debug, Default)]
struct CharacterInfo {
    x: f32,
    y: f32,
    rotate: Option<f32>,
    middle: bool,
    anchored_chunk: bool,
    inline_size: f32,
    text_length_shift_x: f32,
    text_length_shift_y: f32,
    item: usize,
}

struct Line {
    items: Vec<Item>,
    result: Vec<CharacterInfo>,
    css_positions: Vec<(f32, f32)>,
}

/// Whether a character starts its own item. Blink: `HasUpdated` in
/// `svg_text_layout_attributes_builder.cc`.
fn has_updated(data: &CharacterData) -> bool {
    data.x.is_some()
        || data.y.is_some()
        || data.dx.is_some()
        || data.dy.is_some()
        || data.rotate.is_some()
        || data.anchored_chunk
}

/// Lays the text out as one CSS line in scaled space and records each
/// character's position (step 2 of the algorithm, Blink's `SetFlags`).
fn css_positions(
    spans: &[FlatSpan<'_>],
    events: &[Event],
    fonts: &SpanFonts,
    resolved: &[CharacterData],
    sf: f32,
) -> Line {
    // The text, split where a chunk starts: Chromium shapes each chunk on
    // its own, so kerning does not cross an absolute position.
    let mut texts: Vec<(usize, &str)> = Vec::new();
    let mut index = 0;
    for event in events {
        if let Event::Text { span, text } = event {
            let mut piece_start = 0;
            for (offset, _) in text.char_indices() {
                if offset > piece_start && resolved[index].anchored_chunk {
                    texts.push((*span, &text[piece_start..offset]));
                    piece_start = offset;
                }
                index += 1;
            }
            if piece_start < text.len() {
                texts.push((*span, &text[piece_start..]));
            }
        }
    }
    let starts_chunk = |texts: &[(usize, &str)], k: usize| -> bool {
        let before: usize = texts[..k].iter().map(|(_, t)| t.chars().count()).sum();
        resolved[before].anchored_chunk
    };
    let shaper = TextShaper::new();
    let mut line = Line {
        items: Vec::new(),
        result: Vec::new(),
        css_positions: Vec::new(),
    };
    let mut inline_offset: i64 = 0;
    // The addressable index of the current group's first character.
    let mut group_base = 0;
    let mut start = 0;
    while start < texts.len() {
        // Consecutive text with the same font is shaped as one run.
        let font_index = fonts.of_span[texts[start].0];
        let mut end = start + 1;
        while end < texts.len()
            && fonts.of_span[texts[end].0] == font_index
            && !starts_chunk(&texts, end)
        {
            end += 1;
        }
        let run_text: String = texts[start..end].iter().map(|(_, t)| *t).collect();
        let font = &fonts.fonts[font_index].1;
        let shaped = Arc::new(shaper.shape(&run_text, font, TextDirection::Ltr));
        // Each glyph's advance, by the character its cluster starts at.
        let mut glyph_advances: Vec<Vec<f32>> = vec![Vec::new(); shaped.num_characters];
        for run in &shaped.runs {
            for (g, advance) in run.advances.iter().enumerate() {
                let c = run.start_index + run.clusters[g];
                if let Some(list) = glyph_advances.get_mut(c) {
                    list.push(*advance);
                }
            }
        }
        // The caret position of each character within an item, summing
        // glyph advances from the item's start in single precision, as a
        // `ShapeResult` copied for the item does. A cluster's advance (a
        // ligature) is split evenly between its characters.
        let positions = |from: usize, to: usize| -> Vec<f32> {
            let mut out = Vec::with_capacity(to - from + 1);
            let mut x = 0.0f32;
            let mut c = from;
            while c < to {
                let mut next = c + 1;
                while next < to && glyph_advances[next].is_empty() {
                    next += 1;
                }
                let mut advance = 0.0f32;
                for a in &glyph_advances[c] {
                    advance += a;
                }
                let share = advance / (next - c) as f32;
                for j in 0..next - c {
                    out.push(x + share * j as f32);
                }
                x += advance;
                c = next;
            }
            out.push(x);
            out
        };
        let mut char_start = 0;
        for &(span, text) in &texts[start..end] {
            let char_end = char_start + text.chars().count();
            let metrics = fonts.metrics(span);
            let style = spans[span].style;
            let (baseline_ascent, baseline_descent) =
                baseline_metrics(&metrics, style.dominant_baseline);
            let mut block_offset = 0;
            let mut ancestor = Some(span);
            while let Some(index) = ancestor {
                if spans[index].parent.is_some() {
                    block_offset += span_shift(&spans[index], &fonts.metrics(index), sf);
                }
                ancestor = spans[index].parent;
            }
            let y = from_layout(block_offset);
            let mut item_start = char_start;
            while item_start < char_end {
                let mut item_end = item_start + 1;
                while item_end < char_end && !has_updated(&resolved[group_base + item_end]) {
                    item_end += 1;
                }
                let carets = positions(item_start, item_end);
                let inline_size = to_layout_ceil(carets[item_end - item_start]);
                let character_bounds = carets
                    .windows(2)
                    .map(|pair| {
                        (
                            from_layout(to_layout_floor(pair[0])),
                            from_layout(to_layout_ceil(pair[1])),
                        )
                    })
                    .collect();
                let mut item = Item {
                    span,
                    characters: group_base + item_start..group_base + item_end,
                    baseline_ascent: from_layout(baseline_ascent),
                    height: from_layout(baseline_ascent + baseline_descent),
                    ascent: metrics.int_ascent(),
                    character_bounds,
                    font: Arc::clone(font.primary_font().expect("a resolved font")),
                    glyphs: Vec::new(),
                    advances: Vec::new(),
                    offsets: Vec::new(),
                    shape: Arc::clone(&shaped),
                    shape_characters: item_start..item_end,
                };
                for run in &shaped.runs {
                    for g in 0..run.glyphs.len() {
                        let c = run.start_index + run.clusters[g];
                        if (item_start..item_end).contains(&c) {
                            if item.glyphs.is_empty() {
                                item.font = Arc::clone(&run.font_data);
                            }
                            item.glyphs.push(run.glyphs[g]);
                            item.advances.push(run.advances[g]);
                            item.offsets.push(run.offsets[g]);
                        }
                    }
                }
                let position = (from_layout(inline_offset), y);
                let item_index = line.items.len();
                for c in item_start..item_end {
                    let first = c == item_start;
                    line.result.push(CharacterInfo {
                        x: position.0,
                        y,
                        middle: !first,
                        anchored_chunk: first && line.items.is_empty(),
                        inline_size: if first { from_layout(inline_size) } else { 0.0 },
                        item: item_index,
                        ..CharacterInfo::default()
                    });
                    line.css_positions.push(position);
                }
                line.items.push(item);
                inline_offset += inline_size;
                item_start = item_end;
            }
            char_start = char_end;
        }
        group_base += char_start;
        start = end;
    }
    line
}

/// A character's box in scaled space, before its item's rotation. Blink:
/// `ScaledCharacterRectInContainer` in `svg_text_query.cc`, with
/// `FragmentItem::LocalRect`.
fn character_rect(fragment: &TextFragment, character: usize) -> Rect {
    let (x, width) = if fragment.characters.len() == 1 {
        (0.0, fragment.rect.width)
    } else {
        let (left, right) = fragment.character_bounds[character - fragment.characters.start];
        (left, right - left)
    };
    Rect {
        x: x + fragment.rect.x,
        y: fragment.rect.y,
        width,
        height: fragment.rect.height,
    }
}

/// The start of a character's advance on its baseline, in user units: the
/// SVG DOM's `getStartPositionOfChar`.
pub fn start_position(layout: &TextLayout, character: usize) -> Point {
    let fragment = &layout.fragments[layout.character_fragments[character]];
    let rect = character_rect(fragment, character);
    let point = Point {
        x: rect.x,
        y: rect.y + fragment.baseline_ascent,
    };
    to_user(layout, fragment, point)
}

/// The end of a character's advance on its baseline, in user units: the
/// SVG DOM's `getEndPositionOfChar`.
pub fn end_position(layout: &TextLayout, character: usize) -> Point {
    let fragment = &layout.fragments[layout.character_fragments[character]];
    let rect = character_rect(fragment, character);
    let point = Point {
        x: rect.x + rect.width,
        y: rect.y + fragment.baseline_ascent,
    };
    to_user(layout, fragment, point)
}

/// A character's box in user units: the SVG DOM's `getExtentOfChar`.
pub fn extent(layout: &TextLayout, character: usize) -> Rect {
    let fragment = &layout.fragments[layout.character_fragments[character]];
    let mut rect = character_rect(fragment, character);
    if fragment.angle != 0.0 {
        let m = fragment.rotation();
        let corners = [
            (rect.x, rect.y),
            (rect.x + rect.width, rect.y),
            (rect.x + rect.width, rect.y + rect.height),
            (rect.x, rect.y + rect.height),
        ]
        .map(|(x, y)| {
            let (x, y) = (f64::from(x), f64::from(y));
            (
                (m.a * x + m.c * y + m.e) as f32,
                (m.b * x + m.d * y + m.f) as f32,
            )
        });
        let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|c| c.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
        let max_y = corners
            .iter()
            .map(|c| c.1)
            .fold(f32::NEG_INFINITY, f32::max);
        rect = Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        };
    }
    let scale = 1.0 / layout.scaling_factor;
    Rect {
        x: rect.x * scale,
        y: rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

/// The sum of the characters' advances in user units: the SVG DOM's
/// `getComputedTextLength`.
pub fn computed_text_length(layout: &TextLayout) -> f32 {
    layout
        .fragments
        .iter()
        .fold(0.0, |sum, f| sum + f.rect.width / layout.scaling_factor)
}

fn to_user(layout: &TextLayout, fragment: &TextFragment, point: Point) -> Point {
    let point = if fragment.angle != 0.0 {
        let m = fragment.rotation();
        let (x, y) = (f64::from(point.x), f64::from(point.y));
        Point {
            x: (m.a * x + m.c * y + m.e) as f32,
            y: (m.b * x + m.d * y + m.f) as f32,
        }
    } else {
        point
    };
    let scale = 1.0 / layout.scaling_factor;
    Point {
        x: point.x * scale,
        y: point.y * scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists() {
        let xs = parse_length_list("10, 20.5 1em,3%");
        assert_eq!(xs.len(), 4);
        assert_eq!(xs[2].unit, Unit::Em);
        assert!(parse_length_list("10 bogus").is_empty());
        assert_eq!(parse_number_list("10 -20,30"), vec![10.0, -20.0, 30.0]);
        assert!(parse_number_list("10 x").is_empty());
    }

    #[test]
    fn parses_keywords() {
        assert_eq!(TextAnchor::parse("middle"), Some(TextAnchor::Middle));
        assert_eq!(Baseline::parse("central"), Some(Baseline::Central));
        assert_eq!(Baseline::parse_alignment("baseline"), Some(Baseline::Auto));
        assert_eq!(BaselineShift::parse("super"), Some(BaselineShift::Super));
        assert!(matches!(
            BaselineShift::parse("20%"),
            Some(BaselineShift::Length(Length {
                unit: Unit::Percent,
                ..
            }))
        ));
    }

    #[test]
    fn scaling_factor_is_the_rms_of_the_axes() {
        assert_eq!(scaling_factor(&Matrix::IDENTITY), 1.0);
        let m = Matrix::new(2.0, 0.0, 0.0, 1.0, 5.0, 7.0);
        assert_eq!(scaling_factor(&m), (2.5f64).sqrt() as f32);
        assert_eq!(
            scaling_factor(&Matrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)),
            1.0
        );
    }

    fn stroked() -> SvgPaintStyle {
        SvgPaintStyle {
            stroke: crate::paint::SvgPaint::Color(skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0)),
            shape_rendering: crate::paint::ShapeRendering::CrispEdges,
            ..SvgPaintStyle::default()
        }
    }

    #[test]
    fn glyph_paints_follow_paint_order_and_are_anti_aliased() {
        let [first, second] = fragment_paints(&stroked(), 1.0, None, &Matrix::IDENTITY);
        let (fill, stroke) = (first.unwrap(), second.unwrap());
        assert_eq!(fill.style(), skia_safe::PaintStyle::Fill);
        assert_eq!(stroke.style(), skia_safe::PaintStyle::Stroke);
        // `shape-rendering` does not apply to text.
        assert!(fill.is_anti_alias() && stroke.is_anti_alias());

        let mut style = stroked();
        style.paint_order = crate::paint::PaintOrder::parse("stroke").unwrap();
        let [first, _] = fragment_paints(&style, 1.0, None, &Matrix::IDENTITY);
        assert_eq!(first.unwrap().style(), skia_safe::PaintStyle::Stroke);

        style.fill = crate::paint::SvgPaint::None;
        assert!(fragment_paints(&style, 1.0, None, &Matrix::IDENTITY)[1].is_none());
    }

    #[test]
    fn the_stroke_follows_the_scaled_font() {
        let mut style = stroked();
        style.stroke_style.width = 1.5;
        let [_, stroke] = fragment_paints(&style, 2.0, None, &Matrix::IDENTITY);
        assert_eq!(stroke.unwrap().stroke_width(), 3.0);
        style.non_scaling_stroke = true;
        let [_, stroke] = fragment_paints(&style, 2.0, None, &Matrix::IDENTITY);
        assert_eq!(stroke.unwrap().stroke_width(), 1.5);
    }
}
