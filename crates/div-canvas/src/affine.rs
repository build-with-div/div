//! `blink::AffineTransform` (`platform/transforms/affine_transform.{h,cc}`,
//! Apple BSD licence), the double-precision transform a canvas state keeps.
//!
//! Skia's canvas matrix is float and receives the same operations
//! separately, as Blink does. The two are kept apart on purpose: path
//! re-expression and point mapping use this one, drawing uses Skia's.

use skia_safe::{Matrix, M44};

use crate::math::clamp_to_float_nan_to_zero;

/// `[a, b, c, d, e, f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineTransform {
    m: [f64; 6],
}

impl Default for AffineTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl AffineTransform {
    pub const IDENTITY: Self = Self {
        m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self {
            m: [a, b, c, d, e, f],
        }
    }

    pub fn a(&self) -> f64 {
        self.m[0]
    }
    pub fn b(&self) -> f64 {
        self.m[1]
    }
    pub fn c(&self) -> f64 {
        self.m[2]
    }
    pub fn d(&self) -> f64 {
        self.m[3]
    }
    pub fn e(&self) -> f64 {
        self.m[4]
    }
    pub fn f(&self) -> f64 {
        self.m[5]
    }

    pub fn values(&self) -> [f64; 6] {
        self.m
    }

    pub fn is_identity(&self) -> bool {
        self.m == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    }

    pub fn is_identity_or_translation(&self) -> bool {
        self.m[0] == 1.0 && self.m[1] == 0.0 && self.m[2] == 0.0 && self.m[3] == 1.0
    }

    pub fn x_scale_squared(&self) -> f64 {
        self.m[0] * self.m[0] + self.m[1] * self.m[1]
    }

    pub fn y_scale_squared(&self) -> f64 {
        self.m[2] * self.m[2] + self.m[3] * self.m[3]
    }

    pub fn det(&self) -> f64 {
        self.m[0] * self.m[3] - self.m[1] * self.m[2]
    }

    /// `std::isnormal(Det())`.
    pub fn is_invertible(&self) -> bool {
        self.det().is_normal()
    }

    pub fn inverse(&self) -> Self {
        let mut result = Self::IDENTITY;
        if self.is_identity_or_translation() {
            result.m[4] = -self.m[4];
            result.m[5] = -self.m[5];
            return result;
        }
        let determinant = self.det();
        if !determinant.is_normal() {
            return result;
        }
        let t = &self.m;
        result.m[0] = t[3] / determinant;
        result.m[1] = -t[1] / determinant;
        result.m[2] = -t[2] / determinant;
        result.m[3] = t[0] / determinant;
        result.m[4] = (t[2] * t[5] - t[3] * t[4]) / determinant;
        result.m[5] = (t[1] * t[4] - t[0] * t[5]) / determinant;
        result
    }

    /// `this = this * other` (Blink's `PreConcat`, `operator*=`).
    pub fn pre_concat(&mut self, other: &Self) -> &mut Self {
        *self = multiply(self, other);
        self
    }

    pub fn multiplied(&self, other: &Self) -> Self {
        multiply(self, other)
    }

    pub fn rotate_radians(&mut self, a: f64) -> &mut Self {
        let cos_angle = a.cos();
        let sin_angle = a.sin();
        let rot = Self::new(cos_angle, sin_angle, -sin_angle, cos_angle, 0.0, 0.0);
        self.pre_concat(&rot)
    }

    pub fn scale_non_uniform(&mut self, sx: f64, sy: f64) -> &mut Self {
        self.m[0] *= sx;
        self.m[1] *= sx;
        self.m[2] *= sy;
        self.m[3] *= sy;
        self
    }

    pub fn translate(&mut self, tx: f64, ty: f64) -> &mut Self {
        self.m[4] += tx * self.m[0] + ty * self.m[2];
        self.m[5] += tx * self.m[1] + ty * self.m[3];
        self
    }

    /// `MapPoint`: double arithmetic, then `ClampToFloat` (NaN to 0).
    pub fn map_point(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x as f64, y as f64);
        (
            clamp_to_float_nan_to_zero(self.m[0] * x + self.m[2] * y + self.m[4]),
            clamp_to_float_nan_to_zero(self.m[1] * x + self.m[3] * y + self.m[5]),
        )
    }

    /// `MapRect(gfx::RectF)`: `(x, y, width, height)`.
    pub fn map_rect(&self, rect: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = rect;
        let (rx, ry, mut rw, mut rh) = if self.is_identity_or_translation() {
            let (px, py) = self.map_point(x, y);
            (px, py, w, h)
        } else {
            // gfx::QuadF(rect).BoundingBox() over the four mapped corners.
            let corners = [
                self.map_point(x, y),
                self.map_point(x + w, y),
                self.map_point(x + w, y + h),
                self.map_point(x, y + h),
            ];
            let mut rl = corners[0].0;
            let mut rr = corners[0].0;
            let mut rt = corners[0].1;
            let mut rb = corners[0].1;
            for &(cx, cy) in &corners[1..] {
                rl = rl.min(cx);
                rr = rr.max(cx);
                rt = rt.min(cy);
                rb = rb.max(cy);
            }
            (rl, rt, rr - rl, rb - rt)
        };
        rw = clamp_to_float_nan_to_zero(rw as f64);
        rh = clamp_to_float_nan_to_zero(rh as f64);
        (rx, ry, rw, rh)
    }

    pub fn to_sk_matrix(&self) -> Matrix {
        let f = |v: f64| clamp_to_float_nan_to_zero(v);
        Matrix::new_all(
            f(self.a()),
            f(self.c()),
            f(self.e()),
            f(self.b()),
            f(self.d()),
            f(self.f()),
            0.0,
            0.0,
            1.0,
        )
    }

    pub fn to_sk_m44(&self) -> M44 {
        let f = |v: f64| clamp_to_float_nan_to_zero(v);
        M44::row_major(&[
            f(self.a()),
            f(self.c()),
            0.0,
            f(self.e()),
            f(self.b()),
            f(self.d()),
            0.0,
            f(self.f()),
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }
}

fn multiply(t1: &AffineTransform, t2: &AffineTransform) -> AffineTransform {
    if t1.is_identity_or_translation() {
        return AffineTransform::new(
            t2.a(),
            t2.b(),
            t2.c(),
            t2.d(),
            t1.e() + t2.e(),
            t1.f() + t2.f(),
        );
    }
    AffineTransform::new(
        t1.a() * t2.a() + t1.c() * t2.b(),
        t1.b() * t2.a() + t1.d() * t2.b(),
        t1.a() * t2.c() + t1.c() * t2.d(),
        t1.b() * t2.c() + t1.d() * t2.d(),
        t1.a() * t2.e() + t1.c() * t2.f() + t1.e(),
        t1.b() * t2.e() + t1.d() * t2.f() + t1.f(),
    )
}
