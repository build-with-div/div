# RFC 0006 A2: partial raster (27 November 2026)

Tier 0 item 0.7, step 4 of [RFC 0006](../alpha/rfcs/0006-rendering-pipeline.md).
Branch `pipeline/a2-partial-raster`, code commit `f158ff8` on `main`
`aa61757`. SVG lane, owned by the engine track; the `divui` change is
reviewed by the framework track. The later commits add a `divui` test,
regenerate the renderer contract's style inventory and correct A1's record;
none of them changes a binary measured below.

**Rebase onto `main` `7796adf`.** After these measurements the branch was
rebased onto `7796adf`, which adds #154 (`matchMedia`), #155 (an RFC) and
#156 (component harness). Every code patch ID is unchanged (the code commit
is `09aa0a67` by `git patch-id --stable` before and after), and the
renderer and v0.2 contract checks pass at the rebased head, so the census
pair, the gates and the cost carry. They were measured without #154's code.
Only the record's own commit changed, to merge the engine track's log.

## What changed

- **Partial raster** (`div-paint/src/partial_raster.rs`, new). A
  `RetainedRaster` keeps the last frame's raster. For the next recording it
  rasters again only the 256-pixel tiles whose visible rect meets the damage,
  each tile whole from the new recording, and keeps every other pixel.
  - Every physical pixel belongs to exactly one tile's visible rect (the
    full raster's own tile loop decides this), so a damaged tile ends with
    the full raster's pixels.
  - An undamaged tile keeps the previous frame's pixels. They equal the full
    raster's because A1's damage covers every pixel that differs between
    two full frames (A1's gate, rerun below).
  - A tile drawn over an earlier frame first clears its visible rect, as
    the full raster draws each tile over a cleared surface.
  - It rasters in full when the recording rasters untiled (at most 512
    pixels each way, which the RFC keeps whole), and when the viewport,
    scale, raster configuration, LCD surface or raster strategy differs.
  - Viewport controls (the root scrollbars) are drawn over the tiles, so
    when a recording has them the tiles are kept in their own surface and
    the controls and the scanline closure are composed over a byte copy of
    it each frame.
- **Shared tile loop** (`div-paint/src/render.rs`). `rasterize_picture`'s
  tile loop and its overlay and closure are now `replay_tiles` and
  `compose_viewport_overlay`, used by both paths, so the partial path cannot
  drift from the full one. No comment was edited, reworded or moved.
- **Compositor** (`div-compositor`). `SoftwareCompositor::render` keeps the
  previous scene, its raster and its frame. The next scene rasters from
  them with `physical_damage_since` (the damage the A1 gate checks), and
  only the rects that can have changed are read back into a copy of the
  previous frame's pixels. With viewport controls, the whole frame is read
  back. `render_png` and the diagnostic renders are unchanged.
- **Per-document compositor** (`divui`). `Document::render_to_bitmap` uses
  one compositor kept in the document instead of a new one per call. It is
  created on first use and dropped with the document. `render_to_png_buffer`
  still uses a fresh one.
- **Image fingerprints** (`div-paint/src/damage.rs`, A1's file). A
  composited scrollbar's track is rastered into a new image in every
  recording, and A1 fingerprinted images by id, so every scrollbar was
  damaged in every frame. Raster images up to 1 MiB are now fingerprinted by
  their pixels and image info (size, color type, alpha type and color
  space); others keep their id. The same pixels drawn the same way give the
  same output, so the damage stays sound; the gate below checks it.

Ganesh GL keeps the full raster: `GaneshGlCompositor` is unchanged. Present
(A3) still copies the whole frame.

## Public API

No `divui` signature changes. New items, all `#[doc(hidden)]` because they
cross crates only:

| Item | Crate | Used by |
| --- | --- | --- |
| `RetainedRaster` (`new`, `update`, `surface`) | `div-paint` | `div-compositor` |
| `RasterUpdate { tiles, changed }` | `div-paint` | `div-compositor` |
| `PartialRasterStats { frames, tiles }` | `div-compositor` | tests |
| `SoftwareCompositor::partial_raster_stats` | `div-compositor` | tests |

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Damage and partial-raster gate, Linux, 1x and 2x | `f158ff8` on `main` `aa61757` | Pass, 100,898 steps per scale, 0 with uncovered pixels, 0 with a partial frame unequal to the full raster, 0 fixture errors, 2 known aborts |
| The same gate, Windows, 1x and 2x | `f158ff8` | Pass, the same counts, 2 known aborts |
| Census, all four profiles | `aa61757` to `f158ff8` | 16,297 of 23,728 exact before and after, every status, mismatch and Div image hash identical |
| CSS Grid coverage, all four profiles | `aa61757` to `f158ff8` | 1,473 of 4,204 exact before and after, 156 errors before and after, 0 tests changed, identical id lists |
| Linux `div-paint`, `div-compositor`, `div-engine`, `divui`, `div-text`, `div-svg` | `f158ff8` | 3,435 passed, 0 failed, 10 ignored (190 targets); `pixel-compare` checks |
| Linux `divui` `retained_raster_teardown` (new) | `66c213f` | Pass |
| Linux `div-compositor`, `divui`, `div-paint`, and `div-engine`'s `partial_raster`, `paint_damage` and `match_media` | rebased head on `main` `7796adf` | 1,686 passed, 0 failed, 8 ignored (94 targets); `pixel-compare` checks |
| Skia flag check, platform-scoped test ledger, v0.2 and renderer contracts, format of changed files | final head | Pass |

**The gate.** `pixel_compare damage-coverage` now keeps a
`SoftwareCompositor` for each fixture and renders every step through it as
well as in full. A step fails if a pixel outside the damage differs between
the two full rasters, as before, or if the retained compositor's frame
(size, stride, generation and every byte) differs from the full raster. The
first frame of each fixture is compared the same way. A1's comparison and
thresholds are unchanged and there is no tolerance. The fixture crate was
built unoptimized
(`--config profile.release.package.pixel-compare.opt-level=0`) with the
engine crates in release, as for A1. With the image fingerprints, damage is
the same as A1's or smaller: at 1x the damaged share of the viewport falls
from 4.9% to 4.6% for `color`, 7.9% to 7.8% for `font-size` and 12.3% to
12.2% for animations, and the other mutations are unchanged. The changed
areas are A1's. These are the Linux figures; Windows differs from them by at
most 0.1 point.

**Known aborts.** The two `grid-definition_grid-support-named-grid-lines`
references (`-002-ref`, `-003-ref`) abort the fixture process at both scales
on both platforms, as in A1: exit -6 on Linux and `0xC0000409` on Windows,
each counted only with the `memory allocation of N bytes failed` message.
They come from the engine-owned overflow at
`crates/div-layout/src/grid.rs:350` that A1 recorded.

**Focused tests** (`div-engine/tests/partial_raster.rs`, new). A page of
twelve boxes, some translucent and some at fractional offsets, with a
nested scroller and optionally a scrolling root, goes through nine changes:
background, opacity, margin, append, nested scroll, root scroll, display
none, width and removal. After every change the retained frame must equal
a fresh compositor's frame byte for byte, at 1x, 2x and 1.25 with a
1023x601 viewport, with and without viewport controls. The tests also check
that a small page rasters in full, that skipped scenes still give full
raster pixels, and that a scene with no damage rasters no tile. At 1x a
background change rasters at most two tiles.

**Per-document compositor.** `divui/tests/retained_raster_teardown.rs`
(new) renders three frames through `render_to_bitmap`, so the second and
third are partial, then drops the document and checks that a weak element
handle no longer upgrades, which needs the document's inner state to be
freed. `native_raster_options` (existing) checks the same after one
`render_to_bitmap`. Every `divui` teardown test passes. The sample apps'
leak probes are not in this repository and were not run.

**Skia flags.** Both gates and both census sides used `main`'s flags,
including #152's four raster defines: Linux from the `ci.yml` Linux job
(read back from the build's `args.gn`, with `-O2`), Windows from the
`windows-msvc` job (with `/clang:-ffp-contract=off`). The Windows gate used
MSVC toolset 14.52.37025 from Visual Studio 18 Build Tools, not CI's 14.44,
which is no longer installed. Skia's ninja ran with `-j4` through
`SKIA_NINJA_COMMAND`.

**Census pair.** Both sides were built and measured in this run, under the
census lock, with no `--profile` (the four default profiles; the non-1x ones
use the forced device scale, so a separate real-zoom run would repeat them),
16 jobs, Chromium 147.0.7727.50 and the shared oracle cache. Census 23,728
comparisons: 0 with a changed status, mismatch or Div image hash.

**Evidence.** The receipts are in the data repository at `fc65f020`, under
`docs/progress/evidence/partial-raster-a2-*`: the census and coverage
deltas, a sources file with both sides' binary, build-receipt and summary
hashes, Skia flags, Chromium build and coverage id list, the gate summary
and its Linux and Windows logs, and the frame-cost and P7 logs with their
conditions and binary hashes. The full census summaries and build receipts
are not there; they hold local paths. The data commit is archive-only and
follows `26fec8c7`, `main`'s pin, so `fidelity-data.lock` was not moved.

## Cost

All measurements are WSL on a 24-thread machine (i9-12900KF), release
builds, under the census lock, with `main` `aa61757` and A2 `f158ff8` built
in separate target directories and checked to differ (only A2's binaries
have `RetainedRaster` symbols). The probes were copied unchanged from A1's
cost run. Load average 2.4 to 4.0 throughout.

**`measure_frame_cost`** (sample-motion `9b37723`, 1280x800 at 1x, 120 frames
after 10 warm-up frames, three alternating rounds). The probe renders with
`Document::render_to_bitmap`, so A2's frames after the first are partial.
Median raster time (scene build, raster and readback), main to A2, with
every round of A2 below every round of main unless noted:

| Property | 1 element | 10 | 100 | 1,000 |
| --- | ---: | ---: | ---: | ---: |
| transform | 4.08 to 0.21 | 1.41 to 0.29 | 1.69 to 0.70 | 3.35 to 2.89 |
| opacity | 1.43 to 0.22 | 1.50 to 0.29 | 1.53 to 0.62 | 2.59 to 2.45 (ranges overlap) |
| background-color | 1.45 to 0.21 | 1.50 to 0.28 | 1.69 to 0.56 | 2.52 to 2.34 (ranges overlap) |
| width | 1.43 to 0.17 | 1.44 to 0.25 | 1.60 to 0.57 | 2.55 to 2.32 |

Median total frame time for one animated element falls to 0.23 to 0.27 ms,
from 1.51 to 4.19 ms on main, depending on the property. At 1,000 elements,
sampling and update dominate and the total ranges overlap (A2 to main ratio
0.92 to 0.98). The idle 1280x800
frame (no change) rasters in 0.16 ms instead of 1.45 (median of round
medians). These are headless frames without a present; RFC 0006's
one-square target (under 1 ms, from 6.95 ms) is measured in a window and
also needs A3, so it is not claimed here.

**`start_cost_by_animation_count`** (same probe). Per-animation start cost
ranges overlap at 100 and 300 animations. At 1,000, A2's three rounds are
6.40 to 6.53 ms and main's are 5.95 to 8.14 ms, so no difference is shown.

**P7** (sample-canvas probe, 640x480 canvas in a 1024x768 viewport, 65
frames, five alternating rounds). Median Div render per frame 11.38 ms on
main and 10.57 ms on A2 (ranges 11.21 to 11.93 and 10.23 to 10.78, every
round of A2 below every round of main). The canvas changes every frame, so
its tiles are rastered again each time. The
unchanged-document render falls from 2.48 to 0.105 ms (ranges 2.41 to 2.62
and 0.101 to 0.126). Draw and push times are equal. P1 to P5 of the same
probe binary pass with identical output on both sides; P6 fails on both at
the probe's own pointer-capture call (`probes.rs:264`), which predates A2.

**Windows.** The Windows suites run in CI; the Windows damage gate is in
the verification table. No Windows timings were taken.
