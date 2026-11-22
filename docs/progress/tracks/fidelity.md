# Chromium fidelity progress

The fidelity lane owns text rasterization and font fidelity (`div-text`,
raster profiles) and paint-level pixel fidelity. Root causes in layout,
style or the cascade go to the engine track.

## Open gaps

Each named gap is pinned by a test that fails if its count changes.

| Gap | Owner | Pinned by | Size | Opened |
| --- | --- | --- | --- | --- |
| `opacity-lcd-grayscale-raster`: Chromium's grayscale raster of LCD-configured text in an opacity group that cc does not let keep LCD text differs from Div's | fidelity (text raster) | `native_opacity_lcd_text.rs`, page `filter-elsewhere` | 1,271 text pixels at 1x, plus 700 in the blurred box | 17 November 2026 |
| `rounded-border-raster-1.25x`: the outer curve of a solid rounded border ring at 1.25x differs from Chromium's by up to 7, and the clipped content next to it follows | fidelity (border raster) | `native_rounded_clip.rs`, page `rrect` | 23 pixels at 1.25x, all in the bottom-left corner | 17 November 2026 |
| `gradient-dither-fractional-scale`: Chromium's dithered gradient blends to 129 on a rounded clip's curve where Div blends to 128 | fidelity (gradient raster) | `native_rounded_clip.rs`, page `gradient-clip` | 36 pixels at 1.25x | 17 November 2026 |

## 22 November 2026: HTML element names (S6a and S6b)

Platform-plan item 0.3 (S6) gives each element the HTML name it was created
with, beside its layout and paint kind. Both changes are prerequisites of
RFC 0004's browser style preset.

- **S6a** (`div-dom`, `div-engine`): `NodeData` keeps the element's
  `LocalName`. Type selectors and the cascade's tag buckets match that name, so
  `div {}` no longer matches `p` or `h1`.
- **S6b** (`divui`): `Element::create` lowercases the tag and keeps it as the
  local name.
  - The element table covers every tag that sample-markdown's sanitizer allows
    (`crates/markdown-tree/src/sanitize.rs`, hast-util-sanitize 5.0.2's
    default schema). Flow elements are block `Div`s and phrasing elements are
    inline `Span`s, as the HTML Standard's rendering section displays them.
  - Any other valid element name creates an inline element that keeps its
    name, as Blink's `HTMLUnknownElement` does. "Valid" is DOM's current rule,
    which Chromium 147 follows (measured below): `_x`, `:x` and `éx` are
    accepted, while `1x`, `_x!` and `a b` are rejected.
  - `rp`, `basefont`, `noembed`, `noframes` and `param` are hidden with no
    other behavior. They are inline elements whose constructor display is
    `none`, as `li`'s is `list-item`.
  - The rest of the standard's hidden elements (`head`, `script`, `style`,
    `template`, `title`, `meta`, `link`, `base`, `area`, `datalist`), plus
    `noscript` and `dialog`, have behavior Div lacks, so they are still
    rejected with `UnknownTag`.
  - `html` and `body` are rejected too. Layout and paint treat every `Html`
    and `Body` node as the document's own (`div-layout/src/block.rs` body
    overflow propagation, `div-paint/src/painter.rs` body outlines), so content
    must not create a second one. S6c builds the root through the engine.

No preset exists yet, so a named element is styled only by author rules.

**Chromium 147 measurements** (pinned Windows Chrome for Testing,
`getComputedStyle` and `document.createElement` through
`tools/qualification/windows_chrome.py`):

- `rp`, `basefont`, `noembed`, `noframes` and `param` compute `display: none`.
- `html` and `body` compute `display: block`.
- `q` computes `display: inline`, and its `::before` and `::after` have
  `content: open-quote` and `close-quote`.
- `createElement` accepts `_x`, `:x`, `éx`, `_`, `:`, `x!` and `a.b-c`, and
  throws `InvalidCharacterError` for `1x`, `_x!`, `x y`, `x/y`, `x>y`, a name
  with NUL and the empty name.

**Named gaps:**

- **`q` quotes.** Chromium draws quotation marks from its user-agent
  `q::before` and `q::after`. Div's `q` is a plain inline element. This needs
  generated `content` with `open-quote` and `close-quote` and a `quotes`
  property. The design note's first part does not list `q`, so the browser
  preset adds it once Div supports them.
- **Of-type pseudo-classes.** `:nth-of-type` and the other of-type
  pseudo-classes are not supported yet (`selector_parser.rs`). When they are,
  "same type" must compare local names, not kinds, or `p` and `h1` (both `Div`
  kind) would count as one type. This came from the engine's review of S6a.

**Verification** (WSL, CI Skia flags, census under the census lock):

- `cargo fmt --all --check`: clean.
- `cargo test --locked --no-fail-fast -p divui -p div-macros -p div-components`
  with `divui/async divui/http divui/testing`: no failures.
