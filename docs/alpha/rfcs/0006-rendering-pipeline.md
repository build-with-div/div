# RFC 0006: One rendering pipeline: damage, then layers

Status: proposed, 13 November 2026. Owner: engine track (Workstream A,
Tier 0 item 0.7). Consumers: framework track (`div-platform` presenters,
`divui` frame scheduling), the canvas and SVG sessions, the sample apps.
Implements platform decision 1 (Q2): partial redraw is designed once,
damage regions first and then layers, and canvas slots, animation layers
and View Transitions snapshots are built on it.

## Problem

Every visual change redoes the whole frame. Measured at `main` `5a315f42`:

- **Damage is always the full viewport.** `Engine::update_once` builds
  `damage` as one viewport rect (`crates/div-engine/src/lib.rs:2686`).
  `SceneSnapshot` stores it and no presenter reads it.
- **Paint re-records the whole document** into one `SkPicture` on any
  `paint` or `compositing` dirty bit (`DirtyState::visual`,
  `lib.rs:319`). Dirty state is document-wide. No node has its own
  needs-paint state, and no recording is reused.
- **Raster is the whole viewport on the CPU** (`rasterize_picture`,
  `crates/div-paint/src/render.rs:645`). The only cache is the last whole
  frame (`SoftwareCompositor::render`, `crates/div-compositor/src/lib.rs:333`).
- **Present copies the whole frame**: softbuffer `present()` after a
  per-pixel copy (`copy_frame`, `crates/div-platform/src/desktop.rs`), or a
  full `TexSubImage2D` and one quad on the GL path.
- **Animations re-cascade the document.** Sampling re-runs the authored
  cascade for every root (`sample_animations`,
  `crates/div-engine/src/animation.rs:1589`; `recompute_authored_styles`,
  `lib.rs:631`). Starting one animation samples all running ones
  (`start_animation`, `animation.rs:802`), so starting N is quadratic.
- **Layers are metadata.** `RetainedScenePlan` names layers, but
  `authored_layers_are_partitioned()` is always false
  (`crates/div-compositor/src/retained_scene.rs:319`). Opacity, filters
  and masks are `save_layer` groups inside the one picture. `<canvas>`
  paints a solid colour (`crates/div-paint/src/painter.rs:6278`).
  Compositor-thread animation is a recorded release blocker
  (`docs/architecture/threading-model.md:66`).

What the samples measured (release, Windows, 13 November; numbers from
their design docs):

| Sample | Case | Cost |
| --- | --- | --- |
| motion | 1 animated 24-px square, 1280×800 | 6.95 ms median per frame, 6.65 ms of it raster |
| motion | idle page re-rastered | 7.15 ms median |
| motion | 1,000 `animate()` starts | 16 s (16 ms each) |
| motion | 100 animated cells, native | 64–68 frames/s on a 239 Hz display, CPU-bound |
| canvas | probe P7, canvas animation | 25–29 ms per frame |
| editor | one edit in a CodeMirror-shaped viewport | 272 ms (layout dominates; item 0.6) |

Four proposals asked for overlapping pieces of this separately: canvas
slots (sample-canvas `rfc-canvas.md` §5), damage and layers (sample-motion
F15), View Transitions snapshots (F14), and partial raster for the editor.
Decision 1 makes them one design.

## Proposal

Chromium's pipeline has the parts Div lacks, and Div's pixels must match
Chromium's, so the stages follow Blink and cc where they affect pixels:

| Chromium 147 | What it does | Div stage |
| --- | --- | --- |
| `PaintController` (`UseCachedItemIfPossible`, `UseCachedSubsequenceIfPossible`) | Paint output is a list of chunks keyed by display item client; unchanged clients reuse their recording | A1, A4 |
| `RasterInvalidator::Generate` | Compares old and new chunks; a changed chunk invalidates the union of its old and new visual rects | A1 |
| `PaintUnderInvalidationChecker` | Debug check that a reused recording equals a fresh one | A4 gate |
| cc tiles, `TileManager` | Rasters 256-px tiles; invalidation re-rasters only covered tiles | A2 (Div already reproduces cc's tiling, `RASTER_TILE_SIZE_PX`, `render.rs:23`) |
| `DamageTracker::UpdateDamageTracking` | Accumulates the screen damage that is drawn and presented | A2, A3 |
| `PaintArtifactCompositor`, `CompositingReasonFinder` | Turns chunks into `cc::Layer`s by compositing reasons | C1 |
| `PictureLayerImpl` raster scale, `LCDTextDisallowedReason` | Layers raster at a chosen scale; text in some layers loses LCD antialiasing | C1 (these change pixels) |
| cc animations on the compositor thread | Transform, opacity and filter curves tick without the main thread | C2 |
| `ViewTransitionStyleTracker` | Captures elements as snapshots shown through `::view-transition-*` | D |

### Stage A: damage regions (no layers)

**A1. Paint chunks and damage.** `record_fragment` keeps recording, but
emits the document as a list of chunks instead of one opaque picture. A
chunk is the paint of one box and one paint phase between two
property-state changes. Each chunk has:

- a stable identity (node, pseudo-element, phase);
- device-space visual bounds after transforms and clips, rounded out to
  whole pixels;
- a fingerprint: a hash of the chunk's serialized `SkPicture` and its
  property state.

Damage is computed as `RasterInvalidator` does. A chunk that is new,
removed, moved or re-fingerprinted damages its old rect and its new rect;
an unchanged chunk damages nothing. The frame still re-records and rasters
everything, so A1 changes no pixels. It replaces the full-viewport rect
with real damage, and it is fully testable alone.

**A2. Partial raster.** The compositor keeps the last frame's raster and
re-rasters only the 256-px tiles that intersect the damage, replaying the
picture clipped to each tile. Div's full raster already replays the
picture tile by tile to match cc (`render.rs:23`), so re-rastering one
tile reproduces exactly that tile's pixels. A surface Div rasters untiled
(at most 512 px each way, as cc's `max_untiled_layer_size`) is re-rastered
whole: clipping it to a smaller rect would change analytic antialiasing
near the clip, as `rasterize_picture`'s own comment records. Pictures are recorded with a bounding-box hierarchy, so
playback skips ops outside the tile.

**A3. Present with damage (framework).** The presenters copy and upload
only damaged rects:

- softbuffer: `Buffer::present_with_damage`, 0.4.8 is already locked;
- GL: one `TexSubImage2D` per rect;
- a full present only on resize or surface loss.

**A4. Recording reuse.** Per-node paint invalidation marks only the
chunks a change can affect. `InvalidationClass` already classifies every
property, and layout diffs fragments. Unaffected chunks reuse their
recording, as `UseCachedItemIfPossible` does. This removes the full
re-record that dominates the motion sample's update stage at N = 1,000.

**Canvas slots** come with A2. Paint records a slot chunk at each
canvas's content box, with a content version kept outside the document
lifecycle. A content change damages the slot's rect: no relayout, no
re-record, and only the tiles under it are re-rastered.

### Stage B: targeted style sampling (R5)

This stage is independent of A and can land in any CI gap:

- an animation tick re-resolves only the animated nodes and the
  descendants that inherit from them, not every root;
- `start_animation` samples only the new animation;
- starts in one frame batch into one style pass.

This makes N starts linear (the motion sample's
`start_cost_by_animation_count`).

### Stage C: layers

**C1. Layerization by Chromium's compositing reasons.** Chunks are
grouped into layers as `PaintArtifactCompositor` does. A new layer starts
for a direct compositing reason:

- an active transform, opacity or filter animation;
- `will-change: transform` or `opacity`;
- a foreign layer: canvas, video or a GPU surface;
- a composited scroller;
- a View Transitions snapshot;
- overlap with an earlier composited layer.

Each layer is rastered and retained separately, and the compositor
composes the layers with property trees for transform, clip, effect and
scroll.

Div must layerize the way Chromium does, not just somehow. Layerization
changes pixels: raster scale, LCD text and the rounding of composited
transforms. The oracle is Chromium's own layer tree for the same fixture,
from `LayersAsJSON` with compositing reasons. A Div layer tree that
differs from it is a failure, even when the pixels happen to agree.

A transform or opacity change on a layer then updates one property node:
no re-record and no re-raster. Its damage is the layer's old and new
screen rects. Scrolling a composited scroller moves its layer.

**C2. Compositor-thread animation.** The layer tree, property trees and
curves for transform, opacity and filter go to a compositor thread
through `SceneMailbox`. That thread ticks them and presents while the UI
thread is blocked. This closes the release blocker in
`threading-model.md`. The gate is the motion sample's `frame_pacing`
with 50 ms UI-thread blocks: no frame longer than the vsync interval.

### Stage D: View Transitions

This stage builds on C1:

- **Capture:** `startViewTransition` rasters each named element's
  stacking context, and the root, into a snapshot layer.
- **Display:** the `::view-transition-*` pseudo-element tree shows the
  snapshots as replaced content.
- **Animation:** the default CSS animations run as C2 layer animations.

The pseudo-element tree and its styles are engine work outside this RFC.

### Gates (every stage)

- **Exactness is unchanged.** The census and CSS Grid coverage show no
  change at each step (D4).
- **Partial equals full.** A test mode re-rasters every frame in full and
  compares it with the partial frame, byte for byte. It runs in CI over a
  mutation corpus: each census fixture under one change per invalidation
  class, scrolls and animation steps. A4 adds the under-invalidation check:
  a reused chunk must equal a fresh recording.
- **Damage covers every changed pixel.** For A1, the pixels that differ
  between two full frames must lie inside the damage.
- **Cost is measured.** `measure_frame_cost` and `frame_pacing` from
  sample-motion, P7 from sample-canvas, all on one idle host. Target: the
  one-square frame drops from 6.95 ms to under 1 ms (A1–A3); 1,000
  transform animations hold 60 frames/s (B, A4, C1).

## Affected tracks

| Track | What changes |
| --- | --- |
| engine | Owns A1, A2, A4, B, C1, C2 and D in `div-paint`, `div-engine`, `div-compositor` and `div-style` |
| framework | A3 in `div-platform` (damage-aware present). Frame statistics (record, raster, present times and damage area), which the motion sample asked for, as public API in `divui`. `requestAnimationFrame` (decision 4) runs before A's style pass. C2 adds a compositor thread to the event loop |
| canvas session | Builds canvas slots on A2, and GPU canvases as foreign layers on C1 |
| SVG session | None beyond painting into chunks like any other box |
| components | None. Pixels must not change |

There is no public API change before A3's statistics. The damage list in
`SceneSnapshot` is already public and starts carrying real rects in A1.

## Migration

Each step is one PR, measured as described above, and `main` stays green
between them:

1. This RFC (docs).
2. A1, chunked recording and real damage, with the damage-coverage test.
3. B, targeted sampling and linear start. It is independent and can
   land between any two steps.
4. A2, tile re-raster, with the partial-equals-full CI mode.
5. A3 (framework), present with damage.
6. Canvas slots (canvas session), on A2.
7. A4, recording reuse, with the under-invalidation check.
8. C1, layerization against Chromium's layer trees; then C2, the
   compositor thread.
9. D, View Transitions.

Until C1 lands, a translucent GPU canvas over Div content keeps its
current limitation. The canvas RFC records it.
