//! Paint servers (RFC 0009 V3b): `linearGradient`, `radialGradient` and
//! `pattern` as the `fill` or `stroke` of a shape.
//!
//! Ported from BSD files of Chromium 147.0.7727.50:
//!
//! - `platform/graphics/gradient.cc`: the stop list Skia is given
//!   (`FillSkiaStops`: the first stop copied to 0 and the last to 1), the
//!   spread method's tile mode, the interpolation setup and the dithering
//!   (`ApplyToFlags`), and the radii clamped to zero;
//! - `platform/graphics/paint_record_pattern.cc` and `cc/paint/
//!   paint_shader.cc`: a pattern is its tile's recording drawn by a picture
//!   shader that repeats in both directions and rasterizes at the drawing
//!   scale (`ScalingBehavior::kRasterAtScale`);
//! - `core/paint/svg_object_painter.cc` (in [`crate::paint`]): a paint
//!   server paints black with the opacity as alpha, quantized to 8 bits,
//!   and its fallback colour applies when the server does not.
//!
//! Blink's gradient and pattern layout objects and elements, and their
//! attribute collection, are LGPL and are not read. What they compute is
//! written from SVG 2 (§14.2 and §14.3) and checked against Chromium's
//! pixels by `tests/paint_corpus.rs` (`tests/fidelity/fixtures/
//! svg-paint-v4`):
//!
//! - **Attributes and `href`** ([`GradientAttributes`],
//!   [`PatternAttributes`]): an attribute the element does not set comes
//!   from the element it references, whatever that element's own source;
//!   geometry attributes only between gradients of the same kind. A
//!   gradient with no `stop` children takes the stops of the one it
//!   references, and a pattern with no children takes its content.
//! - **Lengths.** For `userSpaceOnUse`, against the referencing element's
//!   viewport; for `objectBoundingBox`, as fractions (`50%` is `0.5`).
//! - **Stops.** An offset is a number or a percentage, clamped to [0, 1],
//!   and no smaller than the offset before it.
//! - **The gradient space.** For `objectBoundingBox` the unit square maps
//!   to the reference box, and `gradientTransform` applies inside that
//!   space. A server in `objectBoundingBox` units does not apply to an
//!   empty box.
//! - **The radial focal point.** `fx` and `fy` default to the centre, `fr`
//!   to zero. The focal circle is the start circle of a two-point conical
//!   gradient and is used as given.
//! - **The pattern tile.** Its rectangle is `x`, `y`, `width` and `height`
//!   in `patternUnits`; its content is in `patternContentUnits` (scaled by
//!   the box for `objectBoundingBox`) or fitted by `viewBox`, from the
//!   tile's origin. `patternTransform` applies to the tile. A tile of zero
//!   area does not apply.
//!
//! References, cycles and which elements are servers are the caller's: a
//! reference that does not resolve to a server, or closes a cycle, is no
//! server, and its fallback colour (if any) paints instead.

use crate::length::{Axis, Length, LengthContext};
use crate::paint::to_sk_matrix;
use crate::transform::Matrix;
use crate::viewport::{view_box_transform, PreserveAspectRatio};
use crate::Rect;
use skia_safe::gradient::interpolation::{ColorSpace, HueMethod, InPremul};
use skia_safe::gradient::{shaders, Colors, Gradient as SkGradient, Interpolation};
use skia_safe::{
    Color4f, FilterMode, Picture, PictureRecorder, Point as SkPoint, Rect as SkRect, Shader,
    TileMode,
};
use std::fmt;

/// `gradientUnits`, `patternUnits` and `patternContentUnits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

impl Units {
    /// Parses the attribute; `None` for anything else, which leaves the
    /// attribute unset.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "userSpaceOnUse" => Some(Self::UserSpaceOnUse),
            "objectBoundingBox" => Some(Self::ObjectBoundingBox),
            _ => None,
        }
    }
}

/// `spreadMethod`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpreadMethod {
    #[default]
    Pad,
    Reflect,
    Repeat,
}

