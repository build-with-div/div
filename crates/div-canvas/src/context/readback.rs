//! `createImageData`, `getImageData` and `putImageData`, from
//! `blink::BaseRenderingContext2D` (`base_rendering_context_2d.cc`, Chromium
//! BSD). Pixels move through Skia's `readPixels` and `writePixels`, as in
//! Blink's unaccelerated canvas, so the lossy premultiplied alpha round trip
//! is Skia's.

use skia_safe::{AlphaType, ColorType, ImageInfo};

use super::CanvasRenderingContext2D;
use crate::{CanvasError, ImageData};

/// `base::saturated_cast<int>(base::SafeUnsignedAbs(v))`.
fn saturated_abs(v: i32) -> i32 {
    v.unsigned_abs().min(i32::MAX as u32) as i32
}

impl CanvasRenderingContext2D {
    /// `createImageData(sw, sh)`: transparent black, with the sides' absolute
    /// values.
    pub fn create_image_data(&self, sw: i32, sh: i32) -> Result<ImageData, CanvasError> {
        ImageData::validate_and_create(sw.unsigned_abs(), Some(sh.unsigned_abs()), None, true)
    }

    /// `createImageData(imagedata)`: transparent black, the same size.
    pub fn create_image_data_from(&self, image: &ImageData) -> Result<ImageData, CanvasError> {
        ImageData::validate_and_create(image.width(), Some(image.height()), None, true)
    }

    /// `getImageData`. Pixels outside the canvas are transparent black. The
    /// transform, clip and other state do not apply.
    pub fn get_image_data(
        &mut self,
        mut sx: i32,
        mut sy: i32,
        mut sw: i32,
        mut sh: i32,
    ) -> Result<ImageData, CanvasError> {
        if sw.checked_mul(sh).is_none() {
            return Err(CanvasError::Range);
        }
        if !self.origin_clean {
            return Err(CanvasError::Security);
        }
        if sw == 0 || sh == 0 {
            return Err(CanvasError::IndexSize);
        }
        if sw < 0 {
            sx = sx.checked_add(sw).ok_or(CanvasError::Range)?;
            sw = saturated_abs(sw);
        }
        if sh < 0 {
            sy = sy.checked_add(sh).ok_or(CanvasError::Range)?;
            sh = saturated_abs(sh);
        }
        let (Some(right), Some(bottom)) = (sx.checked_add(sw), sy.checked_add(sh)) else {
            return Err(CanvasError::Range);
        };

        let mut image = ImageData::validate_and_create(sw as u32, Some(sh as u32), None, true)?;
        // A read entirely outside the canvas leaves the zeroed buffer.
        if sx < self.width && sy < self.height && right > 0 && bottom > 0 {
            let info = ImageInfo::new((sw, sh), ColorType::RGBA8888, AlphaType::Unpremul, None);
            let row_bytes = sw as usize * 4;
            let read = self
                .surface
                .read_pixels(&info, image.data_mut(), row_bytes, (sx, sy));
            debug_assert!(read, "the read rectangle meets the canvas");
        }
        Ok(image)
    }

    /// `putImageData(imagedata, dx, dy)`.
    pub fn put_image_data(&mut self, data: &ImageData, dx: i32, dy: i32) {
        self.put_image_data_dirty(
            data,
            dx,
            dy,
            0,
            0,
            data.width() as i32,
            data.height() as i32,
        );
    }

    /// `putImageData` with a dirty rectangle. The pixels replace the
    /// canvas's: the transform, clip, `globalAlpha`, composite mode and
    /// shadow do not apply.
    #[allow(clippy::too_many_arguments)]
    pub fn put_image_data_dirty(
        &mut self,
        data: &ImageData,
        dx: i32,
        dy: i32,
        mut dirty_x: i32,
        mut dirty_y: i32,
        mut dirty_width: i32,
        mut dirty_height: i32,
    ) {
        if dirty_width.checked_mul(dirty_height).is_none() {
            return;
        }
        if dirty_width < 0 {
            if dirty_x < 0 {
                dirty_x = 0;
                dirty_width = 0;
            } else {
                dirty_x += dirty_width;
                dirty_width = saturated_abs(dirty_width);
            }
        }
        if dirty_height < 0 {
            if dirty_y < 0 {
                dirty_y = 0;
                dirty_height = 0;
            } else {
                dirty_y += dirty_height;
                dirty_height = saturated_abs(dirty_height);
            }
        }

        // Blink's `gfx::Rect` arithmetic saturates. In 64 bits nothing
        // overflows, and the results agree because the image and the canvas
        // both lie inside the `int` range.
        let (dx, dy) = (i64::from(dx), i64::from(dy));
        let left = i64::from(dirty_x).max(0);
        let top = i64::from(dirty_y).max(0);
        let right = (i64::from(dirty_x) + i64::from(dirty_width)).min(i64::from(data.width()));
        let bottom = (i64::from(dirty_y) + i64::from(dirty_height)).min(i64::from(data.height()));
        if left >= right || top >= bottom {
            return;
        }
        let dest_left = (left + dx).max(0);
        let dest_top = (top + dy).max(0);
        let dest_right = (right + dx).min(i64::from(self.width));
        let dest_bottom = (bottom + dy).min(i64::from(self.height));
        if dest_left >= dest_right || dest_top >= dest_bottom {
            return;
        }

        // PutByteArray.
        let row_bytes = data.width() as usize * 4;
        let first = (dest_top - dy) as usize * row_bytes + (dest_left - dx) as usize * 4;
        let info = ImageInfo::new(
            (
                (dest_right - dest_left) as i32,
                (dest_bottom - dest_top) as i32,
            ),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let wrote = self.canvas().write_pixels(
            &info,
            &data.data()[first..],
            row_bytes,
            (dest_left as i32, dest_top as i32),
        );
        debug_assert!(wrote, "the write rectangle lies inside the canvas");
    }
}
