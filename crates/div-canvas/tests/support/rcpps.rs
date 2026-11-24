//! This CPU's class for Skia's reciprocal estimate.
//!
//! Skia's AVX2 raster pipeline uses RCPPS, unrefined, in the color-dodge,
//! color-burn, hue, saturation, color and luminosity blend modes
//! (`rcp_fast` in `SkRasterPipeline_opts.h`). Intel and AMD return different
//! estimates for some inputs, and pinned Chromium's pixels for those modes
//! differ between the two vendors. The fingerprint is FNV-1a 64 over RCPPS of
//! every float in [1, 2), 8 lanes at a time, each output folded in as a `u32`.
//! It is `None` without AVX, where Skia takes another path.
//!
//! Shared by `chromium_traces.rs` and `tools/canvas/rcpps_fingerprint.rs`.

/// The fingerprint, or `None` where it cannot be computed.
pub fn fingerprint() -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx") {
            // SAFETY: AVX was detected above.
            return Some(unsafe { sweep() });
        }
    }
    None
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn sweep() -> u64 {
    use std::arch::x86_64::{_mm256_loadu_ps, _mm256_rcp_ps, _mm256_storeu_ps};
    let mut hash: u64 = 14_695_981_039_346_656_037;
    let mut input = [0f32; 8];
    let mut output = [0f32; 8];
    for m in (0..1u32 << 23).step_by(8) {
        for (i, value) in input.iter_mut().enumerate() {
            *value = f32::from_bits(0x3f80_0000 + m + i as u32);
        }
        _mm256_storeu_ps(
            output.as_mut_ptr(),
            _mm256_rcp_ps(_mm256_loadu_ps(input.as_ptr())),
        );
        for value in output {
            hash ^= u64::from(value.to_bits());
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
    }
    hash
}