- Expanded census and CSS Grid coverage, S6a `6c7bdd9` against S6b's first
  version `0c1ce64`:
  both sides are 16,296 exact, 7,432 different, 0 errors (census) and
  1,473 / 2,575 / 156 (coverage).
  - All 23,728 census pages and 1,051 coverage tests keep the same status, pixel
    count and Div image hash.
  - The census builds its pages through the engine, not `Element::create`
    (`pixel-compare` does not depend on `divui`), so the change cannot reach
    it. The review changes since then touch the element table, plus an
    accessor and a debug assertion on named creation in the engine, none of
    which `pixel-compare` calls.
- **Not measured:** sample-markdown's semantic mode (0/48 pages and 0/428 boxes
  at its last measurement). Its harness pins a Div commit and replaces rejected
  tags with stand-ins. It is measured again with the style preset (S5), which
  its pages need.

## 19 November 2026: border-image tiling as Blink's NinePieceImageGrid

- **Problem:** at 1.25x, 1.5x and 2x, `css-backgrounds/border-image-repeat-005`
  differed from Chromium by 8,684, 22,650 and 20,000 pixels.
  - The test: a 100 px SVG, `border-image: url(svg) 0 fill / 0px space round`,
    in a 275 x 100 box.
  - At 2x Chromium draws one 400 x 200 tile, centered; Div drew two
    200 x 200 tiles 50 px apart, as the test's reference expects. Chromium
    147 fails its own reftest at a device scale.
- **Blink** (147.0.7727.50, `nine_piece_image_grid.cc`,
  `nine_piece_image_painter.cc`):
  - `SetDrawInfoMiddle` scales the middle piece's width as the top edge (else
    the bottom edge) and its height as the left edge (else the right). An edge
    counts when its slice and width are positive (`Edge::IsDrawable`) and scales
    by width over slice. Without one the scale is `EffectiveZoom()`.
  - The painter already sizes a generated or SVG image at that zoom
    (`StyleImage::ImageSize`), so its middle tile is zoomed twice. Its FIXME
    says border-image is broken with full page zooming. A raster image's size is
    its pixels, so its middle tile is not.
  - `ComputeTileParameters`: `space` with no whole tile has no spacing, and the
    piece is skipped (`CalculateSpaceNeeded`). `repeat` centers one tile (phase
    `(dst - src) / 2`), not the row of tiles.
- **Change** (`crates/div-paint/src/painter.rs`):
  - `border_image_middle_scale` and `border_image_tile_size` give the middle
    piece Blink's scale, with the zoom for generated and SVG sources and 1 for
    raster ones.
  - `border_image_axis_tiles` skips a `space` piece with no whole tile and
    centers one `repeat` tile.
  - Div used scale 1 for the middle (or the middle's own stretch ratio on the
    other axis), drew one centered tile for `space`, and centered the row for
    `repeat` (different when the count is even).
- **Census and coverage** (WSL, 16 jobs, under the census lock), base main
  `98cffc2` against the change at `fa0f034`:
  - census: 16,296 → 16,297 exact, 7,432 → 7,431 different, 0 errors on both;
  - coverage: 1,473 exact, 2,575 different, 156 errors on both, and no Div
    image changed.

  Three Div images changed, all `border-image-repeat-005`: 2x 20,000 → 0,
  1.25x 8,684 → 1,306 and 1.5x 22,650 → 300. No other page changed.
- **Remaining:** `border-image-repeat-005` at 1.25x and 1.5x. Chromium pixel
  snaps the border image area in device pixels (`ToPixelSnappedRect`); at 1.5x
  its tile spans device 124–348 crisp, Div's 123.75–348.75 with blended edges.
  Owner: engine (RFC 0011 stage 2).
- **Verification:** `cargo test --locked -p div-paint --lib` (WSL, CI Skia
  flags): 280 passed, including
  `border_image_middle_scales_as_blink_nine_piece_image_grid` and the updated
  `border_image_axis_tiling_covers_stretch_repeat_round_and_space`. The census
  pair as above, compared per page.

### RFC 0011 stage 2 gaps measured with it

Measured on the engine lane's real-zoom census at `fadf26f` (Chromium 147
real zoom). Not claimed by the fidelity lane.

- **Double border stripes** (`background-attachment-local/attachment-local-clipping-color-6-ref`
  and `-image-6-ref`, exact at 1x since #82): 8,438 and 25,841 pixels at
  1.25x, 5,058 and 21,850 at 1.5x, 8,027 and 8,033 at 2x, all on the stripes
  of a `10px double` border on a 50%-radius box.
  - Blink divides a double border into stripes in whole device pixels: outer
    `(w + 1) / 3`, inner from `(2w + 1) / 3`. At 2x (w = 20) that is 7, 6, 7.
  - Div's rounded double border (`line_width = (bt / 3.0).round()` in
    `painter.rs`) divides in CSS px and scales: 6, 8, 6 at 2x and 4.5, 6, 4.5
    at 1.5x (Chromium 5, 5, 5). At 1.25x Chromium also paints the 12.5 px
    border 12 px wide.
