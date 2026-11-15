# RFC 0009: Inline SVG painted Chromium's way

Status: proposed, 15 November 2026. Owner: the SVG lane, working in
engine-owned crates through integration requests. Consumers: the
framework track (`view!`, `Element`), the components track (icons), the
canvas lane (`Path2D` from path data) and the samples: charts, markdown,
motion and git. Implements platform decision 3 (Q9) and Tier 1 item 1.4
(V1). Plans Tier 2 item 2.3 (V2, V3, V5). Depends on Tier 0 items 0.1
(cascade), 0.3 (element identity) and 0.5 (custom properties).

## Problem

Div has no inline SVG. Measured at `main` `9168ac9`:

- **No SVG elements.** `ElementTag` is a flat enum with no namespace
  (`crates/div-dom/src/tree.rs:133`). `svg` is one tag kind (`:162`).
  `Element::create` lowercases the name and rejects every other SVG name
  (`path`, `g`, `circle`, `clipPath` and the rest) with
  `Error::UnknownTag` (`crates/divui/src/element.rs:1206-1249`). `view!`
  compiles `<path>` and panics at run time
  (`crates/div-macros/src/view.rs:460`).
- **Attributes lose their case and meaning.** Names are lowercased on
  set and get (`tree.rs:820`, `:826`), so `viewBox` is stored as
  `viewbox`. No presentation attribute maps to style, and `view!` has no
  syntax for `xlink:href`.
- **`<svg width=200 height=100>` lays out 300×0.** Intrinsic sizing
  treats `svg` as replaced with the 300×150 fallback
  (`crates/div-layout/src/intrinsic_sizing.rs:3506`). Block layout lays
  it out as an empty block, because it has no replaced metadata
  (`block.rs:1653`).
- **No SVG properties.** None of `fill`, `stroke`, the `stroke-*`
  longhands, `fill-rule`, `paint-order`, `vector-effect` or `stop-color`
  is in `crates/div-style/property-schema.csv`. `var()` is rejected
  (`declaration_block.rs:89`).
- **Static SVG images use a second painter.** `parse_static_svg`
  (`crates/div-paint/src/image_resource.rs:2329`) scans strings rather
  than parsing XML. It paints filled and stroked `rect` and the bounding
  rectangles of closed axis-aligned `path` data, and ignores everything
  else. So no Lucide icon paints: they are stroked paths in
  `currentColor` (`docs/progress/tracks/components.md:483`).
- **Hit testing and accessibility know no SVG geometry.** Hit testing is
  by rectangle (`crates/div-engine/src/lib.rs:2733`). `svg` maps to
  `Role::SvgRoot` and nothing else (`accessibility.rs:892`).

What exists to build on: `foreignObject` as a fixed viewport
(`block.rs:1603`), Skia paths, dashes, fill rules and matrix
concatenation already used for CSS painting
(`crates/div-paint/src/painter.rs`), and the property generator
(`tools/style/generate_properties.py`).

Who needs it: all 70 of 70 shadcn chart examples (root, `g` and `path`
in 70, text in 60, `line` in 44, arcs in 34, `circle` in 28, dashes in
11, `clipPath` in 10, gradients in 2; custom properties in SVG paint in
69). Also Lucide icons (sample-git gap 17, components C4), Markdown
badges and KaTeX paths, and motion (GSAP DrawSVG, lottie). The charts
sample measured default Skia painting against Chromium's SVG output: at
scale 1, up to 1,443 pixels per chart differ (all by 1/255); at scale 2,
3,480 pixels of the line chart differ, and the bars differ by up to
24/255. So SVG must go through Chromium's paint
setup, as Div's CSS painting already does.

## Licence boundary

Div stays permissively licensed. Nobody on this work opens, reads or
paraphrases LGPL or GPL source, including from memory. Many of Blink's
SVG files descend from KSVG2 and WebKit and are LGPL. Their licence
headers were checked at tag `147.0.7727.50`, header only, by a script
that prints the class and the first copyright line:

| Header | Files (Blink, Chromium 147) |
| --- | --- |
| BSD-style (may be ported) | `paint/svg_shape_painter`, `svg_object_painter`, `svg_root_painter`, `svg_container_painter`, `svg_model_object_painter`, `svg_mask_painter`, `svg_foreign_object_painter`, `clip_path_clipper`, `text_fragment_painter`; `platform/geometry/path`, `path_builder`, `stroke_data`; `platform/graphics/graphics_context`; `layout/svg/layout_svg_rect`, `layout_svg_ellipse`, `layout_svg_model_object`, `layout_svg_text`, `svg_text_layout_algorithm`, `svg_text_layout_attributes_builder`, `svg_text_query`, `svg_inline_node_data.h`, `layout_svg_resource_paint_server`, `layout_svg_foreign_object`, `svg_content_container`, `transformed_hit_test_location`, `svg_layout_info.h`; `svg/svg_resource`, `svg_tree_scope_resources`, `svg_geometry_element`, `svg/graphics/svg_image`; `style/style_svg_resource`; `css/css_properties.json5`, `css/svg.css` |
| LGPL (not read) | `layout/svg/layout_svg_shape`, `layout_svg_path`, `layout_svg_root`, `layout_svg_container`, `layout_svg_transformable_container`, `layout_svg_viewport_container`, `layout_svg_hidden_container`, `svg_layout_support`, `svg_resources`, `svg_marker_data.h`, every `layout_svg_resource_*` except the paint server, `layout_svg_inline_text`, `layout_svg_text_path`, `layout_svg_image`; `paint/scoped_svg_paint_state`; `svg/svg_path_parser`, `svg_path_builder`, `svg_path_byte_stream*`, `svg_path_utilities`, `svg_path_query`, `svg_parser_utilities`, `svg_length`, `svg_length_context`, `svg_preserve_aspect_ratio`, `svg_fit_to_view_box`, `svg_transform`, `svg_transform_list`, `svg_point_list`, `svg_element`, `svg_svg_element`, `svg_graphics_element`, `svg_use_element`, and the element files for `path`, `rect`, `circle`, `ellipse`, `line`, `poly*`, `text`, `tspan`, `defs`, `symbol`, `gradient`, `clipPath`, `marker`, `a`, `title`; `css/resolver/style_adjuster` |

The rules:

1. **Port only permissive files.** Each porting PR names the file, the
   tag and the header class it checked. A file with no licence header is
   treated as not cleared.
2. **Where the Chromium code is LGPL, write it clean.** The behavior
   comes from the W3C specifications (SVG 2, the SVG 1.1 implementation
   notes in Appendix F, CSS Masking 1, CSS Transforms 1, CSS Fill and
   Stroke 3), from Skia (BSD), and from Chromium 147 observed as a black
   box: pixels, `getBBox`, `getTotalLength`, `getPointAtLength`,
   `getScreenCTM`, `elementFromPoint` and computed styles. These are
   chiefly path parsing and arcs, lengths, `viewBox` and
   `preserveAspectRatio`, shape geometry, hit testing, resources,
   markers and `use`.
3. **A gap the specifications and observation cannot close stays a
   gap.** It is recorded as a fidelity gap with its measurement. It is
   never closed by reading the LGPL source.
4. **Dependencies must be permissive.** For example, XML parsing for
   static images uses `roxmltree` or `quick-xml` (MIT/Apache). Chromium
   itself uses libxml2 (MIT).

The audit script and its full output go into the repository with the
first V1 PR, under `tools/svg/`.

## Proposal

### 1. Elements and identity (joins Tier 0 item 0.3)