impl SpreadMethod {
    /// Parses the attribute; `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "pad" => Some(Self::Pad),
            "reflect" => Some(Self::Reflect),
            "repeat" => Some(Self::Repeat),
            _ => None,
        }
    }

    /// `Gradient::CreateShaderInternal`'s tile mode.
    fn tile_mode(self) -> TileMode {
        match self {
            Self::Pad => TileMode::Clamp,
            Self::Reflect => TileMode::Mirror,
            Self::Repeat => TileMode::Repeat,
        }
    }
}

/// A `stop` element's `offset` attribute: a number or a percentage, clamped
/// to [0, 1]. Anything else is 0.
pub fn parse_stop_offset(text: &str) -> f32 {
    let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let value = match text.strip_suffix('%') {
        Some(number) => number.parse::<f32>().map(|v| v / 100.0),
        None => text.parse::<f32>(),
    };
    match value {
        Ok(v) if v.is_finite() => v.clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// One gradient stop: its offset, and its `stop-color` with `stop-opacity`
/// multiplied into the alpha (sRGB, not premultiplied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color4f,
}

/// The stops of a gradient's `stop` children in document order, each
/// offset raised to the largest before it (SVG 2 §14.2.3).
pub fn gradient_stops(stops: impl IntoIterator<Item = (f32, Color4f)>) -> Vec<GradientStop> {
    let mut largest = 0.0f32;
    stops
        .into_iter()
        .map(|(offset, color)| {
            largest = largest.max(offset);
            GradientStop {
                offset: largest,
                color,
            }
        })
        .collect()
}

/// Which gradient element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
}

/// A gradient element's attributes as set on it, before `href`
/// inheritance. `stops` is `Some` when the element has `stop` children.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientAttributes {
    pub kind: GradientKind,
    pub units: Option<Units>,
    pub transform: Option<Matrix>,
    pub spread: Option<SpreadMethod>,
    pub stops: Option<Vec<GradientStop>>,
    pub x1: Option<Length>,
    pub y1: Option<Length>,
    pub x2: Option<Length>,
    pub y2: Option<Length>,
    pub cx: Option<Length>,
    pub cy: Option<Length>,
    pub r: Option<Length>,
    pub fx: Option<Length>,
    pub fy: Option<Length>,
    pub fr: Option<Length>,
}

impl GradientAttributes {
    /// No attribute set.
    pub fn new(kind: GradientKind) -> Self {
        Self {
            kind,
            units: None,
            transform: None,
            spread: None,
            stops: None,
            x1: None,
            y1: None,
            x2: None,
            y2: None,
            cx: None,
            cy: None,
            r: None,
            fx: None,
            fy: None,
            fr: None,
        }
    }

    /// `href` inheritance from `referenced`, itself already inherited from
    /// what it references: the attributes this element does not set. The
    /// geometry attributes come only from a gradient of the same kind.
    pub fn inherit(&mut self, referenced: &Self) {
        fn fill<T: Clone>(own: &mut Option<T>, other: &Option<T>) {
            if own.is_none() {
                own.clone_from(other);
            }
        }
        fill(&mut self.units, &referenced.units);
        fill(&mut self.transform, &referenced.transform);
        fill(&mut self.spread, &referenced.spread);
        fill(&mut self.stops, &referenced.stops);
        if self.kind != referenced.kind {
            return;
        }
        for (own, other) in [
            (&mut self.x1, &referenced.x1),
            (&mut self.y1, &referenced.y1),
            (&mut self.x2, &referenced.x2),
            (&mut self.y2, &referenced.y2),
            (&mut self.cx, &referenced.cx),
            (&mut self.cy, &referenced.cy),
            (&mut self.r, &referenced.r),
            (&mut self.fx, &referenced.fx),
            (&mut self.fy, &referenced.fy),
            (&mut self.fr, &referenced.fr),
        ] {
            fill(own, other);
        }
    }