- **Collapsed borders in fragmented tables**
  (`css-break/table/table-collapsed-borders-paint-*`, 8 pages; exact at 1x):
  770–1,260 pixels at 2x, 4,500–8,555 at 1.25x and 1.5x.
  - At 2x the edges are crisp and one device pixel apart (Div 272 and 662,
    Chromium 271 and 661): the tables sit in columns at half CSS pixels
    (column width 92.5).
  - `collapsed_table_painter.rs` snaps in CSS px before the device scale
    (`snap_size`, `SnappedRect`); Chromium snaps `LayoutUnit`s that are device
    pixels under zoom for DSF.

## 19 November 2026: the Windows web-font fixture (RFC 0012)

- **What:** step 3 of RFC 0012's migration.
  - `tools/qualification/capture_webfont_oracle.py --platform windows`
    captures `webfont-fontations-v1` with the pinned Windows
    Chrome-for-Testing 147.0.7727.50 at 1x and real-zoom 2x, through
    `windows_chrome.py`. That refuses to capture unless the system matches
    the oracle pin: ClearType on, RGB order, no per-display ClearType tuning.
  - The frames are `chromium-windows-1.png` and `chromium-windows-2.png`,
    with `manifest-windows.json`. The Linux files are unchanged.
  - `crates/divui/tests/native_webfont_fontations.rs` now also runs on
    Windows, building the page with
    `RasterConfiguration::chromium_windows_lcd()` and pinning each row that
    is not exact.
  - No Div code changes.
- **Frames:** a second capture with `--check` reproduces both. They are
  byte-identical to the frames behind RFC 0012's measurement.
- **Result (Windows, main `992d4d6`, CI's Windows Skia flags):**
  - 1x: all 6 rows exact. This is the gap RFC 0012 closes for apps once
    the framework's Windows default lands; it was 11,750 pixels with
    `RasterConfiguration::default()`.
  - 2x: no row exact; 3,969, 4,751, 4,622, 5,742, 4,542 and 5,735 pixels.
- **Cause at 2x:** in all 6 rows the glyph raster is identical and the row
  is offset by one whole device pixel. Div's ink is 1 device px higher for
  Geist Mono and Geist 650, and lower for Inter at both sizes. A one-pixel
  shift leaves 0 differing pixels in every row. This is the same line-box
  offset as on Linux. Owner: engine (device-pixel layout, RFC 0011), not
  rasterization.
  - Unlike Linux, Inter 13px on Windows is exact after the shift.
- **Verification:**
  - `python tools/qualification/capture_webfont_oracle.py --platform
    windows --chrome out/chrome-win/win-147.0.7727.50/chrome-win64/chrome.exe
    --check` (Windows): the recapture matches.
  - `cargo test --locked -p divui --features divui/testing --test
    native_webfont_fontations` (Windows, MSVC, CI Skia flags): passes with
    the 6 listed 2x gaps.
  - The same test on Linux (WSL, CI Skia flags) passes with its 14 gaps, unchanged.

## 17 November 2026: content and backgrounds under rounded clips

- **Problem:** eight census rows at 1x differ from Chromium on the
  anti-aliased edge of a rounded clip. This change takes the three whose
  cause it found: `css-backgrounds/border-radius-clip-001`,
  `background-clip-padding-box-with-border-radius` and
  `background-rounded-image-clip-001`. The other five are listed under
  remaining rows below.
- **Blink's paths** (147.0.7727.50, `box_painter_base.cc`,
  `rounded_inner_rect_clipper.cc`, `graphics_context.cc`):
  - A background clipped to the border box takes the fast path,
    `PaintFastBottomLayer`: one `drawRRect` (or `DrawImageRRect`).
  - A background clipped to the padding or content box takes the slow
    path: `RoundedInnerRectClipper` sets one anti-aliased rounded clip, then
    `FillRect` of the snapped border box (or `DrawTiledBackground` for an
    image layer, so a gradient keeps its shader, which Chromium dithers).
  - Content under a rounded `overflow` clip draws through one anti-aliased
    `ClipRRect`.
- **Change** (`div-paint`, `painter.rs`):
  1. Rounded overflow clips use `clip_rrect` with anti-aliasing. The packed
     coverage mask (a save layer with `DstIn`, one coverage quantum above
     the shape) is gone, with `packed_rounded_clip_mask`,
     `rounded_overflow_content_mask` and `fragment_subtree_has_text`.
  2. A background color whose `background-clip` is the padding or content
     box is drawn as Blink's slow path: an anti-aliased `clip_rrect` of the
     clip box, then the border box rectangle. Blink decides on the
     specified clip (`is_border_fill`), not the clip a local background of
     a scroll container ends up with: that background is clipped to the
     padding box but keeps the direct rounded-rect fill.
  3. A constant gradient (`linear-gradient(green, green)`) under a border
     radius stays a gradient instead of becoming a solid color, so it is
     drawn through the gradient shader like Chromium's.
