# RFC 0010: Real-zoom Chromium as the oracle at every non-1x scale

Status: proposed, 16 November 2026. Owner: the fidelity lane. Consumers:
every lane that compares Div with Chromium at a device scale other than 1:
the engine track (the census, CSS Grid coverage, the qualification
probes), the framework track, the components track, the canvas and SVG
lanes, and the samples.

Decision, from the user on 16 November 2026: "Switch to real zoom".

## Problem

Every Chromium oracle Div captures at a device scale other than 1 sets the
scale through CDP device emulation: `Emulation.setDeviceMetricsOverride`
with `deviceScaleFactor: <scale>`. This is not how Chromium behaves on a
high-DPI display. There, the device scale is the browser's own, as with
`--force-device-scale-factor=<scale>`, and Blink lays out in device pixels
(zoom for DSF).

Measured on 16 November 2026, on Linux, the census platform. The capture
was pinned Chrome 147.0.7727.50 through the census harness's own launcher
and font environment:

- **49 frames at 1.25x and 1.5x.** The five fractional-edges probe pages,
  plus 30 census ids that are exact at 1x and 2x but differ at 1.25x
  and/or 1.5x (10 split boxes, 10 tables, 10 of the engine's off-grid
  layout candidates). Only 1 frame is byte-identical between the two
  methods; 27 differ in paint only and 21 differ in layout.
- **Controls:** at 1x, all 35 frames are identical. At 2x, 29 of 35 are.
- **Layout under real zoom.** CSS-px geometry lands on device-pixel steps:
  0.8 px at 1.25x, 1/3 px at 1.5x. For example, a fragment is 98.4 tall
  instead of 100, a box is 119.2 wide instead of 120, and a document is
  302.67 tall instead of 310. This matches the framework lane's
  ResizeObserver capture on Windows: a 1px border becomes 0.8px and the
  scrollbar becomes 15.2px. Under emulation, Blink lays out at 1x and
  only rasterizes at the scale.
- **Paint under real zoom.** With equal geometry, box edges are snapped to
  whole device pixels. Under emulation they are antialiased at fractional
  device positions. For example, the edge at device x 56.25 has coverage
  0.25 under emulation and none under real zoom.
- **Checks on the method.**
  - The emulated captures reproduce the census's cached oracle frames
    (all 39 census frames) and the committed probe fixtures (all 10).
  - Real zoom with no CDP override at all gives the same frames as the
    real-zoom method below (10 of 10 probe frames).

Scripts, rows and per-frame results:
[`archive/docs/progress/evidence/dsf-oracle-2026-11-16/`](https://github.com/build-with-div/div-fidelity-data/tree/44a972b6bff04e5c7f03158d91fa3082db9dd3d8/archive/docs/progress/evidence/dsf-oracle-2026-11-16) in
build-with-div/div-fidelity-data at 44a972b.

So at 1.25x, 1.5x and 2x the census and every other non-1x oracle measure
Div against a configuration that real users do not get.

## Proposal

At every device scale other than 1, Chromium oracles are captured with
real zoom. Scale 1 captures are unchanged; the two methods are identical
there.

**Capture method** (scale ≠ 1), in the shared launcher
`render_chrome` (`tools/accountability/run_all_pixel_comparisons.py`)
and every other capture tool:

1. **Launch** the pinned binary with `--force-device-scale-factor=<scale>`.
   The window is `--window-size=<max(width,500)+16>,<max(height,300)+160>`
   in DIPs, which is large enough to hold the viewport.
2. **Size the viewport** with `Emulation.setDeviceMetricsOverride`
   `{width, height, deviceScaleFactor: 0, mobile: false, screenWidth: width,
   screenHeight: height}`. A `deviceScaleFactor` of 0 overrides nothing
   about the scale: the page keeps the browser's own (forced) DSF. Only the
   viewport size is fixed. Window sizing alone cannot give the census
   viewports, because headless windows snap to device pixels: a 1278 or
   1279 DIP window gives 1279.2 or 1281.6 CSS px at 1.25x. The measurement
   above shows the size override changes no pixel.
3. **Require** `innerWidth`, `innerHeight` and `devicePixelRatio` to equal
   the profile's, as now. The screenshot (`Page.captureScreenshot`,
   `fromSurface`) is the physical frame. The capture still requires two
   identical consecutive screenshots.

**Oracle identity.** Each oracle records how its scale was set:
`device_scale_method: "forced"` (scale ≠ 1) or `"none"` (scale 1). The
census oracle cache identity (`chromium_oracle_identity`) and the summary's
`chromium` block carry it. A cached emulated frame can therefore never
satisfy a real-zoom lookup, and a summary says which oracle it measured.
The census harness's hash already changes with the launcher, so every
census frame is recaptured once. The 1x frames must come back
byte-identical to today's; that is the check that the switch left 1x
untouched.

