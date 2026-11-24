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
#[cfg(test)]
mod lifetime_tests;
mod segmenter;
mod shape_result;
mod shaper;
mod web_font_outlines;

pub(crate) use harfbuzz::release_parent_fonts;
pub use segmenter::{RunSegment, RunSegmenter, Script};
pub use shape_result::{
    ShapeResult, ShapeResultCharacterData, ShapeResultRun, TextDirection, TextRasterPolicy,
};
pub use shaper::TextShaper;
pub(crate) use web_font_outlines::release_web_font_outlines;
