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
headers were checked at tag `147.0.7727.50`, header only, file by file,
by `tools/svg/licence_audit.py`. It audited 512 files: 229 BSD, 280 LGPL,
and three name tables (`svg_tag_names.json5`, `svg_attribute_names.json5`,
`xlink_attribute_names.json5`) that carry no licence header. Licences are
per file, and a `.cc` and its `.h` can differ. 14 pairs do; among the
files this RFC uses, `layout_svg_resource_paint_server.cc` is BSD while
its `.h` is LGPL. The files this RFC names, and every file in the SVG
layout directory:

| File (under `third_party/blink/renderer/`) | Header |
| --- | --- |
| `core/css/css_properties.json5` | BSD (may be ported) |
| `core/css/svg.css` | BSD (may be ported) |
| `core/layout/svg/layout_svg_block.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_block.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_container.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_container.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_container_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_ellipse.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_ellipse.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_filter_primitive.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_filter_primitive.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_foreign_object.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_foreign_object.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_foreign_object_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_hidden_container.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_hidden_container.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_image.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_image.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_image_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_inline.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_inline.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_inline_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_inline_text.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_inline_text.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_model_object.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_model_object.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_path.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_path.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_rect.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_rect.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_rect_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_resource_clipper.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_clipper.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_container.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_container.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_filter.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_filter.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_gradient.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_gradient.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_linear_gradient.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_linear_gradient.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_marker.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_marker.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_masker.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_masker.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_paint_server.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_resource_paint_server.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_pattern.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_pattern.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_radial_gradient.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_resource_radial_gradient.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_root.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_root.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_root_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_shape.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_shape.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_shape_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_text.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_text.h` | BSD (may be ported) |
| `core/layout/svg/layout_svg_text_path.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_text_path.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_text_test.cc` | BSD (may be ported) |
| `core/layout/svg/layout_svg_transformable_container.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_transformable_container.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_tspan.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_tspan.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_viewport_container.cc` | LGPL (not read) |
| `core/layout/svg/layout_svg_viewport_container.h` | LGPL (not read) |
| `core/layout/svg/layout_svg_viewport_container_test.cc` | BSD (may be ported) |
| `core/layout/svg/resolved_text_layout_attributes_iterator.h` | BSD (may be ported) |
| `core/layout/svg/svg_character_data.cc` | BSD (may be ported) |
| `core/layout/svg/svg_character_data.h` | LGPL (not read) |
| `core/layout/svg/svg_content_container.cc` | BSD (may be ported) |
| `core/layout/svg/svg_content_container.h` | BSD (may be ported) |
| `core/layout/svg/svg_hit_test_perftest.cc` | BSD (may be ported) |
| `core/layout/svg/svg_inline_node_data.h` | BSD (may be ported) |
| `core/layout/svg/svg_layout_info.h` | BSD (may be ported) |
| `core/layout/svg/svg_layout_support.cc` | LGPL (not read) |
| `core/layout/svg/svg_layout_support.h` | LGPL (not read) |
| `core/layout/svg/svg_layout_support_test.cc` | BSD (may be ported) |
| `core/layout/svg/svg_layout_tree_as_text.cc` | BSD (may be ported) |
| `core/layout/svg/svg_layout_tree_as_text.h` | BSD (may be ported) |
| `core/layout/svg/svg_marker_data.cc` | LGPL (not read) |
| `core/layout/svg/svg_marker_data.h` | LGPL (not read) |
| `core/layout/svg/svg_resources.cc` | LGPL (not read) |
| `core/layout/svg/svg_resources.h` | LGPL (not read) |
| `core/layout/svg/svg_text_layout_algorithm.cc` | BSD (may be ported) |
| `core/layout/svg/svg_text_layout_algorithm.h` | BSD (may be ported) |
| `core/layout/svg/svg_text_layout_algorithm_test.cc` | BSD (may be ported) |
| `core/layout/svg/svg_text_layout_attributes_builder.cc` | BSD (may be ported) |
| `core/layout/svg/svg_text_layout_attributes_builder.h` | BSD (may be ported) |
| `core/layout/svg/svg_text_layout_attributes_builder_test.cc` | BSD (may be ported) |
| `core/layout/svg/svg_text_query.cc` | BSD (may be ported) |
| `core/layout/svg/svg_text_query.h` | BSD (may be ported) |
| `core/layout/svg/transform_helper.cc` | BSD (may be ported) |
| `core/layout/svg/transform_helper.h` | BSD (may be ported) |
| `core/layout/svg/transformed_hit_test_location.cc` | BSD (may be ported) |
| `core/layout/svg/transformed_hit_test_location.h` | BSD (may be ported) |
| `core/paint/clip_path_clipper.cc` | BSD (may be ported) |
| `core/paint/clip_path_clipper.h` | BSD (may be ported) |
| `core/paint/clip_path_clipper_test.cc` | BSD (may be ported) |
| `core/paint/outline_painter.cc` | BSD (may be ported) |
| `core/paint/outline_painter.h` | BSD (may be ported) |
| `core/paint/outline_painter_test.cc` | BSD (may be ported) |
| `core/paint/svg_background_paint_context.cc` | BSD (may be ported) |
| `core/paint/svg_background_paint_context.h` | BSD (may be ported) |
| `core/paint/svg_container_painter.cc` | BSD (may be ported) |
| `core/paint/svg_container_painter.h` | BSD (may be ported) |
| `core/paint/svg_container_painter_test.cc` | BSD (may be ported) |
| `core/paint/svg_foreign_object_painter.cc` | BSD (may be ported) |
| `core/paint/svg_foreign_object_painter.h` | BSD (may be ported) |
| `core/paint/svg_image_painter.cc` | BSD (may be ported) |
| `core/paint/svg_image_painter.h` | BSD (may be ported) |
| `core/paint/svg_mask_painter.cc` | BSD (may be ported) |
| `core/paint/svg_mask_painter.h` | BSD (may be ported) |
| `core/paint/svg_model_object_painter.cc` | BSD (may be ported) |
| `core/paint/svg_model_object_painter.h` | BSD (may be ported) |
| `core/paint/svg_object_painter.cc` | BSD (may be ported) |
| `core/paint/svg_object_painter.h` | BSD (may be ported) |
| `core/paint/svg_root_painter.cc` | BSD (may be ported) |
| `core/paint/svg_root_painter.h` | BSD (may be ported) |
| `core/paint/svg_shape_painter.cc` | BSD (may be ported) |
| `core/paint/svg_shape_painter.h` | BSD (may be ported) |
| `core/paint/text_fragment_painter.cc` | BSD (may be ported) |
| `core/paint/text_fragment_painter.h` | BSD (may be ported) |
| `core/paint/text_fragment_painter_test.cc` | BSD (may be ported) |
| `core/style/style_svg_resource.cc` | BSD (may be ported) |
| `core/style/style_svg_resource.h` | BSD (may be ported) |
| `core/style/svg_paint.cc` | LGPL (not read) |
| `core/style/svg_paint.h` | LGPL (not read) |
| `core/svg/graphics/svg_image_for_container.cc` | LGPL (not read) |
| `core/svg/graphics/svg_image_for_container.h` | BSD (may be ported) |
| `core/svg/svg_resource.cc` | BSD (may be ported) |
| `core/svg/svg_resource.h` | BSD (may be ported) |
| `core/svg/svg_tree_scope_resources.cc` | BSD (may be ported) |
| `core/svg/svg_tree_scope_resources.h` | BSD (may be ported) |
| `platform/geometry/path.cc` | BSD (may be ported) |
| `platform/geometry/path.h` | BSD (may be ported) |
| `platform/geometry/path_builder.cc` | BSD (may be ported) |
| `platform/geometry/path_builder.h` | BSD (may be ported) |
| `platform/geometry/path_builder_test.cc` | BSD (may be ported) |
| `platform/geometry/stroke_data.cc` | BSD (may be ported) |
| `platform/geometry/stroke_data.h` | BSD (may be ported) |
| `platform/graphics/gradient.cc` | BSD (may be ported) |
| `platform/graphics/gradient.h` | BSD (may be ported) |
| `platform/graphics/graphics_context.cc` | BSD (may be ported) |
| `platform/graphics/graphics_context.h` | BSD (may be ported) |

