# Root causes of the remaining census differences, 15 November 2026

This report classifies every census row on which Div's picture differs from
Chromium's, by root cause and owning subsystem, so the fidelity work can
start on the causes that affect the most rows. It changes no code and no
gate.

Two kinds of number appear below and are kept apart:

- **Measured.** Counted over every non-exact row by
  [`analyze.py`](fidelity-root-causes-2026-11-15/analyze.py) from the
  census pictures. Rerunning it on the same run gives the same numbers.
- **Estimated.** A stratified random sample of 90 of the 703 failing tests
  was reviewed by eye and given one cause each; the estimates scale those
  labels to every test and row in each stratum. They come with a 95%
  interval and are not counts.

## Inputs

| Input | Value |
| --- | --- |
| Evidence data | `build-with-div/div-fidelity-data` at `3646fb28bb64d105643cc88db8293af78cb86077`, the commit `fidelity-data.lock` pins on `main` (`127179c`) |
| Census receipt | [`flex-pass-cache-div-census-v1.json`](evidence/flex-pass-cache-div-census-v1.json): base `5b7b8e47`, head `18fffb9c`, 22,519 of 23,728 rows exact |
| Coverage receipt | [`flex-pass-cache-div-coverage-v1.json`](evidence/flex-pass-cache-div-coverage-v1.json): 2,227 of 4,204 rows exact |
| Per-row pictures | The private run behind that receipt: `expanded-summary.json` at commit `18fffb9ce36f413cca898e1cb2967fc7ab083232`, `profile_result_sha256` `688b533a4bb91156193a583809977ab9717c99ebd4d09e579af603a4af595004`, 1,209 different, 0 errors |
| Chromium | 147.0.7727.50, Linux Chrome for Testing, the census oracle |

The pictures are the receipt's own run. Its renderer sources match `main`
at `9168ac9` except for a doc comment in
`crates/div-layout/src/pass_cache.rs`; `main` has since moved only in CI
and tools (`127179c`). The pictures themselves are not in the data
repository (about 4,800 PNGs); the per-row table
[`rows.csv`](fidelity-root-causes-2026-11-15/rows.csv) and
[`summary.json`](fidelity-root-causes-2026-11-15/summary.json) hold
everything the report uses.

`docs/status.md` still gives 22,517 exact; the receipt above records
22,519.

### Reproduction

The census (WSL, 16 jobs, about 75 minutes, under the census lock):

```sh
python3 tools/qualification/run_renderer_matrix.py --suite expanded \
  --pixel-compare <build>/pixel_compare --build-receipt <receipt.json> \
  --chrome <linux-147.0.7727.50>/chrome-linux64/chrome \
  --cache-dir <cache> --results-dir <run> --jobs 16
```

The classification (any platform, Python 3 with NumPy and Pillow, about a
minute):

```sh
python3 docs/progress/fidelity-root-causes-2026-11-15/analyze.py \
  --run <run> --output root-causes.json
```

It asserts that its mismatch count for every row equals the census
summary's, draws the same sample from its fixed seed (2026), and writes
`rows`, `measured`, `sample` and `estimated` as in `summary.json`.

## Measured

### Where the differences are

The 1,209 non-exact rows belong to 703 tests. By profile:

| Profile | Scale | Rows not exact |
| --- | --- | --- |
| `legacy-800x600@1` | 1 | 44 |
| `mobile-375x667@2` | 2 | 165 |
| `desktop-1280x720@1.25` | 1.25 | 598 |
| `desktop-1920x1080@1.5` | 1.5 | 402 |

By the scales at which a test fails:

| Failing scales | Tests |
| --- | --- |
| 1.25 only | 252 |
| 1.25 and 1.5 | 205 |
| 1.25, 1.5 and 2 | 83 |
| 1.5 only | 73 |
| all four | 34 |
| 1.25 and 2 | 23 |
| 2 only | 18 |
| 1 only | 7 |
| 1.5 and 2 | 5 |
| 1, 1.5 and 2 | 2 |
| 1 and 1.25 | 1 |