- **Native test:** `crates/divui/tests/native_rounded_clip.rs` with fixture
  `tests/fidelity/fixtures/rounded-clip-v1`: six pages at scales 1, 1.25,
  1.5 and 2 from pinned Linux Chrome 147 (real zoom), captured by
  `tools/qualification/capture_rounded_clip_oracle.py`. Differing pixels,
  before → after at 1x and 1.25x, after only at 1.5x and 2x (main was not
  measured there):

  | Page | 1x | 1.25x | 1.5x | 2x |
  | --- | --- | --- | --- | --- |
  | `circle`: green block overflowing a circular clip | 307 → 0 | 0 | 0 | 0 |
  | `circle-img`: green `<img>` in the same clip | 130 → 0 | 0 | 0 | 0 |
  | `rrect`: rounded overflow clip inside a solid border | 108 → 0 | 144 → 23 | 0 | 0 |
  | `padding-clip`: background color clipped to a rounded padding box | 42 → 0 | 456 → 457 | 0 | 0 |
  | `gradient-clip`: solid gradient clipped to a rounded content box | 18 → 0 | 26 → 36 | 0 | 0 |
  | `local-hidden`: local background of a rounded scroll container under a translucent border | 0 | 1,161 | 0 | 0 |

  `local-hidden` was added after the first census run of this change lost
  it (see below), so its row shows only this change. Keying the slow path
  on Div's effective clip instead of the specified one gives 39, 1,161, 61
  and 71 differing pixels.

  The remaining frames are pinned as gaps:
  - `rrect` at 1.25x: `rounded-border-raster-1.25x` (Open gaps).
  - `padding-clip` at 1.25x: the padding box's edges fall on device 37.5
    and 162.5. Chromium ends them on whole device pixels; Div lays out in
    CSS px. Owner: engine (device-pixel layout, RFC 0011). The one added
    pixel lies on that fractional edge.
  - `gradient-clip` at 1.25x: `gradient-dither-fractional-scale` (Open
    gaps). The solid-color shortcut hid the dither difference at this scale
    and, at 1x, caused the 18 differing pixels.
  - `local-hidden` at 1.25x: the 10px border and the box's 10px offset fall
    on half device pixels. Owner: engine (device-pixel layout, RFC 0011).
- **Census and coverage** (WSL, 16 jobs, under the census lock), base
  main `5231c6c` against the change at `afbe7eb`:
  - census: 16,287 → 16,296 exact, 7,441 → 7,432 different, 0 errors on
    both sides;
  - coverage: 1,473 exact, 2,575 different, 156 errors on both sides, and
    no Div image changed.

  No page was lost. Nine became exact: `border-radius-clip-001` and
  `background-rounded-image-clip-001` at 1x, 1.5x and 2x,
  `background-clip-padding-box-with-border-radius` at 1x, and the
  `attachment-local-clipping-color-6-ref` and `-image-6-ref` pages at 1x.
  18 Div images changed. 15 differ less, and three at 1.25x differ more:
  `background-rounded-image-clip-001` 26 → 44 (the gradient dither gap),
  `attachment-local-clipping-color-6-ref` 8,436 → 8,438 and
  `background-clip-padding-box-with-border-radius` 1,353 → 1,354.
  The first census run of this change, at `b271bc1`, lost
  `background-attachment-local-hidden` at 1x (39 pixels) and 1.5x (61);
  the second commit fixes that, and the `local-hidden` page pins it.
- **Remaining rounded-clip rows** (not diagnosed): `rounded-clipped-border`
  and its reference, `border-radius-clipping-with-transform-001` and its
  reference, and `border-radius-clip-002`.
- **Verification:**
  - `cargo test --locked -p divui --features "divui/async divui/http
    divui/testing" --test native_rounded_clip` (WSL, CI Skia flags) passes.
  - `python3 tools/qualification/capture_rounded_clip_oracle.py --chrome
    <pinned Linux chrome> --check` matches the fixture.
  - The census pair under `census_lock.py`, with per-page comparison of the
    two `expanded-summary.json` and `coverage-summary.json`.
  - `cargo test -p div-paint` and the full divui suite were not run
    locally (another lane's census held the machine); CI runs them.

## 17 November 2026: LCD text inside opacity groups, where cc keeps it

- **Problem:** the components' disabled buttons (`disabled:opacity-50`)
  differ from Chromium by 502 (light) and 489 (dark) pixels at 1x.
  Chromium keeps the label as LCD text inside the 50% group, but Div drew
  it in grayscale.
- **cc's condition** (147.0.7727.50, `cc/paint/paint_op_buffer.cc`
  `PaintOpBuffer::Playback`): cc decides once per buffer, which is one cc
  layer's display list. It preserves LCD text when:
  - the layer's raster can use LCD text (pixel geometry known),
  - the buffer draws text, and
  - no op in it, including ops in nested records, is a `SaveLayerOp`.

  When it preserves, every `SaveLayerAlphaOp` in the layer opens with
  `kPreserveLCDText | kInitWithPrevious`. Blink emits a `SaveLayerOp` for
  a filter, a blend mode or a mask (`paint_chunks_to_cc_layer.cc`).
  Opacity alone emits a `SaveLayerAlphaOp`, and backdrop-filter emits a
  `SaveLayerFiltersOp`; neither prevents preservation. So one filter
  anywhere in the layer turns preservation off for every opacity group in
  that layer.
