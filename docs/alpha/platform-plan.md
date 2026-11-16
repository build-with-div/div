# Div web platform plan, consolidated from six sample investigations

> Brought into the repository on 16 November 2026 from the parent session's
> notes, unchanged except for one local path. The decisions the user accepted
> on 13 November follow it, in
> [Decisions, 13 November 2026](#decisions-13-november-2026). The
> [alpha.1 definition](plan.md#alpha1-definition-16-november-2026) takes all
> of Tier 0 and three items of Tiers 1 and 2 from it, and keeps the current
> state of each item. The "Div at `565db46a`" columns here describe that
> commit only. References to `consolidated-plan.md` mean this file.

Written 13 November 2026. All six samples measured Div at `565db46a`.
Sources: the `docs/` folders of build-with-div/sample-canvas (PR #1 head
`canvas-phase1-design`), sample-charts (`main`), sample-editor (PR #1 head
`design`), sample-interactions (PR #1 head `probes-and-design`),
sample-motion (PR #1 head `phase1-investigation`) and sample-markdown
(PR #1 head `phase1-investigation`), plus sample-git `docs/framework-gaps.md`
and the capability table in its `docs/design.md` (`main`). No repository was
modified.

## Executive summary

Across 30+ npm packages, the six samples found the same small set of
platform foundations underneath them. The cascade and selectors are already
scheduled. The browser UA stylesheet with real HTML element identity is not.
Neither are `requestAnimationFrame` (5 of 6 samples), `ResizeObserver` (5),
a unified Pointer Events model (5, building on the scheduled headless
pointer work), or a rendering pipeline that does not re-lay out, re-record
and re-raster the whole window for every change. That last item is the
measured blocker for an editor (272 ms per edit for a CodeMirror-sized
viewport), canvas (25–29 ms per pushed frame) and motion (about 7 ms of
full-viewport raster per frame, and quadratic animation start). After those
foundations, the work with the widest reach is inline SVG painted
Chromium's way (charts, motion, markdown, git icons; 70 of 70 shadcn chart
examples need it), the ARIA mapping (4 samples), CSS
Transitions/`@keyframes` from style rules, the Canvas 2D context as a port
of Blink's canvas2d (4 samples), and the observers. The large single-sample
items come after the shared ones: contenteditable with Selection/Range,
HTML drag and drop with OS bridges, `GPUCanvasContext`, View Transitions
and WebGL2. Every sample proposes measuring against pinned Chromium 147,
with WPT where Div's tooling can run it and package-vs-port harnesses
elsewhere. Several WPT areas need tooling first, because almost all of
their tests are testharness scripts. The main open design decisions are:

- one compositor design for canvas slots, animation layers and View
  Transitions snapshots;
- the canvas route A device-sharing model;
- the D1 compile-time stylesheet rule against packages that inject CSS or
  write custom properties at runtime;
- converging API shapes for rAF, `ResizeObserver`, Pointer Events and
  Selection.

## Not read, or read only in part

| Item | Status |
| --- | --- |
| `charts-canvas-api-requests.md` ("shared notes", cited by sample-canvas and sample-charts) | Not in any of these repositories; not read |
| sample-interactions `docs/framework-gaps.md` (cited by its design) | Not present at the PR head; not read |
| sample-editor, sample-charts `framework-gaps.md` | Do not exist yet (planned for phase 2) |
| Probe sources and raw logs (`docs/probes`, `probes/results`, `evidence/`) | Not read; numbers below are quoted from the design docs, which cite them |
| sample-canvas `npm-platform-usage.md` | Skimmed (scan counts) |
| sample-git `docs/design.md` | Only the capability-gap section |
| Div RFC 0001, RFC 0004, `docs/alpha/plan.md` in build-with-div/div | Not read; status taken from the samples and a local copy of `docs/alpha/plan.md` |

Key: **Cv** canvas, **Ch** charts, **Ed** editor, **In** interactions,
**Mo** motion, **Md** markdown, **Git** sample-git (gap number).
**n** = how many of the six samples need the feature. **Sched** values:
**yes** = on the scheduled list; **part** = partly scheduled (the
scheduled part is named); **no** = not scheduled. Sizes: S, M, L, XL, with
differing sample estimates reconciled in the Size note.

## 1. Platform features (deduplicated)

### Style and CSS

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | Selectors and cascade: class, attribute (`[data-state]`), combinators, `:hover`, `:focus-visible`, `:not`, `:has`, `:nth-child`, `:target`, `:dir` | Ch (shadcn `ChartContainer`), Ed (CM6 themes), In (Radix `data-state`), Mo (tailwindcss-animate), Md (github-markdown-css: 324 selectors), Git 1, 2 | 5 | Inline styles only; A1 in progress | engine | L | **yes** (#16–#19) | WPT `css/selectors`; markdown stylesheet mode | — |
| S2 | Stylesheet ingestion: `<style>`, third-party sheets in a browser-compatible mode (drop what Chromium drops, report the rest), runtime-injected sheets | Md (github-markdown-css), Ed (CM6 `style-mod` injects sheets), Ch (shadcn `ChartStyle` `<style>`) | 3 | RFC 0001 makes an unsupported selector a compile error | engine (A1 macros) | M | no | Markdown stylesheet mode; drop report | S1 |
| S3 | Custom properties: `var()` (including SVG paint and `@keyframes`), `@property`, `setProperty('--x')` | Ch (69/70 examples), Ed, In (Radix `--radix-*`, cmdk `--cmdk-list-height`), Mo (tailwindcss-animate `--tw-enter-*`, react-spring), Md (60 tokens, 124 uses), Git 19 | 5 | Rejected (`declaration_block.rs:89-91`) | engine | L (Ch/Md said M for tokens; Mo said L with `@property` and keyframes) | **part** (A2 tokens; `@property`, keyframe `var()` and runtime writes not stated in A2) | WPT `css-variables`, `css-properties-values-api` | S1 |
| S4 | Media queries and `matchMedia` with change events; OS `prefers-color-scheme`/`prefers-reduced-motion`; `resolution` | Cv (DPR change), Mo (F9), Md (MD-6), Git 19 | 3 | Manual `set_prefers_reduced_motion` only | engine (MQ) + framework (OS settings) | M + S | **part** (A2 media, dark mode); OS reduced motion and `resolution` are not | WPT `mediaqueries` | S1 |
| S5 | UA stylesheet (browser preset, RFC 0004) plus presentational attributes (`align`, `img width/height`, `ol start/reversed`, `li value`) | Md (MD-3, MD-18; measured 0/392 boxes without it); implicitly every port that emits HTML (Ch tooltip and legend, Ed CM6 DOM, In Radix) | 1 measured, all implied | None; RFC 0004 proposed, not started | engine | L | no | Markdown semantic mode against pages with no author CSS; WPT `html/rendering` | S1, S6 |
| S6 | HTML element identity (every tag GitHub's sanitizer allows, local name kept) and an `html`/`body` root | Md (MD-1, MD-2: 7 tags rejected; body fills the viewport despite an 8 px margin), Ch (SVG names are case-sensitive) | 2 | `Element::create` rejects `blockquote`, `pre`, `code`, `del`, `kbd`, `sub`, `sup`; `h1`–`p` become anonymous `Div` | framework (`tag_definition`, `view!`) + engine (div-dom `LocalName`) | M + M | no | Markdown rejection report goes to zero | — |
| S7 | Value literals and schema: single-value `border-*-radius`, `border-*-width: 1px`, `list-style-type` under its own name, `overflow-clip-margin: content-box`, `width: max-content`, `disclosure-open/closed`, `appearance`, `color-scheme`, `caret-color`, logical longhands | Md (MD-10, MD-14), Git 4, In (F17 cursor) | 2 | Rejected or misnamed (fidelity-results table) | engine | S each | **part** (`inset`, the flex and border shorthands, `cursor`, `resize`) | Markdown computed mode | — |
| S8 | `::before`/`::after`/`::marker` from sheets | Md (MD-13: footnote brackets, clearfix) | 1 | `set_pseudo_style` covers first-line, first-letter, marker and placeholder only | engine | S after A1 | no | Markdown footnotes fixture | S1 |
| S9 | List markers and counters (`decimal`, `lower-roman`, `circle`, `square`, `list-item` counter, `inside`) | Md (MD-7) | 1 | `none` and `disc` only | engine | M | no | WPT `css-lists`, `css-counter-styles`; markdown lists fixture | — |
| S10 | Table layout: anonymous table wrapping, `min/max/fit-content`, `border-collapse` | Md (MD-8: 0/54 cells), In (TanStack Table, AG Grid) | 2 | Each row laid out as its own table | engine | L | no (A4 lists a tables coverage suite after alpha.1) | WPT `css-tables`; markdown table fixture | S7 |
| S11 | `text-decoration-line` painting | Md (MD-9: `del`, links) | 1 | Paints nothing | engine | S–M | no | WPT `css-text-decor`; markdown inline fixture | — |
| S12 | Line-box and glyph-origin snapping (LayoutUnit) | Md (MD-12: 1/64 px at 13.6 px, h1 at 1x), Cv (R4: canvas text) | 2 | Off by 1/64 px; some glyphs one device pixel off | engine (text) | M | **part** (A6 real-font text) | Markdown headings/code fixtures; A6 baselines | — |
| S13 | CSS `cursor` keyword set; cursor follows the pointer captor | In (F17), Git 4 | 1 | 6 values; cursor from the hit element | engine + framework | S | **part** (cursor literal) | WPT `css/css-ui` | I1 |
| S14 | `user-select` and its drag-selection behaviour | In (dnd-kit, panels), Ed, Md | 3 | Missing | engine | S–M | no | WPT `css/css-ui` | E2 |
| S15 | `touch-action`, touch panning, `pointercancel` | In (dnd-kit, panels), Mo (@use-gesture) | 2 | Missing | engine + framework | M | no | WPT `pointerevents` | I1 |

### HTML elements, forms, overlays, accessibility

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| H1 | `<img src>` loading through the resource provider, base URL, intrinsic size from the bytes, `error`; releasable resources | Md (MD-15), Cv (gap 4: frames leak), Ch (item 17: 153.6 MB kept) | 3 | `src` loads nothing; 300×150 without a size; registry is append-only and the cache never frees | framework + engine | M | no | Markdown image fixture; leak probe (Ch probe 3, Cv P3) | — |
| H2 | `a[href]`: focus, activation, `:any-link`, navigation callback, fragment scroll and `:target`, `open_url` | Md (MD-17) | 1 | `a` is a plain span | framework + engine | M | no | Interaction traces | S1 |
| H3 | Form-control metrics and parity: checkbox box, textarea caret/selection/composition paint, scrolling, Up/Down, O(document) keystrokes | Md (MD-11: checkbox 0×0), Ed (21), Git 27 | 2 | Partial | engine (forms) | M | **part** (textarea padding and placeholder colour; `<select>` initial value) | WPT `the-textarea-element`; harness | — |
| H4 | Top layer and `<dialog>` (`showModal`, `::backdrop`, Escape) | In (F4: Radix, cmdk), Git 14 | 1 | `set_modal_root` and hand-made portals | engine | L | **yes** | WPT `the-dialog-element` | S5 |
| H5 | `popover` attribute (light dismiss, `popovertarget`, toggle events) | In (F5: Radix, Floating UI) | 1 | Missing | engine | M | no (A3 alpha.2 in the alpha plan) | WPT `html/semantics/popovers` | H4 |
| H6 | CSS anchor positioning completion: `anchor()`, `anchor-size()`, `position-try-*`, `@position-try`, cross-containing-block anchors | In (F6: Floating UI natively) | 1 | Subset: `anchor-name`, `position-anchor`, 9 `position-area` values | engine | M–L | no (A3 alpha.2) | WPT `css-anchor-position` | H4, H5 |
| H7 | `inert` | In (F7: Radix `hideOthers`) | 1 | Modal root only | engine | S–M | no | WPT `inert` | H4 |
| H8 | ARIA: `role` attribute and states/properties/relations mapped into AccessKit | In (F13: tree, grid, listbox; Tabs' `aria-selected` never exposed), Ch (graphics roles), Ed (17), Md (roles from element names) | 4 | `role` ignored; only `aria-label`, `-description`, `-hidden` read | engine (accessibility) | M | no | `core-aam`/`html-aam`; CDP `Accessibility.getFullAXTree` against Div | S6 |
| H9 | Accessible editable text (UIA TextPattern / AT-SPI Text with positions; selection on the control) | Ed (17) | 1 | Selection on the run, not the control; no character positions | engine (accessibility) | M | no | AX tree against Chromium; NVDA/Orca by hand | E2 |

### SVG

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| V1 | Inline SVG core: namespace, `svg` sizing and `viewBox`, `g`, `path` (full grammar), basic shapes, presentation attributes as hints, fill/stroke/dash/opacity/`transform`, painted with Chromium's `SVGShapePainter` setup; static `image/svg+xml` through the same tree and painter | Ch (70/70 shadcn), Mo (F16: lottie, GSAP DrawSVG), Md (badges, KaTeX paths), Cv (`Path2D` from SVG path data), Git 17 (Lucide) | 4 | Child elements are `UnknownTag`; `<svg>` lays out 300×0; the static rasterizer paints `rect` and filled axis-aligned paths only | engine | L | **part** (stroked icon painting, low priority; public SVG API, framework) | WPT `svg/shapes`, `svg/path`, `svg/painting`, `svg/coordinate-systems`; Recharts pages | S1, S3 |
| V2 | SVG text: `text`, `tspan`, `x`/`y`/`dx`/`dy` with `em`, `text-anchor`, `dominant-baseline` | Ch (60/70), Md (badges) | 2 | Missing | engine | L | no | WPT `svg/text` | V1 |
| V3 | Paint servers and clipping: `defs`, gradients, `pattern`, `clipPath`, `mask`, `marker`, `url(#id)` | Ch (10 clip, 2 gradient), Mo (lottie masks), Md (Mermaid markers) | 3 | Missing | engine | M | no | WPT `svg/pservers`, `css-masking/clip-path` | V1 |
| V4 | SVG geometry: `getBBox`, `getTotalLength`, `getPointAtLength`, `getComputedTextLength`, `pathLength` | Ch (10+), Mo (GSAP, Motion path drawing), Md (Mermaid) | 3 | Missing | framework (engine geometry) | M | no | WPT `svg/types` cases, ported | V1, V2 |
| V5 | SVG hit testing, `pointer-events`, `tabindex`/focus/roles on SVG | Ch (pie tooltips; `accessibilityLayer` in 70/70) | 1 | No geometry to hit | engine + framework | M | no | Hover and focus traces | V1, H8 |

### Canvas and GPU

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C1 | `HTMLCanvasElement`: bitmap from `width`/`height` (default 300×150), bitmap display, detached and stacked canvases, fallback content in the AX tree (W1, W10, W16) | Cv, Ch (Chart.js, ECharts layers), Mo (lottie canvas), Ed (Monaco minimap) | 4 | Paints nothing; 300×0 without CSS size | engine | S + S + S | no | WPT `canvas-host` and sizing reftests | — |
| C2 | `CanvasRenderingContext2D` core: paths, fill/stroke, line styles, dashes, state, transforms, clip, `globalAlpha`, composite, styles, gradients, patterns (W2, W3); a port of Blink `canvas2d` into a new `div-canvas` crate | Cv (Konva, Fabric, rough.js, PixiJS), Ch (Chart.js, ECharts), Mo (lottie), Ed (minimap) | 4 | None | engine | L (Ch: L for all canvas; Cv: L split M+M) | no | WPT M1 core 431 tests (generated from YAML); call-trace replay against Chromium `--disable-gpu` | C1 |
| C3 | Canvas text and `measureText`, shadows, `getImageData`, `Path2D` and `isPointIn*`, `drawImage` (W4–W8) | Cv, Ch, Ed (`measureText`) | 3 | Reusable pieces in div-paint and div-text | engine | M + S/M + S + M + M (≈L) | no | WPT buckets: text 90, shadows 50, readback 37, Path2D 73, images 212 | C2, S12 |
| C4 | `toDataURL`/`toBlob` (W9) | Cv (Fabric, Konva, Babylon, Excalidraw), Ch (export) | 2 | PNG encoding behind `testing` | framework | S | no | Decoded-PNG comparison | C2 |
| C5 | `navigator.gpu`, `GPUCanvasContext`, wgpu presenter, `copyExternalImageToTexture` (W17, W18) | Cv (three.js, Babylon, deck.gl, PixiJS 8) | 1 | CPU frame uploaded to one GL texture; no alpha | framework (platform) + engine (scene) | L + S | no | WebGPU CTS subset; composition compared with Chromium on SwiftShader (headless availability unchecked) | R4, R6 |
| C6 | `OffscreenCanvas`, `createImageBitmap`, `ctx.filter`, layers, `reset`, colour spaces (W19, W20) | Cv (PixiJS 8, Babylon) | 1 | None | engine + framework | M + L | no | WPT `offscreen`, `filters`, `layers` | C2 |
| C7 | WebGL2 (W21) | Cv (three.js, Babylon defaults) | 1 | None | framework | XL | no | WebGL CTS | C5 |

### Frames and the rendering pipeline

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| R1 | `requestAnimationFrame`/`cancelAnimationFrame`, run in `begin_frame` before sampling and style; a pending callback requests a frame; on every window, including `AppSpec`; headless `advance_frame` and `render_at(t)` | Cv (all packages), Ch (Recharts, Chart.js), Ed (CM6 measure cycle), In (Floating UI, dnd-kit), Mo (Motion, GSAP, react-spring, lottie) | 5 | None; nearest is `PlatformEvent::Presented`, which runs after present and only on `App::builder()` | framework | M (Ed/In said S, Cv/Mo said S–M, Ch said M) | no | WPT `html/webappapis/update-rendering` ordering subset | — |
| R2 | Frame production: vsync pacing on every backend, present timestamps, high-resolution timers, occlusion throttling | Mo (F5), Cv (`frame_loop`) | 2 | GL swap interval 1 only; `sleep(1 ms)` takes 15.4 ms | framework (platform) | M | no | `frame_pacing` example | R1 |
| R3 | `devicePixelRatio` getter and change notification | Cv (W12), Ch (DPR) | 2 | Inside `ViewportMetrics` only | framework | S | no | — | S4 |
| R4 | Damage and partial raster: union of old and new rects, re-raster only the tiles under change, present with damage; **canvas slots** (content change ≠ relayout/re-record) | Cv (§5.1, P7), Mo (F15 step 2), Ed (F step 4), Ch (ECharts hover layer over 100k points) | 4 | Damage is always the full viewport; every visual change re-records the whole document | engine (+ framework present) | L (Cv M/L slots; Mo M damage) | no | Gate: a partial frame equals a full re-record pixel for pixel; `measure_frame_cost`, P7 | — |
| R5 | Targeted style sampling; O(1) animation start | Mo (F15 step 1) | 1 | Every frame and every `animate()` re-cascades all roots | engine | S–M | no | `start_cost_by_animation_count` | — |
| R6 | Compositor layers (promotion, raster scale per Chromium) and off-thread animation | Mo (F15 steps 3–4; fractional-scale finding), Cv (W17 quads) | 2 | Layers are diagnostic metadata; off-thread animation is a Div release blocker (`threading-model.md`) | engine + compositor + framework | L | no (a known release blocker) | `frame_pacing` with UI-thread blocks; sampled-time probe | R4 |
| R7 | Layout cost: per-box cost (~0.2 ms per inline box), shaping cache, no double shaping, subtree layout caching, partial paint re-record, `content-visibility: auto` | Ed (F: 272 ms per edit), In (88–128 ms per column resize), Ch (probe 2: 0.15 ms per element to build), Git 8, 26 | 3 | Whole-document relayout per change | engine (layout, text) | L | **part** (nested-flex cache; long-list profiling) | Editor `window_flow_cost` against Chromium on the same machine (not yet measured) | — |

### Observers and fonts

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| O1 | `ResizeObserver` (content, border and `device-pixel-content-box`, loop limit) and window `resize` | Cv (Chart.js, Excalidraw, PixiJS 8), Ch (`ResponsiveContainer` 70/70), Ed (CM6, Monaco), In (8 packages), Mo (Motion, react-spring) | 5 | None | engine (observations after layout) + framework (delivery) | M (Ed said S–M) | no | WPT `resize-observer` (JS cases ported) | R1 |
| O2 | `IntersectionObserver` | Ed (CM6), In (Floating UI, infinite lists), Mo (`inView`) | 3 | None | engine + framework | M | no | WPT `intersection-observer` | O1 |
| O3 | `MutationObserver` | Ed (CM6, ProseMirror, Lexical), Mo (AutoAnimate) | 2 | None | engine (DOM mutation points) + framework | M | no | WPT `dom/nodes/MutationObserver-*` | — |
| O4 | `FontFace`/`document.fonts` (`ready`, load events), `@font-face` in sheets, WOFF2 on Linux | Cv (Excalidraw, PixiJS), Ed (CM6, Monaco), Md (KaTeX) | 3 | `register_font_face` with no load state | framework + engine (text) | S–M | no | WPT `css-font-loading` | S2 |

### Input and events

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| I1 | Pointer Events: mouse produces `pointer*` then compatibility `mouse*`; `offsetX/Y` from the hit test's `world_to_local`; `buttons`, `pointerType`, `isPrimary`, pressure, tilt, screen coordinates; implicit capture release with `got/lostpointercapture`; mouse `pointerId` | Cv (Konva, Fabric, Excalidraw), Ch (Recharts brush), Ed, In (dnd-kit, panels), Mo (Motion, @use-gesture), Git 16 | 5 | Mouse fires only `mouse*`; capture never released; viewport coordinates only | framework + engine (`interaction.rs`) | M | **part** (headless pointer events and pointer capture) | WPT `pointerevents`; Cv P6 (offsets under `rotate`) | T1 |
| I2 | UI Events completeness: `click` on the common ancestor, `dblclick` and `detail`, `auxclick`, right click must not fire `click`, `contextmenu`, focus on press to the focusable ancestor, `KeyboardEvent.code`, `Insert`, modifier keys | Cv, Ch (item 14), Ed (15), In (F1), Git 15 | 4 | Right click fires `click`; no `dblclick`/`detail`/`auxclick` | framework | S–M | **part** (`contextmenu` and popup menus) | WPT `uievents`; event traces | I1 |
| I3 | Wheel: positive `deltaY` scrolls down (natively inverted today), system lines per notch, `deltaMode`, Shift+wheel | In (F2) — affects every Div app | 1 | Inverted; 40 px per notch | framework (div-platform) | S | no | WPT `uievents` wheel; unit test on the winit mapping | — |
| I4 | Geometry, scroll and focus details: `elementsFromPoint`, `offsetParent`, `visualViewport`, `scrollend`, `overscroll-behavior`, scroll anchoring, `focus()` scrolling into view with `preventScroll` | In (F10–F12: TanStack Virtual, AG Grid, Radix), Ed | 2 | Core geometry works (`bounding_rect`, `client_rects`, `hit_test`) | engine | S each | no | WPT `css/cssom-view` | — |
| I5 | Clipboard events (`copy`/`cut`/`paste`) and `DataTransfer` formats (`text/html`, custom) | Ed (8), In (AG Grid), Md (copy) | 3 | Plain text only, no events | framework (platform, events) | M | no | WPT `clipboard-apis` | E2 |
| I6 | HTML drag and drop, `DataTransfer` (`files`, `effectAllowed`, drag image) and OS bridges (OLE, XDND, Wayland) | Ed (9), In (F3: react-dnd, react-arborist, AG Grid) | 2 | Missing; OS drops discarded with no position | engine (events, drag image) + framework (OS) | L (Ed said M; In said in-app M + OS in M + out L) | no | WPT `html/editing/dnd`; CDP `Input.dispatchDragEvent` traces | I1, I5 |
| I7 | Window chrome: `app-region: drag`, window position/state, move/resize events | In (F19) | 1 | `WindowSpec` title/size/decorations | framework | M–L | **part** (window activation) | — | — |

### Editing

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| E1 | `contenteditable` editing host, `beforeinput`/`input` with the full `inputType` set, `getTargetRanges()`, caret painting, undo grouping | Ed (CM6, ProseMirror, Lexical) | 1 | Not focusable; typing does nothing; 10 input types on form controls only | engine (editing) + framework (events) | L + M | no | WPT `editing/`, `input-events/`; CDP-driven behaviour traces | E2, O3 |
| E2 | Selection API and Range (`getClientRects`, `caretPositionFromPoint`), selection painting, `::selection`, document text selection and copy | Ed (4–6), Md (MD-16) | 2 | Form-control selection only | engine (layout offset map, paint) + framework | L + M + M | no | WPT `selection/`, `dom/ranges/`, `css/cssom-view` | S14, I2 |
| E3 | Composition events and IME at the caret (`set_ime_cursor_area`, preedit cursor) for any editing host | Ed (7) | 1 | Form controls only; preedit cursor dropped | framework (platform) | M | no | WPT `uievents`; CDP `Input.imeSetComposition`; native IME by hand | E1, E2 |
| E4 | `EditContext` | Ed (20: newer Monaco) | 1 | Missing | framework + engine | L | no | WPT `editing/edit-context` | E3 |

### Animation

| ID | Feature | Samples (packages) | n | Div at `565db46a` | Owner | Size | Sched | Fidelity measure | Deps |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | CSS Transitions from style rules: `transition-*`, start on computed-value change, `transition*` events, `allow-discrete`, `@starting-style` | Mo (F1), Ch (item 11: `stroke-dasharray`), In (cmdk height) | 3 | Engine effect with Chromium timing, only via explicit `Element::transition`; fires `animation*` events | engine | L | no | WPT `css-transitions` (180 files; needs script translation T3) | S1, S3 |
| A2 | CSS Animations: `@keyframes`, `animation-*`, events with detail, computed `animationName` | Mo (F2: tailwindcss-animate, Radix `Presence`), In (Radix), Ed (CM6 cursor blink), Ch (item 11) | 4 | Keyframe effects exist; no `@keyframes`; empty events | engine | L | no | WPT `css-animations` (207); census sample-at-time | S1, S3 |
| A3 | Web Animations API surface: multi-property `animate()`, `Animation` handle, `reverse`, `finished`, `commitStyles`, auto-removal, `getAnimations`, `document.timeline`, keyword easings | Mo (F3: Motion, AutoAnimate) | 1 | Single-property `animate()` with id-based controls | framework (API) + engine (effect model) | M | no | WPT `web-animations` (151; T3) | R1 |
| A4 | Interpolation coverage and raster parity: `visibility` rule, animated fractional `scale()` edges, individual transforms, `filter`, `box-shadow`, `clip-path`, `interpolate-size` | Mo (F6) | 1 | `visibility` flips at p = 0.5 (pixel-confirmed) | engine | S (visibility, scale) / M each | no | WPT interpolation tables (327 files); sampled-time probe | R6 for scale |
| A5 | Scroll-driven animation syntax and `ScrollTimeline`/`ViewTimeline` objects | Mo (F13) | 1 | Engine timelines exist, not Chromium-qualified | engine + framework | M | no | WPT `scroll-animations` (226) | A2, A3 |
| A6 | View Transitions | Mo (F14) | 1 | None | engine + compositor | L | no | WPT `css-view-transitions` (445) | A2, R6 |
| A7 | Retaining removed keyed children until exit finishes (React's contract; not a web feature) | Mo (F18: `AnimatePresence`, Radix `Presence`) | 1 | `Show`/`For` unmount at once; `DynChild` unverified | framework | S–M | no | Probe (phase 2) | — |

### Measurement infrastructure

| ID | Feature | Samples | n | Owner | Size | Sched | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| T1 | `TestApp`: frame advance, pointer/drag/wheel/right/double-click helpers, frame-timed event delivery | In (T1), Cv, Mo | 3 | framework | S | no | Needed before I1 traces and testdriver-Actions WPT |
| T2 | Behaviour-trace harness: pointer, drag, wheel, touch and file-drop steps, with per-step geometry and AX-tree comparison | In (T2), Ed | 2 | components | S–M | no | Extends `tests/fidelity/components` |
| T3 | WPT script translation to native calls (extend `javascript_mutation_ir.mjs`; map rAF to deterministic frames), interpolation-table generator, census sample-at-time, canvas YAML generator | Mo, Cv | 2 | engine (tools) | M per suite | no | Most of the animation, canvas, observer and pointer WPT areas are testharness scripts. Without T3 those areas cannot be measured |

## 2. Recommended order

There is one engine lane and one framework lane. Each merges one PR at a
time, with about 2 h of CI per merge. Scheduled work comes first in each
lane. The engine lane alone already holds about 11 scheduled PRs: #16–#19,
#13, the nested-flex cache, literals, `<select>`/textarea, long-list
profiling, the top layer and `<dialog>`, and SVG strokes. That is roughly
22 h of serial CI before new engine work. So S-sized items should fill
gaps, and coherent additions should join a scheduled PR (for example the S7
literals into the literal-parsing PR) rather than queue separately. The
components track is not serialized with these lanes.

### Tier 0: foundations

| # | Lane | Item | Size | Unblocks |
| --- | --- | --- | --- | --- |
| 0.1 | engine | S1 cascade and selectors (#16–#19) — scheduled | L | Ch, Ed, In, Mo, Md, Git |
| 0.2 | engine | S7 literals: add the measured markdown values to the scheduled literal PR | S | Md, Git, In |
| 0.3 | engine | S6 element identity and `html`/`body` root | M | Md; prerequisite of S5, H8 |
| 0.4 | engine | S5 UA stylesheet (text and sections, then lists and tables first, per Md) | L | Md (all its 0/392 semantic boxes), every HTML port |
| 0.5 | engine | S3/S4 custom properties and media (A2) — scheduled | L | Ch, Ed, In, Mo, Md |
| 0.6 | engine | R7 per-box cost and shaping cache, after the scheduled nested-flex cache | L | Ed (critical path), In, Ch, Git |
| 0.7 | engine | R4 damage and partial raster, plus R5 targeted sampling/O(1) start | M + S–M | Mo, Cv, Ed; makes canvas slots possible |
| 0.8 | framework | Scheduled: `{expr}` children, zbus pins, `current_document`, RFC 0003, headless pointer events and capture | — | Git, all |
| 0.9 | framework | I3 wheel direction | S | every app |
| 0.10 | framework | R1 `requestAnimationFrame` + T1 frame advance | M | Cv, Ch, Ed, In, Mo |
| 0.11 | framework | O1 `ResizeObserver` (with its small engine part) | M | Cv, Ch, Ed, In, Mo |
| 0.12 | framework | I1 Pointer Events unified model, extending the scheduled headless work; T1 input helpers | M | Cv, Ch, Ed, In, Mo |
| 0.13 | engine tools | T3 WPT script translation (rAF-aware) | M | Measurement for A1–A3, O1–O3, I1, C2 |

### Tier 1: wide reach for the cost

| # | Lane | Item | Size | Samples |
| --- | --- | --- | --- | --- |
| 1.1 | framework | I2 UI Events completeness (`dblclick`/`detail`, `auxclick`, click target, `code`) | S–M | Cv, Ch, Ed, In |
| 1.2 | framework | R3 `devicePixelRatio`, S4 `matchMedia` change events and OS reduced-motion/colour-scheme | S + S | Cv, Ch, Mo, Md |
| 1.3 | engine | H8 ARIA mapping | M | In, Ch, Ed, Md |
| 1.4 | engine | V1 inline SVG core with Chromium's painter; the scheduled icon-stroke item lands as this stage, and the static images move onto it | L | Ch, Mo, Md, Cv, Git |
| 1.5 | engine | Quick measured fixes: S11 text decoration, A4 `visibility`, S8 `::before`/`::after` | S each | Md, Mo |
| 1.6 | engine | C1 canvas element sizing, display, detached/stacked, fallback | S ×3 | Cv, Ch, Mo, Ed |
| 1.7 | engine | A2 `@keyframes`, then A1 transitions from rules | L + L | Mo, In, Ed, Ch |
| 1.8 | framework | A3 Web Animations API surface | M | Mo |
| 1.9 | framework | O3 `MutationObserver`, O2 `IntersectionObserver` | M + M | Ed, Mo, In |
| 1.10 | framework | S13 cursor keywords and captor cursor; O4 `FontFace` | S + S–M | In, Cv, Ed, Md |
| 1.11 | framework | R2 frame pacing and high-resolution timers | M | Mo, Cv |
| 1.12 | components | T2 trace-harness pointer/drag/AX steps | S–M | In, Ed |

### Tier 2: large shared features

| # | Lane | Item | Size | Samples |
| --- | --- | --- | --- | --- |
| 2.1 | engine | C2 Canvas 2D core (new `div-canvas` crate), then C3 in the order text, readback, shadows, `Path2D`, `drawImage` | L + ≈L | Cv, Ch, Mo, Ed |
| 2.2 | engine | R4 canvas slots on top of 0.7 | M/L | Cv, Ch |
| 2.3 | engine | V2 SVG text, V3 paint servers and clip, then V5 hit testing and focus | L + M + M | Ch, Md, Mo |
| 2.4 | engine | E2 Selection/Range/caret-from-point (layout offset map), S14 `user-select` | L + M + M | Ed, Md, In |
| 2.5 | engine | E1 `contenteditable` and `beforeinput` | L | Ed |
| 2.6 | engine | S9 list markers, S10 tables, H3 form-control metrics, S12 text snapping (with A6) | M + L + M + M | Md, In, Ed |
| 2.7 | engine | H5 `popover`, H7 `inert`, H6 anchor positioning (after the scheduled `<dialog>`) | M + S–M + M–L | In |
| 2.8 | framework | V4 SVG geometry APIs, C4 `toDataURL`, H1 `<img src>`, H2 links | M + S + M + M | Ch, Mo, Md, Cv |
| 2.9 | framework | E3 IME, I5 clipboard events | M + M | Ed, In, Md |
| 2.10 | engine/framework | I4 geometry/scroll/focus details, S15 `touch-action` | S each + M | In, Mo |

### Tier 3: single-sample or very large

I6 drag and drop (in-app, then OS drops in, then drag-out); R6 compositor
layers and off-thread animation (a Div release blocker, prerequisite of A6
and of exact animated `scale()`); C5 `GPUCanvasContext` and the wgpu
presenter; A5 scroll-driven syntax; A6 View Transitions; A7 retained
children; I7 window chrome; E4 `EditContext`; C6 `OffscreenCanvas` and
filters; C7 WebGL2; Md KaTeX and Mermaid (MD-21, MD-22).

## 3. Contradictions and open design questions

| # | Topic | Positions | Question to decide |
| --- | --- | --- | --- |
| Q1 | **Canvas route A (surface handoff)** | Cv recommends option 2: `getContext("webgpu")` → `GPUCanvasContext` on wgpu. Div hands out adapters and devices from its own instance, and a wgpu presenter composites canvas textures as underlays with a hole punch. Div content stays CPU-rastered and is uploaded with alpha. Option 1 (a Skia callback in paint) is rejected because it breaks the no-callback compositor contract. Option 3 (a native child window) is rejected: it has no web equivalent and nothing can overlap it | (a) A shared device (R2: app device loss can hit the compositor) or separate devices with per-OS interop. (b) Accept that translucent canvases over Div content below them wait for layer partitioning (R3). (c) The oracle depends on headless Chromium 147 exposing SwiftShader WebGPU, which is unverified. (d) WebGPU before WebGL2 (XL). (e) The presenter rewrite is framework L and must keep the CPU presenter's pixel tests green (Cv migration step 7) |
| Q2 | One compositor design or three | Cv canvas slots (§5.1), Mo F15 damage, layers and off-thread curves, Mo F14 View Transition snapshots and Ed F step 4 all propose partial re-record and raster plus layers, separately | Design once (R4 → R6), with canvas slots and animation layers as clients. Otherwise two scene models diverge |
| Q3 | Runtime CSS against D1 and RFC 0001 compile-time sheets | Md: a `stylesheet!` compat mode that drops what Chromium drops; "no runtime CSS text needed". Ed: CM6 injects stylesheets at runtime (`style-mod`). In/Mo: Radix, cmdk and react-spring write custom properties at runtime. Git 3: dynamic `style:` needs typed values | Whether ports must precompile every sheet, whether runtime `setProperty('--x')` and sheet adoption exist, and how strict and compat modes coexist |
| Q4 | `requestAnimationFrame` API shape | Cv: free `request_animation_frame(cb) -> u64`. Ed/Mo: `Document::request_animation_frame` → `FrameRequestId`/handle. In: `div::request_animation_frame` → `FrameHandle`. Sizes S to M | One shape (per document, typed handle); every window, including `AppSpec` |
| Q5 | `ResizeObserver` shape and owner | Cv: callback gets `(&[entries], &observer)`, `observe(target, options)` with three boxes. In: callback gets entries only, `observe(&el, ResizeObserverBox)`. Ch: framework-owned. In/Ed: engine + framework | One API; the engine computes after layout and the framework delivers before paint (Cv's split) |
| Q6 | Pointer Events types | Cv: `pointer_type() -> &str`, `buttons() -> u16`, tilt as `i32`, `offset_x()`. In: `Option<PointerType>`, `Buttons` bitflags, tilt as `(f32, f32)`, `offset_position()`. In asks whether the mouse `pointerId` should change 0 → 1 (Chromium uses 1). Mo calls it framework-only; Cv/In say framework + engine | Pick In's typed shape with web values; decide the `pointerId` |
| Q7 | Selection API | Ed: a web-shaped `Range`/`Selection`/`StaticRange` with UTF-8 byte offsets in Rust and UTF-16 at the FFI. Md: `Document::selection() -> Option<SelectionRange>`, `selected_text()`, `select_all()` | Ed's model as the base, Md's as conveniences on it; confirm the UTF-8 offset decision |
| Q8 | `DataTransfer` | Ed: `get/set_data`, `types`, `set_drag_image`; owner framework, M. In: adds `files`, `items` and effects; engine + framework, L. Open: `PathBuf`s or `File` objects | One type for clipboard and drag; files representation |
| Q9 | SVG scope against the schedule | The scheduled item is "stroked SVG painting for icons (low priority)" plus a framework public SVG API. Ch needs the full inline SVG tree (70/70 examples) and asks that static images use the same painter. Mo and Md need SVG too | Raise priority, and implement icon strokes as V1 stage 1 rather than a second painter in `image_resource.rs`. Also: how `view!` spells `stroke-width`, `xlink:href` and case-sensitive names |
| Q10 | Canvas fidelity method | Ch: WPT `html/canvas` is JS, so call-trace replay stands in. Cv: generate Rust tests from WPT's 19 YAML files (1,157 tests) and also replay traces | Use both; the YAML generator is T3 work |
| Q11 | Canvas 2D owner | Ch names a "canvas" owner; Cv names engine (a new `div-canvas` crate) | Engine. Being a new crate makes it the best candidate if a third lane ever opens |
| Q12 | Animation API continuity | Mo: should CSS-created animations appear in `get_animations()`, and does the single-property `Element::animate → AnimationId` stay? | One WAAPI surface; keep or retire the old call |
| Q13 | Deterministic Chromium capture for package comparisons | Mo: `--enable-begin-frame-control`/`HeadlessExperimental.beginFrame`, or virtual time. Cv uses `--disable-gpu` for canvas | Measure byte stability first (Mo) |
| Q14 | `contenteditable` before `EditContext` | Ed rejects a Div-specific `EditContext` first (3 of 4 editors need `contenteditable`) | Confirm the order |
| Q15 | Non-web contracts | Mo F18 (retain removed children); Cv/Git: `node_ref` in `view!` | Accept as framework work outside the web-feature rule |
| Q16 | Window chrome naming | In: `app-region` is Chromium's non-standard property | Adopt the name or not |

## 4. Measurements reported by the samples

All are against Div `565db46a`. Each holds only under the conditions
given; none is a census result.

| Sample | Conditions | Result |
| --- | --- | --- |
| Cv | Windows 11, 24 cores, RTX 3090, 239 Hz display, release; **CPU at 100% from concurrent builds (pessimistic)** | P1: a `<canvas>` with no CSS size is 300×0 (Chromium 300×150). P7 headless, 640×480 canvas in a 1024×768 viewport, 60 frames, 2 runs: app draw 0.80/0.81 ms, handoff 1.03/1.44 ms, **Div render 25.5/29.2 ms**, render with no change 7.6/8.9 ms. `frame_loop` with an 800×600 window: GL 44.6 fps (mean 22.3 ms, p95 24.8), softbuffer 41.3 fps (mean 24.1 ms, p95 27.0); bound by Div's frame, not vsync. P3: reusing a cache key shows stale pixels |
| Cv | Regex classification of WPT `html/canvas` at Chromium `09d377d9`; **an estimate** | 1,350 element tests, 1,157 generated from YAML. M1 = 607 (core 431, shadows 50, compositing 16, Path2D 73, readback 37). M2 = 302 (images 212, text 90) |
| Ch | Windows 11, 24 cores, release, headless `Document`, CPU raster, one run each | Probe 6, default Skia raster against Chromium SVG: at scale 1, line 1,443 px, grid 955, area 787 and bars 48 differ, all by 1/255. At scale 2: grid 0, line 3,480 (max 15), bars 97 (max 24), area 1,791 (max 4). Probe 2, absolutely positioned dots: 10k build 1,466 ms, layout 396 ms, raster 42.8 ms; 100k build 14.8 s; about 0.15 ms per element. Probe 3: stroking 100k points takes 253 ms at 800×400; LTTB to 1,600 points takes 0.3 ms; 120 scaffold image swaps keep 153.6 MB. Probe 7: SVG children `UnknownTag`; `<svg width=200 height=100>` lays out 300×0 |
| Ch | shadcn@4.23.0, 70 examples (scan rules in its appendix) | Root/`g`/`path` 70, CSS on SVG 70, `ResizeObserver` 70, custom properties 69, SVG text 60, `line` 44, arcs 34, `circle` 28, dashes 11, `clipPath` 10, gradients 2 |
| Ed | Windows 11, 24 cores, headless `TestApp`, 800×600; **dev profile** (Div at opt-level 2, probes unoptimized); shared machine; 1–2 runs | CM6-shaped viewport over 100k lines, one span per token, with a 1000 px margin (1,921 boxes): layout 340 ms, **edit + layout + paint 272 ms**, scroll 454 ms. Visible lines only, plain text (43 boxes): layout 10.6–13.7 ms. Monaco-shaped flex shell (43 visible lines): layout 867–887 ms. Layout costs **about 0.2 ms per inline box** in a block container and about 2.7× that in a flex column. Shaping a 45-character line takes 23.3 µs. `<textarea>` keystroke: 26 ms at 1k lines, 300 ms at 10k, 2.79 s at 100k |
| In | Windows 11, 24 cores, headless, test profile (app unoptimized, dependencies at opt-level 2), single-threaded runner, other builds running; ranges over 2–3 runs; "not benchmarks" | `For` mounts 1,000 rows in 435–671 ms and 5,000 in 3.4–5.6 s. A windowed list over 100k rows mounts in 51–118 ms and re-windows in 22–37 ms. Resizing one column of a 40×8 grid takes 88–128 ms with flex rows or 60–67 ms with block rows, against 5–6 ms for an unchanged frame. A splitter drag move takes 4.5–7.2 ms. Native Windows (posted messages): one wheel notch forward scrolled 500 → 540 → 580 (**inverted**). 33 headless probe tests |
| Mo | Windows 11 Pro 26300, 24 cores, RTX 3090, 5120×1440 at 239 Hz, release; **CPU 83–100%**; an upper bound | Headless frame at 1280×800 (median): 1 transform cell 6.95 ms total, of which 6.65 ms is raster; 1000 transform cells 80.42 ms; idle 7.15 ms. One `animate()` costs 986 µs at N=100 and 15,978 µs at N=1000, so starting N is quadratic (1000 take about 16 s; native first frame at 18,056 ms). Native, 100 cells: GL 14.81 ms mean (67.5/s), software 15.73 ms; 300 cells 23.21 ms; a 50 ms UI block gives a 65 ms frame. `sleep(1 ms)` takes 15.4 ms (median) |
| Mo | Sampled-time probe against Chromium 147.0.7727.50 (WSL), 200×120 at 1×, exact RGBA | **46 of 54 samples pixel-identical.** Not exact: animated `scale(1→2)` at t = 250 and 750; `visibility` at t = 100 and 250 |
| Mo | Div's WPT porter on Chromium WPT `09d377d9`; upstream counts at `cb1a0cb1` | Portable: css-animations 86/238, css-transitions 5/186, css-easing 1/13. None exercises the animation engine (the porter freezes at t = 0). Suites: web-animations 151 (133 testharness), css-animations 207, css-transitions 180, scroll-animations 226, view-transitions 445. Interpolation tables: 327 files, 1,137 `test_interpolation` blocks |
| Md | Chrome for Testing 147.0.7727.50, Linux (WSL), census real-font fontconfig; Div `chromium_linux_lcd`, pinned FreeType; 12 fixtures × light/dark = 24 pages at 1x and 2x; exact pixels; boxes within 1/128 px | Semantic mode: **0/48 renders exact, 0/392 boxes**. Computed mode (Chromium styles copied in): **8/48 renders exact, 214/392 boxes, 46/92 blocks exact at 1x, 34/92 at 2x**. Tables 0/54 cells, `pre` 0/8, checkbox `input` 0/14 (0×0). Rejected tags: 7. Rejected literals listed per declaration (for example 128 single-value radii) |
| Git | Windows 11, headless `TestApp`, debug with dependencies at opt-level 2, one run each | Nested flex columns, 100 rows: depth 0 takes 0.27 s, depth 6 takes 34.1 s (about ×2.2 per level); blocks stay at 0.15–0.16 s. A 1,500-line diff takes about 2.6 s (about 1.7 ms per line) after the workaround; before it, a file click took 256 s. `For` over 2,000 rows mounts in about 600 ms |

Already working, per the probes: `getBoundingClientRect` after transforms,
typed computed style, focus/`tabindex`/`focusin`, `scroll_to`/smooth
scrolling/`scroll_into_view`, `position: sticky`, pointer capture routing,
transform/opacity skipping layout, and the engine's animation timing model
and easings. Each was matched against Chromium within the scope its probe
checked.

## Decisions, 13 November 2026

The user accepted all of these recommendations on 13 November 2026. They resolve the open questions in section 3 above (Q numbers in brackets). The owning track records each one in its RFC or design doc when it starts the work.

### Shape the scheduled work

1. **One rendering design (Q2).** Design partial redraw once. The engine's Tier 0 item 0.7 (damage tracking and partial raster) becomes it: damage regions first, then layers. Canvas slots, animation layers and View Transitions snapshots are all built on top of it. Owner: engine, which writes it as an RFC before implementing.
2. **Runtime CSS (Q3), option B now and C later.**
   - Keep compile-time strict checking (RFC 0001) for code authored for Div.
   - Add a browser-compatible ingestion mode for third-party sheets: it drops what Chromium drops and reports what it dropped.
   - Support runtime custom-property writes (`style.setProperty('--x', …)`).
   - Later, alongside the editor work: runtime sheet insertion (`insertRule`, adopted stylesheets), the CSSOM that CodeMirror's style-mod needs.
   - Owners: engine (ingestion, custom properties), framework (the runtime API surface).
3. **SVG moves earlier (Q9).**
   - Inline SVG painted Chromium's way (V1) starts right after the engine's Tier 0 items.
   - The scheduled stroked-icons item becomes V1 stage 1, not a second painter.
   - `view!` uses SVG's real attribute names (`stroke-width`, `viewBox`, case-sensitive).
   - Owners: engine (painting), framework (`view!` spelling).
4. **API shapes (Q4–Q6).** The framework's RFC follows these directions; it still comes to the parent for review of the details.
   - `requestAnimationFrame` is per document, returns a typed handle and works in every window.
   - `ResizeObserver` is a single API with three boxes. The engine computes after layout and the framework delivers before paint.
   - Pointer Events use typed enums and bitflags carrying web values. The mouse `pointerId` is 1, as in Chromium.

### Defaults for later features (not scheduled yet)

5. **WebGPU canvas, route A (Q1).**
   - Use `getContext("webgpu")` → `GPUCanvasContext` on wgpu.
   - Div and the app share one device, and Div recovers from device loss.
   - A translucent canvas over Div content waits for the layers in decision 1.
   - First step: verify that headless Chromium 147 exposes SwiftShader WebGPU for the oracle.
   - Owner: framework (presenter), with the engine.
6. **Selection (Q7).** Use the editor sample's web-shaped `Range`/`Selection`/`StaticRange`, with UTF-8 byte offsets in Rust and UTF-16 at the FFI. Markdown's conveniences sit on top. Owner: engine.
7. **DataTransfer (Q8).** One type for clipboard and drag. Files are web-style `File` objects that wrap paths. Owner: framework, with the engine.
8. **Canvas 2D (Q10, Q11).**
   - The engine owns it, as a new `div-canvas` crate.
   - It is tested two ways: Rust tests generated from WPT's 19 YAML files (1,157 tests, as part of T3) and replay of recorded call traces against Chromium.
9. **Animation API (Q12).** One Web Animations surface. CSS-created animations appear in `getAnimations()`. The old single-property `Element::animate → AnimationId` is retired after its users migrate. Owner: framework.
10. **Deterministic capture (Q13).** Measure Chromium's byte stability across runs before choosing between begin-frame control and virtual time. Owner: sample-motion's measurement.
11. **Editing order (Q14).** `contenteditable` comes before `EditContext`. Owner: engine.
12. **Non-web contracts (Q15).** Accept `node_ref` in `view!` and keeping removed children while exit animations run (needed for framer-motion's AnimatePresence) as framework work outside the web-feature rule.
13. **Window chrome (Q16).** Adopt Chromium's `app-region` property name. Owner: framework.
