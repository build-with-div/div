# RFC 0019: Chromium-exact number line heights

Status: Approved by the user on 27 November 2026, after review by the
framework and components tracks. Owner: the engine track (`div-style`,
`div-text`). Consumers: framework (`divui`, `div-ffi`, `div-macros`),
components, and the fidelity lane. Implementation waits until R7 lands,
because the change goes beyond what the licence-rewrite freeze allows in
`div-style/src/enums.rs`.

## Problem

A unitless `line-height` can come out 1/64px short of Chromium's when the
product of the number and the font size lands just under a layout unit. The
components font specimens show it: `line-height: 1.4285714` at
14px gives 19.984375px in Div and 20px in Chromium 147. The components
track's before-side evidence at `4dc219ed` shows the drift on 24 font
images; Inter at 1x is about 3,100 pixels off. Once #153 lands, the
specimens use Tailwind's real `calc(1.25 / 0.875)` and `calc(1 / 0.75)`
(as the Chromium oracle's `reference.css` does). The components pages then
no longer exercise number storage, so this RFC carries its own fixture with
the literal number. The specimens' remaining differences at 1.25x, 1.5x and
2x belong to RFC 0011, not to this fix.

**Chromium's rule.** `StyleBuilderConverter::ConvertLineHeight`
(`style_builder_converter.cc`, BSD) stores a number as
`Length::Percent(ClampTo<float>(number * 100.0))`. The number is the parser's
double. The line box (`inline_box_state.cc:135`, BSD) takes
`ComputedLineHeightAsFixed`. The body of that function is in LGPL
`computed_style.cc`, which we have not read, so its arithmetic comes from
black-box testing only. The fit below matches every finite number case in
the `calc-v1` oracle:

```
p    = (float)(number_f64 * 100.0)
used = LayoutUnit::trunc((float)((float)LayoutUnit(font_size) * p) / 100.0f)
```

| Model | `calc-v1` number cases matched |
| --- | --- |
| The rule above, in float | 84 of 84 |
| The same rule in double | 81 of 84 |
| Div on `main` (`trunc(f32(font) * f32(number))`) | 81 of 84: the 3 named GAPS in `calc_oracle.rs` |

Truncation is correct. Rounding, or adding an epsilon, would break
`1.15`@16 (18.390625). Precision is lost earlier. `LineHeight::Number(f32)`
parses `1.4285714` straight to f32 (1.42857134…). Its × 100 is one f32 step
below Blink's percent, so the truncation falls to 19.984375.

**No f32 encoding can be exact.** The stopgap the user declined (option B)
stores the f32 whose × 100 rounds to Blink's percent. That still misses
989 of 4.94 M pairs (18,165 decimals in [0, 4] with up to 7 digits, × 272
font sizes from 6 to 72px), against 1,128 on `main`. In [1, 1.28) × 2^k, f32
numbers are about 1.56 times sparser than f32 percents, so some percents
have no f32 number at all. The sweep compares against the model above, not
against Chromium. Only the 84 oracle cases are Chromium measurements.

## Proposal

Store a number line height the way Chromium stores it: as the f32 percent of
the parser's double.

```rust
// div-style, enums.rs (after R7): only the payload type changes
pub enum LineHeight {
    Normal,
    /// Unitless multiplier, stored as Blink stores it: the f32 of
    /// (number as f64 × 100).
    Number(LineHeightNumber),
    Length(f32),
    Percentage(f32),
}

// div-style, new file line_height_number.rs, re-exported by one
// `pub use` line in div-style/src/lib.rs and by divui next to LineHeight
/// A unitless line height, held as Blink's f32 percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineHeightNumber(f32); // private field: the percent

impl LineHeightNumber {
    /// From a parsed number: ClampTo<float>(max(number, 0) * 100).
    pub const fn new(number: f64) -> Self;
    /// From a stored percent, as the macros' constructor tokens and
    /// animation's lerp produce it. Applies the same clamp.
    pub const fn from_percent(percent: f32) -> Self;
    /// Blink's stored value.
    pub const fn percent(self) -> f32;
    /// percent as f64 / 100. For display and debugging only: it is not a
    /// serialization source and does not round-trip the authored number.
    pub fn get(self) -> f64;
}
impl From<f64> for LineHeightNumber { .. }
impl LineHeight { pub const fn number(number: f64) -> Self; }
```

New public items: `LineHeightNumber`, its `new`, `from_percent`, `percent`
and `get`, `From<f64> for LineHeightNumber`, and `LineHeight::number`. The
payload of `LineHeight::Number` changes from `f32` to `LineHeightNumber`.

- **Non-finite numbers and the clamp.**
  - Text: CSS text cannot produce a non-finite number, and the parsers
    reject one.
  - `calc()` infinity: `math.rs` clamps it to a finite double
    (`clamp_double`) before conversion, as it does today. `new` then clamps
    to the largest finite f32 percent. `calc-v1` pins `calc(1 / 0)` at 13.5,
    14, 15.75 and 16px: the box height matches Chromium (33554432) today and
    must still match. Its `1lh` stays a named gap owned by `div-layout` (Div
    33554432, Chromium 33554428). This RFC doesn't change it.
  - `calc()` NaN: `math.rs` maps it to 0, as today. No `calc-v1` case pins
    this, so `line-height-number-v1` adds `calc(0 / 0)` and
    `calc(infinity - infinity)`. If Chromium differs, the PR follows Chromium.
  - `new` and `from_percent` are `const fn`s on the workspace's Rust 1.85,
    using only `f64::max` (const-stable), comparisons and `as` casts. They
    reproduce `ClampTo<float>` with explicit comparisons, because `as f32`
    alone would overflow to infinity: a value at or above the largest finite
    f32 gives `f32::MAX`, and NaN gives 0. NaN reaches them only from direct
    Rust callers, and mapping it to 0 matches `math.rs`. We have not read how
    Blink's `ClampTo` handles NaN.
- The parsers (`property.rs` text and shorthand paths, and `math.rs` for
  `calc()` numbers) parse to f64 and call `LineHeightNumber::new`. The
  `calc()` path already holds a double.
- `used_line_height` applies the rule above, and so does the `lh` unit for
  a number line height. `calc-v1` pins `1lh` equal to the box height in
  every number case, including the 3 named GAPS (`1lh` 19.984375, Chromium
  20).
- Equality, and so style-change detection and the start of transitions,
  compares percents, as Blink compares `Length`s. Two numbers with the same
  percent are the same computed value.
- Interpolation lerps the two percents in f32, as a working assumption: we
  have not confirmed how Blink interpolates `line-height` numbers. The
  implementation PR pins it with a new animation oracle case before the
  change lands. If Chromium differs, the PR follows Chromium and records the
  difference here.
- **`calc()` numbers.** Blink converts a `calc()` number the same way, and
  `math.rs` already holds the double, so both kinds of number take one path.
  At the sizes the Tailwind specimens use (`calc(1 / 0.75)`@12,
  `calc(1.25 / 0.875)`@14, and the `text-base` to `text-4xl` pairs), the
  model gives the same used value as `main`, so those pages, which are exact
  at 1x today, stay as they are. Over the sweep's 272 font sizes,
  `calc(1 / 0.75)` and `calc(2 / 1.5)` differ from `main` at 33 sizes each
  (for example @13.5, `main` gives 18 and the model gives 17.984375). The
  other Tailwind ratios and keywords differ at none. None of those 33 sizes
  is in `calc-v1`, so they are predictions, and the new oracle below
  measures them. `calc(1 / 3)`@13.5, a named gap today, becomes exact.
- The places that compute from the number directly move to the percent
  rule: `block.rs:11365` (the list marker), `painter.rs:11166`,
  `relative.rs:63` (`line_height_px`, behind `used_line_height`) and
  `relative.rs:154` (the `lh` resolver, which already applies
  `LayoutUnit(font) × (number × 100) / 100` truncated, but from the f32
  number).
- Animation lerps through `LineHeightNumber::from_percent`.

**Option considered: `Number(f64)`.** It is a smaller public change, since
`LineHeight::Number(1.5)` keeps compiling and only f32 variables break. It
also gives exact used values. But two numbers inside one f32 percent step
compare unequal, so Div would start a transition where Chromium doesn't. It
also keeps two representations for one computed value. We recommend the
newtype because it ports Chromium's storage rather than approximating it.

## Affected tracks

| Track | Impact |
| --- | --- |
| framework, `divui` | `set_line_height(LineHeight)` is unchanged. Callers that write `LineHeight::Number(1.5)` change to `LineHeight::number(1.5)`: 12 sites in 10 files (8 in 7 examples, 4 in 3 tests). `LineHeightNumber` is re-exported next to `LineHeight` |
| framework, `div-ffi` | No ABI change. A C caller passes a line height as text through `divui_style_value_parse` (a compound handle, tag 6) or as a length (tag 1). No C type carries the number, so `divui.map` and the header stay the same. One site changes: the parity test at `lib.rs:4683` |
| framework, `div-macros` | `style!`, `stylesheet!` and `view!` emit `ConstructorTokens`. The derive can't be used: for a tuple struct, `div-style-derive`'s `construct()` emits `::div::typed_style::LineHeightNumber(f32::from_bits(..))` in the caller's crate, and the private field makes that E0423. `LineHeightNumber` implements `ConstructorTokens` by hand instead, emitting `LineHeightNumber::from_percent(::core::primitive::f32::from_bits(BITS))`, so compile-time values are bit-identical to run-time parsing. No macro syntax changes. A new test checks that `style!{ line-height: 1.4285714 }` equals the run-time parse, percent bits included |
| fidelity, pixel-compare and WPT tools | 2 sites in `tests/fidelity/pixel-compare/src/main.rs`. 32,703 literals in the 15 generated WPT port files under `pixel-compare/src/wpt/` (the 6 `wpt_css_grid` parts and 9 other suites). The emitter `tools/wpt/port_wpt.py:8632` writes `LineHeight::Number({float(val)})` and changes to `LineHeight::number(..)`. The generated files are regenerated from it, and the ledger and codegen checks must pass. `tools/wpt/test_sp14_text_port.py:104` expects the old text and is updated |
| components | No source change, and no image change expected. Since #153 the specimens use `calc(1.25 / 0.875)`, which goes through the `math.rs` path that this RFC also changes. The expected used values are unmeasured, from the model and from components' arithmetic: 20px (`text-sm`, `calc(1.25 / 0.875)`@14), 16px (`text-xs`, `calc(1 / 0.75)`@12), 14px (`1`@14) and 24px (`1.5`@16), each the same as on `main`. The components run in the D4 pair checks them |
| fidelity | The literal-number case moves to `line-height-number-v1`. It names `1.4285714`@14 explicitly, because option B would fix that pair, so it is not among the 989. Before-side evidence: components' run at `4dc219ed` (`F:\div-out\components\lh-calc-evidence\before-*`; 24 font images; Inter at 1x about 3,100 px off). The `calc-v1` GAPS shrink by 3 |
| engine | `div-style` (`enums.rs` payload type, a new `line_height_number.rs` with one `pub use` line in `lib.rs`, `property.rs`, `math.rs`, `animation.rs`), `div-text` (`font/relative.rs`), and `div-layout` and `div-paint` (marker approximations). `font/metrics.rs`, `font/description.rs` and `font_types.rs` are not touched |

Provenance comments stay. The `enums.rs` lines "Text layout enums (extracted
from Blink computed_style_constants.h)" and "Blink: `Length` with `kAuto`
for `normal`…" are kept verbatim, and the new doc lines go after them.