- **Change** (`div-paint`):
  - `paint_record.rs`: an `LcdTextLayerScope` per recording canvas. These
    are the document picture (`render.rs`) and the viewport's scrolling
    contents (`painter.rs`).
  - A `will-change: transform` subtree, which Div rasters on a non-LCD
    surface, gets a scope without LCD text.
  - `layer_lcd_text_contents` scans the layer's fragments for text and
    for filters or masks.
  - CSS opacity (`save_layer_opacity_f`) opens with both flags when the
    scope allows it. Emulation alpha layers are unchanged.
- **Second cause:** when a picture is finished, SkRecord's optimizer folds
  SaveLayer → one draw → Restore into the draw. At 1x, a group holding one
  text run therefore had no layer at all. cc never folds a
  `SaveLayerAlphaOp` into text (`paint_op_buffer_iterator.cc`). Div now
  draws text inside Save + clip to the local clip bounds + Restore, which
  the optimizer keeps.
- **Native test:** `crates/divui/tests/native_opacity_lcd_text.rs` with
  fixture `tests/fidelity/fixtures/opacity-lcd-text-v1`. The fixture holds
  four pages at scales 1, 1.25, 1.5 and 2 from pinned Linux Chrome 147
  (real zoom), captured by
  `tools/qualification/capture_opacity_lcd_oracle.py`:
  - text in a group over the page: 1,329 → 0 differing pixels at 1x;
  - a disabled-button-like box: 557 → 0 at 1x;
  - the same group with a blurred box elsewhere in the layer (not
    eligible): Chromium and Div now both draw the text in grayscale;
  - the text without a group: the reference page.

  Above 1x, every page's LCD-colored pixel count equals Chromium's. The
  remaining differences there match the no-group page's. They are
  device-pixel layout (owner: engine, RFC 0011).
- **Remaining gap** (`opacity-lcd-grayscale-raster`, under Open gaps):
  in the not-eligible page, Chromium's grayscale raster
  of LCD-configured text differs from Div's: 1,271 text pixels at 1x, plus
  700 in the blurred box. Owner: fidelity (follow-up).
- **Census and coverage** (WSL, 16 jobs, under the census lock), base
  main `7bbddf8` against the change at `929ae81`, with the real-zoom
  fidelity data `c82502d`:
  - census: 16,287 exact, 7,441 different, 0 errors on both sides;
  - coverage: 1,459 exact, 2,589 different, 156 errors on both sides.

  No page was lost or gained, and no Div image changed. Most census pages
  draw aliased Ahem, and only real-font pages use `chromium-linux-lcd`. No
  census page exercises this change, so the native test is its evidence.
- **Expected downstream:** once build-with-div/div#60 merges, the
  components lane re-measures `button-disabled-light` and `-dark` (502
  and 489 pixels at 1x). They are expected to drop to the blend-rounding
  remainder, about 56 and 62 pixels, each off by at most 2 per channel. That measurement
  is still to come.
- **Verification:**
  - `cargo test --locked -p divui --features "divui/async divui/http
    divui/testing" --test native_opacity_lcd_text` (WSL, CI Skia flags)
    passes.
  - `cargo test --locked -p div-paint` and the divui suite pass.
  - `python3 tools/qualification/capture_opacity_lcd_oracle.py --chrome
    <pinned Linux chrome> --check` matches the fixture (captured twice).
  - The census pair: `census_lock.py run --wait -- div-census.sh <sha>`,
    with per-page comparison of the two `expanded-summary.json` and
    `coverage-summary.json`.

## 16 November 2026: the site showcase at real zoom

- **What:** `site/showcase/capture.py` rerun on the real-zoom launcher
  (RFC 0010), which recaptures the 2x Chromium frame
  `site/src/assets/showcase/example-chromium-light.png`. No Div code
  changes.
- **Result:** Chromium now places the edges of the boxes sized by their
  text (the two buttons and the Live badge) one device pixel away. The
  card's borders and the colors are unchanged. Div's frame differs from
  the new one in 9,410 of 387,200 pixels (7,659 against the emulated
  frame). The home page now says only the colors and the card's borders
  match, and that text, and the buttons sized by it, can land a pixel off.
  Owner of the remaining gap: engine (device-pixel layout, RFC 0011).