    /// The gradient with the initial values for what is unset, its lengths
    /// resolved: for `userSpaceOnUse` against `context`, the referencing
    /// element's.
    pub fn resolve(&self, context: &LengthContext) -> Gradient {
        let units = self.units.unwrap_or(Units::ObjectBoundingBox);
        let context = units_context(units, context);
        let percent = |value: f32| Length {
            value,
            unit: crate::length::Unit::Percent,
        };
        let len = |length: Option<Length>, initial: f32, axis: Axis| {
            length.unwrap_or(percent(initial)).resolve(axis, &context)
        };
        let geometry = match self.kind {
            GradientKind::Linear => GradientGeometry::Linear {
                x1: len(self.x1, 0.0, Axis::Horizontal),
                y1: len(self.y1, 0.0, Axis::Vertical),
                x2: len(self.x2, 100.0, Axis::Horizontal),
                y2: len(self.y2, 0.0, Axis::Vertical),
            },
            GradientKind::Radial => {
                let cx = len(self.cx, 50.0, Axis::Horizontal);
                let cy = len(self.cy, 50.0, Axis::Vertical);
                GradientGeometry::Radial {
                    cx,
                    cy,
                    r: len(self.r, 50.0, Axis::Other),
                    // `fx` and `fy` coincide with the centre when unset.
                    fx: self
                        .fx
                        .map_or(cx, |l| l.resolve(Axis::Horizontal, &context)),
                    fy: self.fy.map_or(cy, |l| l.resolve(Axis::Vertical, &context)),
                    fr: len(self.fr, 0.0, Axis::Other),
                }
            }
        };
        Gradient {
            geometry,
            units,
            transform: self.transform.unwrap_or(Matrix::IDENTITY),
            spread: self.spread.unwrap_or_default(),
            stops: self.stops.clone().unwrap_or_default(),
        }
    }
}

/// What lengths resolve against in `units`: the referencing element's
/// context, or the unit square for `objectBoundingBox`, where a
/// percentage is a fraction.
pub(crate) fn units_context(units: Units, context: &LengthContext) -> LengthContext {
    match units {
        Units::UserSpaceOnUse => *context,
        Units::ObjectBoundingBox => LengthContext {
            viewport_width: 1.0,
            viewport_height: 1.0,
            ..*context
        },
    }
}

/// A gradient's geometry in its units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientGeometry {
    Linear {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Radial {
        cx: f32,
        cy: f32,
        r: f32,
        fx: f32,
        fy: f32,
        fr: f32,
    },
}

/// A resolved gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub geometry: GradientGeometry,
    pub units: Units,
    pub transform: Matrix,
    pub spread: SpreadMethod,
    pub stops: Vec<GradientStop>,
}

/// The map from the unit square to `reference_box`, or `None` for an
/// empty box.
fn bounding_box_transform(reference_box: Option<Rect>) -> Option<Matrix> {
    let b = reference_box?;
    if b.width <= 0.0 || b.height <= 0.0 {
        return None;
    }
    Some(
        Matrix::translate(f64::from(b.x), f64::from(b.y))
            .then_apply(&Matrix::scale(f64::from(b.width), f64::from(b.height))),
    )
}

impl Gradient {
    /// The shader in the painted element's user space, with `additional`
    /// (the text painter's shader transform) applied outside it. `None`
    /// when the gradient does not apply: `objectBoundingBox` units with an
    /// empty or missing reference box. A gradient with no stops applies,
    /// transparent.
    pub fn shader(
        &self,
        reference_box: Option<Rect>,
        additional: Option<&Matrix>,
    ) -> Option<Shader> {
        let units = match self.units {
            Units::UserSpaceOnUse => Matrix::IDENTITY,
            Units::ObjectBoundingBox => bounding_box_transform(reference_box)?,
        };
        let local = additional
            .copied()
            .unwrap_or(Matrix::IDENTITY)
            .then_apply(&units)
            .then_apply(&self.transform);
        let local = to_sk_matrix(&local);
        let (colors, positions) = skia_stops(&self.stops);
        let interpolation = Interpolation {
            in_premul: InPremul::No,
            color_space: ColorSpace::SRGB,
            hue_method: HueMethod::Shorter,
        };
        // `Gradient::CreateShader`: `SkShaders` with the colours in sRGB.
        let gradient = SkGradient::new(
            Colors::new(&colors, Some(&positions), self.spread.tile_mode(), None),
            interpolation,
        );
        match self.geometry {
            GradientGeometry::Linear { x1, y1, x2, y2 } => shaders::linear_gradient(
                (SkPoint::new(x1, y1), SkPoint::new(x2, y2)),
                &gradient,
                Some(&local),
            ),
            GradientGeometry::Radial {
                cx,
                cy,
                r,
                fx,
                fy,
                fr,
            } => shaders::two_point_conical_gradient(
                // `RadialGradient::CreateShader`: radii below zero are zero.
                (SkPoint::new(fx, fy), fr.max(0.0)),
                (SkPoint::new(cx, cy), r.max(0.0)),
                &gradient,
                Some(&local),
            ),
        }
    }
}

