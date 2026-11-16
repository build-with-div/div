//! SVG elliptical arcs as conics.
//!
//! Ported from Skia's `SkPathBuilder::arcTo(SkPoint rad, SkScalar angle,
//! ArcSize, SkPathDirection, SkPoint)` (src/core/SkPathBuilder.cpp), with
//! the `SkMatrix` operations it uses (`setRotate`, `preRotate`, `setScale`,
//! `preScale`, affine point mapping) inlined. Skia is Copyright 2015 Google
//! Inc. and is used under its BSD-style licence (Skia's LICENSE file).
//!
//! The port keeps every operation in single precision and in Skia's
//! order, so the conics are bit-identical to Skia's, with one deliberate
//! difference. Skia calls the platform's single-precision `sin`, `cos`,
//! `tan` and `atan2`, which differ between C libraries, and Div on glibc
//! 2.39 drew `arc/rotation--60` a few ulps away from Chromium. Here each
//! one is computed in double precision and rounded to single, which gives
//! the correctly rounded result (barring double-rounding ties): what
//! Chromium 147 produced in the corpus on glibc 2.43, whose single-precision
//! functions are correctly rounded, and on Windows.

use crate::Point;

/// `SK_FloatPI` (`3.14159265f`): the same single-precision value.
const PI: f32 = std::f32::consts::PI;
const SIN_COS_NEARLY_ZERO: f32 = 1.0 / 65536.0; // SK_ScalarSinCosNearlyZero
const NEARLY_ZERO: f32 = 1.0 / 4096.0; // SK_ScalarNearlyZero

fn sin(x: f32) -> f32 {
    f64::from(x).sin() as f32
}

fn cos(x: f32) -> f32 {
    f64::from(x).cos() as f32
}

fn tan(x: f32) -> f32 {
    f64::from(x).tan() as f32
}

fn atan2(y: f32, x: f32) -> f32 {
    f64::from(y).atan2(f64::from(x)) as f32
}

fn snap_to_zero(v: f32) -> f32 {
    if v.abs() <= SIN_COS_NEARLY_ZERO {
        0.0
    } else {
        v
    }
}

/// `sk_float_round`: `floor(x + 0.5)` in double precision.
fn round(x: f32) -> f32 {
    (f64::from(x) + 0.5).floor() as f32
}

/// An affine `SkMatrix`: `x' = (x·sx + y·kx) + tx`, `y' = (x·ky + y·sy) + ty`.
#[derive(Clone, Copy)]
struct Affine {
    sx: f32,
    kx: f32,
    ky: f32,
    sy: f32,
}

impl Affine {
    /// `setRotate(degrees)`.
    fn rotate(degrees: f32) -> Self {
        let rad = degrees * (PI / 180.0);
        let (s, c) = (snap_to_zero(sin(rad)), snap_to_zero(cos(rad)));
        Self {
            sx: c,
            kx: -s,
            ky: s,
            sy: c,
        }
    }

    /// `setScale(sx, sy)` then `preRotate(degrees)`. With a scale on the
    /// left, each entry of the concatenation (`muladdmul` in Skia) is one
    /// product plus zero, so a single rounded product.
    fn scale_then_rotate(sx: f32, sy: f32, degrees: f32) -> Self {
        let r = Self::rotate(degrees);
        Self {
            sx: sx * r.sx,
            kx: sx * r.kx,
            ky: sy * r.ky,
            sy: sy * r.sy,
        }
    }

    /// `preScale(sx, sy)`.
    fn pre_scale(self, sx: f32, sy: f32) -> Self {
        if sx == 1.0 && sy == 1.0 {
            return self;
        }
        Self {
            sx: self.sx * sx,
            ky: self.ky * sx,
            kx: self.kx * sy,
            sy: self.sy * sy,
        }
    }

    fn map(&self, p: Point) -> Point {
        Point::new(
            (p.x * self.sx + p.y * self.kx) + 0.0,
            (p.x * self.ky + p.y * self.sy) + 0.0,
        )
    }
}

/// One piece of an arc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ArcPiece {
    Line(Point),
    Conic(Point, Point, f32),
}

