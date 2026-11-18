# Web-font raster under size and transform (T5, T6), 18 November 2026

Engine fillers for SVG and HTML text, from the SVG lane's text corpus
(`docs/progress/svg-text-paint-2026-11-17.md`). Linux Chromium draws web
fonts through Skia's Fontations scaler. Div draws them through a custom
typeface holding skrifa outlines hinted at the physical size, under a
unit-size font (`div-text` `shaping/web_font_outlines.rs`). Skia therefore
never sees the size or the transform the glyphs are drawn at.

- **T5, LCD size limit:** Skia masks glyphs above 48 device px in grayscale.
- **T6, hinting under rotation and skew:** Chromium leaves the glyphs
  unhinted when the device matrix is not axis-aligned.

## Named gaps

- **T7, glyphs under a non-uniform scale.**
  - **Owner:** engine (`div-text`).
  - **Cause:** unmeasured.
  - **Cases:** after T4, `xf/scale-nonuniform` and `xf/viewbox-none` in
    `tests/fidelity/fixtures/svg-text-v1/pixel-gaps.txt` have exact
    positions, but their glyphs still differ: 578 pixels at 1x and 1,526 at
    2x on Linux.
  - **Measurement task:** after T5 and T6. It may follow the same hinting
    rule as T6.
