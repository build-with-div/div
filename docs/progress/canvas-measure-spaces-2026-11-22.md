# `measureText` bounds of spaces, 22 November 2026

The engine lane found that `measureText("  ")` reported ink bounds of 0 where
Chromium 147.0.7727.50 reports the second space's origin (for example, -33.297
and 33.297). This was already on `main` and did not come from T8.

## Cause and fix

`div-canvas`'s `RectF::union` returned early when the other rectangle was
empty. `gfx::RectF::Union` first checks whether the rectangle itself is empty,
and if so takes the other one whole, even an empty one. Each space is its own
item with empty ink at its own origin, so Chromium's result keeps the last
space's position, with zero width.

The port now follows `Union` and `UnionEvenIfEmpty`, including the one-step
widening when float error leaves the result short of the old edge. `RectF`'s
size also follows `gfx::SizeF`, which makes any size at or below
`8 * FLT_EPSILON` zero.

## Remaining gap, in `div-text`

The new trace found a separate failure. `word-spacing` on U+00A0 is added by
Chromium but not by `div-text`, whose `apply_spacing` adds it to U+0020 only.
With `7.3px` on two no-break spaces at 33.3px, Div's width is 21.168 and
Linux Chromium's is 35.768. `div-text` belongs to the engine track, so this PR does not touch it. The gap
is still there after T8's HarfBuzz shaper landed on `main`. The observation
`text-measure-spaces#78` is a named known gap on both platforms, and the test
fails as soon as it starts to match, so the entry cannot go stale.

## Sources

Each file's licence header was checked alone before its body was read. No
file was quarantined. `ui/gfx/geometry/rect_f.cc` and
`ui/gfx/geometry/size_f.h` (Chromium BSD) were fetched under the standing
rule, from `chromium.googlesource.com` at the pinned tag, and logged in
`SOURCES.txt`. `glyph_bounds_accumulator.h`, `shape_result.cc` and
`plain_text_node.cc` were read to confirm the cause.

## Verification

The trace `text-measure-spaces` makes 96 `measureText` calls: 12 strings of
spaces, tabs, no-break spaces and letters, across three fonts, two alignments,
right-to-left, word spacing and letter spacing. The capture left every earlier
trace's entry unchanged on both platforms. Rebased on `main` at 59d4b202, after
the gradients, the corpus is 48 traces, 178 canvases and 477 observations.

| Platform | Canvases exact | Observations exact |
| --- | --- | --- |
| Linux, local (Intel) | 177 of 178 | 476 of 477 |
| Windows | from the `windows-msvc` CI job | from the same job |

The canvas left on Linux is the known gap `text-transform-composite.c`. The
observation left on each platform is the word-spacing gap above.

```bash
cargo test -p div-canvas
```