**530 of the 703 tests (735 rows) fail only at the fractional scales 1.25
and 1.5.** Only 44 rows differ at 1x.

By font profile: 867 rows use the registered Ahem font, 257 the
deterministic test collection, and 85 registered real fonts. Real fonts
are rare in the census: 904 of its 23,728 rows use them, and 819 of those
are exact.

### What the differences look like

For each row, `analyze.py` measures where the mismatched pixels sit:

- 1,160 rows have every mismatched pixel on a colour edge of one picture
  or the other;
- 1,055 rows have no mismatched pixel whose whole 3×3 neighbourhood also
  differs, so the differences are lines one or two pixels wide, not areas;
- 994 rows differ on at most 1,000 pixels, and 419 on at most 100;
- for 1,118 rows, no whole-picture shift of up to 4 pixels explains more
  than 10% of the mismatches, so the differences are local, not one
  offset.

Each test is put in one stratum by its largest row:

| Stratum | Rule | Tests | Rows |
| --- | --- | --- | --- |
| fractional-thin | not real-font, not area; fails only at 1.25 and 1.5 | 490 | 684 |
| other-thin | not real-font, not area; fails at 1 or 2 | 121 | 332 |
| real-font | the row uses registered real fonts | 48 | 85 |
| area | over 30% of mismatches inside a 3×3 block, or over 0.2% of the picture | 44 | 108 |

Whether each side of a mismatched pixel is a flat fill colour (one that
covers many pixels of either picture) or a blend (anti-aliasing, coverage,
a glyph mask), weighted by pixels:

| Stratum | Both flat | Chromium blend, Div flat | Div blend, Chromium flat | Both blend |
| --- | --- | --- | --- | --- |
| fractional-thin | 0.23 | 0.28 | 0.10 | 0.39 |
| other-thin | 0.32 | 0.11 | 0.08 | 0.49 |
| real-font | 0.10 | 0.15 | 0.11 | 0.64 |
| area | 0.91 | 0.03 | 0.01 | 0.05 |

In the thin strata, an edge that one engine draws as a hard step the other
often draws as a blend, and the reverse. In the area stratum both sides
are flat colours: content is in a different place.

## Estimated causes

The 90 reviewed tests were drawn per stratum (30 fractional-thin, 30
other-thin, 15 real-font, 15 area) and each was given the cause its
largest row shows, from Chromium, Div and difference crops magnified 4×.
`LABELS` in `analyze.py` records every label; `summary.json` lists the
sample.

| Code | Cause | Tests (95% interval) | Rows | Owner |
| --- | --- | --- | --- | --- |
| F | Sub-pixel geometry of axis-aligned edges: a box edge or a seam between fragments lands on a different device pixel, or one side anti-aliases it | 445 (380–511) | 617 | open: layout coordinate space and paint snapping, see below |
| I | Image and background-image tiles: `background-repeat: round` and `space`, `background-size`, clipped images | 98 (37–158) | 181 | fidelity lane, paint (`crates/div-paint/src/painter.rs`, `image_resource.rs`) |
| A | Diagonal and curved edges: transforms, rounded clips, gradient stripes | 44 (9–78) | 117 | fidelity lane, paint anti-aliasing |
| L | Layout: box geometry differs by more than a pixel | 37 (23–51) | 116 | engine track, layout |
| C | Native controls and scrollbars: textarea resizer, scroll buttons, range thumb, `details` marker, `select` border, scrollbar gutter | 34 (17–51) | 92 | fidelity lane, paint (native theme in `painter.rs`, `crates/div-engine/src/scroll_button.rs`) |
| T | Text glyphs: one glyph or a run drawn differently or at another position | 34 (20–47) | 67 | fidelity lane, `div-text`; baseline position is engine layout |
| B | Dotted and dashed borders | 6 (0–12) | 13 | fidelity lane, paint |
| P | Clip and effect geometry | 6 (0–12) | 6 | fidelity lane, paint |

Per stratum, the reviewed labels were:

