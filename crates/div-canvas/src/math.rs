//! Numeric helpers with Blink's exact semantics (`wtf/math_extras.h`, Apple
//! BSD licence; `base/numerics` `saturated_cast`).

/// `static_cast<float>(M_PI)`.
pub const PI_FLOAT: f32 = std::f64::consts::PI as f32;
/// `kPiFloat * 2.0f`.
pub const TWO_PI_FLOAT: f32 = PI_FLOAT * 2.0;
/// `static_cast<float>(M_PI_2)`.
pub const PI_OVER_TWO_FLOAT: f32 = std::f64::consts::FRAC_PI_2 as f32;

/// `base::saturated_cast<float>(double)` with the default handler: NaN stays
/// NaN, and a value outside float's range becomes the infinity of its sign
/// (`SaturationDefaultLimits::Overflow` is the infinity for float). Canvas
/// callers then reject it with `std::isfinite`.
pub fn saturated_f32(value: f64) -> f32 {
    if value.is_nan() {
        return f32::NAN;
    }
    if value > f32::MAX as f64 {
        return f32::INFINITY;
    }
    if value < f32::MIN as f64 {
        return f32::NEG_INFINITY;
    }
    value as f32
}

/// `ClampTo<float>(double)` with the default bounds `[lowest, max]`. Blink
/// DCHECKs against NaN; callers check finiteness first.
pub fn clamp_to_f32(value: f64) -> f32 {
    if value >= f32::MAX as f64 {
        return f32::MAX;
    }
    if value <= f32::MIN as f64 {
        return f32::MIN;
    }
    value as f32
}

/// `ClampToWithNaNTo0<float>(double)`, used by `AffineTransform::ClampToFloat`.
pub fn clamp_to_float_nan_to_zero(value: f64) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    clamp_to_f32(value)
}

/// `Rad2deg(float)`: `r * (180.0f / kPiFloat)`.
pub fn rad2deg_f32(r: f32) -> f32 {
    r * (180.0f32 / PI_FLOAT)
}

/// `WebCoreFloatNearlyEqual(a, b)`: Skia's `SkScalarNearlyEqual` with
/// `SK_ScalarNearlyZero` (1/4096). Written from Skia's definition; the Blink
/// wrapper (`skia_utils.h`) was not part of the approved source set.
pub fn float_nearly_equal(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0 / 4096.0
}

/// C `fmodf`, which Rust's `%` on `f32` implements exactly (IEEE remainder
/// truncated toward zero, as `fmodf`).
pub fn fmodf(x: f32, y: f32) -> f32 {
    x % y
}

/// `ClampNonFiniteToZero(float)`.
pub fn clamp_non_finite_to_zero(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}
