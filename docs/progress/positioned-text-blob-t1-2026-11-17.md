# T1: text blobs at caller-given glyph positions, 17 November 2026

`div-text` can now build a text blob whose glyphs sit where the caller places them, instead of where the advances put them. SVG text needs this: its `x`, `y`, `dx` and `dy` lists and its anchoring place characters one by one. The SVG lane asked for it (T1 in its requests to the engine lane); the parent approved it as an engine filler after R0b.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `ShapeResult::to_positioned_text_blob(characters, placements, raster_policy, device_scale, device_origin_x)`: a blob for the glyphs of a character range, one caller-given origin per glyph, in run order and in the units of the advances. A placement replaces the accumulated advance and the glyph offset | `div-text` | `crates/div-text/src/shaping/shape_result.rs` |
| Each run's raster policy and strike font, and the x handed to Skia (the LCD phase), move from `to_text_blob_with_raster_policy_at_scale` into `run_raster` and `raster_x`, which both builders call. A run therefore rasterizes the same way whoever positions its glyphs, and placements equal to the advances give the same blob | `div-text` | `shape_result.rs` |
| A range that splits a cluster, an empty range, or a placement count that does not match the range's glyphs gives `None` (`whole_cluster_glyphs`) | `div-text` | `shape_result.rs` |

The API is positions only. The SVG lane measured rotated SVG text on Windows Chromium 147 both ways: one positioned run per fragment under the fragment's rotation, and one RSXform blob. The two were byte-identical in all 20 captures. So T1 offers no RSXform form.

The refactor leaves `to_text_blob_with_raster_policy_at_scale` computing the same values in the same order. The census below checks that it changes no pixel.

The census measured `affb95e5` on `main` at `85fa7380`. This PR is rebased onto a later `main`, which no measured commit includes; its T1 commit is otherwise the measured one. The evidence is in div-fidelity-data `b679bf2f`, which `fidelity-data.lock` now pins.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| New tests | `cargo test --locked -p div-text --test positioned_text_blob_tests` (Windows) | 4 passed |
| `div-text` | `cargo test --locked -j 4 -p div-text --no-fail-fast` (Windows) | 1,120 passed, 0 failed, 1 ignored |
| Blob users | `cargo test --locked -j 4 -p div-paint -p div-engine --no-fail-fast` (Windows) | 1,792 passed, 0 failed |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `affb95e5` against `main` at `85fa7380` | 16,287 of 23,728 exact before and after; 0 Div images changed, 0 gains, 0 losses, 0 errors |
| CSS Grid coverage, all four profiles (WSL) | coverage matrix, same head and base | 1,473 of 4,204 exact before and after; 0 statuses or images changed |

The tests, in `crates/div-text/tests/positioned_text_blob_tests.rs`:
- `placements_equal_to_the_advances_give_the_advance_blob`: "Hello, world" at 10, 13.33 and 16 px; all six raster policies; device scales 1 and 2; with and without a device origin. Bounds and pixels on an RGB-stripe (LCD) surface equal those of the advance-positioned blob.
- `a_character_range_takes_only_its_glyphs`: a range's blob equals the blob of `sub_range` over the same characters, for every policy.
- `moved_glyphs_paint_where_they_are_placed`.
- `ranges_and_placements_that_do_not_fit_give_none`: an empty range, too few placements, too many, and a range ending between a base and its combining mark.

## Not done here

- **SVG text's use of it:** `div-svg` (the SVG lane).
- **Web fonts on Windows:** Windows Chromium paints Geist and Inter with LCD subpixel AA. Div's resolved faces have `AntiAlias` edging, so `ChromiumAuthorLcd` falls back to `Skia` grayscale for them, in HTML text as in SVG text. T1 passes the policy through unchanged. The edging belongs to the Windows LCD text work (the fidelity lane), routed through the parent on 17 November 2026; the SVG text pixel cases on Windows that need LCD are named gaps of that work, not of T1.
- **T4 (font size truncated to 1/100 px) and T3 (advances truncated to 1/65536 px):** the next two engine filler PRs.