The rules:

1. **Port only permissive files, file by file.** Each porting PR names
   each file, the tag and its row in the audit. A BSD `.cc` whose `.h`
   is LGPL is ported without its header: Div writes its own interface
   from what the `.cc` and the specifications need. A file with no
   licence header is treated as not cleared.
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

The audit script and its full output are part of this RFC:
`tools/svg/licence_audit.py` and
`tools/svg/licence-audit-147.0.7727.50.tsv`.
The table above is the subset this RFC names. The TSV lists every file
in `core/layout/svg` and `core/svg`, plus the painter, style and
platform files this RFC names, one row per file, `.h` and `.cc`
separately. A port must check its file's row. A file not in the TSV is
not cleared until the script is run on it.

## Proposal

### 1. Elements and identity (joins Tier 0 item 0.3)

- **Namespace and names.** SVG elements are elements in `div-dom`. They
  carry the SVG namespace and their case-sensitive local name. The
  engine lands the namespace field (HTML, SVG, MathML) with element
  identity in item 0.3. The SVG element kinds come in this RFC's PRs.
  An unknown name in the SVG namespace creates a generic SVG element
  that renders nothing; it is not an error.
- **Creation API.** It is shaped like `createElementNS`, for example
  `Element::create_ns(&doc, Namespace::Svg, "clipPath")`. `Element::create`
  stays shaped like `createElement` and creates HTML elements. Two
  existing calls create SVG elements today: `Element::create(&doc,
  "svg")` and `create_svg_foreign_object`. Both keep working, as
  deprecated shorthands for the SVG namespace, until their users move to
  `create_ns`.
