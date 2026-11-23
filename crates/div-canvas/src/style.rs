//! Fill and stroke styles: `blink::CanvasStyle` (`canvas_style.{h,cc}`),
//! `blink::CanvasGradient` (`canvas_gradient.{h,cc}`) and `blink::Gradient`
//! (`platform/graphics/gradient.{h,cc}`), all Apple BSD, for the legacy sRGB
//! colours this crate parses.
//!
//! `CanvasGradient`'s `colorInterpolationMethod`, `hueInterpolationMethod` and
//! `premultipliedAlpha` are behind the experimental `CanvasGradientCSSColor4`
//! feature in Chromium 147, so script cannot reach them and they are not
//! ported: every gradient interpolates unpremultiplied in sRGB.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use div_style::Color;
use skia_safe::{
    gradient::{self, interpolation, Interpolation},
    shaders, Color4f, Matrix, Paint, Point, Shader, TileMode,
};

use crate::context::{parse_canvas_color, serialize_canvas_color};
use crate::CanvasError;

/// `gfx::ClampFloatGeometry`: NaN becomes 0, and the value is kept within
/// the float range divided by a million.
fn clamp_float_geometry(value: f32) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    let max = (f64::from(f32::MAX) / 1e6) as f32;
    value.clamp(-max, max)
}

/// `ClampNonFiniteToSafeFloat` for a point.
fn clamp_point(point: (f32, f32)) -> Point {
    Point::new(clamp_float_geometry(point.0), clamp_float_geometry(point.1))
}

/// `Color::ToGradientStopSkColor4f` for a legacy sRGB colour.
fn stop_color(color: Color) -> Color4f {
    Color4f::new(color.r, color.g, color.b, color.a)
}

#[derive(Debug, Clone, Copy)]
enum GradientKind {
    Linear {
        p0: (f32, f32),
        p1: (f32, f32),
    },
    Radial {
        p0: (f32, f32),
        r0: f32,
        p1: (f32, f32),
        r1: f32,
    },
    /// `rotation` in degrees. A canvas conic gradient always sweeps 0 to 360.
    Conic {
        center: (f32, f32),
        rotation: f32,
    },
}

#[derive(Debug, Clone, Copy)]
struct ColorStop {
    stop: f64,
    color: Color,
}

/// `blink::Gradient` as `CanvasGradient` constructs it: pad spread,
/// unpremultiplied, degenerate gradients disallowed.
struct Gradient {
    kind: GradientKind,
    stops: Vec<ColorStop>,
    stops_sorted: bool,
    /// The outer `None` is an empty cache. Skia may decline to make a shader,
    /// which leaves the paint's colour to draw, as a null `SkShader` does.
    cached_shader: Option<Option<Shader>>,
}

impl Gradient {
    fn new(kind: GradientKind) -> Self {
        Self {
            kind,
            stops: Vec::new(),
            stops_sorted: true,
            cached_shader: None,
        }
    }

    /// `AddColorStop`. Equal offsets count as unsorted, and the stable sort
    /// then keeps them in insertion order.
    fn add_color_stop(&mut self, stop: ColorStop) {
        self.stops_sorted = match self.stops.last() {
            None => true,
            Some(last) => self.stops_sorted && last.stop < stop.stop,
        };
        self.stops.push(stop);
        self.cached_shader = None;
    }

    /// `SortStopsIfNecessary`.
    fn sort_stops_if_necessary(&mut self) {
        if self.stops_sorted {
            return;
        }
        self.stops_sorted = true;
        // Offsets are validated, so none is NaN.
        self.stops.sort_by(|a, b| a.stop.total_cmp(&b.stop));
    }