**No emulated oracles remain.** A capture tool that sets
`deviceScaleFactor` to anything other than 0 or 1 is a defect. A test in
`tools/accountability/test_chromium_capture.py` checks the shared launcher.

## Affected tracks

Each oracle set is recaptured by its owner, with the new method, in the
owner's own PR. The inventory below lists them; the fidelity lane changes
only its own files and the shared launcher.

The inventory covers main 154d0a7, the data repository at 1c158de and
every sample. No committed oracle was captured with a forced scale.

| Owner | Oracle at 1.25x, 1.5x, 1.75x, 2x or 3x | Capture | Recapture |
| --- | --- | --- | --- |
| engine and fidelity | The census and CSS Grid coverage frames (the local oracle cache, not committed) | `run_renderer_matrix.py` → `render_chrome` | The launcher PR |
| engine | `crates/div-engine/tests/native_scrollbar_frame_chromium_oracles/` (32) | `capture_native_scrollbar_oracle.py` → `render_chrome` | Rerun the tool |
| engine | `crates/div-engine/tests/` `column_paint_phase_oracles/` (6), `fixtures/nested-multicol-chromium-oracle/` (4), `native_scrollbar_arrow_chromium_oracles/` (4), `source_layer_chromium_oracles/` (12), `source_layer_scrollbar_chromium_oracles/` (8); `crates/div-paint/tests/fixtures/rounded-source-layer-chromium-oracle/` (4) | External drivers outside the repository | A new in-repo capture tool for each set |
| fidelity | `tests/fidelity/fixtures/webfont-fontations-v1/` (3) | `capture_webfont_oracle.py` → `render_chrome` | Rerun the tool |
| fidelity | `tests/fidelity/fixtures/fractional-edges-v1/` (15; #53, unmerged) | `capture_fractional_edges_oracle.py` → `render_chrome` | On #53 |
| fidelity or framework | Windows text values in `crates/divui/tests/native_font_units.rs` and `native_line_height.rs` (3 tests), from `windows-text-oracle-v1.json` | `probe_windows_text_oracle.py` → `WindowsChrome` | Rerun the tool |
| components | `tests/fidelity/components/evidence/reference-v1.json` (162 of 216 rows) | `reference_harness.py compare` → `render_chrome` | Rerun the harness |
| (site) | `site/src/assets/showcase/example-chromium-light.png` (2x) | `site/showcase/capture.py` → `render_chrome` | Rerun the tool |
| sample-charts | `docs/probes/results/fidelity.json` (16 entries at 2x) | `docs/probes/compare.py` → `render_chrome` | Rerun the tool |
| sample-markdown | `docs/evidence/fidelity-v1.json` (24 entries at 2x) | `harness/compare.py`, its own CDP call | Change the tool, then rerun |

Some tools set the scale with their own CDP call, which the launcher
change does not reach:
- `reference_harness.py` `geometry`;
- `probe_native_geometry_oracle.py --scale`;
- `native_collapsed_tables/run_matrix.py`;
- `sample-markdown/harness/compare.py`.

Each moves to `device_scale_launch` or the same method in its owner's PR.

The data repository's census results and evidence are history and stay
as they are: the `gates/docs/renderer/generated/` census summaries, the
per-PR `archive/docs/progress/evidence/` receipts and the pre-alpha
`archive/docs/renderer/` images. A few receipts are measurements that
code was tuned against, and are re-measured by their owners:
- the scrollbar clip and compositing receipts, which
  `crates/div-paint/src/painter.rs` cites;
- the Windows text receipts.

The derived root-cause table `docs/progress/fidelity-root-causes-2026-11-15/`
is rerun after the new census.

Expected effect: exact counts at 1.25x, 1.5x and 2x drop, because Div
lays out in CSS px and replays paint at the scale (its current model),
which matches emulation, not real zoom. That drop is the new baseline. It
is not a regression in Div, and closing it is engine work (layout in
device pixels) and paint work (device-pixel edge snapping).

### Inventory addendum, 16 November 2026

The table above missed the Linux text values that
`crates/divui/tests/native_font_units.rs` and `native_line_height.rs` pin
at five scales. They came from emulated observations (the
`reference-only-3642` archive and `native-font-relative-v1`).
`probe_windows_text_oracle.py --platform linux` now measures them at real
zoom (`linux-text-oracle-v2.json`, next to `windows-text-oracle-v2.json`).
Both tests assert Chromium at scale 1 and list real-zoom Chromium's
values above scale 1 as gaps (engine, RFC 0011).

A sweep for others went through all 307 scale loops in 109 files under
`crates/` and `examples/`, keyword searches for single-scale captures,
and the Python tool tests. It found these tests, which pin a value
attributed to Chromium at a scale other than 1. They are not in the table
above, and nothing in them was re-captured at real zoom. Each owner
re-measures its own. The `src/` unit-test captures are not committed, so
they need a new capture.

| Owner | Tests | Chromium source cited | Scales |
| --- | --- | --- | --- |
| engine (layout) | `div-layout/src/intrinsic_sizing.rs` `chromium_atomic_word_fraction_survives_an_integer_raster_font_policy` | `native-general-font-chromium-v3824` | 5 |
| engine | `div-engine/tests/`: `fragmented_client_geometry.rs` (3 tests), `accessibility_pages.rs` (fractional legal bounds), `animation_progress_timelines.rs` (2), `animation_attachment_ranges.rs` (manual view seek), `opacity_operation_folding.rs` (4), `split_multicol_padded_continuation.rs`, `clip_path_owned_reference.rs`, `sticky_owned_geometry.rs`, `overflow_clip_owned_radii.rs`, `table_background_overflow_phases.rs`, and the geometry asserts of `nested_multicol_outer_fragmentation.rs` | stock Chromium 147 DOM or RGBA observations (`v4408`, `Chrome4912`, `v4610`/`v4618` and others) | 4 or 5 |
| engine (paint) | `div-paint/src/painter.rs`: `scroller_compositing_follows_blink_overflow_and_lcd_text_preference`, 7 border and decoration tests at 1.25x, `spaced_pattern_keeps_its_fractional_border_box_edge` (1.5x), 2 control tests at 2x, 3 fractional-edge coverage tests, `raster_background_keeps_packed_opacity_under_an_ancestor`, `rotated_aliased_inline_shift_follows_the_physical_strike`; `image_resource.rs` (3); `render.rs` `large_grayscale_glyph_mask_keeps_raw_coverage_as_chromium_skia` | Chrome 147 captures of WPT pages and layer traces | 1.25, 1.5, 2 or 5 |
| engine (text) | `div-text/src/font/platform.rs` `linux_subpixel_borrow_*` (2) | Chrome 147 at 2x | 2 |
| engine (tools) | `tools/qualification/test_scrollbar_probe_tools.py` (a trace from 1.25x, decoded in CSS px) | Chrome 147 trace | 1.25 |
| framework | `examples/c/scroll_metrics.c` `viewport(scale)` | Chromium's viewport metrics | 5 |

Checked and not affected:
- the entries already in the table;
- `divui/tests/resize_observer.rs`, whose capture tool already passes
  `--force-device-scale-factor`;
- about 15 tests that cite Chromium for behavior or ordering rather than a
  scale-dependent value;
- about 70 loop files that assert only Div's own determinism, replay or
  round-trips;
- the capture-method tests (`test_renderer_matrix.py`,
  `test_chromium_capture.py`, `test_summarize_renderer_census.py`).

## Migration

1. **This RFC (docs only).** The measurement is already in the data
   repository (commit 44a972b). The next `fidelity-data.lock` bump, in the
   launcher PR, pins it.
2. **Launcher PR (fidelity lane).**
   - `render_chrome`, `tools/qualification/windows_chrome.py` and the other
     shared capture tools use real zoom for scale ≠ 1, and the oracle
     identity and summary carry `device_scale_method`.
   - A recapture with `--check` (two captures, byte-identical) proves the
     method reproduces.
   - The PR brings a new census and CSS Grid coverage run at the new oracle,
     under the census lock. Its receipts are a new data-repo commit and a
     `fidelity-data.lock` bump, and the new exact counts per profile are
     reported next to the old ones.
   - The fractional-edges probe (#53, not yet merged) is rebased on it and
     its 1.25x, 1.5x and 2x fixtures are recaptured there.
3. **Each other oracle set**, in its owner's PR, recaptured with the shared
   launcher or the same method, with that owner's tests repinned to the new
   counts.

**How the baseline resets.** It is a new oracle, not a weakened gate.
- Comparisons stay exact. No tolerance is added and no gate is relaxed.
- The emulated census and coverage results stay in the data repository as
  history, under their existing paths, and are never mixed into the new
  census.
- From the launcher PR on, "no regressions" (decision D4) is measured
  against the first real-zoom census.
- A PR that compares across the reset reports the two baselines
  separately.

`main` stays green throughout. Until an owner recaptures, that owner's
committed emulated oracles still match their tests, because nothing in
them changes. The census baseline moves once, in the launcher PR.
