# Chromium fidelity progress

The fidelity lane owns text rasterization and font fidelity (`div-text`,
raster profiles) and paint-level pixel fidelity. Root causes in layout,
style or the cascade go to the engine track.

## 15 November 2026: root causes of the remaining census differences

- **What:** a classification of all 1,209 non-exact census rows (703 tests)
  by root cause and owner, with the measured counts and the reviewed-sample
  estimates kept apart:
  [fidelity-root-causes-2026-11-15.md](../fidelity-root-causes-2026-11-15.md).
  Docs only; no code or gate changed.
- **Input:** evidence data `3646fb28`, the census receipt at head
  `18fffb9c` (22,519 of 23,728 exact) and its per-row pictures.
- **Verification:** `python3
  docs/progress/fidelity-root-causes-2026-11-15/analyze.py --run <run>
  --output root-causes.json` (Windows) reproduces every row's mismatch
  count from the census summary and the reviewed sample.
- **Top causes (estimated):** sub-pixel geometry of axis-aligned edges at
  fractional scales (about 617 rows), image and background tiles (181),
  diagonal and curved edges (117), layout (116, engine), native controls
  (92), text (67).
- **Remaining:** every cause above. The sub-pixel geometry cause is a
  hypothesis about where rounding happens, to be tested before any fix.
