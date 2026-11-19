# Web-font raster under size and transform (T5, T6), 18 November 2026

Engine fillers for SVG and HTML text, from the SVG lane's text corpus
(`docs/progress/svg-text-paint-2026-11-17.md`). Linux Chromium draws web
fonts through Skia's Fontations scaler. Div draws them through a custom
typeface holding skrifa outlines hinted at the physical size, under a
unit-size font (`div-text` `shaping/web_font_outlines.rs`). Skia therefore
never saw the size or the transform the glyphs are drawn at.

- **T5, LCD size limit:** Skia masks glyphs above 48 device px in grayscale.
- **T6, hinting under rotation and skew:** Chromium leaves the glyphs
  unhinted when the device matrix is not axis-aligned.

Div now takes both decisions itself, from the font size and the matrix the
blob is drawn under. On Linux, 20 more SVG text captures are exact (264 of
290, was 244), and the 22 rotated and skewed captures that still differ
drop to a fraction of their old pixel counts. The census and CSS Grid
coverage do not change.

## What Skia does

Read from Div's own Skia (BSD), the copy rust-skia `a31b86b` builds:
`src/core/SkScalerContext.cpp` and `src/ports/SkTypeface_fontations.cpp`.
Chromium 147 pins Skia `abbe599f`, which was not fetched; these rules are
long-standing in both.

- **`too_big_for_lcd`** (`SkScalerContext::MakeRecAndEffects`). When the
  device matrix has scale or affine bits, a glyph is too big for an LCD mask
  when `det(post 2x2) × size² > 48²`, with the 2x2 entries relaxed to
  1/1024 (`sk_relax`). Otherwise when `size > 48`. Skia then asks for A8
  generated from an LCD mask (`kGenA8FromLCD`).
- **Fontations `onFilterRec`.**
  - It clears `kGenA8FromLCD`, so above the limit the mask is plain A8
    coverage, and with no LCD mask `kFull` hinting collapses to `kNormal`.
  - It sets hinting to `kNone` unless the glyphs are axis-aligned: no
    synthetic skew (`fPreSkewX == 0`), and either both off-diagonal or both
    diagonal entries of the 2x2 zero. A 90-degree rotation is
    axis-aligned.
- **Fontations constructor.** A font that relies on its TrueType
  instructions keeps strong hinting whatever `onFilterRec` chose. Div's
  `WebFontHinting::select` already checks this first, so it is unchanged.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `DeviceRaster`: the relaxed 2x2 of the device matrix, with `too_big_for_lcd(size)` and `is_axis_aligned(skew_x)` as above | `div-text` | `crates/div-text/src/shaping/web_font_outlines.rs` |
