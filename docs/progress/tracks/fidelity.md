# Chromium fidelity progress

The fidelity lane owns text rasterization and font fidelity (`div-text`,
raster profiles) and paint-level pixel fidelity. Root causes in layout,
style or the cascade go to the engine track.

## 17 November 2026: LCD text inside opacity groups, where cc keeps it

- **Problem:** the components' disabled buttons (`disabled:opacity-50`)
  differ from Chromium by 502 (light) and 489 (dark) pixels at 1x.
  Chromium keeps the label as LCD text inside the 50% group, but Div drew
  it in grayscale.
- **cc's condition** (147.0.7727.50, `cc/paint/paint_op_buffer.cc`
  `PaintOpBuffer::Playback`): cc decides once per buffer, which is one cc
  layer's display list. It preserves LCD text when:
  - the layer's raster can use LCD text (pixel geometry known),
  - the buffer draws text, and
  - no op in it, including ops in nested records, is a `SaveLayerOp`.

  When it preserves, every `SaveLayerAlphaOp` in the layer opens with
  `kPreserveLCDText | kInitWithPrevious`. Blink emits a `SaveLayerOp` for
  a filter, a blend mode or a mask (`paint_chunks_to_cc_layer.cc`).
  Opacity alone emits a `SaveLayerAlphaOp`, and backdrop-filter emits a
  `SaveLayerFiltersOp`; neither prevents preservation. So one filter
  anywhere in the layer turns preservation off for every opacity group in
  that layer.
- **Change** (`div-paint`):
  - `paint_record.rs`: an `LcdTextLayerScope` per recording canvas. These
    are the document picture (`render.rs`) and the viewport's scrolling
    contents (`painter.rs`).
  - A `will-change: transform` subtree, which Div rasters on a non-LCD
    surface, gets a scope without LCD text.
  - `layer_lcd_text_contents` scans the layer's fragments for text and
    for filters or masks.
  - CSS opacity (`save_layer_opacity_f`) opens with both flags when the
    scope allows it. Emulation alpha layers are unchanged.
- **Second cause:** when a picture is finished, SkRecord's optimizer folds
  SaveLayer → one draw → Restore into the draw. At 1x, a group holding one
  text run therefore had no layer at all. cc never folds a
  `SaveLayerAlphaOp` into text (`paint_op_buffer_iterator.cc`). Div now
  draws text inside Save + clip to the local clip bounds + Restore, which
  the optimizer keeps.
- **Native test:** `crates/divui/tests/native_opacity_lcd_text.rs` with
  fixture `tests/fidelity/fixtures/opacity-lcd-text-v1`. The fixture holds
  four pages at scales 1, 1.25, 1.5 and 2 from pinned Linux Chrome 147
  (real zoom), captured by
  `tools/qualification/capture_opacity_lcd_oracle.py`:
  - text in a group over the page: 1,329 → 0 differing pixels at 1x;
  - a disabled-button-like box: 557 → 0 at 1x;
  - the same group with a blurred box elsewhere in the layer (not
    eligible): Chromium and Div now both draw the text in grayscale;
  - the text without a group: the reference page.

  Above 1x, every page's LCD-colored pixel count equals Chromium's. The
  remaining differences there match the no-group page's. They are
  device-pixel layout (owner: engine, RFC 0011).
- **Remaining gap:** in the not-eligible page, Chromium's grayscale raster
  of LCD-configured text differs from Div's: 1,271 text pixels at 1x, plus
  700 in the blurred box. Owner: fidelity (follow-up).
- **Census and coverage** (WSL, 16 jobs, under the census lock), base
  main `7bbddf8` against the change at `929ae81`, with the real-zoom
  fidelity data `c82502d`:
  - census: 16,287 exact, 7,441 different, 0 errors on both sides;
  - coverage: 1,459 exact, 2,589 different, 156 errors on both sides.

  No page was lost or gained, and no Div image changed. Most census pages
  draw aliased Ahem, and only real-font pages use `chromium-linux-lcd`. No
  census page exercises this change, so the native test is its evidence.
- **Verification:**
  - `cargo test --locked -p divui --features "divui/async divui/http
    divui/testing" --test native_opacity_lcd_text` (WSL, CI Skia flags)
    passes.
  - `cargo test --locked -p div-paint` and the divui suite pass.
  - `python3 tools/qualification/capture_opacity_lcd_oracle.py --chrome
    <pinned Linux chrome> --check` matches the fixture (captured twice).
  - The census pair: `census_lock.py run --wait -- div-census.sh <sha>`,
    with per-page comparison of the two `expanded-summary.json` and
    `coverage-summary.json`.

## 16 November 2026: the site showcase at real zoom

- **What:** `site/showcase/capture.py` rerun on the real-zoom launcher
  (RFC 0010), which recaptures the 2x Chromium frame
  `site/src/assets/showcase/example-chromium-light.png`. No Div code
  changes.