/// `Gradient::FillSkiaStops`: the stops, with the first copied to 0 and the
/// last to 1 when they are not there. No stops is transparent black.
fn skia_stops(stops: &[GradientStop]) -> (Vec<Color4f>, Vec<f32>) {
    let mut colors = Vec::with_capacity(stops.len() + 2);
    let mut positions = Vec::with_capacity(stops.len() + 2);
    match stops.first() {
        None => {
            positions.push(0.0);
            colors.push(Color4f::new(0.0, 0.0, 0.0, 0.0));
        }
        Some(first) if first.offset > 0.0 => {
            positions.push(0.0);
            colors.push(first.color);
        }
        Some(_) => {}
    }
    for stop in stops {
        positions.push(stop.offset);
        colors.push(stop.color);
    }
    if *positions.last().unwrap() < 1.0 {
        positions.push(1.0);
        colors.push(*colors.last().unwrap());
    }
    (colors, positions)
}

/// A pattern element's attributes as set on it, before `href` inheritance.
/// `content` is `Some` when the element has children; the caller's handle
/// for them.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternAttributes<C> {
    pub units: Option<Units>,
    pub content_units: Option<Units>,
    pub transform: Option<Matrix>,
    pub x: Option<Length>,
    pub y: Option<Length>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub view_box: Option<Rect>,
    pub preserve_aspect_ratio: Option<PreserveAspectRatio>,
    pub content: Option<C>,
}

impl<C> Default for PatternAttributes<C> {
    fn default() -> Self {
        Self {
            units: None,
            content_units: None,
            transform: None,
            x: None,
            y: None,
            width: None,
            height: None,
            view_box: None,
            preserve_aspect_ratio: None,
            content: None,
        }
    }
}

impl<C: Clone> PatternAttributes<C> {
    /// `href` inheritance from `referenced`, itself already inherited: the
    /// attributes this element does not set, and the content when it has
    /// no children.
    pub fn inherit(&mut self, referenced: &Self) {
        fn fill<T: Clone>(own: &mut Option<T>, other: &Option<T>) {
            if own.is_none() {
                own.clone_from(other);
            }
        }
        fill(&mut self.units, &referenced.units);
        fill(&mut self.content_units, &referenced.content_units);
        fill(&mut self.transform, &referenced.transform);
        fill(&mut self.x, &referenced.x);
        fill(&mut self.y, &referenced.y);
        fill(&mut self.width, &referenced.width);
        fill(&mut self.height, &referenced.height);
        fill(&mut self.view_box, &referenced.view_box);
        fill(
            &mut self.preserve_aspect_ratio,
            &referenced.preserve_aspect_ratio,
        );
        fill(&mut self.content, &referenced.content);
    }

    /// The pattern with the initial values for what is unset, its tile
    /// lengths resolved (against `context` for `userSpaceOnUse`), and its
    /// content mapped by `content`.
    pub fn resolve<T>(&self, context: &LengthContext, content: impl FnOnce(&C) -> T) -> Pattern<T> {
        let units = self.units.unwrap_or(Units::ObjectBoundingBox);
        let context = units_context(units, context);
        let len =
            |length: Option<Length>, axis: Axis| length.map_or(0.0, |l| l.resolve(axis, &context));
        Pattern {
            units,
            content_units: self.content_units.unwrap_or(Units::UserSpaceOnUse),
            transform: self.transform.unwrap_or(Matrix::IDENTITY),
            tile: Rect::new(
                len(self.x, Axis::Horizontal),
                len(self.y, Axis::Vertical),
                len(self.width, Axis::Horizontal),
                len(self.height, Axis::Vertical),
            ),
            view_box: self.view_box,
            preserve_aspect_ratio: self.preserve_aspect_ratio.unwrap_or_default(),
            content: self.content.as_ref().map(content),
        }
    }
}

