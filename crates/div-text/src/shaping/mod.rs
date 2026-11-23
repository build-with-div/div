//! Text shaping — glyph shaping with HarfBuzz, called directly (RFC 0013).
//!
//! Mirrors Blink's shaping subsystem (`platform/fonts/shaping/`).
//! Takes text + font and produces positioned glyphs for rendering.
//!
//! Architecture:
//! - `RunSegmenter` splits text into uniform runs by Unicode script.
//! - `TextShaper` shapes each run with HarfBuzz (`harfbuzz.rs`).
//! - `ShapeResult` holds the output: glyph IDs, positions, and metadata.

mod harfbuzz;
mod segmenter;
mod shape_result;
mod shaper;
mod web_font_outlines;

pub(crate) use harfbuzz::release_parent_fonts;
#[doc(hidden)]
pub use harfbuzz::retained_parent_font_count;
pub use segmenter::{RunSegment, RunSegmenter, Script};
pub use shape_result::{
    ShapeResult, ShapeResultCharacterData, ShapeResultRun, TextDirection, TextRasterPolicy,
};
pub use shaper::TextShaper;
