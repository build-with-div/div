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