/// A resolved pattern. `content` is its children, recorded in the
/// content's own user space (`None` for no children).
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern<T> {
    pub units: Units,
    pub content_units: Units,
    pub transform: Matrix,
    /// `x`, `y`, `width` and `height` in `units`.
    pub tile: Rect,
    pub view_box: Option<Rect>,
    pub preserve_aspect_ratio: PreserveAspectRatio,
    pub content: Option<T>,
}

impl Pattern<Picture> {
    /// The shader in the painted element's user space, with `additional`
    /// applied outside it. `None` when the pattern does not apply: a tile
    /// of zero area, or `objectBoundingBox` units with an empty or missing
    /// reference box.
    pub fn shader(
        &self,
        reference_box: Option<Rect>,
        additional: Option<&Matrix>,
    ) -> Option<Shader> {
        let bounding_box = bounding_box_transform(reference_box);
        let tile = match self.units {
            Units::UserSpaceOnUse => self.tile,
            Units::ObjectBoundingBox => {
                let b = reference_box.filter(|_| bounding_box.is_some())?;
                Rect::new(
                    b.x + self.tile.x * b.width,
                    b.y + self.tile.y * b.height,
                    self.tile.width * b.width,
                    self.tile.height * b.height,
                )
            }
        };
        if !(tile.width > 0.0 && tile.height > 0.0) {
            return None;
        }
        // The content's transform within the tile.
        let content_transform = match self.view_box {
            Some(view_box) => view_box_transform(
                view_box,
                self.preserve_aspect_ratio,
                tile.width,
                tile.height,
            ),
            None => match self.content_units {
                Units::UserSpaceOnUse => Matrix::IDENTITY,
                Units::ObjectBoundingBox => {
                    let b = reference_box.filter(|_| bounding_box.is_some())?;
                    Matrix::scale(f64::from(b.width), f64::from(b.height))
                }
            },
        };
        let bounds = SkRect::from_wh(tile.width, tile.height);
        let mut recorder = PictureRecorder::new();
        let canvas = recorder.begin_recording(bounds, false);
        if let Some(content) = &self.content {
            canvas.draw_picture(content, Some(&to_sk_matrix(&content_transform)), None);
        }
        let record = recorder.finish_recording_as_picture(Some(&bounds))?;
        let local = additional
            .copied()
            .unwrap_or(Matrix::IDENTITY)
            .then_apply(&self.transform)
            .then_apply(&Matrix::translate(f64::from(tile.x), f64::from(tile.y)));
        Some(record.to_shader(
            (TileMode::Repeat, TileMode::Repeat),
            FilterMode::Linear,
            Some(&to_sk_matrix(&local)),
            None,
        ))
    }
}

/// A resolved paint server, as a paint references it.
#[derive(Clone)]
pub enum PaintServer {
    Gradient(Gradient),
    Pattern(Pattern<Picture>),
}

impl fmt::Debug for PaintServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gradient(gradient) => f.debug_tuple("Gradient").field(gradient).finish(),
            Self::Pattern(pattern) => f
                .debug_struct("Pattern")
                .field("units", &pattern.units)
                .field("content_units", &pattern.content_units)
                .field("transform", &pattern.transform)
                .field("tile", &pattern.tile)
                .field("view_box", &pattern.view_box)
                .field("content", &pattern.content.is_some())
                .finish(),
        }
    }
}