- **`view!` infers the namespace, as the HTML parser does.** Inside an
  `svg` subtree, elements are created in the SVG namespace. Inside
  `foreignObject`, they are HTML again. Names are taken as written. The
  HTML specification's table for adjusting SVG tag names also applies,
  so `clippath` becomes `clipPath`, as it does when parsed.
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
  pixels), not by reading Chromium's parser. The arc approximation
  belongs to `div-svg`'s path-data code and serves SVG path data only.
  It is not shared with canvas `arc()` and `arcTo()`, which follow the
  canvas lane's own port.
- **Shapes as paths.**
  - `rect`, with the `rx`/`ry` rules and clamping of SVG 2 §10.2, as
    Chromium 147 applies them. The corpus's first run shows
    `rx="auto"` as an attribute does not behave like SVG 2's `auto`.
    Blink's `layout_svg_rect` (BSD) supplies its fast paths for fills
    and strokes.
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

The canvas lane parses `Path2D` strings with the same path-data
parser.

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
  `paint-order` gives.
- **Paint chunks (amends RFC 0006).** RFC 0006 keys a chunk by (node,
  pseudo-element, phase) but defines it as one box's paint. This RFC
  amends that definition: an SVG element that paints is a chunk client
  like a box. Each shape, each SVG text block and the outer `svg`'s own
  background and border chunk separately, each with its own property
  state (transform, clip, effect). This matches Chromium, whose SVG
  layout objects are display item clients. It is also what makes
  Recharts' per-element hover redraw only the changed shape. Chunking
  at the outer `svg` instead would re-record and re-raster the whole
  chart on every change. Until RFC 0006's A1 lands, nothing differs:
  everything still records into one picture.
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
the image's concrete size and painted by this same painter. Chromium
sizes an SVG image for its container in `svg_image_for_container.cc`,
which is LGPL (its `.h` is BSD). So that sizing is written from the
specifications (the CSS Images default sizing algorithm, SVG 2's
intrinsic dimensions) and checked against Chromium captures. The
string-scanning rasterizer in `image_resource.rs` is deleted once its
existing tests pass on the new path.

### 6. Text (V2)