- **Namespace and names.** SVG elements are elements in `div-dom`. They
  carry the SVG namespace and their case-sensitive local name. This RFC
  asks item 0.3 (`LocalName`) to include a namespace with HTML, SVG and
  MathML values. `svg` is always in the SVG namespace, and so is every
  element created inside it. Children of `foreignObject` are HTML again,
  as the HTML parser's foreign-content rules give. Unknown SVG names
  create a generic SVG element that renders nothing; they are not an
  error.
- **Attributes.** Names keep their case in the SVG namespace (`viewBox`,
  `preserveAspectRatio`, `gradientTransform`), and `xlink:href` is
  stored in the XLink namespace. Values stay strings. Each element
  parses the attributes it uses into typed values when they change,
  never during paint.
- **Presentation attributes.** SVG 2's list of presentation attributes
  maps to CSS declarations at the presentation-hint level, as in
  Chromium: they lose to every author rule, inline style included. The
  geometry attributes are presentation attributes in SVG 2: `x`, `y`,
  `width`, `height`, `cx`, `cy`, `r`, `rx` and `ry` (`d` comes later).
  Values parse with CSS syntax in SVG's attribute mode, which accepts
  unitless lengths. They can use `var()`, as shadcn's
  `fill="var(--color-desktop)"` does. Chromium's handling is confirmed
  by an oracle fixture before Div relies on it.
- **The SVG layout tree.** Inside an outer `svg`, elements do not create
  CSS boxes. They form an SVG subtree of these kinds:
  - containers: `g`, nested `svg`, `a`, `switch`;
  - shapes;
  - text (V2);
  - hidden containers, which never paint directly: `defs`, `clipPath`,
    `mask`, `marker`, gradients, `pattern`, `symbol`;
  - `foreignObject`, which hosts CSS boxes again. The existing support
    moves under it unchanged.

  This removes Recharts' per-element box layout cost: one element per
  dot, tick and grid line.

### 2. Geometry: a new `div-svg` crate

The licence-clean geometry lives in a new crate, `crates/div-svg`. It
does not depend on layout, paint or the DOM, and it is unit-tested
alone. It contains:

- **Path data.** The full SVG 2 grammar (`M L H V C S Q T A Z` with
  their relative forms), with the specification's error handling: the
  path renders up to the first error. Arcs follow SVG 1.1 F.6: endpoint
  to centre parameterization and radius correction. Chromium 147 draws
  an arc as some number of curve segments. Div picks its approximation
  by measuring it against Chromium (`getPointAtLength` samples and
  pixels), not by reading Chromium's parser.
- **Shapes as paths.**
  - `rect`, with the `rx`/`ry` `auto` rules and clamping of SVG 2
    §10.2. Blink's `layout_svg_rect` (BSD) supplies its fast paths for
    fills and strokes.
  - `circle`, and `ellipse` with `rx`/`ry` `auto`; its fast paths come
    from `layout_svg_ellipse` (BSD).
  - `line`.
  - `polyline` and `polygon`: the `points` grammar, where an odd count
    drops the last coordinate.
- **Lengths.** User units and percentages against the nearest viewport:
  width, height, or `sqrt((w² + h²) / 2)` for non-directional lengths.
  Font-relative units come from the computed font.
- **Viewport transforms.** `viewBox` and `preserveAspectRatio`, using
  SVG 2's equivalent-transform algorithm. Nested `svg` creates a new
  viewport, which clips unless `overflow` is `visible`.
- **Bounding boxes.** Fill, stroke and object bounding boxes. Paint uses
  them for invalidation, V3 for `objectBoundingBox` units, and V4 for
  `getBBox`.

The canvas lane parses `Path2D` strings with the same parser.

### 3. Sizing the outer `svg` (div-layout, integration request)

The outer `svg` is a replaced element.

- **Natural size.** It comes from the `width` and `height` presentation
  attributes (`auto` when absent). The natural aspect ratio comes from
  `viewBox`. With neither resolvable, it falls back to 300×150.
