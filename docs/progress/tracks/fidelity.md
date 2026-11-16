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

## 15 November 2026: table part backgrounds keep fractional cell edges

- **What:** Blink paints a table part's background (column, row, section)
  into each cell with the cell as the painting area
  (`TableCellPainter::PaintBackgroundForTablePart`): the part's generated
  tile, clipped to the cell, is drawn once with antialiasing. Div clipped
  the draw to the cell with a hard clip, which snapped fractional device
  cell edges. Div now uses its existing border-box generated-tile path for
  these cells (`crates/div-paint/src/painter.rs`, `paint_background_layers`).
  Canvas backgrounds are unchanged. Owner: paint (fidelity lane).
- **Census (full expanded suite, a8 build, WSL):** main `dd8ba04` 22,520 of
  23,728 exact; this change 22,539 (+19), 0 losses, no other row changed.
  The gains are the nine `css_break` table col/row/section paint tests at
  1.25 and at 1.5, and
  `css_backgrounds/background-image-table-cells-straddling-no-repeat` at
  1.25. CSS Grid coverage (4,204 rows) is unchanged row for row.
- **Verification:** `cargo test --locked -p div-paint`
  (`table_part_gradient_keeps_fractional_cell_edge_coverage` fails without
  the change); `python tools/qualification/generate_renderer_contract.py
  --check`; the census under `census_lock.py`.
- **Remaining:** collapsed-border and caption edges in the same table tests,
  and the rest of the snapped-edge cause.

## 15 November 2026: Linux web fonts rasterize from skrifa outlines

- **What:** Chromium on Linux creates every `@font-face` typeface with
  `SkTypeface_Make_Fontations` (`web_font_typeface_factory.cc`). Div now
  paints anti-aliased runs of registered faces on Linux from a custom
  typeface of skrifa outlines at the physical size, hinted as Skia's
  Fontations scaler maps the resolved hinting
  (`crates/div-text/src/shaping/web_font_outlines.rs`). The deterministic
  test collection stands in for the census's Fontconfig system fonts and
  keeps FreeType; the aliased Ahem policies are unchanged. Owner: text
  (fidelity lane).
- **Typeface cache:** keyed by the typeface id, variation coordinates,
  physical size, hinting, LCD mask and force-autohint flag; least recently
  used, 32 entries; an entry restarts past 4,096 glyphs.
- **Evidence:** pinned Chromium 147 frames of Geist Mono 400, Inter 400 and
  Geist 650 at 13px and 16px, at scales 1, 1.25, 1.5 and 2
  (`tests/fidelity/fixtures/webfont-fontations-v1`, captured by
  `tools/qualification/capture_webfont_oracle.py`). 21 of 24 rows are exact
  (0 of 24 on the FreeType path).
- **Census:** full census under `census_lock.py` (WSL, a8 build) at main
  8871c13 and at 863a56f. Expanded suite: 22,539 of 23,728 exact on both.
  Coverage matrix: 2,227 exact, 1,821 different and 156 errors of 4,204 on
  both. Every row's status, pixel count, diff signature and Div RGBA hash
  is identical; only build identity (commit, binary, cache identity)
  differs. `pixel-compare` builds every census profile on
  `deterministic_test()` and Fontconfig system fonts and registers no web
  fonts, so this is a no-regression check, not evidence for the change.
- **Verification:** `cargo test --locked -p divui --features
  "divui/async divui/http divui/testing" --test native_webfont_fontations`
  (Linux); `cargo test --locked -p div-text`
  (`ahem_policies_are_unchanged_by_web_font_provenance` and the cache
  tests); `python tools/qualification/generate_renderer_contract.py
  --check`; the census under `census_lock.py`.
- **Remaining:** Inter 13px at 1.25, 1.5 and 2x (2,729, 4,227 and 5,889
  pixels): Chromium's baseline sits 1-2 device pixels lower, on a whole
  pixel; line-box metrics or layout, not rasterization. Monochrome web
  fonts keep FreeType; vertical web-font text and the div-components pages
  were not measured.