- fractional-thin (30): F 25, I 4, A 1;
- other-thin (30): I 8, C 7, F 6, A 6, T 2, L 1;
- real-font (15): T 8, F 4, L 3;
- area (15): L 8, P 2, B 2, C 2, A 1.

Representative tests (the profile is the one reviewed):

- **F.** `css_break/flexbox_multi-line-column-flex-fragmentation-051` and
  `-057` at 1.25 (33.333px columns put each column seam on a different
  device column), `css_flexbox/flexbox-align-self-vert-rtl-004`,
  `css_break/table_table-border-008` at 1.5,
  `css_backgrounds/box-shadow_slice-inline-fragmentation-001` (Div
  anti-aliases an edge Chromium snaps hard),
  `css_position/static-position_vlr-ltr-rtl.tentative`.
- **I.** `css_backgrounds/background-repeat-round-1a`, `-1c`,
  `background-repeat-space-1c` to `-8`,
  `background-size_background-size-contain`.
- **A.** `css_break/transform-006` to `-009`,
  `css_overflow/rounded-overflow-clip-visible`,
  `css_backgrounds/border-radius-content-edge-001`.
- **C.** `css_overflow/scroll-markers_scroll-buttons-enabled-vertical-ltr`,
  `css_sizing/range-percent-intrinsic-size-1-ref`,
  `css_flexbox/flexbox-basic-textarea-vert-001-ref`,
  `css_display/display-contents-details-001-ref`,
  `css_overflow/scrollbar-gutter-vertical-lr-002`.
- **T.** `css_sizing/slice-nowrap-intrinsic-size` and
  `clone-nowrap-intrinsic-size` at 1x (one glyph after an inline box
  boundary), `css2_floats/float-nowrap-4-ref`,
  `css_multicol/multicol-breaking-005`,
  `css_flexbox/flexbox-align-self-baseline-horiz-001a` (whole runs
  shifted).
- **L.** `css_sizing/clone-intrinsic-size`,
  `css_display/display-contents-dynamic-fieldset-legend-001`,
  `css_break/out-of-flow-in-multicolumn-039`,
  `css_break/table_repeated-section_variable-fragmentainer-size-003-crash`,
  `css_flexbox/flexbox-align-self-baseline-horiz-008`.
- **B.** `css_sizing/fit-content-block-size-fixedpos`.
- **P.** `css_overflow/overflow-clip-content-visual-overflow`.

### Ranking

By rows affected, F is about half of all differences (617 of 1,209
estimated), then I, A, L and C at about 90 to 180 each.

By how fundamental they are:

1. **F, sub-pixel geometry.** One mechanism across every feature area
   (fragmentation, flex, multicol, tables, positioning), and it is why
   1.25 and 1.5 carry 1,000 of the 1,209 rows. Div's renderer contract
   lays out in logical CSS pixels and replays the paint record with the
   device scale (`docs/renderer/contract.md`, device transform before
   raster). Chromium lays out with the zoom set to the device scale, so
   its LayoutUnit (1/64 px) rounding and pixel snapping happen in device
   pixels. **This is a hypothesis.** It fits the scale pattern and the
   seams seen in review, but no test has yet shown that changing where
   rounding happens turns these rows exact. Whether to lay out at device
   scale is a layout architecture question; snapping at paint time is
   paint.
2. **T, text.** Few census rows, because the census is mostly Ahem and the
   test collection, but every real page draws text: every components-track
   page differs inside its text (`docs/progress/tracks/components.md`).
   The census does not measure this well; see below.
3. **I, A, C, B, P.** Paint fidelity of individual features, each with its
   own fix.
4. **L.** Layout differences of more than a pixel; engine-owned, feature
   by feature.

## Grid coverage

