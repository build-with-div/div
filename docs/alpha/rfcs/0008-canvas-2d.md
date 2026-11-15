# RFC 0008: Canvas 2D in a `div-canvas` crate

Status: proposed, 15 November 2026. Owner: the Canvas 2D lane, working in
engine-owned code (platform decision 8: the engine owns Canvas 2D in a new
`div-canvas` crate). Consumers: sample-canvas (Konva, Fabric, rough.js,
PixiJS, Excalidraw), sample-charts (Chart.js, ECharts), sample-motion
(lottie's canvas renderer) and sample-editor (Monaco's minimap). Requests
to: the engine track (the replaced canvas in `div-dom`, `div-layout`,
`div-paint` and `div-engine`), the framework track (the public `divui`
types), the components track (RFC 0005's canvas YAML generator) and the
inline SVG lane (one SVG path-data parser). Covers plan rows C1–C3 (items
1.6 and 2.1) and the canvas part of R4 (item 2.2).

## Problem

Four samples port packages that draw through `CanvasRenderingContext2D`, and
Div has none of it. Measured at `main` `9168ac9`:

- A `<canvas>` with no CSS size lays out 300×0, where Chromium gives 300×150
  (sample-canvas probe P1). `ElementTag::Canvas` is laid out as replaced
  by intrinsic sizing but not by block layout, which takes the replaced
  path only when `replaced` is set (`crates/div-layout/src/block.rs:1653`),
  and nothing sets it for a canvas.
- Nothing draws into a canvas. The only canvas paint is a stub that fills
  the content box with one colour (`RendererNodeState::EmbeddedCanvasColor`,
  `paint_embedded_canvas` in `crates/div-paint/src/painter.rs`).
- The workaround the samples use today, pushing app-rastered pixels as an
  `image/x-openui-rgba8` resource, re-records and re-rasters the whole
  document per frame (P7: 17.7–29.2 ms per frame for a 640×480 canvas), and
  reusing a resource key shows stale pixels (P3), because the decode cache
  is keyed by `sha256` and never evicted.

Plain Skia calls do not reproduce Chromium's canvas: argument validation,
arc and ellipse construction, the shadow filters, the composite modes that
draw through a layer, text placement and readback rounding all come from
Blink. So this is a port of Blink, not a new design.

## Proposal

### 1. Crate boundary

`crates/div-canvas` is a new engine crate. It depends on `div-geometry`,
`div-style` (colour and font parsing), `div-text` (fonts, shaping, text
blobs), `div-svg` (RFC 0009's path-data parser, for `Path2D` strings only)
and `skia-safe`, and on nothing that knows about documents.

| Layer | Owns |
| --- | --- |
| `div-canvas` | A standalone 2D context: the state stack, `CanvasPath` and `Path2d`, styles, gradients, patterns, drawing, text, shadows, readback, `ImageData`, `TextMetrics`. Its backing store, and immutable `CanvasSnapshot`s of it. Works without a document, so it is unit-testable against Chromium bitmaps alone |
| `div-dom` | A `ReplacedResourceKind::Canvas(CanvasSlotId)` variant. `TransparentCanvas` stays for form controls; the `EmbeddedCanvasColor` stub is removed |
| `div-layout` | Nothing new: the canvas becomes an ordinary replaced element |
| `div-paint` | Paints a canvas slot at the content box as Blink's `HTMLCanvasPainter` does (it depends on `div-canvas` only for `CanvasSnapshot`) |
| `div-engine` | The canvas table: per element, the bitmap size, the context kind, the context and its slot. Resolves `<img>` and canvas image sources to decoded images, and the canvas's computed font, for `div-canvas` |
| `divui` (framework) | The public `HtmlCanvasElement`, `CanvasRenderingContext2d` and value types |

`div-canvas` never reads the DOM: `drawImage` receives an already decoded,
immutable image, and `font` relative units receive the element's computed
font. That keeps it below `div-paint`, with no cycle, and lets OffscreenCanvas
(C6) reuse it later.

### 2. The element (C1)

Ported from `core/html/canvas/html_canvas_element.cc`,
`core/layout/layout_html_canvas.cc` and `core/paint/html_canvas_painter.cc`.

- **Bitmap size.** `width` and `height` are parsed with HTML's rules for
  non-negative integers. A missing or invalid value gives 300 or 150.
  Setting either attribute, even to its current value, resets the bitmap
  to transparent black (opaque black for `alpha: false`) and the context to
  its initial state, as Blink's `Reset` does.
- **Layout.** Creating a canvas sets its `ReplacedContent` from the bitmap
  size, so it is 300×150 with no CSS. This fixes P1. A size change is an
  `Intrinsic` invalidation.
- **Paint.** The snapshot fills the content box under `object-fit` and
  `object-position`. Its sampling follows `image-rendering` as
  `HTMLCanvasPainter` chooses it, with no stretching hacks.
- **Contexts.** One context per canvas. `get_context_2d` returns the same
  context every time, and returns `None` if the canvas already has a context
  of another kind (WebGPU, C5). Settings are `alpha`, `will_read_frequently`,
  `color_space` (`srgb` first; `display-p3` with C6) and `desynchronized`,
  which is accepted and has no effect, since Div's canvas is already
  synchronous and CPU-backed.
- **Detached canvases** (W10) work fully: the bitmap exists without layout,
  and `drawImage` from one canvas to another reads its snapshot.
- **Fallback content** (W16): children are not rendered, but stay in the
  accessibility tree and keep their focusability. That follows Blink's
  layout and accessibility rules, ported with the element.

### 3. The public API (framework)

The Rust API is the WebIDL in snake case, as sample-canvas's
`docs/rfc-canvas.md` §1–2 proposed. This RFC adopts that draft's names, with
the changes below. Typed conveniences may sit on top, never in place of the
web members.

```rust
let canvas = HtmlCanvasElement::create(&doc)?;   // or from_element(el)
canvas.set_width(640)?;
let ctx = canvas
    .get_context_2d(CanvasRenderingContext2dSettings::default())?
    .expect("a fresh canvas has no other context");
ctx.set_fill_style("rgb(0 128 255 / 0.5)");       // ignored if it does not parse
ctx.begin_path();
ctx.arc(80.0, 80.0, 40.0, 0.0, TAU, false)?;    // IndexSizeError if radius < 0
ctx.fill(CanvasFillRule::Nonzero);
let metrics = ctx.measure_text("Total");
let pixels = ctx.get_image_data(0, 0, 10, 10)?;
```

- A `CanvasRenderingContext2d` handle is `!Send` and bound to its canvas.
  Calls go straight to the context, which the engine's canvas table shares
  with the handle, so drawing does not re-enter the document's lifecycle.
- `DomException` appears only where WebIDL throws. A non-finite argument to
  an `unrestricted double` makes the call return without effect, and an
  unparsable style or font string is ignored, as in the browser.
- `Element` stays untyped. `HtmlCanvasElement::from_element` returns `None`
  for other tags, so Div adds no typed element hierarchy.
- `to_data_url` and `to_blob` (C4) are framework work on top of
  `CanvasSnapshot`. Whether `to_blob` is asynchronous, as on the web, is
  decided in that PR.
- `node_ref` in `view!` (decision 12) is framework work outside this RFC.

### 4. Blink sources and how they are ported

The port follows Chromium 147.0.7727.50, the version every Div oracle uses.
The local Chromium tree (`out/chromium-src`) is a partial checkout at
`09d377d9`, which is 147.0.7727.24, without the canvas file contents. Before
the first port, the file list below is re-taken at the 147.0.7727.50 tag.
The WPT tests are a separate pin (§7).

| Blink source (Chromium 147) | Div module | Content |
| --- | --- | --- |
| `modules/canvas/canvas2d/canvas_2d_recorder_context.{h,cc}` | `context.rs`, `draw.rs` | The drawing API, argument validation, and the `Draw`/`CompositedDraw` paths |
| `modules/canvas/canvas2d/base_rendering_context_2d.{h,cc}` | `readback.rs` | `getImageData`/`putImageData`, `createImageData` |
| `modules/canvas/canvas2d/canvas_rendering_context_2d.{h,cc}` | `text.rs`, `context.rs` | Text and font, `drawFocusIfNeeded`, the element-bound context |
| `modules/canvas/canvas2d/canvas_rendering_context_2d_state.{h,cc}`, `clip_list.{h,cc}` | `state.rs` | The state stack, paint flags, shadow filters and the clip list |
| `modules/canvas/canvas2d/canvas_path.{h,cc}`, `path_2d.{h,cc}`; `platform/geometry/path.{h,cc}`, `path_builder.{h,cc}`, `stroke_data.{h,cc}` | `path.rs` | Path building (arc, `arcTo`, ellipse, `roundRect`), `Path2d`, point and stroke hit tests |
| `modules/canvas/canvas2d/canvas_style.{h,cc}`, `cached_color.h`, `canvas_gradient.{h,cc}`, `canvas_pattern.{h,cc}`; `platform/graphics/gradient.{h,cc}`, `pattern.{h,cc}`, `image_pattern.{h,cc}` | `style.rs` | Fill and stroke styles, serialization and shaders |
| `modules/canvas/canvas2d/canvas_image_source_util.{h,cc}` | `image.rs` | `drawImage` rectangle rules and sampling |
| `core/html/canvas/text_metrics.{h,cc}`, `canvas_font_cache.{h,cc}` | `text.rs` | `measureText` and font parsing and caching |
| `core/html/canvas/image_data.{h,cc}` | `image_data.rs` | `ImageData` |
| `core/html/canvas/html_canvas_element.{h,cc}`, `core/layout/layout_html_canvas.{h,cc}`, `core/paint/html_canvas_painter.{h,cc}` | `div-engine`, `div-paint` | The element (§2) |

Not ported:

- V8 bindings, garbage collection, UKM and identifiability metrics;
- hibernation, GPU and out-of-process raster, and resource dispatch;
- the experimental `drawMesh` and WebGPU-transfer files (`mesh_2d_*`,
  `canvas_2d_gpu_transfer*`).

Div ports the logic and the order of operations, not the plumbing. Each
ported routine keeps Blink's name in a comment. Each module names its source
files and the Chromium version.

**Licences.** Div stays permissive, by the user's binding rule.

- Before a file is ported, its header is read at the pinned tag. Only files
  under Chromium's BSD-style licence, or another permissive licence, are
  ported.
- The two canvas files read so far are both permissive.
  `html_canvas_painter.cc` is Chromium BSD. `layout_html_canvas.cc` is
  Apple's two-clause BSD.
- Ported code keeps the original copyright notices. The crate ships the
  Chromium licence text, and the Apple notice where a file carries one.
- A file under LGPL or GPL is not read past its header. The behaviour is
  written from the specification, and any difference measured against
  Chromium is recorded as a fidelity gap.
- **Porting stops at the boundary.** Porting stops at any call into a file
  that has not been cleared as permissive. That callee is either cleared
  first, or written from the specification, or replaced by Div's own code.
  For example, canvas font resolution leads into Blink's
  `core/css/resolver/font_builder.cc`, which is LGPL. So the canvas `font`
  is resolved by `div-style`'s font shorthand and `div-text`, never by a
  port of that file.

**SVG path data is LGPL in Blink.** `core/svg/svg_path_parser.cc` is LGPL,
as RFC 0009's licence table and a header scan both confirm. So
`new Path2D(d)` waits for RFC 0009's migration step 2, which lands
`div-svg` with its clean-room path-data parser written from the SVG
specification. `Path2D` uses that parser and no other. Only the parser is
shared. Canvas `arc()` and `ellipse()` keep their own path code (§5),
because the canvas arc rules are not SVG's elliptical-arc conversion.

**Maths.** The goal is to match Chromium's result on each OS, using
permissive code only. Arcs call `sin` and `cos` from C++. On Linux,
Chromium takes them from glibc's libm, which is copyleft and is never read
or ported. Where Chromium's result comes from copyleft libm code, Div uses
its own or a permissive implementation. It measures the result against
Chromium on that OS and records any mismatch as a gap.

### 5. Backing store, text, images, readback, shadows, `Path2D`, compositing

- **Backing store.** A CPU raster surface sized to the bitmap, N32 and
  premultiplied, like Chromium's unaccelerated canvas resource provider.
  That is the path Chromium takes with `--disable-gpu`, which the oracles
  use. Draws go straight to the surface, so readback sees every draw so far.
  `CanvasSnapshot` wraps `image_snapshot()`, which is copy-on-write and
  immutable. It can cross threads, so `SceneSnapshot` keeps its no-callback
  contract. Chromium's size and area limits are ported with the element.
  The surface's text properties (greyscale or LCD) are set as Chromium's
  provider sets them. The first text PR measures both `alpha: true` and
  `alpha: false` against the oracle.
- **Text** (after S12). `font` is parsed with `div-style`'s font shorthand.
  Relative units resolve against the canvas element's computed font (10px
  sans-serif when detached), and the getter serializes as Blink does.
  Shaping uses `div-text`'s `TextShaper`, with the context's `direction`,
  `letterSpacing`, `wordSpacing`, `fontKerning`, `fontStretch`,
  `fontVariantCaps` and `textRendering`. Alignment and baselines come from
  the font metrics as `TextMetrics` computes them, and `maxWidth` scales
  horizontally. Glyphs are drawn as a text blob with the context's paint,
  so fill and stroke styles, shadows, alpha, composite mode and clip all
  apply. `measureText` returns all twelve metrics. `TextCluster` and the
  newer `TextMetrics` methods come later. Web fonts (`FontFace`, W15) and
  WPT's `CanvasTest.ttf` reach the context through the document's font
  collection.
- **Images.** `drawImage` takes an `<img>` (decoded by `div-paint`'s image
  resources, with Chromium's decoding rules), another canvas (its snapshot;
  a canvas drawn onto itself is snapshotted first), and later an
  `ImageBitmap` (C6). Blink's source-rectangle clipping and its mapping of
  `imageSmoothingEnabled`/`imageSmoothingQuality` to sampling options are
  ported. Drawing an SVG image (`image/svg+xml`) depends on RFC 0009's
  static-image painter: `drawImage` rasters it through that painter at the
  destination size, as Chromium does, and is not supported for SVG sources
  before it lands.
- **Origin-clean (tainting).** The API is designed now and implemented when
  H1 (`<img src>` from URLs) lands.
  - Each canvas has an origin-clean flag, set to true on creation and on
    reset. `CanvasImageSource` reports whether a source is origin-clean.
  - Drawing a source that is not origin-clean (`drawImage`, a pattern)
    clears the flag, under HTML's rules.
  - Readback of a canvas that is not origin-clean fails with `SecurityError`:
    `get_image_data` returns `Err(DomException)`, and so do C4's
    `to_data_url` and `to_blob`.
  - The signatures already return `Result`, so the API does not change when
    H1 lands.
  - Until then every source is a local resource, so it is origin-clean and
    the flag never clears. This is recorded as a gap, not stubbed.
- **Readback.** `getImageData` converts the premultiplied surface to
  unpremultiplied RGBA8 with the same Skia conversion Chromium uses, so the
  lossy alpha round trip matches. `putImageData` ignores the transform,
  alpha, composite mode, shadow and clip, and honours the dirty rectangle.
  `will_read_frequently` changes nothing, because the store is already on
  the CPU. Float16 `ImageData` comes with C6.
- **Shadows.** Drawn when the shadow colour is not transparent and the blur
  or an offset is non-zero. The state builds Blink's shadow-only and
  shadow-and-foreground image filters, converting blur to sigma Blink's way.
  The offset and blur ignore the current transform.
- **`Path2D` and hit tests.** `Path2d::new`, from another path, from SVG
  path data (`div-svg`'s parser, after RFC 0009 step 2; §4), and `add_path`
  with a matrix. `isPointInPath` and
  `isPointInStroke` map the point through the inverse transform and use
  Blink's `Path::Contains` and `StrokeContains`, with the current line
  state.
- **Compositing order (C3).** Each operation follows the spec's drawing
  model in Blink's order: the shape is drawn, then its shadow (when one is
  drawn), then `globalAlpha` is applied, then the result is composited
  within the clip. Most operations draw directly, with paint flags that
  carry the shadow filter and the blend mode. The modes that change pixels
  outside the shape (`copy`, `source-in`, `source-out`, `destination-in`,
  `destination-atop`) go through `CompositedDraw`: the shadow and the
  foreground are drawn into a layer over the clip, then composited with the
  mode. Div ports which modes take which path. Blink's overdraw
  optimizations change no pixels and are left out.

### 6. Presentation, before and after RFC 0006

- **Now (before RFC 0006 A2).** The canvas paints as an ordinary replaced
  element. The painter draws the current `CanvasSnapshot` directly, never
  through the `sha256` image cache, so P3 cannot occur. A draw marks the
  document paint-dirty, so the frame re-records and re-rasters in full:
  correct pixels at today's cost. The canvas shows its bitmap as of the
  frame's paint step. Under RFC 0007's `begin_frame` order, drawing in a
  `requestAnimationFrame` callback appears in that same frame. This settles
  the "end of task or frame" question in sample-canvas's draft: Div has no
  task loop, and the frame is the commit point.
- **With canvas slots (RFC 0006 migration step 6, after A2).**
  - Paint records a slot chunk at the content box. The chunk's identity is
    the canvas node and its phase. Its recording refers to the slot, not to
    the pixels, so new content does not change the chunk.
  - `SceneSnapshot` carries a slot table: slot, snapshot and version.
  - The compositor binds each slot's current snapshot at raster time. A
    content change damages only the slot's device rectangle: no relayout,
    no re-record, and only the tiles under it are re-rastered.
  - Stacked canvases (ECharts' hover layer) are slots in paint order.
  - Gates: RFC 0006's partial-equals-full check, with canvas draws added to
    its mutation corpus; and P7 re-measured on an idle host. No speed is
    claimed before it is measured.
- **With layers (RFC 0006 C1).**
  - Whether a 2D canvas is a foreign layer is copied from Chromium.
    Chromium's own layer tree (`LayersAsJSON`) for a software 2D canvas
    under `--disable-gpu` decides it, since Div must layerize as Chromium
    does. RFC 0006 C1 currently lists every canvas as a foreign layer and
    needs a one-line amendment to match. That amendment goes to the engine
    track.
  - A WebGPU canvas (C5) is always a foreign layer: the same element and
    slot mechanism, holding a GPU texture instead of a CPU image.
    `div-canvas` does not depend on wgpu.
  - Until C1, a translucent WebGPU canvas over Div content keeps its current
    limitation. WebGPU is outside this lane. Its oracle (SwiftShader under
    Chromium 147, verified by sample-canvas) needs GPU flags, while 2D
    canvas uses `--disable-gpu`. The two oracles are never mixed.

### 7. Fidelity plan

The target is exact RGBA everywhere below. Every result that is not exact
is recorded with its cause and its owning subsystem, and no exactness gate
is weakened. The required profiles are defined as follows:

- **Bitmap results** (what a context draws, and every read) do not depend
  on viewport or device scale, because the bitmap size comes from `width`
  and `height`. They are captured once per OS, on Linux and on Windows, and
  must be exact on both.
- **Page results** (sizing, `object-fit`, scaling, stacking, slots) use the
  renderer contract's four required profiles: 800×600@1, 1280×720@1.25,
  1920×1080@1.5 and 375×667@2. Like the contract, they are captured on
  Linux with the `chromium_linux_lcd` raster configuration.

1. **Oracles.** Chromium 147.0.7727.50 for Testing, `--headless
   --disable-gpu`, using the existing capture paths:
   `run_all_pixel_comparisons.py`'s `render_chrome` and
   `windows_chrome.py`. Two captures:
   - the **bitmap**, read with `getImageData` over the whole canvas, which
     is exact and independent of page compositing;
   - the **page screenshot**, for sizing, `object-fit`, scaling and stacking.
2. **WPT, generated** (decision 8; RFC 0005 migration step 5). The 1,157
   YAML tests come from RFC 0005's pinned WPT revision, Chromium
   `09d377d9`, so the project has one WPT pin. A later bump moves every
   suite together, and the YAML is never taken from the .50 tree. Each
   generated test has two independent results:
   - WPT's own assertions, with WPT's own tolerances;
   - an exact comparison of the whole bitmap with Chromium's capture of
     the same test.

   The ledger reports both results, per YAML file and per bucket. The
   sample census buckets (M1 607, M2 302) are regex estimates, so the
   ledger replaces them.
3. **Call-trace replay** (decision 8).
   - **Format.** One trace format: JSON lines of `{"call": name, "args":
     [...]}`, `{"set": prop, "value": v}` and `{"get": prop}`, each with
     the canvas's id. NaN, the infinities and −0 are encoded as strings.
   - **Conversion.** A converter takes the existing Konva (`'=prop'`) and
     rough.js/board (`'prop='`, `'get prop'`) traces.
   - **Comparison.** A trace replays against Div's context and against
     Chromium's. Final bitmaps must match exactly. Every read must match
     bit for bit: getters, `measureText`, `getImageData`, `isPointIn*`.
   - **Why it is clean.** Traces record argument values, so differences in
     JavaScript maths in the packages never reach the comparison.
   - **Sources.** Konva, rough.js and the board sample traces exist
     already. Some of them came from Node oracles rather than Chromium.
     Each one is re-recorded with Chromium 147.0.7727.50 and
     `--disable-gpu` before it becomes a gate. Chart.js and ECharts are
     recorded next, the same way.
   - **Storage.** Small traces and their oracles go in
     `tests/fidelity/fixtures/canvas-traces-v1/`; bulk captures go in
     `div-fidelity-data`.
4. **Standalone tests.** Because `div-canvas` needs no document, each
   implementation PR brings Rust tests that draw with the crate directly and
   compare the bitmap with a Chromium capture of the same calls. The tests
   use the format from item 3, so they become the first traces.
5. **The element gate (C1).** The element PR must pass, exactly at the four
   page profiles, the plan's WPT `canvas-host` tests and the sizing and
   rendering reftests at the same pin:
   - `html/canvas/element/canvas-host/` (81 files);
   - `html/rendering/replaced-elements/attributes-for-embedded-content-and-images/canvas-aspect-ratio.html`
     and `canvas-dimension-attributes.html`;
   - `html/rendering/replaced-elements/embedded-content-rendering-rules/`:
     `canvas-fallback`, `canvas-update-with-border-object-fit`,
     `canvas_scale` and `canvas_without_context_a`;
   - the `context.*` and `fallback.*` tests in
     `html/semantics/embedded-content/the-canvas-element/`, and its
     `canvas-descendants-focusability-*` tests for W16.

   Any of these that cannot be expressed through the public API is listed
   with its reason, never dropped silently.
6. **No regression.** Census fixtures that contain `<canvas>` are listed in
   the element PR. Their pixel changes are reported as moves toward
   Chromium (D4).

## Affected tracks

| Track | What changes |
| --- | --- |
| engine | Agrees the boundary in §1. The Canvas 2D lane opens its own PRs that touch `div-dom`, `div-engine` and `div-paint`: the `ReplacedResourceKind::Canvas` variant, the canvas table, canvas paint and slot chunks on A2. Each such PR names the engine-track files it touches. The coordinator orders them with the engine track, and the merge queue tests them together. RFC 0006 C1 gets the one-line foreign-layer amendment (§6). Glyph-origin snapping (S12, engine text) comes before canvas text |
| framework | Owns the `divui` surface in §3 (`HtmlCanvasElement`, `CanvasRenderingContext2d`, value types), C4 export, `devicePixelRatio` (W12), `FontFace` (W15), `node_ref` and WebGPU (C5) |
| components | RFC 0005's canvas YAML generator (its step 5) targets the §3 API, and reports per YAML file |
| inline SVG lane | `div-svg`'s clean-room path-data parser (RFC 0009 step 2), shared with `Path2D`; the static-image painter, which `drawImage` of an SVG image uses. Arc code is not shared |
| fidelity lane | The fonts the oracles use |
| samples | Replace the `image/x-openui-rgba8` workaround with the context, member by member, and record Chart.js and ECharts traces |

## Migration

Each step is one focused PR. `main` stays green between them, and each PR
reports its own measurements.

1. This RFC (docs only).
2. **`div-canvas` core**, standalone:
   - state and `save`/`restore`, transforms, `globalAlpha`, composite-mode
     names;
   - `CanvasPath` (paths, arcs, ellipses, `roundRect`);
   - rectangles, `fill`/`stroke`/`clip` with colour styles, line styles and
     dashes;
   - the trace format, with standalone Chromium-compared tests.
3. **The element:**
   - the canvas replaced kind, the size rules and the P1 fix;
   - the canvas table, and canvas paint before slots;
   - the `divui` types (framework review);
   - the census check for fixtures that contain `<canvas>`;
   - the element gate in §7 item 5.
4. Gradients and patterns.
5. The WPT YAML ledger, once RFC 0005's generator lands.
6. C3, in the plan's order:
   - text (after S12);
   - readback;
   - shadows;
   - `Path2D`, whose path-data constructor waits for RFC 0009 step 2;
   - `drawImage`, where SVG sources wait for RFC 0009's static-image
     painter.
7. Canvas slots on RFC 0006 A2 (its step 6).
8. C4 export (framework); later C6 (`OffscreenCanvas`, `ImageBitmap`,
   `filter`, layers, `reset`, colour spaces).

Until step 3 lands, the samples keep their labelled `div-gap` workaround.
