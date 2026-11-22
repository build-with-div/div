# SVG number fractions and the chart/legend arcs (20 November 2026)

SVG lane, the arc follow-up to RFC 0009 V3d part 1. Paint corpus v6 listed
`chart/legend` as a named gap: "arc radius within an ulp of half the chord;
same on a direct path; cause unmeasured".

## What was measured

- **The arc path.** Skia's SVG `arcTo` (BSD, `src/core/SkPathBuilder.cpp`
  at abbe599f, fetched under an earlier approval) is the same in Chromium
  147's Skia and rust-skia's, so the arc code was not the difference.
  Two other constructions, both black-box experiments that were not kept,
  made things worse:
  - cubics of at most a quarter turn for every arc brought `chart/legend`
    to 16 and 8 differing pixels, but broke eleven arc captures that
    conics match exactly;
  - Skia's oval `arcTo` was worse still.
- **The radius.** Div read the legend's radius `5.33` as 5.3300004, one
  ulp above the float nearest 5.33. Div's port of Skia's `arcTo` gives the
  same thing from both values except at this point, where the radius is
  within an ulp of half the chord. There the centre lands on the chord
  from 5.3300004, and 0.0026 px off it from 5.33.
- **The parse.** Div accumulated each fraction digit into the value
  (`value += digit × scale`). Summing the fraction digits on their own and
  then adding them to the integer part reads `5.33` as the nearest float
  and still reads `1.9e2` as 190.00001525878906 (`path/exponents`).
- **A direct check.** A new geometry case, `path/fractions`
  (`M1.354 1.771 L5.33 5.33 L1.834 10.67`), uses numbers that the old
  order, the new order and correct rounding each read differently.
  Chromium 147 reports the bounding box x and y as 1.3540000915527344 and
  1.7709999084472656, and the last point's x as 1.8340001106262207, on
  Windows and on Linux. These are the new order's values. The old order
  gives 1.3539998531341553, 1.7710001468658447 and 1.8339998722076416;
  correct rounding gives 1.3539999723434448, 1.7710000276565552 and
  1.8339999914169312.

## What changed

- `crates/div-svg/src/number.rs`: the fraction is accumulated on its own
  and added to the integer part, with a unit test of the measured values.
- `tools/svg/geometry_corpus.py`: the `path/fractions` case (126 cases).
  It was recaptured on Windows and Linux; every other case is identical
  apart from its slot position (`svg_rect`, `screen_ctm`), which the test
  does not compare.
- `svg-geometry-v1/known-gaps.txt`: `path/fractions points` is listed
  under cause 1, length along `path` data, like the other `path/` cases.
  Its bounding box, total length and hit grids are exact.
- `svg-paint-v6/known-gaps.txt`: the `chart/legend` gap is removed; both
  scales are exact.
- `tools/svg/README.md`: the counts, the number rule and the v6 result.

## Verification

`cargo test --locked --no-fail-fast --package div-svg` on main 2c25496,
on Windows (MSVC) and on Linux (WSL, `CARGO_INCREMENTAL=0`, `-j 4`):

- paint v6: 122 of 122 exact (was 120);
- paint v1 147/147, v2 115/115, v3 106/108, v5 116/116, v7 167/172, and
  v4 203/206 on Windows and 204/206 on Linux: all unchanged;
- geometry: 76 of 126 exact, 81 listed fields (the new case adds one);
- text: positions and pixels as main lists them (positions: 0
  case-scales differ on either OS; pixels: Windows 0 of 290, all listed;
  Linux 264 of 290);
- 40 unit tests pass, one of them new.

`cargo fmt --all --check` and
`python tools/release/generate_v02_contract.py --check` pass.

## Sources

Nothing new was fetched. The Skia files came from the earlier approved
fetch (`out\chromium-ref`, `SOURCES-svg-skia-arc-2026-11-19.txt`). No Blink file was opened: the
parse rule comes from the corpus captures alone.
