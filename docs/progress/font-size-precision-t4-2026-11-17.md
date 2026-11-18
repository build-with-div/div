# T4: font sizes truncated to 1/100 device px, 17 November 2026

Div now makes each font at the size pinned Chromium 147 makes it, and finds it as Chromium's font cache does. The SVG lane found the difference (T4 in its requests to the engine lane). Its text-position corpus differed from Chromium in the `xf/scale-nonuniform`, `xf/skew` and `xf/viewbox-none` cases only by font size. The parent approved T4 as an engine filler after T1. T4 is clean-room: it rests on black-box measurements of pinned Chrome, and no Blink font source was read.

## What Chromium does

All measurements are on Windows Chrome-for-Testing 147.0.7727.50. Each case lays a 36-character text out at one CSS size, as a span and as SVG `<text>`. Two cases were shaped with one font when both their span width and their `getComputedTextLength` are equal.

- **Made size.** A font is made at its device-pixel size truncated to 1/100 px, after `size-adjust` and `font-size-adjust`.
  - At 1x, 16.000 to 16.009 px give one width, and 16.010 px another.
  - At 2x, 16.009 px alone is made at 32.01 device px, not 32.018.
  - With `size-adjust: 150%`, 16.009 px is made at 24.01 (24.0135 truncated), not at 16.00 × 1.5.
- **f32 arithmetic.** The truncation runs in f32. Of the 2,001 two-decimal sizes from 10 to 30 px, 76 drop to the hundredth below; 16.05 is 16.0499992 in f32. One truncation in f32 predicts exactly these; the same arithmetic in f64 does not.
- **Cache key.** A document's font cache finds a font by the device size truncated twice, before `size-adjust`. A later size with the same key takes the font made for the first.
  - At 2x, 16.001 and 16.009 px share the key 32.00, so whichever the page meets first sets the font for both. Each alone has its own.
  - At 2x, 13.001 and 13.009 px keep their own fonts: their keys are 26.00 and 26.01.
  - At 1x, 16.22 px is made at 16.21. That value is 16.2099991 in f32, so it keys at 16.20 and, after 16.21 px, uses 16.21's 16.20 font.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `font_cache_size(size, device_scale_factor)`: the size a font is made at, the device size truncated to 1/100 px in f32, in CSS px. `make_platform_data` makes every document font at it, after `size-adjust` and `font-size-adjust` | `div-text` | `crates/div-text/src/font/description.rs`, `collection.rs` |
| `font_cache_key_size`: the device size truncated twice. The collection's instance key uses it in place of the exact size (and of the specified size, which does not change the font made). A later size with the same key takes the cached font | `div-text` | `description.rs`, `collection.rs` |
| The font-relative resolver (`ch`, `ex`, `lh`) no longer floors the size itself. The primary font is already made at the truncated size, and flooring first applied the truncation twice (16.05 would give 16.03's font) | `div-text` | `crates/div-text/src/font/relative.rs` |
| The `FontCache` facade test that held 16.001 and 16.009 px apart now checks that they share a font | `div-text` | `crates/div-text/src/font/cache.rs` |

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `tools/qualification/capture_font_size_precision_oracle.py` on Windows with pinned Chrome 147.0.7727.50, written to `tests/fidelity/fixtures/font-size-precision-v1/chromium-windows.json` | 78 runs, 4,514 cases, for DejaVu Sans and Geist as web fonts. Page runs at 1x: the 2,001 two-decimal sizes, thousandths around 13.33, 16.00 and 25.29, and `size-adjust` 150% and 110% and `font-size-adjust` 0.6. Each size alone at 1.5x and 2x. Eight size pairs in both orders on one page at 1.5x and 2x |
| Oracle test | `cargo test --locked -p div-engine --test font_size_precision` (Windows) | Div gives two sizes one font exactly when Chromium does in all 4,008,046 pairs, apart from 25 named gaps (below) |
| SVG text corpus | `cargo test --locked -p div-svg --test text_corpus` on Windows and on Linux (WSL), at `main` `de980254` | The 10 `xf/*` gaps that `tests/fidelity/fixtures/svg-text-v1/known-gaps.txt` listed under T4 are exact, so this PR removes them. All 24,056 Linux numbers are now exact; on Windows only the 7 T3 cases still differ. In `pixel-gaps.txt` (on `main` `de980254`, after #85), the 6 Windows `xf/*` captures now differ only in raster tone (at most 16 per channel) and move to the Windows text raster block; the 4 Linux captures under a non-uniform scale (`xf/scale-nonuniform`, `xf/viewbox-none`) drop from 608 to 578 differing pixels at 1x and from 1,710 to 1,526 at 2x, and stay listed as an engine gap whose cause is not yet measured (T5/T6). Linux was run with CI's Skia flags |
| Unit tests | `font_cache_size_truncates_device_pixels_to_hundredths_in_f32`, `font_cache_key_size_truncates_twice`, `sizes_with_one_font_cache_key_share_a_font` | Pass |
| Touched crates | `cargo test --locked -j 4 -p div-text -p div-layout -p div-paint -p div-engine --no-fail-fast` (Windows) | 8,883 passed, 0 failed |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `77529df1` against `main` at `85fa7380` | 16,287 of 23,728 exact before and after; 0 gains, 0 losses, 0 errors. 5 Div images changed, all closer to Chromium, all at `desktop-1280x720@1.25` (below) |
| CSS Grid coverage, all four profiles (WSL) | coverage matrix, same head and base | 1,473 of 4,204 exact before and after; 0 statuses or images changed |

All five rows paint scroll-button or `input` text, whose default size is 13.333 px: 16.666… device px at 1.25x, which T4 makes at 16.66. No row changed at the other three profiles. Mismatched pixels, before and after:

| Row | Before | After |
| --- | ---: | ---: |
| `wpt/css_overflow/scroll-markers_root-scroll-button` | 614 | 603 |
| `wpt/css_overflow/scroll-markers_scroll-button-on-object-ref` | 1,112 | 1,107 |
| `wpt/css_overflow/scroll-markers_scroll-buttons-appearance` | 3,422 | 3,373 |
| `wpt/css_position/position-absolute-semi-replaced-stretch-input` | 77,428 | 77,424 |
| `wpt/css_position/position-absolute-semi-replaced-stretch-input-ref` | 77,428 | 77,424 |

The census measured `77529df1` on `main` at `85fa7380`. This PR is rebased onto a later `main` (`de980254`), which no measured commit includes. Its T4 commit is otherwise the measured one. The evidence is in div-fidelity-data `24125445`. `fidelity-data.lock` on `main` already pins `f70a3744` (0.1 PR 3a), which contains it, so this PR does not change the lock.

## Named gaps

| Case | Chromium | Div | Owner |
| --- | --- | --- | --- |
| `font-size-adjust: 0.6` pages: 11 DejaVu Sans and 14 Geist size pairs | shares a font in a pattern that fits a cache key taken after the adjustment (not confirmed) | keys by the unadjusted size | text (engine), a measured follow-up |

## Not done here

- **Optical sizing.** `opsz` still follows the exact CSS size. Chromium's choice was not measured.
- **Linux.** The census measures Linux Chromium. The SVG lane measured the same truncation on Linux (scaled SVG text). This oracle is Windows only.
- **T3**, advances truncated to 1/65536 px: the next engine filler PR.