    /// `FillSkiaStops`: the sorted stops, with stops at both 0 and 1.
    fn skia_stops(&self) -> (Vec<Color4f>, Vec<f32>) {
        let mut colors = Vec::with_capacity(self.stops.len() + 2);
        let mut positions = Vec::with_capacity(self.stops.len() + 2);
        match self.stops.first() {
            // A gradient with no stops is transparent black.
            None => {
                positions.push(0.0);
                colors.push(Color4f::new(0.0, 0.0, 0.0, 0.0));
            }
            Some(first) if first.stop > 0.0 => {
                positions.push(0.0);
                colors.push(stop_color(first.color));
            }
            Some(_) => {}
        }
        for stop in &self.stops {
            positions.push(clamp_float_geometry(stop.stop as f32));
            colors.push(stop_color(stop.color));
        }
        if positions.last().is_some_and(|&last| last < 1.0) {
            positions.push(1.0);
            colors.push(*colors.last().expect("a stop was just pushed"));
        }
        (colors, positions)
    }

    /// `CreateShaderInternal` and each subclass's `CreateShader`.
    fn create_shader(&mut self) -> Option<Shader> {
        self.sort_stops_if_necessary();
        let (colors, positions) = self.skia_stops();
        // `ResolveSkInterpolation` for legacy colours.
        let sk_gradient = gradient::Gradient::new(
            gradient::Colors::new(&colors, Some(positions.as_slice()), TileMode::Clamp, None),
            Interpolation {
                in_premul: interpolation::InPremul::No,
                color_space: interpolation::ColorSpace::SRGB,
                hue_method: interpolation::HueMethod::Shorter,
            },
        );
        match self.kind {
            GradientKind::Linear { p0, p1 } => {
                if p0 == p1 {
                    return Some(shaders::empty());
                }
                gradient::shaders::linear_gradient(
                    (clamp_point(p0), clamp_point(p1)),
                    &sk_gradient,
                    None,
                )
            }
            GradientKind::Radial { p0, r0, p1, r1 } => {
                // Skia's radii must be positive.
                let radius0 = clamp_float_geometry(r0).max(0.0);
                let radius1 = clamp_float_geometry(r1).max(0.0);
                if p0 == p1 && radius0 == radius1 {
                    return Some(shaders::empty());
                }
                gradient::shaders::two_point_conical_gradient(
                    (clamp_point(p0), radius0),
                    (clamp_point(p1), radius1),
                    &sk_gradient,
                    None,
                )
            }
            GradientKind::Conic { center, rotation } => {
                // Skia's sweep angles are relative to the x-axis, not the
                // y-axis.
                let skia_rotation = rotation - 90.0;
                let center = Point::new(center.0, center.1);
                let mut matrix = Matrix::new_identity();
                if skia_rotation != 0.0 {
                    matrix.pre_rotate(skia_rotation, center);
                }
                gradient::shaders::sweep_gradient(center, (0.0, 360.0), &sk_gradient, &matrix)
            }
        }
    }
}

/// A `CanvasGradient`. Clones are the same object: a color stop added through
/// one shows in every style that holds it, at the next draw.
#[derive(Clone)]
pub struct CanvasGradient(Rc<RefCell<Gradient>>);

impl CanvasGradient {
    fn new(kind: GradientKind) -> Self {
        Self(Rc::new(RefCell::new(Gradient::new(kind))))
    }

    pub(crate) fn linear(p0: (f32, f32), p1: (f32, f32)) -> Self {
        Self::new(GradientKind::Linear { p0, p1 })
    }

    pub(crate) fn radial(p0: (f32, f32), r0: f32, p1: (f32, f32), r1: f32) -> Self {
        Self::new(GradientKind::Radial { p0, r0, p1, r1 })
    }

    pub(crate) fn conic(rotation: f32, center: (f32, f32)) -> Self {
        Self::new(GradientKind::Conic { center, rotation })
    }

    /// `addColorStop(offset, color)`.
    pub fn add_color_stop(&self, offset: f64, color: &str) -> Result<(), CanvasError> {
        if !(0.0..=1.0).contains(&offset) {
            return Err(CanvasError::IndexSize);
        }
        let color = parse_canvas_color(color).ok_or(CanvasError::Syntax)?;
        self.0.borrow_mut().add_color_stop(ColorStop {
            stop: offset,
            color,
        });
        Ok(())
    }

    /// `Gradient::ApplyToFlags`' cached shader.
    fn shader(&self) -> Option<Shader> {
        let mut gradient = self.0.borrow_mut();
        if let Some(shader) = &gradient.cached_shader {
            return shader.clone();
        }
        let shader = gradient.create_shader();
        gradient.cached_shader = Some(shader.clone());
        shader
    }
}