| `web_font(source_font, glyphs, device)`: the outline cache key holds the mask Skia keeps (`lcd` is false above the limit) and the hinting after `onFilterRec` (`None` unless axis-aligned). The returned font draws grayscale when LCD was asked for but the limit fired | `div-text` | `web_font_outlines.rs` |
| `ShapeResult::to_text_blob_under` and `to_positioned_text_blob_under` take the device matrix the blob is drawn under. The old entry points pass none (identity), as before | `div-text` | `crates/div-text/src/shaping/shape_result.rs` |
| HTML text passes the canvas's device matrix, read once and shared with the device-clip test, so the canvas is queried as often as before. The physical-scale blob passes it scaled by the inverse physical scale | `div-paint` | `crates/div-paint/src/text_painter.rs` |
| SVG text passes the canvas matrix, scaled back by the glyph scale factor and turned by the glyph's own `rotate`. The paint-server shader transform is unchanged (agreed with the SVG lane) | `div-svg` | `crates/div-svg/src/text.rs` |
| `div-text` unit tests initialise Skia at load, as `div-paint`'s do (`#[cfg(test)] initialize_skia_at_load!();`). A test that rasterizes a variable font ran without `SkGraphics::Init` while other tests ran it on parallel threads; the fidelity lane's `SkGraphics::Init` audit found it | `div-text` | `crates/div-text/src/lib.rs` |
| SVG text gaps: the T5 block and `run/italic` are removed; the T6 block becomes "T6 residue" with the new counts; T7 is named | `div-svg` corpus | `tests/fidelity/fixtures/svg-text-v1/pixel-gaps.txt`, `tools/svg/README.md` |

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Unit tests | `lcd_masks_stop_at_skias_size_limit`, `only_axis_aligned_glyphs_are_hinted`, `the_key_holds_the_mask_and_hinting_skia_keeps`, `web_fonts_above_the_lcd_limit_draw_grayscale` | Pass |
| SVG text corpus, Linux (WSL, CI's Skia flags) | `cargo test --locked -p div-svg --test text_corpus` | 264 of 290 captures exact, was 244. All 18 T5 captures and `run/italic` at 1x and 2x are exact. The 22 T6 captures drop, for example `list/rotate` at 1x from 483 to 66 differing pixels and `paint/rotate-stroke` at 2x from 3,372 to 997. T7 is unchanged (578 at 1x, 1,526 at 2x) |
| Linux suites (WSL, CI's Skia flags) | `cargo test --locked --no-fail-fast -j 4 -p divui -p div-engine -p div-svg -p div-text -p div-paint` at `b6dca584` | 3,336 passed, 0 failed |
| Windows suites | `cargo test --locked -j 4 -p div-text -p div-paint -p div-svg -p div-engine -p divui` at `b6dca584` | 3,333 passed, 0 failed. The whole `div-svg` suite passes, so its paint v1 to v5 gaps and the Windows `svg-text-v1` gaps are unchanged |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `b6dca584` against `main` at `c65b1245` | 16,296 of 23,728 exact before and after; 0 gains, 0 losses, 0 errors; Chromium and Div identical in all 23,728 comparisons. No census fixture draws a web font above 48 device px or under a rotated or skewed matrix |
| CSS Grid coverage (WSL) | coverage matrix, same head and base | 1,473 of 4,204 exact before and after, and 894 of 3,153 at the three real-zoom profiles; 0 statuses or images changed |

The tests, census and coverage measured `b6dca584`. This PR's code
differs from it only in `cargo fmt` line breaks. The evidence is in
div-fidelity-data `31eeae71`, pinned by this PR's `fidelity-data.lock`.

### After rebasing onto `main` `992d4d68`

`main` gained, among others, the corpus tests' Skia initialization (#92),
SVG masks (#97) and the Windows text raster preset (RFC 0012, #90 and
#93). The rebased head, `b4685cf7`, was measured again:

| Check | Command | Result |
| --- | --- | --- |
| SVG text corpus, Linux (WSL, CI's Skia flags) | `cargo test --locked -p div-svg --test text_corpus -- --nocapture` | 264 of 290 captures exact, and every listed gap differs by exactly its listed count, as before the rebase; positions: 24,056 numbers, none differ |
| Linux suites (WSL, CI's Skia flags) | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-text -p div-layout -p div-paint -p div-svg -p div-engine -p divui` | 9,455 passed, 0 failed |
| Windows suites | The same seven crates | 9,449 passed, 0 failed. `div-svg` passes, so the Windows `svg-text-v1` and paint v1 to v5 gaps are unchanged |
| Generators and ledgers | As above, and `fidelity_data.py verify --profile all` | All pass, except that SP20 was not checked locally: its inventory hashes Chromium's WPT tree and this machine's checkout is not the pinned one. This PR changes none of its inputs; CI checks it |

The census and coverage were not run again. No census fixture draws a
web font above 48 device px or under a rotated or skewed matrix, so the
rebase does not change which census paths this PR reaches.

## Named gaps

- **T6 residue, rotated and skewed glyphs.**
  - **Owner:** engine (`div-text`).
  - **Cause:** unmeasured. No further Skia rule was found in the scaler
    record.
  - **Cases:** 11 captures at 1x and 11 at 2x on Linux, listed in
    `pixel-gaps.txt`; each now differs at a few glyph edges.
  - **Measurement task:** after this PR.
- **T7, glyphs under a non-uniform scale.**
  - **Owner:** engine (`div-text`).
  - **Cause:** unmeasured. T6's rule leaves these glyphs hinted, because a
    non-uniform scale is axis-aligned. The lead is the Fontations scaler's
    `computeMatrices(kVertical)`: it hints at the vertical scale and maps
    the outlines by the remaining matrix.
  - **Cases:** `xf/scale-nonuniform` and `xf/viewbox-none`, 578 pixels at
    1x and 1,526 at 2x on Linux. Their positions are exact since T4.
  - **Measurement task:** after this PR.

## Not done here

- **Windows.** Windows Chromium draws web fonts through DirectWrite, not
  Fontations, so these rules are Linux's. The Windows text raster is
  RFC 0012's.
- **Skia `abbe599f`.** The rules were read from Div's Skia; a fetch of
  Chromium's pinned Skia needs approval.
