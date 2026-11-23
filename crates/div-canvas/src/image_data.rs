//! `blink::ImageData` (`core/html/canvas/image_data.{h,cc}`, Apple BSD) for
//! the 8-bit sRGB format. Float formats and colour spaces come with C6.

use crate::CanvasError;

/// Blink stores an `ImageData`'s size in a `gfx::Size`, so each side fits an
/// `int`.
const MAX_SIDE: u32 = i32::MAX as u32;

/// Unpremultiplied RGBA8888 pixels, row by row from the top left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl ImageData {
    /// `new ImageData(width, height)`: transparent black.
    pub fn new(width: u32, height: u32) -> Result<Self, CanvasError> {
        Self::validate_and_create(width, Some(height), None, false)
    }

    /// `new ImageData(data, width, height)`. Without `height`, the height is
    /// what `data`'s length gives.
    pub fn from_data(data: Vec<u8>, width: u32, height: Option<u32>) -> Result<Self, CanvasError> {
        Self::validate_and_create(width, height, Some(data), false)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, four bytes each.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixels, writable. The length is fixed.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// `ImageData::ValidateAndCreate`. `context_2d_error_mode` makes a size
    /// that overflows a `RangeError`, as a 2D context reports it; a
    /// constructor reports an `IndexSizeError`.
    pub(crate) fn validate_and_create(
        width: u32,
        height: Option<u32>,
        data: Option<Vec<u8>>,
        context_2d_error_mode: bool,
    ) -> Result<Self, CanvasError> {
        if width == 0 || width > MAX_SIDE {
            return Err(CanvasError::IndexSize);
        }
        if let Some(height) = height {
            if height == 0 || height > MAX_SIDE {
                return Err(CanvasError::IndexSize);
            }
        }

        // The size in elements is checked in an `unsigned`. V8's typed-array
        // limit is above that on 64-bit platforms, so this is the limit that
        // applies. Without a height the size has no height yet, but `4 *
        // width` must still fit.
        let Some(size_in_elements) = 4u32
            .checked_mul(width)
            .and_then(|n| n.checked_mul(height.unwrap_or(0)))
        else {
            return Err(if context_2d_error_mode {
                CanvasError::Range
            } else {
                CanvasError::IndexSize
            });
        };

        let Some(data) = data else {
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(size_in_elements as usize)
                .map_err(|_| CanvasError::Range)?;
            pixels.resize(size_in_elements as usize, 0);
            return Ok(Self {
                width,
                height: height.unwrap_or(0),
                data: pixels,
            });
        };

        let length = u32::try_from(data.len()).map_err(|_| CanvasError::NotSupported)?;
        if length == 0 || length % 4 != 0 {
            return Err(CanvasError::InvalidState);
        }
        let length_in_pixels = length / 4;
        if length_in_pixels % width != 0 {
            return Err(CanvasError::IndexSize);
        }
        let expected_height = length_in_pixels / width;
        if height.is_some_and(|height| height != expected_height) {
            return Err(CanvasError::IndexSize);
        }
        Ok(Self {
            width,
            height: expected_height,
            data,
        })
    }
}