- **Layout.** A port of Blink's LayoutNG SVG text where it is BSD
  (`svg_text_layout_algorithm`, `svg_text_layout_attributes_builder`,
  `svg_text_query`, `svg_inline_node_data.h`, `layout_svg_text`) onto
  Div's inline layout and `div-text` shaping. It covers `text`; `x`,
  `y`, `dx`, `dy` and `rotate` lists; and the anchors and baselines.
- **Clean-room parts of V2.** Several files these BSD files use are
  LGPL, so these parts are written from the specifications and checked
  against Chromium:
  - `tspan` and inline SVG text boxes (`layout_svg_tspan`,
    `layout_svg_inline`);
  - the per-character position record (`svg_character_data.h`): Div
    defines its own from SVG 2's text layout algorithm, not from that
    header;
  - the shared base of the SVG text and `foreignObject` layouts
    (`layout_svg_block`).

  A port of a BSD file that calls into these uses Div's own clean-room
  equivalent and does not open the LGPL file.
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
  Div's CSS gradient code and `layout_svg_resource_paint_server.cc`
  (BSD; its `.h` is LGPL and is not read).
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
- **`offset` for SVG targets (RFC 0007).** RFC 0007 defines `offset` as
  the point in the target's padding box. An SVG element inside an outer
  `svg` has no CSS box. Chromium 147 then reports the point relative to
  the padding edge of the outer `svg` element. The SVG user space does
  not apply: neither the `viewBox` scale nor the `g` transform above the
  target changes it. Measured on the Windows CfT
  over CDP: a `rect` under `translate(10 5)` in an outer `svg` at
  (40, 30) with a 5 px border, `viewBox` scale 2, clicked at client
  (110, 90), reports `offsetX/Y` (65, 55). Its user-space point is
  (19, 19). Hits on the outer `svg` itself follow RFC 0007 unchanged.
  So for SVG targets, the engine's hit result supplies the local point
  in the outer `svg`'s padding box. The probe did not cover a CSS
  transform on the outer `svg`, a transform on the target itself, or a
  nested viewport. V5's fixture measures those on Linux before it
  merges.
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
2. **Lucide icons.** The V1 stage 1 gate covers the 20 icons the
   components track vendors from `lucide-static` 1.70.0. Each icon is
   captured in 32 variants:
   - inline and as `<img>`;
   - at 16 and 24 px;
   - at an integer origin and at an origin offset by 0.5 px;
   - in two colour pairs;
   - at scales 1 and 2.

   That makes 640 captures. The criterion, as for Recharts, is exact
   captures out of 640. Every non-exact capture is listed with its
   differing pixel count and maximum channel difference. The target is
   640 of 640. The stage 1 PR states its count, and every non-exact
   capture becomes a recorded gap with an owner. Later PRs may not lower
   the count.
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
| framework | `view!` keeps case-sensitive SVG element names (decision 3) and takes attribute names as written, including `stroke-width`, `viewBox` and `xlink:href`. Framework item 7 ("Public SVG drawing, for icons") folds into this RFC, on the authority of accepted decision 3, which makes stroked icons V1 stage 1. `Element::create_ns` and the namespace inference in `view!` (section 1) are framework work. Typed helpers (a `PathData` builder) and the V4 geometry API come later |
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
     - the SVG element kinds, on the namespace field that item 0.3
       lands;
     - outer `svg` sizing;
     - `g`, `path` and the basic shapes;
     - the V1 fill and stroke properties, as presentation attributes
       and `style`;
     - the `transform` attribute;
     - the painter ports;
     - static `image/svg+xml` on the same path;
     - the licence audit tool.
   - Gates: the Lucide corpus and the geometry corpus.
   - Stage 1 depends on two engine pieces and adds no interim priority
     of its own. Presentation attributes need the cascade's
     presentation-hint level (item 0.1). The `style` attribute comes from
     the engine's R0b (MD-24, editor-22), where inline style beats
     author rules as in Chromium.
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

- **Arc approximation.** If no permissive approximation matches
  Chromium exactly, the remaining arc differences stay recorded gaps
  (licence rule 3).
- **Build cost.** Recharts' per-element build cost, measured against
  Chromium on the same page before any target is set.
