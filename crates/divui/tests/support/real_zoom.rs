//! Chromium values at real zoom that Div does not reproduce yet.
//!
//! Above scale 1 the pinned Chromium oracle runs at real zoom
//! (`--force-device-scale-factor`, RFC 0010). Blink then lays out in device
//! pixels, so font-relative lengths and text positions move by fractions of
//! a CSS pixel with the scale. Div lays out in CSS px at every scale and
//! keeps its scale-1 value. That value is Chromium's at scale 1 and the one
//! each test pins at every scale.
//!
//! A test lists each value where real-zoom Chromium differs from Div as a gap:
//! (scale, case, phase, Chromium's value). For a listed gap, Div's value
//! must differ from Chromium's; a value with no gap is Chromium's own. Every
//! gap must be checked, so a fix that matches Chromium, or a gap that no
//! longer applies, fails until the table is updated. Owner: engine
//! (device-pixel layout, RFC 0011). The values are in the text-oracle v2
//! receipts in build-with-div/div-fidelity-data
//! (`archive/docs/progress/evidence/{windows,linux}-text-oracle-v2.json`).

pub type Gap = (f64, usize, &'static str, f64);

pub struct RealZoomGaps {
    gaps: &'static [Gap],
    checked: Vec<bool>,
}

impl RealZoomGaps {
    pub fn new(gaps: &'static [Gap]) -> Self {
        assert!(
            gaps.iter().all(|gap| gap.0 != 1.0),
            "scale 1 has no device-pixel layout gap"
        );
        Self {
            gaps,
            checked: vec![false; gaps.len()],
        }
    }

    /// Record Div's value for `case` and `phase` at `scale`. Div's value is
    /// already pinned by the caller; this checks that it differs from
    /// Chromium's wherever a gap is listed.
    pub fn check(&mut self, scale: f64, case: usize, phase: &str, div: f32) {
        for (index, gap) in self.gaps.iter().enumerate() {
            if gap.0 == scale && gap.1 == case && gap.2 == phase {
                assert_ne!(
                    f64::from(div),
                    gap.3,
                    "case {case} {phase} at {scale} now matches real-zoom Chromium: remove its gap"
                );
                self.checked[index] = true;
            }
        }
    }

    pub fn finish(self) {
        let unchecked: Vec<&Gap> = self
            .gaps
            .iter()
            .zip(&self.checked)
            .filter(|(_, checked)| !**checked)
            .map(|(gap, _)| gap)
            .collect();
        assert!(unchecked.is_empty(), "gaps never checked: {unchecked:?}");
    }
}