## Migration

1. This RFC, docs only. It can merge before R7.
2. After R7 lands: one engine PR with the type, the parsers, the used value,
   every caller and the tests below. Across the repository that is 32,780
   sites in 45 files. Almost all of them are the regenerated WPT port files,
   so the diff is about 33,000 lines, nearly all mechanical. It must be one
   PR, because the payload change breaks callers, and `main` has to build at
   every commit. Framework reviews the `divui`, `div-ffi` and `div-macros`
   parts. No other track needs a PR. pixel-compare release builds for the
   pair take the census lock, as usual.
3. Pre-alpha, there are no outside users. The alpha notes list the
   constructor change.

## Verification

- The `calc-v1` oracle: all 84 number cases match, and the 3 GAPS in
  `calc_oracle.rs` are removed. Today that test runs on Windows only, with
  Arial.
- A new cross-platform `div-text` test over the 84 number cases: a number
  line height's used value doesn't depend on font tables, so Linux checks it
  too.
- A new `line-height-number-v1` oracle, captured from pinned Chromium 147 by
  the implementation PR. It holds:
  - the literal `1.4285714` at the specimen sizes, the regression fixture
    now that #153 removes it from the components pages;
  - the 989 pairs where option B fails;
  - `calc(0 / 0)` and `calc(infinity - infinity)` (NaN) at the specimen
    sizes;
  - `calc(1 / 0.75)` and `calc(2 / 1.5)` at the 33 sizes where the model
    departs from `main`;
  - a seeded random sample of the sweep.

  It must match exactly, with no tolerance. If Chromium disagrees with the
  model anywhere, the model is wrong, and this RFC is revised before the
  change lands.
- The `div-macros` test above: `style!` and run-time parsing agree to the
  bit, including `1.4285714`.
- A census and CSS Grid coverage pair (D4). #115's two pairs found no census
  page on this path, so we expect no change or a few gains, and no losses.
- A components reference run under the census lock, as part of the D4 pair:
  all 336 comparisons. Every `calc()` and number line height now takes the
  new path, so every components text line depends on it. We expect 0
  changed Div images and no losses. Components offered to run it once the
  PR is up.
