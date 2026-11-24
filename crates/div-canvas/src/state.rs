//! `blink::CanvasRenderingContext2DState` (`canvas_rendering_context_2d_state.cc`,
//! Chromium BSD) for the parts this crate implements: colour styles, line
//! styles, dashes, global alpha and the composite mode.

use div_style::Color;
use skia_safe::{dash_path_effect, BlendMode, Paint, PaintCap, PaintJoin, PaintStyle};

use crate::affine::AffineTransform;
use crate::style::CanvasStyle;
use crate::text::TextState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

impl LineCap {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "butt" => Some(Self::Butt),
            "round" => Some(Self::Round),
            "square" => Some(Self::Square),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Butt => "butt",
            Self::Round => "round",
            Self::Square => "square",
        }
    }

    fn to_skia(self) -> PaintCap {
        match self {
            Self::Butt => PaintCap::Butt,
            Self::Round => PaintCap::Round,
            Self::Square => PaintCap::Square,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

impl LineJoin {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "miter" => Some(Self::Miter),
            "round" => Some(Self::Round),
            "bevel" => Some(Self::Bevel),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Miter => "miter",
            Self::Round => "round",
            Self::Bevel => "bevel",
        }
    }

    fn to_skia(self) -> PaintJoin {
        match self {
            Self::Miter => PaintJoin::Miter,
            Self::Round => PaintJoin::Round,
            Self::Bevel => PaintJoin::Bevel,
        }
    }
}

/// `kCanvasCompositeOperatorNames` then `kCanvasBlendModeNames`, with the
/// `SkBlendMode` each maps to (`ToSkBlendMode`).
const COMPOSITE_NAMES: &[(&str, BlendMode)] = &[
    ("clear", BlendMode::Clear),
    ("copy", BlendMode::Src),
    ("source-over", BlendMode::SrcOver),
    ("source-in", BlendMode::SrcIn),
    ("source-out", BlendMode::SrcOut),
    ("source-atop", BlendMode::SrcATop),
    ("destination-over", BlendMode::DstOver),
    ("destination-in", BlendMode::DstIn),
    ("destination-out", BlendMode::DstOut),
    ("destination-atop", BlendMode::DstATop),
    ("xor", BlendMode::Xor),
    ("lighter", BlendMode::Plus),
    ("normal", BlendMode::SrcOver),
    ("multiply", BlendMode::Multiply),
    ("screen", BlendMode::Screen),
    ("overlay", BlendMode::Overlay),
    ("darken", BlendMode::Darken),
    ("lighten", BlendMode::Lighten),
    ("color-dodge", BlendMode::ColorDodge),
    ("color-burn", BlendMode::ColorBurn),
    ("hard-light", BlendMode::HardLight),
    ("soft-light", BlendMode::SoftLight),
    ("difference", BlendMode::Difference),
    ("exclusion", BlendMode::Exclusion),
    ("hue", BlendMode::Hue),
    ("saturation", BlendMode::Saturation),
    ("color", BlendMode::Color),
    ("luminosity", BlendMode::Luminosity),
];

pub(crate) fn parse_composite(name: &str) -> Option<BlendMode> {
    COMPOSITE_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, mode)| mode)
}

/// `CanvasCompositeOperatorName(CompositeAndBlendOpsFromSkBlendMode(mode))`:
/// the first name in the table that maps to `mode`, except that source-over
/// is named by the composite table, never `normal`.
pub(crate) fn composite_name(mode: BlendMode) -> &'static str {
    COMPOSITE_NAMES
        .iter()
        .find(|(_, m)| *m == mode)
        .map(|&(n, _)| n)
        .unwrap_or("source-over")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintType {
    Fill,
    Stroke,
}

/// One entry of the state stack.
#[derive(Clone)]
pub(crate) struct CanvasState {
    pub transform: AffineTransform,
    pub transform_invertible: bool,
    pub fill_style: CanvasStyle,
    pub stroke_style: CanvasStyle,
    pub global_alpha: f64,
    pub line_width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
    /// Already doubled when odd, and each value already `ClampTo<float>`.
    pub line_dash: Vec<f64>,
    pub line_dash_offset: f32,
    pub composite: BlendMode,
    pub has_clip: bool,
    pub has_complex_clip: bool,
    pub text: TextState,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: AffineTransform::IDENTITY,
            transform_invertible: true,
            fill_style: CanvasStyle::Color(Color::BLACK),
            stroke_style: CanvasStyle::Color(Color::BLACK),
            global_alpha: 1.0,
            line_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 10.0,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
            composite: BlendMode::SrcOver,
            has_clip: false,
            has_complex_clip: false,
            text: TextState::default(),
        }
    }
}

impl CanvasState {
    /// `GetFlags(paint_type, kDrawForegroundOnly)` as an `SkPaint`.
    pub fn paint(&self, paint_type: PaintType) -> Paint {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_blend_mode(self.composite);
        let global_alpha = self.global_alpha as f32;
        match paint_type {
            PaintType::Fill => {
                paint.set_style(PaintStyle::Fill);
                self.fill_style.apply_to_paint(&mut paint, global_alpha);
            }
            PaintType::Stroke => {
                paint.set_style(PaintStyle::Stroke);
                self.stroke_style.apply_to_paint(&mut paint, global_alpha);
                paint.set_stroke_width(self.line_width);
                paint.set_stroke_cap(self.line_cap.to_skia());
                paint.set_stroke_join(self.line_join.to_skia());
                paint.set_stroke_miter(self.miter_limit);
                // UpdateLineDash: no effect unless some element is non-zero.
                if self.line_dash.iter().any(|&d| d != 0.0) {
                    let intervals: Vec<f32> = self.line_dash.iter().map(|&d| d as f32).collect();
                    paint.set_path_effect(dash_path_effect::new(&intervals, self.line_dash_offset));
                }
            }
        }
        paint
    }
}