- **Sizing.** CSS sizing then works as it does for any replaced element.
  `<svg width=200 height=100>` lays out 200×100.
- **Content.** The content is placed by the viewport transform into the
  content box. The UA rules for SVG are ported from `svg.css` (BSD)
  and join the UA stylesheet work (item 0.4).

### 4. Style (div-style, integration request)

- **Properties.** Added through `property-schema.csv`, with the
  inheritance, initial values and animation types of
  `css_properties.json5` (BSD):
  - V1: `fill`, `fill-opacity`, `fill-rule`, `stroke`, `stroke-width`,
    `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`,
    `stroke-dasharray`, `stroke-dashoffset`, `stroke-opacity`,
    `paint-order`, `vector-effect`, `shape-rendering`, plus the
    geometry properties `x`, `y`, `cx`, `cy`, `r`, `rx` and `ry`.
  - V2: `text-anchor`, `dominant-baseline`, `alignment-baseline`,
    `baseline-shift`.
  - V3: `stop-color`, `stop-opacity`, `clip-rule`, `mask-type`, the
    `marker` properties and `color-interpolation`.
  - Existing properties apply too: `opacity`, `transform`,
    `transform-origin`, `transform-box`, `visibility`, `display`,
    `overflow`, `pointer-events` and `color`.
- **The `<paint>` type.** `none`, `<color>` including `currentColor`,
  `url(#id)` with a fallback, `context-fill` and `context-stroke`. V1
  parses `url()` and paints its fallback until V3.
- **Selectors (item 0.1).** Type selectors match SVG elements
  case-sensitively and by namespace, so `clipPath` is not `clippath`.
  Attribute selectors keep the attribute's case.
- **Custom properties (item 0.5).** `var()` in SVG properties and
  presentation attributes uses the same substitution as everything
  else. There is no SVG-specific path.
- **Animation.** CSS transitions and keyframes on SVG properties come
  through the existing animation engine once A1/A2 land
  (`stroke-dashoffset` drawing effects, `fill`, `transform`).

### 5. Paint (V1)

Painting ports Blink's BSD painters onto Div's recorder:

- **`SVGShapePainter` order.** Fill, then stroke, then markers, as
  `paint-order` gives. Each shape is one paint chunk in RFC 0006's
  model.
- **`SVGObjectPainter` paint setup.** It sets the colour, multiplies
  `fill-opacity` or `stroke-opacity` in, and sets anti-aliasing, which
  is off for `shape-rendering: crispEdges`.
- **`StrokeData` from `platform/geometry/stroke_data`.** Width, cap,
  join and miter limit. A dash array that is empty or sums to zero
  draws a solid stroke, as the specifications give, checked against
  Chromium. The dash effect goes on the paint, and the stroke is drawn
  by Skia's stroker.
- **`vector-effect: non-scaling-stroke`.** The stroke is drawn in the
  host coordinate space.
- **`SVGRootPainter` and `SVGContainerPainter`.** The root's transform
  to the border box, viewport clips, and container transforms.
- **Effects per element:** opacity, `clip-path`, `mask` and `filter`.
  Chromium does this in `scoped_svg_paint_state`, which is LGPL, so
  Div reuses its existing CSS effect layers. Any difference measured on
  SVG is a recorded gap until it is fixed from the specifications.

**Static images.** `image/svg+xml` resources are parsed by a permissive
XML parser into an isolated SVG document: no script and no external
loads, as in Chromium's `SVGImage` (BSD). The document is laid out at
the image's concrete size and painted by this same painter. The
string-scanning rasterizer in `image_resource.rs` is deleted once its
existing tests pass on the new path.

### 6. Text (V2)

- **Layout.** A port of Blink's LayoutNG SVG text (BSD:
  `svg_text_layout_algorithm`, `svg_text_layout_attributes_builder`,
  `svg_text_query`) onto Div's inline layout and `div-text` shaping. It
  covers `text` and `tspan`; `x`, `y`, `dx`, `dy` and `rotate` lists;
  and the anchors and baselines.