- **Verification:** `python3 site/showcase/capture.py` (WSL, pinned Linux
  Chrome-for-Testing 147.0.7727.50) run twice: both frames are
  byte-identical (`81ca688c…`).

## 16 November 2026: text oracle values at real zoom on Linux and Windows

- **What:** `tools/qualification/probe_windows_text_oracle.py` gains
  `--platform linux`. It runs the same scenarios on pinned Linux
  Chrome-for-Testing 147.0.7727.50 (WSL) with the census launcher, and
  draws aliased Ahem from `ahem_noaa.conf` as the Linux oracle does. Both
  platforms write a schema 2 receipt that records each scale's
  `device_scale_method`. The receipts are `windows-text-oracle-v2.json`
  and `linux-text-oracle-v2.json` in build-with-div/div-fidelity-data at
  `1b86da8`, pinned by `fidelity-data.lock`. `windows-text-oracle-v1.json`
  stays as history.
- **Scale 1:** both platforms reproduce every value that
  `native_font_units.rs` and `native_line_height.rs` pin.
- **Above scale 1:** real-zoom Chromium lays out font-relative lengths and
  the Ahem word in device pixels, so its values move with the scale. For
  example, at 1.25x 2.5ch is 46 instead of 47.5, and the Ahem word's y is
  485.8 (Windows) or 487.4 (Linux) instead of 487. Div keeps its scale-1
  values at every scale. The tests no longer claim a Chromium match there:
  they pin Div's value and list Chromium's as a named gap
  (`crates/divui/tests/support/real_zoom.rs`). There are 28, 32 and 3 gaps
  on Windows (font units, line height, atomic word) and 27, 28 and 2 on
  Linux. A gap that starts matching, or that is never checked, fails.
  Owner: engine (device-pixel layout, RFC 0011).
- **Inventory:** RFC 0010 gains an addendum covering the missed Linux
  observations and about 35 other tests that pin a Chromium value above
  scale 1, by owner.
- **Verification:**
  - `cargo test --locked -p divui --features "divui/async divui/http
    divui/testing" --test native_font_units --test native_line_height`
    in WSL: 6 passed. Windows: CI.
  - `python tools/qualification/probe_windows_text_oracle.py --chrome
    <pinned Windows chrome.exe> --output <receipt>`: run twice, identical
    values.
  - The same with `--platform linux` in WSL: reproduces every scale-1
    value above.
  - `python tools/fidelity_data.py fetch --profile all`, then `verify`:
    30,517 files match `1b86da8`.

## 16 November 2026: web-font fixtures recaptured at real zoom

- **What:** `tests/fidelity/fixtures/webfont-fontations-v1` recaptured with
  `tools/qualification/capture_webfont_oracle.py` on the real-zoom
  launcher (RFC 0010), and the expected gaps in
  `crates/divui/tests/native_webfont_fontations.rs` set to the new counts.
  No Div code changes.
- **Frames:** the 1x frame is byte-identical to the emulated capture; the
  1.25, 1.5 and 2x frames changed. A second capture with `--check`
  reproduces all four.
- **Result:** 10 of 24 rows exact (21 of 24 against the emulated
  frames). At 1x all 6 are exact, at 1.25 2, at 1.5 2 and at 2x none.
- **Cause:** in 11 of the 14 rows that differ, the glyph raster is
  identical and the row is offset by one whole device pixel: Div's ink is
  1 device px higher (Geist Mono, Geist 650) or lower (Inter 16px), and a
  one-pixel shift leaves 0 differing pixels. Real-zoom Blink lays line boxes
  out in device pixels; Div lays out in CSS px and scales. Owner: engine
  (device-pixel layout, RFC 0011), not rasterization. The other 3 rows are
  the earlier Inter 13px gap (1,339, 2,423 and 4,153 pixels after the
  one-pixel shift).
- **Verification:** `python3 tools/qualification/capture_webfont_oracle.py
  --chrome <pinned Chromium 147.0.7727.50> --check`; `cargo test --locked
  -p divui --features "divui/async divui/http divui/testing" --test
  native_webfont_fontations` (Linux, WSL): passes with the 14 listed gaps.

## 16 November 2026: Chromium oracles at real zoom above scale 1 (RFC 0010)

- **What:** the shared launcher `render_chrome`
  (`tools/accountability/run_all_pixel_comparisons.py`) and
  `tools/qualification/windows_chrome.py` capture every scale other than 1
  with `--force-device-scale-factor` and a viewport-only
  `Emulation.setDeviceMetricsOverride` (`deviceScaleFactor: 0`), as
  RFC 0010 specifies. Scale 1 is unchanged. The census oracle identity
  (schema 2) and the summary's `chromium` block record
  `device_scale_method` (`none` or `forced`), so no emulated frame can
  satisfy a real-zoom lookup.
