# SVG geometry against Chromium 147 (15 November 2026)

RFC 0009 V1 stage 1, first PR: the `div-svg` geometry crate and the
black-box corpus it is checked against. No element, style, layout or
paint integration yet; those wait on integration requests E1 to E3 and
F1 (namespaces, SVG properties with a presentation-hint level, replaced
sizing of the outer `svg`, `view!`).

**What landed.**

- `crates/div-svg`: path data, basic shapes, lengths, transform lists,
  `viewBox` and `preserveAspectRatio`, and the Skia-backed queries
  (length, point at length, bounding box, fill and stroke containment).
  Written from SVG 2 and black-box observation; no Blink code read or
  ported.
- `tools/svg/geometry_corpus.py`: 125 cases, queried in Chromium through
  web APIs only.
- `tests/fidelity/fixtures/svg-geometry-v1/chromium-linux.json` and
  `chromium-windows.json`: captures from Linux CfT 147.0.7727.50 (WSL,
  glibc 2.43) and Windows CfT 147.0.7727.50. They are identical apart
  from their timestamp and platform, and each OS is tested against its
  own.
- `crates/div-svg/src/arc.rs`: a port of Skia's SVG `arcTo` (BSD), with
  trig computed in double precision and rounded to single, so that arcs
  do not depend on the C library's single-precision trig. The first CI run
  (glibc on the runner) differed from Chromium on `arc/rotation--60`'s
  box by a few ulps when calling Skia's `arcTo` directly.
- `known-gaps.txt` next to it, with the cause and owner of each
  remaining difference.

**Verification.**

```sh
cargo test --locked --package div-svg
```

Windows MSVC and Linux (WSL, glibc 2.43), CI Skia flags, 16 November:
10 unit tests and the corpus test pass on both. 76 of 125 cases are exact in every field. In the other 49,
the 80 differing fields are each listed with their cause in
`known-gaps.txt`. Every bounding box and every fill and stroke hit grid
is exact in all 125 cases. CI adds `div-svg` to the Linux and Windows
test lists.

**Remaining differences (owners).**

| Gap | Fields | Owner |
| --- | --- | --- |
| Length along `path` data: Chromium traverses `path` data separately from the drawn path | 73 | div-svg |
| `viewBox` translation under `slice`, one double ulp | 6 | div-svg |
| Size of an outer `svg` with no width or height | 1 | engine (E3) |

`tools/svg/README.md` has the evidence for each, and the choices the
corpus settled: conic arcs, single-precision number accumulation, and
unparsable length attributes used as 0. That last one is presentation
attribute parsing, which goes to the engine with E2.

**Not claimed.** Any rendering result: nothing paints SVG yet. The
Lucide gate (640 captures) comes with the painter.