impl PaintServer {
    /// `LayoutSVGResourcePaintServer::ApplyShader`'s shader: `None` when
    /// the server does not apply to this element.
    pub fn shader(
        &self,
        reference_box: Option<Rect>,
        additional: Option<&Matrix>,
    ) -> Option<Shader> {
        match self {
            Self::Gradient(gradient) => gradient.shader(reference_box, additional),
            Self::Pattern(pattern) => pattern.shader(reference_box, additional),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> LengthContext {
        LengthContext {
            viewport_width: 200.0,
            viewport_height: 100.0,
            font_size: 16.0,
            root_font_size: 16.0,
            x_height: 8.0,
        }
    }

    #[test]
    fn stop_offsets_clamp_and_never_decrease() {
        assert_eq!(parse_stop_offset("25%"), 0.25);
        assert_eq!(parse_stop_offset("-0.5"), 0.0);
        assert_eq!(parse_stop_offset("1.5"), 1.0);
        assert_eq!(parse_stop_offset("x"), 0.0);
        let black = Color4f::new(0.0, 0.0, 0.0, 1.0);
        let offsets: Vec<f32> = gradient_stops([(0.6, black), (0.3, black), (1.0, black)])
            .iter()
            .map(|s| s.offset)
            .collect();
        assert_eq!(offsets, [0.6, 0.6, 1.0]);
    }

    #[test]
    fn href_inherits_unset_attributes_and_stops() {
        let black = Color4f::new(0.0, 0.0, 0.0, 1.0);
        let mut referenced = GradientAttributes::new(GradientKind::Linear);
        referenced.x2 = Length::parse("50%");
        referenced.spread = Some(SpreadMethod::Reflect);
        referenced.stops = Some(gradient_stops([(0.0, black), (1.0, black)]));
        let mut radial = GradientAttributes::new(GradientKind::Radial);
        radial.inherit(&referenced);
        // Common attributes and stops, but no linear geometry.
        assert_eq!(radial.spread, Some(SpreadMethod::Reflect));
        assert_eq!(radial.stops.as_ref().map(Vec::len), Some(2));
        assert_eq!(radial.x2, None);
        let mut linear = GradientAttributes::new(GradientKind::Linear);
        linear.x2 = Length::parse("0.25");
        linear.inherit(&referenced);
        let gradient = linear.resolve(&context());
        assert_eq!(
            gradient.geometry,
            GradientGeometry::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 0.25,
                y2: 0.0
            }
        );
    }

    #[test]
    fn lengths_follow_the_units() {
        let mut attrs = GradientAttributes::new(GradientKind::Radial);
        attrs.cx = Length::parse("25%");
        let obb = attrs.resolve(&context());
        attrs.units = Some(Units::UserSpaceOnUse);
        let user = attrs.resolve(&context());
        let GradientGeometry::Radial { cx, fx, .. } = obb.geometry else {
            panic!()
        };
        assert_eq!((cx, fx), (0.25, 0.25));
        let GradientGeometry::Radial { cx, fx, r, .. } = user.geometry else {
            panic!()
        };
        assert_eq!((cx, fx), (50.0, 50.0));
        // 50% of sqrt((200² + 100²) / 2).
        assert!((r - 79.05694).abs() < 1e-3);
    }

    #[test]
    fn servers_do_not_apply_to_empty_boxes() {
        let black = Color4f::new(0.0, 0.0, 0.0, 1.0);
        let mut attrs = GradientAttributes::new(GradientKind::Linear);
        attrs.stops = Some(gradient_stops([(0.0, black)]));
        let gradient = attrs.resolve(&context());
        assert!(gradient
            .shader(Some(Rect::new(0.0, 0.0, 10.0, 0.0)), None)
            .is_none());
        assert!(gradient.shader(None, None).is_none());
        assert!(gradient
            .shader(Some(Rect::new(0.0, 0.0, 10.0, 10.0)), None)
            .is_some());
        attrs.units = Some(Units::UserSpaceOnUse);
        assert!(attrs.resolve(&context()).shader(None, None).is_some());
        // No stops: a transparent gradient, which still applies.
        attrs.stops = None;
        assert!(attrs.resolve(&context()).shader(None, None).is_some());
    }

    #[test]
    fn pattern_tiles_need_an_area() {
        let mut attrs = PatternAttributes::<()> {
            units: Some(Units::UserSpaceOnUse),
            width: Length::parse("10"),
            ..Default::default()
        };
        let pattern = attrs.resolve(&context(), |_| -> Picture { unreachable!() });
        assert!(pattern.shader(None, None).is_none());
        attrs.height = Length::parse("10%");
        let pattern = attrs.resolve(&context(), |_| -> Picture { unreachable!() });
        assert_eq!(pattern.tile, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(pattern.shader(None, None).is_some());
    }
}