- **Reproduction:** 30 census ids that differ between the two methods, at
  all four profiles: every 1x frame (30) is byte-identical to the earlier
  census, and every non-1x frame that the 16 November forced-scale
  measurement captured (69) matches it.
- **Census and CSS Grid coverage at the new oracle** (Linux, 16 jobs,
  under the census lock, launcher `19269a7` on main `154d0a7`), next to
  the emulated run of the same Div output (`a9e9a46`); Div's frames are
  byte-identical in both runs:

  | Profile | Census exact, emulated → real zoom | Coverage exact, emulated → real zoom |
  | --- | --- | --- |
  | legacy-800x600@1 | 5,888 → 5,888 | 572 → 572 |
  | mobile-375x667@2 | 5,767 → 5,153 | 567 → 487 |
  | desktop-1280x720@1.25 | 5,344 → 2,183 | 529 → 183 |
  | desktop-1920x1080@1.5 | 5,540 → 3,063 | 559 → 217 |
  | Total | 22,539 → 16,287 of 23,728 | 2,227 → 1,459 of 4,204 |

  Every 1x Chromium frame is byte-identical to the emulated run (5,932
  census; 1,012 coverage, with the same 39 errors in both). This is the
  new baseline for "no regressions" (D4). The drop is the oracle, not
  Div: Div lays out in CSS px and replays paint at the scale, which
  matches emulation. Closing it is layout in device pixels (RFC 0011,
  engine) and device-pixel edge snapping.
