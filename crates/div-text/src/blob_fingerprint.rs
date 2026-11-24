//! Content fingerprints of the text blobs Div builds (RFC 0006 A1).
//!
//! Paint damage compares each draw with the previous frame's. A `TextBlob`
//! is rebuilt on every recording, so its `unique_id` always changes, and
//! Skia does not expose a blob's positions or font after it is built. Each
//! builder therefore hashes what it hands to the `TextBlobBuilder` (the
//! font, the glyphs and their positions) and registers the hash under the
//! built blob's id. A blob without an entry is compared by its id, which
//! over-damages but never misses a change.
//!
//! The registry is per thread, because a recording builds and draws its
//! blobs on one thread, and it is cleared when a recording starts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use skia_safe::{Font, GlyphId, Point, TextBlob};

thread_local! {
    static FINGERPRINTS: RefCell<HashMap<u32, u64>> = RefCell::new(HashMap::new());
}

/// Hashes the runs of one blob as its builder fills them.
pub struct BlobFingerprint(std::collections::hash_map::DefaultHasher);

impl Default for BlobFingerprint {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobFingerprint {
    pub fn new() -> Self {
        Self(std::collections::hash_map::DefaultHasher::new())
    }

    /// One run, as written into the builder's buffers.
    pub fn run(&mut self, font: &Font, glyphs: &[GlyphId], positions: &[Point]) {
        let h = &mut self.0;
        hash_font(font, h);
        glyphs.hash(h);
        positions.len().hash(h);
        for p in positions {
            p.x.to_bits().hash(h);
            p.y.to_bits().hash(h);
        }
    }

    /// Registers the hash under the built blob's id and returns the blob.
    pub fn register(self, blob: Option<TextBlob>) -> Option<TextBlob> {
        if let Some(blob) = &blob {
            let fingerprint = self.0.finish();
            FINGERPRINTS.with(|map| map.borrow_mut().insert(blob.unique_id(), fingerprint));
        }
        blob
    }
}

/// Every property of a `Font` that changes the glyphs it rasters.
fn hash_font(font: &Font, h: &mut impl Hasher) {
    // Typeface ids are never reused within a process.
    font.typeface().unique_id().hash(h);
    font.size().to_bits().hash(h);
    font.scale_x().to_bits().hash(h);
    font.skew_x().to_bits().hash(h);
    (font.edging() as i32).hash(h);
    (font.hinting() as i32).hash(h);
    [
        font.is_force_auto_hinting(),
        font.is_embedded_bitmaps(),
        font.is_subpixel(),
        font.is_linear_metrics(),
        font.is_embolden(),
        font.is_baseline_snap(),
    ]
    .hash(h);
}

/// The registered content fingerprint of a blob built on this thread.
pub fn text_blob_fingerprint(blob: &TextBlob) -> Option<u64> {
    FINGERPRINTS.with(|map| map.borrow().get(&blob.unique_id()).copied())
}

/// Forgets every registered blob; called when a recording starts.
pub fn clear_text_blob_fingerprints() {
    FINGERPRINTS.with(|map| map.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_content_gives_equal_fingerprints_across_builds() {
        crate::initialize_skia();
        let font = Font::default();
        let build = |x: f32| {
            let mut fingerprint = BlobFingerprint::new();
            let mut builder = skia_safe::TextBlobBuilder::new();
            let (glyphs, positions) = builder.alloc_run_pos(&font, 2, None);
            glyphs.copy_from_slice(&[1, 2]);
            positions.copy_from_slice(&[Point::new(0.0, 0.0), Point::new(x, 0.0)]);
            fingerprint.run(&font, glyphs, positions);
            fingerprint.register(builder.make()).unwrap()
        };
        let (a, b, c) = (build(5.0), build(5.0), build(5.5));
        assert_ne!(a.unique_id(), b.unique_id());
        assert_eq!(text_blob_fingerprint(&a), text_blob_fingerprint(&b));
        assert_ne!(text_blob_fingerprint(&a), text_blob_fingerprint(&c));
        clear_text_blob_fingerprints();
        assert_eq!(text_blob_fingerprint(&a), None);
    }
}