- **Painting.** Fill and stroke of glyphs, through the SVG branch of
  `text_fragment_painter` (BSD).
- **Later, from the specifications.** `textLength`, `lengthAdjust` and
  `textPath`; Chromium's `layout_svg_text_path` is LGPL.
- **Fonts.** Exactness depends on the fidelity lane's font work. Text
  gaps are recorded against that lane, not hidden.

### 7. Paint servers, clipping, masks and markers (V3)

- **References.** `url(#id)` and `href` resolve per tree scope, ported
  from `svg_tree_scope_resources` and `svg_resource` (BSD). A target
  that is added, removed or changed invalidates its clients. Cycles are
  detected.
- **Gradients and patterns, from the specifications.**
  `linearGradient`, `radialGradient` with a focal point,
  `gradientUnits`, `gradientTransform`, `spreadMethod`, `href`
  inheritance, and stop offset clamping. Shaders are built through
  Div's CSS gradient code and `layout_svg_resource_paint_server` (BSD).
  `pattern` is also written from the specification.
- **Clips and masks.** `clipPath` ports `clip_path_clipper` (BSD),
  including its path fast path against its mask path, `clipPathUnits`
  and `clip-rule`. `clip-path: url(#id)` works on HTML elements too.
  `mask` ports `svg_mask_painter` (BSD), with `mask-type`.
- **Markers, `use` and `symbol`, from the specifications.** For
  markers: orientation, `markerUnits`, `refX`/`refY` and `viewBox`.
  `use` and `symbol` work as a shadow-tree instance.

### 8. Hit testing, focus and accessibility (V5)

- **Hit testing on geometry.** Fill by `fill-rule`, and stroke by the
  stroked outline, under SVG's `pointer-events` values (default
  `visiblePainted`). The local-point mapping ports
  `transformed_hit_test_location` (BSD). The containment rules are
  written from the specifications and checked against
  `elementFromPoint` grids in Chromium 147.
- **Events.** Events target SVG elements and bubble through `g` and
  `svg` into HTML ancestors.
- **Focus.** `tabindex` and `a` focus, with the focus ring from
  `svg_model_object_painter` (BSD).
- **Accessibility, with item 1.3 (H8).** Roles `graphics-document`,
  `graphics-symbol` and `group`, and names from `title`, `desc` and
  ARIA.

The geometry methods of V4 (`getBBox`, `getTotalLength` and the rest)
are framework API on the engine geometry from section 2. The framework
track proposes them separately.

## Fidelity plan

The oracle is pinned Chromium 147.0.7727.50: Linux CfT in WSL for
census work, and the read-only Windows CfT at `out\chrome-win`. Every
comparison is exact: Div's frame against Chromium's frame of the same
input, as in the census. Unmeasured areas are claimed as nothing.

1. **The `div-svg` geometry corpus (black box).** A Chromium run
   produces, for a corpus of paths, shapes, arcs and `viewBox`/
   `preserveAspectRatio` cases:
   - `getPointAtLength` samples;
   - `getTotalLength`;
   - `getBBox`;
   - `getScreenCTM`.

   Div's geometry matches them to the float. This is what keeps the
   clean-room parts honest.
2. **Lucide icons.** Every icon in the components track's Lucide
   version, inline and as `<img>`, at 16 and 24 px, at scales 1 and 2.
   This is the V1 stage 1 gate.
3. **WPT.** The `svg/shapes`, `svg/path`, `svg/painting`,
   `svg/coordinate-systems`, `svg/pservers`, `svg/text` and
   `css/css-masking/clip-path` reftests. Div's porter
   (`tools/wpt/port_wpt.py`) has no `svg` suite today. It compares each
   test with Chromium's render of the same file, not with its
   reference, and it ports only HTML. So two tooling steps come first,
   both owned by the engine track's qualification tooling:
   - loading standalone `.svg` tests as documents;
   - an `svg` suite.

   Pass counts are reported per directory and per run. They are never
   merged into the census unless the full census is rerun.
