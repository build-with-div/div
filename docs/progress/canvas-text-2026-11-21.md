# Canvas 2D text, 21 November 2026

RFC 0008, migration step 3: `div-canvas` gains canvas text. Each call is
compared exactly with Chromium 147.0.7727.50 by replaying call traces, on
Windows and on Linux, each against its own capture.

## What the crate does

- **Attributes:** `font`, `textAlign`, `textBaseline`, `direction`,
  `letterSpacing`, `wordSpacing`, `fontKerning`, `fontStretch`,
  `fontVariantCaps`, `textRendering` and `lang`, with Blink's parsing,
  realization order and serialization. `font` is parsed by div-style's
  `font` shorthand parser (`parse_literal(StyleProperty::Font, …)`) and
  serialized as Blink does: six significant digits, `italic` for an oblique
  style, identifiers bare and other families quoted.
- **Drawing:** `fillText` and `strokeText` with the optional `maxWidth`,
  following `DrawTextInternal`: finite checks, alignment and baseline
  offsets, the `maxWidth` horizontal scale, the bounds used for draw
  skipping and the stroke inflation. The text is shaped and rasterized by
  div-text and drawn as one text blob, as Blink draws a node.
- **Measuring:** `measureText` returns the twelve attributes Chromium 147
  exposes (`emHeightAscent` and `emHeightDescent` sit behind the
  `ExtendedTextMetrics` runtime flag, which is off).
- **Text nodes:** `PlainTextNode`: space normalization, bidi runs, word
  segmentation, per-word shaping, spacing quantized to `TextRunLayoutUnit`,
  ink bounds as `ShapeResult::ComputeInkBounds` computes them, and the
  negative-width outset.
- **Font metrics:** ascent and descent by `AscentDescentWithHacks`, the
  normalized OS/2 typo metrics, and `GetFontBaseline` for every baseline.
- **Shape caches:** the canvas host's caches (`src/shape_cache.rs`): one
  font per description (`UniqueFontSelector`, 50 fonts), and per font a
  `FrameShapeCache` of nodes and shaped words keyed by text and direction,
  with frame generations, `RemoveOldEntries` and the 32,768-entry limit.
  The reuse shows in results: a word keeps the spacing it was shaped with.
  `did_switch_frame` ends a frame; the element step (C1) calls it.
- **Surface:** the backing store gets the surface properties Blink gives a
  canvas with alpha (`ComputeSurfaceProps(false)`): unknown pixel geometry,
  so text is grayscale, with the platform's text contrast and gamma.

`trace` gains `fontFace` lines (a font file loaded as a web font, as
`FontFace` loads it), the text calls and every text attribute, and
`measureText` results as observations.

## Platform behaviour found by measurement

Each of these was found by a mismatch and fixed from Blink's or Skia's
source, then confirmed by the oracle.

1. **Subpixel ascent and descent apply to system fonts only.** Canvas
   asks for them (`SetSubpixelAscentDescent(true)`), but a web font's
   `SimpleFontData` is built with the default, false. So a `FontFace` font
   rounds its ascent and descent at every size, even at 1px and 2px.
2. **No Linux ascent borrow at scale 1.** The borrow reads the render
   style's subpixel positioning, which `gfx::GetFontRenderParams` turns on
   only above a device scale of 1, as div-text's page metrics already
   model. Skia still positions canvas glyphs at subpixel offsets, because
   Blink forces `SkFont::setSubpixel` unless hinting is full.
3. **Linux web fonts draw plain grayscale.** A canvas surface has no pixel
   geometry, so Skia turns an LCD request into grayscale built from an LCD
   mask (`kGenA8FromLCD`). Linux Chromium draws web fonts with the
   Fontations scaler, whose `onFilterRec` drops that flag. div-text draws
   web fonts through a custom typeface that keeps it, so canvas asks for
   grayscale directly for a web font on Linux. This also fixes glyph bounds.
4. **The shape cache is visible.** `" lead"` with `wordSpacing` measured
   10px wider in Chromium than `ShapeResultSpacing` alone gives, because the
   space was taken from the cache, shaped earlier inside `"a b c"`.
5. **Windows canvas text contrast and gamma** come from the surface
   properties, not from Skia's build defaults.
6. **Windows hinting.** Chromium asks for medium hinting on Windows
   (`font_render_params_win.cc`), which DirectWrite treats as slight.
   Div's Windows profile asks for slight, which div-text resolves per family
   and strike as a model of Linux Fontconfig; at 17px and 34px that turned
   hinting off. Canvas asks for medium.

## Sources

Every ported file was read from Chromium 147.0.7727.50 in
`out\chromium-ref`, fetched under the standing rule of 16 November 2026,
each file's licence header checked alone before its body was read. All are
BSD-style. Four LGPL headers (`font.cc`, `font_metrics.h`,
`simple_font_data.h` and `font_platform_data.cc`) were quarantined unread.