- **Result:** Chromium now places the edges of the boxes sized by their
  text (the two buttons and the Live badge) one device pixel away. The
  card's borders and the colors are unchanged. Div's frame differs from
  the new one in 9,410 of 387,200 pixels (7,659 against the emulated
  frame). The home page now says only the colors and the card's borders
  match, and that text, and the buttons sized by it, can land a pixel off.
  Owner of the remaining gap: engine (device-pixel layout, RFC 0011).
- **Verification:** `python3 site/showcase/capture.py` (WSL, pinned Linux
  Chrome-for-Testing 147.0.7727.50) run twice: both frames are
  byte-identical (`81ca688c…`).

## 16 November 2026: text oracle values at real zoom on Linux and Windows

- **What:** `tools/qualification/probe_windows_text_oracle.py` gains
  `--platform linux`. It runs the same scenarios on pinned Linux
  Chrome-for-Testing 147.0.7727.50 (WSL) with the census launcher, and
  draws aliased Ahem from `ahem_noaa.conf` as the Linux oracle does. Both
  platforms write a schema 2 receipt that records each scale's
  `device_scale_method`. The receipts are `windows-text-oracle-v2.json`
  and `linux-text-oracle-v2.json` in build-with-div/div-fidelity-data at
  `1b86da8`, pinned by `fidelity-data.lock`. `windows-text-oracle-v1.json`
  stays as history.
- **Scale 1:** both platforms reproduce every value that
  `native_font_units.rs` and `native_line_height.rs` pin.
- **Above scale 1:** real-zoom Chromium lays out font-relative lengths and
  the Ahem word in device pixels, so its values move with the scale. For
  example, at 1.25x 2.5ch is 46 instead of 47.5, and the Ahem word's y is
  485.8 (Windows) or 487.4 (Linux) instead of 487. Div keeps its scale-1
  values at every scale. The tests no longer claim a Chromium match there:
  they pin Div's value and list Chromium's as a named gap
  (`crates/divui/tests/support/real_zoom.rs`). There are 28, 32 and 3 gaps
  on Windows (font units, line height, atomic word) and 27, 28 and 2 on
  Linux. A gap that starts matching, or that is never checked, fails.
  Owner: engine (device-pixel layout, RFC 0011).
- **Inventory:** RFC 0010 gains an addendum covering the missed Linux
  observations and about 35 other tests that pin a Chromium value above
  scale 1, by owner.
- **Verification:**
  - `cargo test --locked -p divui --features "divui/async divui/http
    divui/testing" --test native_font_units --test native_line_height`
    in WSL: 6 passed. Windows: CI.
  - `python tools/qualification/probe_windows_text_oracle.py --chrome
    <pinned Windows chrome.exe> --output <receipt>`: run twice, identical
    values.
  - The same with `--platform linux` in WSL: reproduces every scale-1
    value above.
  - `python tools/fidelity_data.py fetch --profile all`, then `verify`:
    30,517 files match `1b86da8`.

## 16 November 2026: web-font fixtures recaptured at real zoom

- **What:** `tests/fidelity/fixtures/webfont-fontations-v1` recaptured with
  `tools/qualification/capture_webfont_oracle.py` on the real-zoom
  launcher (RFC 0010), and the expected gaps in
  `crates/divui/tests/native_webfont_fontations.rs` set to the new counts.
  No Div code changes.
- **Frames:** the 1x frame is byte-identical to the emulated capture; the
  1.25, 1.5 and 2x frames changed. A second capture with `--check`
  reproduces all four.
- **Result:** 10 of 24 rows exact (21 of 24 against the emulated
  frames). At 1x all 6 are exact, at 1.25 2, at 1.5 2 and at 2x none.
- **Cause:** in 11 of the 14 rows that differ, the glyph raster is
  identical and the row is offset by one whole device pixel: Div's ink is
  1 device px higher (Geist Mono, Geist 650) or lower (Inter 16px), and a
  one-pixel shift leaves 0 differing pixels. Real-zoom Blink lays line boxes
  out in device pixels; Div lays out in CSS px and scales. Owner: engine
  (device-pixel layout, RFC 0011), not rasterization. The other 3 rows are
  the earlier Inter 13px gap (1,339, 2,423 and 4,153 pixels after the
  one-pixel shift).
- **Verification:** `python3 tools/qualification/capture_webfont_oracle.py
  --chrome <pinned Chromium 147.0.7727.50> --check`; `cargo test --locked
  -p divui --features "divui/async divui/http divui/testing" --test
  native_webfont_fontations` (Linux, WSL): passes with the 14 listed gaps.

## 16 November 2026: Chromium oracles at real zoom above scale 1 (RFC 0010)

- **What:** the shared launcher `render_chrome`
  (`tools/accountability/run_all_pixel_comparisons.py`) and
  `tools/qualification/windows_chrome.py` capture every scale other than 1
  with `--force-device-scale-factor` and a viewport-only
  `Emulation.setDeviceMetricsOverride` (`deviceScaleFactor: 0`), as
  RFC 0010 specifies. Scale 1 is unchanged. The census oracle identity
  (schema 2) and the summary's `chromium` block record
  `device_scale_method` (`none` or `forced`), so no emulated frame can
  satisfy a real-zoom lookup.