4. **Recharts.** The sample-charts pages: the shadcn examples with
   animation off, light and dark, at scales 1 and 2. Chromium renders
   each page. Div renders the same SVG DOM, serialized from Chromium
   after load into a Div fixture. The gate reports exact pages out of
   the total, with every non-exact page listed with its diff counts.
5. **No regression.** The census and CSS Grid coverage show no change
   (D4), and no exactness gate is relaxed for SVG.

## Affected tracks

| Track | What changes |
| --- | --- |
| engine | Element identity (0.3) gains namespaces and case-preserving SVG names and attributes. `div-style` gains the SVG properties, the presentation-hint level and namespace-aware type selectors. `div-layout` sizes the outer `svg` as replaced and skips box layout inside SVG. The UA sheet takes `svg.css`. The SVG lane writes the SVG modules (`div-svg`, `div-paint/src/svg/`, `div-layout/src/svg/`); changes to shared engine structures go to the parent as integration requests. The WPT porter gains `.svg` documents and an `svg` suite |
| framework | `view!` keeps case-sensitive SVG element names (decision 3) and takes attribute names as written, including `stroke-width`, `viewBox` and `xlink:href`. The name check stops panicking for SVG names. Framework item 7 ("Public SVG drawing, for icons") is replaced by this RFC. Typed helpers (a `PathData` builder) and the V4 geometry API come later |
| components | Lucide icons work inline and as images after V1 stage 1. Existing pixel tests must not change |
| canvas lane | `Path2D(string)` uses `div-svg`'s path parser |
| samples | Charts, markdown, motion and git move off their SVG workarounds as each stage lands |

## Migration

Each step is one focused PR, measured as described above, and `main`
stays green between them:

1. This RFC (docs only).
2. **V1 stage 1, stroked icons.**
   - Added:
     - `div-svg`, with path data, arcs, shapes, lengths and viewport
       transforms;
     - the SVG namespace and names, through item 0.3 or its SVG part
       agreed with the engine;
     - outer `svg` sizing;
     - `g`, `path` and the basic shapes;
     - the V1 fill and stroke properties, as presentation attributes
       and `style`;
     - the `transform` attribute;
     - the painter ports;
     - static `image/svg+xml` on the same path;
     - the licence audit tool.
   - Gates: the Lucide corpus and the geometry corpus.
   - If item 0.1 has not merged yet, presentation attributes and the
     `style` attribute apply at the lowest specified-style priority,
     moving to the cascade level when it lands.
3. **V1 stage 2.**
   - Dashes, `paint-order`, `vector-effect`, `shape-rendering`,
     `fill-rule`, nested `svg`, and percentage and font-relative
     lengths.
   - CSS selectors on SVG (after 0.1) and `var()` (after 0.5).
   - Gates: the WPT `svg/shapes`, `svg/path` and `svg/painting`
     reftests through the new suite, and the first Recharts pages
     without text.
4. **V1 stage 3.** Delete the string-scanning rasterizer once every
   existing static-SVG test passes on the new painter.
5. **V2 text, then V3, then V5**, one PR per coherent feature: text;
   gradients and `clipPath`; masks, markers and `use`; hit testing;
   focus and accessibility.

Open questions for the parent and the user:

- **Item 0.3.** Whether it lands namespaces in one PR, or SVG goes first
  in the identity PR's SVG part. Stage 1 cannot start without one of
  them.
- **Arc approximation.** If no permissive approximation matches
  Chromium exactly, the remaining arc differences stay recorded gaps
  (licence rule 3).
- **Build cost.** Recharts' per-element build cost, measured against
  Chromium on the same page before any target is set.