| Blink file | Licence | Ported to |
| --- | --- | --- |
| `modules/canvas/canvas2d/base_rendering_context_2d.cc` (text) | Chromium BSD | `context/text_methods.rs` |
| `modules/canvas/canvas2d/canvas_rendering_context_2d_state.cc` (text) | Chromium BSD | `text.rs` |
| `core/html/canvas/text_metrics.cc` | Chromium BSD | `text.rs` |
| `platform/fonts/plain_text_node.cc` | Chromium BSD | `text.rs` |
| `platform/fonts/shaping/shape_result_spacing.cc` | Chromium BSD | `text.rs` |
| `platform/fonts/skia/skia_text_metrics.cc` | Chromium BSD | `text.rs` |
| `platform/fonts/simple_font_data.cc`, `font_metrics.cc` | Apple BSD | `text.rs` |
| `platform/fonts/shaping/shape_result.cc`, `glyph_bounds_accumulator.h` | Google BSD | `text.rs` |
| `platform/fonts/plain_text_painter.cc` | Chromium BSD | `shape_cache.rs` |
| `platform/fonts/shaping/frame_shape_cache.{h,cc}` | Chromium BSD | `shape_cache.rs` |
| `core/html/canvas/unique_font_selector.cc`, `canvas_font_cache.cc` | Chromium BSD | `shape_cache.rs` |
| `platform/graphics/canvas_resource_provider.cc`, `skia/ext/legacy_display_globals.cc` | Chromium BSD | `context.rs` |

Read for facts only, nothing ported: `font_render_params_win.cc` (Windows
hinting), `skia/BUILD.gn` (gamma defines) and `text_metrics.idl`.

Fetches four to nine are logged in `out\chromium-ref\SOURCES.txt`; the
ninth (`frame_shape_cache.{h,cc}`) was made for this step's cache.

## Verification

Seven text traces join the corpus in
`tests/fidelity/fixtures/canvas-traces-v1/traces`. The fonts are DejaVu
from `crates/div-text/fonts`, loaded as web fonts in both browsers. The
corpus is now 34 traces, 68 canvases and 201 observations.

**Oracles.** Captured with `tools/canvas/capture_canvas_traces.py` from
Chromium for Testing 147.0.7727.50, `--headless --disable-gpu`, on
21 November 2026:

- Windows 11, after the RFC 0012 text raster pin check;
- Linux in WSL 2, with the page census's FreeType feature flag and
  real-font Fontconfig profile.

The core traces' captures are unchanged. For the text traces every
attribute read is the same on both platforms; the text pixels differ on
five of seven traces and some `measureText` bounds by one pixel.

**Commands.**

```bash
# Linux (WSL), CI's Skia flags, 4 jobs
cargo test -p div-canvas
# Windows, prebuilt Skia, 4 jobs
cargo test -p div-canvas
```

**Result on this branch, on Windows** (run under the census lock):

```text
34 traces, 68 canvases, 201 observations checked against Chromium (windows)
test result: ok. 1 passed
```

All 68 canvases are exact byte for byte and all 201 observations are
exact. `KNOWN_GAPS` has no Windows entry.

**Result on this branch, on Linux** (WSL 2, CI's Skia flags):

```text
34 traces, 68 canvases, 201 observations checked against Chromium (linux)
text-transform-composite.c: 681 of 35200 pixels differ (max channel delta 255): known gap: ...
test result: ok. 1 passed
```

All 201 observations and 67 of 68 canvases are exact. The one canvas left
is the first known gap below.

The first Linux run against the new captures had 40 mismatches and the
first Windows run 10; the fixes listed under "Platform behaviour found by
measurement" removed them, one cause at a time.

## Known gaps

1. **Transformed web-font text on Linux** (`text-transform-composite.c`,
   in `KNOWN_GAPS`): 681 of 35,200 pixels differ in the scaled (2x1.5),
   skewed and rotated text; the scaled text's first ink row moves, so
   hinting differs, and rotated coverage differs by up to 58. The cause is
   not yet isolated. Owner: engine track, div-text's web-font outlines under
   a transform (`shaping/web_font_outlines.rs`) and its hinting resolution
   (`font/platform.rs`, `resolve_hinting`), which reads the CSS size, not
   the device strike.
2. **T8 pending.** Advances and the `TextRunLayoutUnit` rounding follow
   div-text as it is now; canvas text is measured again once T8 merges.
3. **`font` parsing:** div-style's shorthand parser accepts pixel sizes
   only: no `font-stretch` keywords in the shorthand, no `bolder`/`lighter`
   and no system fonts. A trace line using `condensed` was removed for this
   reason. Owner: engine track, div-style.
4. **Not measured (no trace yet), ported as read:**
   - system fonts in canvas (subpixel ascent and descent, LCD-from-grayscale
     on Linux): every trace font is a web font;
   - `CanShapeWordByWord` is taken as true (`font.cc` is LGPL, unread);
   - CJK and emoji word splitting, and the `MaybeBidiRtl` and
     zero-width-space character classes, are approximated;
   - word spacing on a no-break space and letter spacing on a zero-width
     space;
   - OpenType `BASE` baselines are not read; VDMX is ignored;
   - spacing units beyond `px`, `em`, `rem` and the absolute units;
   - bidi text beyond the one RTL trace;
   - fonts evicted from `UniqueFontSelector` while the canvas state still
     holds them (Chromium keeps their cache; Div starts a new one).
5. **Tainting.** The text calls do not taint; the API is designed with H1.

## Files outside the crate

- `crates/div-text/src/font/source.rs` (engine track): `is_web_font` becomes
  public, so canvas can tell a `FontFace` font from a system font. No
  behaviour changes. For engine-lane review.
- `crates/div-style`: unchanged. Canvas parses `font` with the existing
  public `parse_literal(StyleProperty::Font, …)`.
- `Cargo.lock`: `div-canvas` now depends on `div-geometry`.
- `tools/canvas/capture_canvas_traces.py`: `fontFace` lines, `measureText`,
  awaiting font loads, and the per-platform text environment.