- **Reproduction:** 30 census ids that differ between the two methods, at
  all four profiles: every 1x frame (30) is byte-identical to the earlier
  census, and every non-1x frame that the 16 November forced-scale
  measurement captured (69) matches it.
- **Census and CSS Grid coverage at the new oracle** (Linux, 16 jobs,
  under the census lock, launcher `19269a7` on main `154d0a7`), next to
  the emulated run of the same Div output (`a9e9a46`); Div's frames are
  byte-identical in both runs:

  | Profile | Census exact, emulated → real zoom | Coverage exact, emulated → real zoom |
  | --- | --- | --- |
  | legacy-800x600@1 | 5,888 → 5,888 | 572 → 572 |
  | mobile-375x667@2 | 5,767 → 5,153 | 567 → 487 |
  | desktop-1280x720@1.25 | 5,344 → 2,183 | 529 → 183 |
  | desktop-1920x1080@1.5 | 5,540 → 3,063 | 559 → 217 |
  | Total | 22,539 → 16,287 of 23,728 | 2,227 → 1,459 of 4,204 |

  Every 1x Chromium frame is byte-identical to the emulated run (5,932
  census; 1,012 coverage, with the same 39 errors in both). This is the
  new baseline for "no regressions" (D4). The drop is the oracle, not
  Div: Div lays out in CSS px and replays paint at the scale, which
  matches emulation. Closing it is layout in device pixels (RFC 0011,
  engine) and device-pixel edge snapping.
- **Evidence:** per-profile, per-area and per-row results in
  [`archive/docs/progress/evidence/real-zoom-baseline-2026-11-16/`](https://github.com/build-with-div/div-fidelity-data/tree/c82502def3b921d2d46a6b2b4b2ca60ebaa6f843/archive/docs/progress/evidence/real-zoom-baseline-2026-11-16)
  in build-with-div/div-fidelity-data at `c82502d`, pinned by
  `fidelity-data.lock`.
- **Verification:** `python3 -m unittest
  tools.accountability.test_chromium_capture
  tools.qualification.test_windows_chrome
  tools.qualification.test_renderer_matrix` (Linux).
- **Not affected:** the canvas (#48) and SVG (#49) lanes capture
  their oracles at scale 1 only, so they need no recapture.
- **Remaining:** each owner's non-1x oracles, per the RFC 0010 inventory.
  The fidelity lane's own: the fractional-edges probe (#53, recaptured on
  its rebase), `webfont-fontations-v1` (done, above), the site showcase PNG
  and the Windows text values.

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

## 16 November 2026: fractional box edges against the real-zoom oracle

- **What:** a native test that compares box edges at fractional device
  positions with pinned Chromium, pixel for pixel. It adds no renderer
  change. Five 120x80 pages: a single box, two abutting boxes, a block
  split across two columns, a table caption split across two columns, and
  an absolutely positioned box whose child overflows into the next column
  (`tests/fidelity/fixtures/fractional-edges-v1/pages.txt`). They are
  captured at scales 1, 1.25, 1.5 and 2 by
  `tools/qualification/capture_fractional_edges_oracle.py` on the census
  launcher, which uses real zoom above scale 1 (RFC 0010); `--check`
  recaptures them and requires identical files.
- **What Chromium does:** at real zoom every edge lands on a whole device
  pixel. Each frame holds only white and green: no antialiased edge and no
  seam between abutting boxes or columns. The edge at CSS x 45 ends at
  device 56 at 1.25x and 68 at 1.5x, and the top edge at CSS y 21 starts at
  26 and 32. (Under the earlier CDP-emulated oracle the same edges were
  antialiased, with a seam of 0.1875 white at 1.25x; those frames are no
  longer the reference.)
- **Result on main:** all five pages are exact at 1x and 2x and differ at
  1.25x and 1.5x: `single` 113 and 135 pixels, `abutting` 224 and 209,
  `multicol` 224 and 209, `caption` 169 and 172, `overflow` 194 and 172,
  pinned as gaps with their own counts. In `single`, `abutting` and
  `multicol` every differing pixel is one of Div's antialiased edge pixels:
  Div lays out in CSS px and rasterizes the fractional device edge with
  partial coverage. In `caption` and `overflow`, 31 (1.25x) and 37 (1.5x)
  more pixels are fully painted in Div and white in Chromium: Div paints
  the box whole under a fragmentainer clip snapped outward to device
  pixels.
- **Owner:** layout in device pixels and device-pixel edge snapping
  (engine, RFC 0011). The fragment-rect paint change measured against the
  emulated oracle does not help here (0 gains and 14 regressions in the
  real-zoom census) and was dropped.
- **Verification:** `python3
  tools/qualification/capture_fractional_edges_oracle.py --chrome <pinned
  Chromium 147.0.7727.50> --check`; `cargo test --locked -p divui
  --features divui/testing --test native_fractional_edges` (Linux).