/// The pieces of the SVG arc from `from` to `to` (SVG 2 §9.5): radii
/// `rx`, `ry` as written, x-axis rotation in degrees, and the large-arc and
/// sweep flags (sweep: clockwise in a y-down space).
pub(crate) fn arc_pieces(
    from: Point,
    rx: f32,
    ry: f32,
    angle: f32,
    large: bool,
    sweep: bool,
    to: Point,
) -> Vec<ArcPiece> {
    // Zero radii and equal endpoints are lines (SVG 2 §9.5.1).
    if rx == 0.0 || ry == 0.0 || from == to {
        return vec![ArcPiece::Line(to)];
    }
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    let mid = Point::new((from.x - to.x) * 0.5, (from.y - to.y) * 0.5);
    let transformed = Affine::rotate(-angle).map(mid);
    let square_rx = rx * rx;
    let square_ry = ry * ry;
    let square_x = transformed.x * transformed.x;
    let square_y = transformed.y * transformed.y;

    // Radii too small to reach are scaled up (SVG 2 §9.5.2).
    let mut radii_scale = square_x / square_rx + square_y / square_ry;
    if radii_scale > 1.0 {
        radii_scale = radii_scale.sqrt();
        rx *= radii_scale;
        ry *= radii_scale;
    }

    let to_unit = Affine::scale_then_rotate(1.0 / rx, 1.0 / ry, -angle);
    let mut unit0 = to_unit.map(from);
    let mut unit1 = to_unit.map(to);
    let mut delta = Point::new(unit1.x - unit0.x, unit1.y - unit0.y);

    let d = delta.x * delta.x + delta.y * delta.y;
    let scale_factor_squared = (1.0 / d - 0.25f32).max(0.0);
    let mut scale_factor = scale_factor_squared.sqrt();
    // Skia's `(arcSweep == kCCW) != arcLarge`, with kCW for `sweep`.
    if sweep == large {
        scale_factor = -scale_factor;
    }
    delta = Point::new(delta.x * scale_factor, delta.y * scale_factor);
    let mut center = Point::new(unit0.x + unit1.x, unit0.y + unit1.y);
    center = Point::new(center.x * 0.5, center.y * 0.5);
    center = Point::new(center.x - delta.y, center.y + delta.x);
    unit0 = Point::new(unit0.x - center.x, unit0.y - center.y);
    unit1 = Point::new(unit1.x - center.x, unit1.y - center.y);
    let theta1 = atan2(unit0.y, unit0.x);
    let theta2 = atan2(unit1.y, unit1.x);
    let mut theta_arc = theta2 - theta1;
    if theta_arc < 0.0 && sweep {
        theta_arc += PI * 2.0;
    } else if theta_arc > 0.0 && !sweep {
        theta_arc -= PI * 2.0;
    }

    // Skia treats very small angles as a line (skbug.com/40040578).
    if theta_arc.abs() < PI / 1_000_000.0 {
        return vec![ArcPiece::Line(to)];
    }

    let from_unit = Affine::rotate(angle).pre_scale(rx, ry);

    // The arc may be slightly more than a quarter turn, so up to a third.
    let segments = (theta_arc / (2.0 * PI / 3.0)).abs().ceil() as i32;
    let theta_width = theta_arc / segments as f32;
    let t = tan(0.5 * theta_width);
    if !t.is_finite() {
        return Vec::new();
    }
    let mut start_theta = theta1;
    let w = (0.5 + cos(theta_width) * 0.5).sqrt();
    let is_integer = |v: f32| v == v.floor();
    let expect_integers = (PI / 2.0 - theta_width.abs()).abs() <= NEARLY_ZERO
        && is_integer(rx)
        && is_integer(ry)
        && is_integer(to.x)
        && is_integer(to.y);

    let mut pieces = Vec::with_capacity(segments as usize);
    for _ in 0..segments {
        let end_theta = start_theta + theta_width;
        let sin_end = snap_to_zero(sin(end_theta));
        let cos_end = snap_to_zero(cos(end_theta));
        let end = Point::new(cos_end + center.x, sin_end + center.y);
        let control = Point::new(end.x + t * sin_end, end.y + -t * cos_end);
        let mut mapped = [from_unit.map(control), from_unit.map(end)];
        // Skia places conics on integers when the input is on integers, so
        // round rects keep their convexity.
        if expect_integers {
            for p in &mut mapped {
                *p = Point::new(round(p.x), round(p.y));
            }
        }
        pieces.push(ArcPiece::Conic(mapped[0], mapped[1], w));
        start_theta = end_theta;
    }
    // The last point is the endpoint by definition.
    if let Some(ArcPiece::Conic(_, end, _)) = pieces.last_mut() {
        *end = to;
    }
    pieces
}