/// Identity, as `===` compares two `CanvasGradient`s.
impl PartialEq for CanvasGradient {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for CanvasGradient {}

impl fmt::Debug for CanvasGradient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gradient = self.0.borrow();
        f.debug_struct("CanvasGradient")
            .field("kind", &gradient.kind)
            .field("stops", &gradient.stops)
            .finish()
    }
}

/// What `fillStyle` and `strokeStyle` read as.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CanvasStyleValue {
    /// The colour, serialized.
    Color(String),
    /// The same gradient object that was assigned.
    Gradient(CanvasGradient),
}

/// `blink::CanvasStyle`.
#[derive(Clone)]
pub(crate) enum CanvasStyle {
    Color(Color),
    Gradient(CanvasGradient),
}

impl CanvasStyle {
    pub fn value(&self) -> CanvasStyleValue {
        match self {
            Self::Color(color) => CanvasStyleValue::Color(serialize_canvas_color(*color)),
            Self::Gradient(gradient) => CanvasStyleValue::Gradient(gradient.clone()),
        }
    }

    /// `ApplyToFlags`. A colour's float alpha is multiplied by the global
    /// alpha. A gradient draws through a black paint that carries the global
    /// alpha, and is always dithered.
    ///
    /// Blink leaves dither set on its flags after a gradient is replaced by
    /// a colour. Skia ignores dither on a solid colour without a mask or
    /// image filter, and this crate has neither yet.
    pub fn apply_to_paint(&self, paint: &mut Paint, global_alpha: f32) {
        match self {
            Self::Color(color) => {
                paint.set_color4f(
                    Color4f::new(color.r, color.g, color.b, color.a * global_alpha),
                    None,
                );
            }
            Self::Gradient(gradient) => {
                paint.set_shader(gradient.shader());
                paint.set_dither(true);
                paint.set_color4f(Color4f::new(0.0, 0.0, 0.0, global_alpha), None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CanvasError, CanvasRenderingContext2D, CanvasStyleValue};

    /// The factories' own finiteness checks. Script cannot reach them, because
    /// the bindings throw a `TypeError` first, so the traces cannot either.
    #[test]
    fn non_finite_factory_arguments_create_no_gradient() {
        let ctx = CanvasRenderingContext2D::new(1, 1).unwrap();
        assert!(ctx
            .create_linear_gradient(f64::NAN, 0.0, 1.0, 0.0)
            .is_none());
        assert!(ctx
            .create_linear_gradient(0.0, 0.0, f64::INFINITY, 0.0)
            .is_none());
        assert!(ctx.create_conic_gradient(f64::NAN, 0.0, 0.0).is_none());
        assert_eq!(
            ctx.create_radial_gradient(0.0, 0.0, f64::NAN, 0.0, 0.0, 1.0),
            Ok(None)
        );
        // A negative radius is checked first.
        assert_eq!(
            ctx.create_radial_gradient(f64::NAN, 0.0, -1.0, 0.0, 0.0, 1.0),
            Err(CanvasError::IndexSize)
        );
    }

    #[test]
    fn a_nan_offset_is_an_index_size_error() {
        let ctx = CanvasRenderingContext2D::new(1, 1).unwrap();
        let gradient = ctx.create_linear_gradient(0.0, 0.0, 1.0, 0.0).unwrap();
        assert_eq!(
            gradient.add_color_stop(f64::NAN, "red"),
            Err(CanvasError::IndexSize)
        );
    }

    #[test]
    fn a_style_reads_back_the_same_gradient_object() {
        let mut ctx = CanvasRenderingContext2D::new(1, 1).unwrap();
        let gradient = ctx.create_linear_gradient(0.0, 0.0, 1.0, 0.0).unwrap();
        let other = ctx.create_linear_gradient(0.0, 0.0, 1.0, 0.0).unwrap();
        ctx.set_fill_gradient(&gradient);
        assert_eq!(ctx.fill_style(), CanvasStyleValue::Gradient(gradient));
        assert_ne!(ctx.fill_style(), CanvasStyleValue::Gradient(other));
    }
}
