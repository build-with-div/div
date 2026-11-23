//! `CanvasRenderingContext2D` for Div (RFC 0008).
//!
//! The crate is a port of Blink's canvas2d at Chromium 147.0.7727.50
//! (`third_party/blink/renderer/modules/canvas/canvas2d`, BSD licences; the
//! notices are in `THIRD_PARTY.md`, and each module names its source files).
//! It draws with Skia onto its own raster backing store and needs no
//! document, so it is tested on its own: each test replays a call trace
//! (see [`trace`]) and compares the bitmap and every read with Chromium's,
//! exactly.
//!
//! This first step covers core state: `save`/`restore`, transforms,
//! `globalAlpha`, composite modes, colour fill and stroke styles, line styles
//! and dashes, paths (lines, curves, `arcTo`, `arc`, `ellipse`, `rect`,
//! `roundRect`), `fill`, `stroke`, `clip`, the rectangle calls and the
//! point-in-path tests. Step 3 adds text: `font` and the other text
//! attributes, `fillText`, `strokeText` and `measureText`, shaped and
//! rasterized by div-text. Readback adds `ImageData`, `createImageData`,
//! `getImageData` and `putImageData`. Gradients, patterns, images, shadows
//! and filters come in later steps of RFC 0008's migration.

mod affine;
mod context;
mod image_data;
mod math;
mod path;
mod shape_cache;
mod state;
mod text;
pub mod trace;

pub use affine::AffineTransform;
pub use context::{CanvasRenderingContext2D, FillRule};
pub use image_data::ImageData;
pub use path::{CanvasPath, RoundRectRadius};
pub use state::{LineCap, LineJoin};
pub use text::{
    CanvasDirection, CanvasFontKerning, CanvasFontStretch, CanvasFontVariantCaps,
    CanvasTextRendering, TextAlign, TextBaseline, TextMetrics,
};

/// A `DOMException` or JavaScript error a canvas call throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasError {
    /// `IndexSizeError` (a negative radius, an invalid `ImageData` size).
    IndexSize,
    /// `RangeError` (`roundRect` radii, an `ImageData` too large to create).
    Range,
    /// `InvalidStateError` (`ImageData` data of an invalid length).
    InvalidState,
    /// `NotSupportedError` (`ImageData` data over 4 GiB).
    NotSupported,
    /// `SecurityError` (reading a canvas that is not origin-clean).
    Security,
}

impl CanvasError {
    /// The error's name as script sees it.
    pub fn name(self) -> &'static str {
        match self {
            Self::IndexSize => "IndexSizeError",
            Self::Range => "RangeError",
            Self::InvalidState => "InvalidStateError",
            Self::NotSupported => "NotSupportedError",
            Self::Security => "SecurityError",
        }
    }
}