- **Evidence:** per-profile, per-area and per-row results in
  [`archive/docs/progress/evidence/real-zoom-baseline-2026-11-16/`](https://github.com/build-with-div/div-fidelity-data/tree/c82502def3b921d2d46a6b2b4b2ca60ebaa6f843/archive/docs/progress/evidence/real-zoom-baseline-2026-11-16)
  in build-with-div/div-fidelity-data at `c82502d`, pinned by
  `fidelity-data.lock`.
- **Verification:** `python3 -m unittest
  tools.accountability.test_chromium_capture
  tools.qualification.test_windows_chrome
  tools.qualification.test_renderer_matrix` (Linux).
- **Not affected:** the canvas (#48) and SVG (#49) lanes capture
  their oracles at scale 1 only, so they need no recapture.
- **Remaining:** each owner's non-1x oracles, per the RFC 0010 inventory.
  The fidelity lane's own: the fractional-edges probe (#53, recaptured on
  its rebase), `webfont-fontations-v1` (done, above), the site showcase PNG
  and the Windows text values.

## 15 November 2026: root causes of the remaining census differences

- **What:** a classification of all 1,209 non-exact census rows (703 tests)
  by root cause and owner, with the measured counts and the reviewed-sample
  estimates kept apart:
  [fidelity-root-causes-2026-11-15.md](../fidelity-root-causes-2026-11-15.md).
  Docs only; no code or gate changed.
- **Input:** evidence data `3646fb28`, the census receipt at head
  `18fffb9c` (22,519 of 23,728 exact) and its per-row pictures.
- **Verification:** `python3
  docs/progress/fidelity-root-causes-2026-11-15/analyze.py --run <run>
  --output root-causes.json` (Windows) reproduces every row's mismatch
  count from the census summary and the reviewed sample.
- **Top causes (estimated):** sub-pixel geometry of axis-aligned edges at
  fractional scales (about 617 rows), image and background tiles (181),
  diagonal and curved edges (117), layout (116, engine), native controls
  (92), text (67).
- **Remaining:** every cause above. The sub-pixel geometry cause is a
  hypothesis about where rounding happens, to be tested before any fix.

## 15 November 2026: table part backgrounds keep fractional cell edges

- **What:** Blink paints a table part's background (column, row, section)
  into each cell with the cell as the painting area
  (`TableCellPainter::PaintBackgroundForTablePart`): the part's generated
  tile, clipped to the cell, is drawn once with antialiasing. Div clipped
  the draw to the cell with a hard clip, which snapped fractional device
  cell edges. Div now uses its existing border-box generated-tile path for
  these cells (`crates/div-paint/src/painter.rs`, `paint_background_layers`).
  Canvas backgrounds are unchanged. Owner: paint (fidelity lane).
- **Census (full expanded suite, a8 build, WSL):** main `dd8ba04` 22,520 of
  23,728 exact; this change 22,539 (+19), 0 losses, no other row changed.
  The gains are the nine `css_break` table col/row/section paint tests at
  1.25 and at 1.5, and
  `css_backgrounds/background-image-table-cells-straddling-no-repeat` at
  1.25. CSS Grid coverage (4,204 rows) is unchanged row for row.
- **Verification:** `cargo test --locked -p div-paint`
  (`table_part_gradient_keeps_fractional_cell_edge_coverage` fails without
  the change); `python tools/qualification/generate_renderer_contract.py
  --check`; the census under `census_lock.py`.
- **Remaining:** collapsed-border and caption edges in the same table tests,
  and the rest of the snapped-edge cause.

## 15 November 2026: Linux web fonts rasterize from skrifa outlines

- **What:** Chromium on Linux creates every `@font-face` typeface with
  `SkTypeface_Make_Fontations` (`web_font_typeface_factory.cc`). Div now
  paints anti-aliased runs of registered faces on Linux from a custom
  typeface of skrifa outlines at the physical size, hinted as Skia's
  Fontations scaler maps the resolved hinting
  (`crates/div-text/src/shaping/web_font_outlines.rs`). The deterministic
  test collection stands in for the census's Fontconfig system fonts and
  keeps FreeType; the aliased Ahem policies are unchanged. Owner: text
  (fidelity lane).
- **Typeface cache:** keyed by the typeface id, variation coordinates,
  physical size, hinting, LCD mask and force-autohint flag; least recently
  used, 32 entries; an entry restarts past 4,096 glyphs.
- **Evidence:** pinned Chromium 147 frames of Geist Mono 400, Inter 400 and
  Geist 650 at 13px and 16px, at scales 1, 1.25, 1.5 and 2
  (`tests/fidelity/fixtures/webfont-fontations-v1`, captured by
  `tools/qualification/capture_webfont_oracle.py`). 21 of 24 rows are exact
  (0 of 24 on the FreeType path).
- **Census:** full census under `census_lock.py` (WSL, a8 build) at main
  8871c13 and at 863a56f. Expanded suite: 22,539 of 23,728 exact on both.
  Coverage matrix: 2,227 exact, 1,821 different and 156 errors of 4,204 on
  both. Every row's status, pixel count, diff signature and Div RGBA hash
  is identical; only build identity (commit, binary, cache identity)
  differs. `pixel-compare` builds every census profile on
  `deterministic_test()` and Fontconfig system fonts and registers no web
  fonts, so this is a no-regression check, not evidence for the change.
- **Verification:** `cargo test --locked -p divui --features
  "divui/async divui/http divui/testing" --test native_webfont_fontations`
  (Linux); `cargo test --locked -p div-text`
  (`ahem_policies_are_unchanged_by_web_font_provenance` and the cache
  tests); `python tools/qualification/generate_renderer_contract.py
  --check`; the census under `census_lock.py`.
- **Remaining:** Inter 13px at 1.25, 1.5 and 2x (2,729, 4,227 and 5,889
  pixels): Chromium's baseline sits 1-2 device pixels lower, on a whole
  pixel; line-box metrics or layout, not rasterization. Monochrome web
  fonts keep FreeType; vertical web-font text and the div-components pages
  were not measured.

## 16 November 2026: fractional box edges against the real-zoom oracle

- **What:** a native test that compares box edges at fractional device
  positions with pinned Chromium, pixel for pixel. It adds no renderer
  change. Five 120x80 pages: a single box, two abutting boxes, a block
  split across two columns, a table caption split across two columns, and
  an absolutely positioned box whose child overflows into the next column
  (`tests/fidelity/fixtures/fractional-edges-v1/pages.txt`). They are
  captured at scales 1, 1.25, 1.5 and 2 by
  `tools/qualification/capture_fractional_edges_oracle.py` on the census
  launcher, which uses real zoom above scale 1 (RFC 0010); `--check`
  recaptures them and requires identical files.
- **What Chromium does:** at real zoom every edge lands on a whole device
  pixel. Each frame holds only white and green: no antialiased edge and no
  seam between abutting boxes or columns. The edge at CSS x 45 ends at
  device 56 at 1.25x and 68 at 1.5x, and the top edge at CSS y 21 starts at
  26 and 32. (Under the earlier CDP-emulated oracle the same edges were
  antialiased, with a seam of 0.1875 white at 1.25x; those frames are no
  longer the reference.)
- **Result on main:** all five pages are exact at 1x and 2x and differ at
  1.25x and 1.5x: `single` 113 and 135 pixels, `abutting` 224 and 209,
  `multicol` 224 and 209, `caption` 169 and 172, `overflow` 194 and 172,
  pinned as gaps with their own counts. In `single`, `abutting` and
  `multicol` every differing pixel is one of Div's antialiased edge pixels:
  Div lays out in CSS px and rasterizes the fractional device edge with
  partial coverage. In `caption` and `overflow`, 31 (1.25x) and 37 (1.5x)
  more pixels are fully painted in Div and white in Chromium: Div paints
  the box whole under a fragmentainer clip snapped outward to device
  pixels.
- **Owner:** layout in device pixels and device-pixel edge snapping
  (engine, RFC 0011). The fragment-rect paint change measured against the
  emulated oracle does not help here (0 gains and 14 regressions in the
  real-zoom census) and was dropped.
- **Verification:** `python3
  tools/qualification/capture_fractional_edges_oracle.py --chrome <pinned
  Chromium 147.0.7727.50> --check`; `cargo test --locked -p divui
  --features divui/testing --test native_fractional_edges` (Linux).