The coverage receipt's 4,204 rows are CSS Grid tests. 2,227 are exact,
1,821 differ and 156 are errors (39 tests at each of 4 profiles, "Div
render failed"). Measured on the same private run:

| Profile | Exact | Different | Errors |
| --- | --- | --- | --- |
| 1 | 572 | 440 | 39 |
| 2 | 567 | 445 | 39 |
| 1.25 | 529 | 483 | 39 |
| 1.5 | 559 | 453 | 39 |

Of 536 failing tests, 478 fail at all four scales, and the median
difference is 15,750 pixels. These are layout differences, not raster
ones; they belong to the engine track's grid work, not to this lane.

## Known failures

- **The native clipping failure** (`actual_native_scrollbar_frames_are_clipped_independently_of_track_and_thumb_ink`
  in `crates/div-engine/tests/native_resize_control_clip.rs`, "control ink
  outside physical border y=57, scale=1.25" in the 17 October checkpoint)
  **now passes.** `main` CI run 43596224429 at `9168ac9` reports it `ok` in
  both the Linux `rust-parity` job (132451182264) and the `windows-msvc`
  job (132451182197). `AGENTS.md` and `docs/CI.md` still describe it as a
  known failure; updating them belongs to their owners.
- **Unowned residuals.** `tools/qualification/residual-ownership-v2.json`
  assigns a root cause to one test
  (`css_flexbox/stretch-flex-item-radio-input`, an external backend
  discrepancy). The other 702 failing tests have no recorded owner; this
  report's per-row table gives each sampled one a reviewed cause and every
  row its stratum.

## Text, and the sample-markdown items

The census under-measures text: 904 of its rows use real fonts, and 85 of
those differ. The components track and `build-with-div/sample-markdown`
(at `b3a74d5a`) see text differences on every page:

- **MD-12** (`docs/design.md` there): the line box is 1/64 px off at
  13.6px (`h6`, `code`); `h1` glyphs (32px, weight 600) differ at 1x; text
  sits one device pixel lower at 2x in some paragraphs. The 1/64 px line
  box is layout (LayoutUnit rounding of the line height); glyph shapes and
  the device-pixel glyph origin are `div-text` and this lane.
- **MD-21** (KaTeX) needs four things:
  - **WOFF2 decoding on Linux: done.** `div-text` decodes WOFF and WOFF2
    to sfnt on every platform before registering the face
    (`crates/div-text/src/font/collection.rs`, `wuff::decompress_woff2`;
    `wuff` 0.2.9 is MIT). It came from
    [open-ui#97](https://github.com/zhuowcui/open-ui/pull/97); the components track
    records its `MalformedFont` errors gone since.
  - **`@font-face` in stylesheets:** needs the stylesheet cascade (the
    runtime CSS work), engine-owned.
  - **Exact inline-block baselines:** engine-owned. Layout takes the
    inline-block baseline from its last line box or synthesizes one
    (`inline_block_last_line_baseline` in
    `crates/div-layout/src/inline/algorithm.rs`); `div-text` supplies the
    font metrics it uses.
  - **SVG paths:** the SVG work, not this lane.
- The components track's "glyphs sit at a different vertical position"
  finding (each line of the font specimen covers different rows, with no
  whole-pixel shift aligning them) is the same kind of difference as MD-12
  and the T rows above.

## Windows

The census runs on Linux only. The one Windows measurement on record is
[`windows-text-raster-policy-v1.json`](evidence/windows-text-raster-policy-v1.json):
the registered-Ahem text-port tests at 1x, compared by
`tools/qualification/compare_windows_text.py` against Windows Chromium
147.0.7727.50 run with `NoFontAntialiasing`. Its head `3db12944` had
2,785 of 2,927 rows exact and 142 different (the base, `fbf98d62`, had
2,134 exact); 43 rows were exact on Linux but not on Windows. It covers
neither the fractional scales nor real fonts. **Div's real-font text on Windows,
rasterized with DirectWrite in Chromium, has never been compared**, so
Windows text fidelity is unqualified.

### Measured at `9168ac9`, 15 November 2026

The renderer sources at `9168ac9` are those of `main`. Windows 11 with MSVC,
`cargo build --release --locked -p pixel-compare` with the CI Skia flags and
`-USK_GAMMA_APPLY_TO_A8`, against Windows Chrome for Testing 147.0.7727.50.
The comparisons ran under the Windows text harness, one run per profile:

```sh
python tools/qualification/compare_windows_text.py --pixel-compare <pixel_compare.exe>   --chrome <chrome-win64/chrome.exe> --profile <profile>   --linux-summary <expanded-summary.json> --output <report.json>
```

Registered Ahem, the 2,927 text-port tests at each profile:

| Profile | Exact | Different | Errors | Exact on Linux, different on Windows |
| --- | --- | --- | --- | --- |
| 1 | 2,877 | 50 | 0 | 22 |
| 2 | 2,788 | 139 | 0 | 10 |
| 1.25 | 2,500 | 425 | 2 | 15 |
| 1.5 | 2,620 | 305 | 2 | 12 |

- **Errors.** The two errors are `css_overflow/overflow-replaced-element-002` and
  its reference: Chrome's screenshots did not settle. Both also differ on
  Linux.
- **The same tests fail.** Almost every Windows difference is a test that also
  differs on Linux.
- **Rows exact on Linux but not on Windows: harness and fonts.** Most of these
  (14 at 1x, 9 at 2x, 8 at 1.25 and 8 at 1.5, reviewed one by one) are the
  harness or font environment, not Div's rendering:
  - pages decoded as windows-1252 because no charset is declared;
  - the harness's injected `@font-face` style painted as text under
    `display: contents`;
  - iframe documents and generic families resolving to Windows' default
    fonts.
- **The rest are Div:**
  - native controls and scroll buttons;
  - one glyph a device row taller in Chrome (10px Ahem at 1.25);
  - a sticky box 1–2 device pixels off at 1.5.

**Real fonts**, a sample of 38 of the 226 real-font tests (every sixth), with
the census's `chromium-linux-lcd` configuration:

| Profile | Exact | Different | Exact on Linux, different on Windows |
| --- | --- | --- | --- |
| 1 | 12 | 26 | 24 |
| 1.25 | 10 | 28 | 24 |

These are estimates of the review, not counts of all real-font rows:

- **About 15 tests: different LCD colour values.** Glyph shapes and positions
  match, but the subpixel colour values on the glyph edges differ, mostly by
  32 or less. Div's DirectWrite path uses the Linux gamma and contrast (1.2
  and 0.2); what Windows Chrome uses has not been measured.
- **About 9 tests: different generic families.** Windows Chrome resolves
  `monospace`, `sans-serif` and `serif` to Consolas, Arial and Times New
  Roman. Div's test collection maps them to DejaVu.

**Windows text fidelity is therefore not qualified.**

## Licence

Every fix in this lane stays MIT or permissive, as
`tools/accountability/dependency-policy.json` requires (Apache-2.0, MIT,
BSD, ISC, Unicode or FTL). FreeType is used under its FTL terms only.
Where Chromium's behaviour comes from copyleft code, Div writes its own or
uses a permissive equivalent, and records any measured mismatch that
remains as a gap.

## Next steps

For the fidelity lane, one focused PR each, proved by the census or a
harness with every gate unchanged or tightened:

1. **Test the F hypothesis before changing anything.** Take a handful of F
   tests (the fragmentation seams at 1.25, a flex edge at 1.5), compute
   Chromium's device-space snapped rectangles and Div's, and show whether
   rounding in device space reproduces Chromium's pixels. The answer
   decides whether F is a paint snapping fix in this lane or a layout
   change for the engine track.
2. **Measure text properly.** A real-font text harness over the
   components and sample-markdown pages on Linux and Windows, so text
   fixes have a gate of their own.
3. **Paint fixes by rows:** I (background tiling for `round` and `space`),
   then A (path and transform anti-aliasing), then C (native controls and
   scrollbars), then B and P.

For the engine track, routed through the parent session: L (the layout
tests above), grid coverage, the F architecture question if step 1 points
at layout, `@font-face` in stylesheets, inline-block baselines and the
MD-12 line box.
