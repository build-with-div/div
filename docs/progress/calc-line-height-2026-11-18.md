# 0.2 PR 1: `calc()` in `line-height`, 18 November 2026

`line-height` now accepts `calc()` and `-webkit-calc()` over numbers,
lengths and percentages, and computes them as Chromium 147 does. The line
box now holds the used line height in layout units, as Chromium's does.

Before this PR:

- any `calc()` in `line-height` was dropped, so a Tailwind text size such
  as `text-sm` (`line-height: calc(1.25 / 0.875)`) fell back to the
  inherited `normal`;
- the line box used the fractional line height, so a block of one line of
  text could be 1/64px taller than in Chromium: `line-height: 1.15` at 14px
  gave a 16.109375px block where Chromium gives 16.09375px.

This is the first PR of item 0.2 (math functions). It adds the shared
`calc()` value that later properties will reuse; `line-height` is its first
consumer.

## Design

The parser and simplifier port Blink's `CSSMathExpressionNode`
(`core/css/css_math_expression_node.cc`) and its evaluation
(`platform/geometry/calculation_value.cc`,
`calculation_expression_node.cc`), read at 147.0.7727.50 under the BSD
licence and listed in `out/chromium-ref/SOURCES-engine-calc-2026-11-18.txt`.
Three LGPL files were removed unread past their headers
(`core/style/computed_style.cc`, `core/css/css_primitive_value.cc`,
`core/css/css_resolution_units.h`); the absolute unit ratios come from CSS
Values 4 instead.

- **Parsing**:
  - `+` and `-` need whitespace on both sides.
  - Parentheses and nested `calc()` nest.
  - The depth limit is 100.
  - The constants are `pi`, `e`, `infinity`, `-infinity` and `NaN`.
  - Blocks left open at the end of the value are closed, as in any CSS
    value.
- **Simplification while parsing**, as Blink does:
  - Literals of one unit combine in double precision, and absolute units
    combine in pixels.
  - Numbers distribute over sums.
  - Numeric division is evaluated directly, so `1.25 / 0.875` is the
    double quotient.
- **Types.** Typed arithmetic follows Blink's `CSSMathType`:
  - `1px / 2px` is the number 0.5.
  - `1px * 2px` and `2 / 1px` are not a number, length or percentage, so
    `line-height` drops them.
  - A sum of a number and a length is invalid.
- **Computed value** (Blink's `StyleBuilderConverter::ConvertLineHeight`):

  | Type | Computes to |
  | --- | --- |
  | Number | The double result, clamped non-negative |
  | Percentage | The font size times the percentage truncated to an integer, over 100 |
  | Length | The double result in pixels, clamped non-negative |
  | Length and percentage | Pixels plus percent / 100 of the font size in layout units, in single precision, truncated to layout units and clamped non-negative |

  NaN computes to 0; an infinite result clamps to the largest float.
- **Resolution.** A `calc()` line height is kept as `StyleValue::Math`
  until the engine resolves lengths. Then the font size, the root font
  size, the viewport and the font metrics are known, and it becomes an
  ordinary `LineHeight`.
- **The used line height** (`div_text::used_line_height`) now snaps to
  layout units:
  - a number's product truncates;
  - a fixed length rounds to the nearest unit (a percentage is a fixed
    length after computing).

  The `lh` unit still resolves against the unsnapped value, which is what
  Chromium's `1lh` reports. The oracle below fixed this rule: 41 cases
  differed with no snap and 19 with a truncating snap, against the 11
  named gaps with the rule above. Literal line heights are affected as
  well as `calc()`.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| The `calc()` parser, simplifier, types and evaluation; `MathFunction`, `MathResolver` | `div-style` | `crates/div-style/src/math.rs` |
| `StyleValue::Math`; the `line-height` literal accepts `calc()`; its computed value | `div-style` | `crates/div-style/src/property.rs` |
| The `Calculated` line-height sum in Blink's order (pixels plus percent of the font size) | `div-style` | `crates/div-style/src/property.rs` |
| A `calc()` line height resolves with the element's font, root font, viewport and metrics | `div-engine` | `crates/div-engine/src/lib.rs` |
| The used line height snaps to layout units | `div-text` | `crates/div-text/src/font/relative.rs` |
| The Chromium oracle and its capture tool | engine qualification | `tests/fidelity/fixtures/calc-v1/chromium-windows.json`, `tools/qualification/capture_calc_oracle.py` |
| The oracle test | engine qualification | `crates/divui/tests/calc_oracle.rs` |

## The oracle

`tools/qualification/capture_calc_oracle.py` drives the pinned Windows
Chrome for Testing 147.0.7727.50 at device scale 1:

```
python tools/qualification/capture_calc_oracle.py --chrome <chrome.exe>
```

It records 328 cases: 8 Tailwind text sizes with their `calc()` line
heights, then 82 literals and expressions at 14px, 16px, 13.5px and
15.75px. For each it records:

- whether the inline style kept the value;
- the height of a block of one line of Arial text;
- the height of a child of `height: 1lh` in an empty block of the same
  style.

`calc_oracle.rs` builds the same page through the public API and requires
the same acceptance and both heights, exactly. Every Tailwind case matches,
including `calc(1.25 / 0.875)` at 14px, which computes to 20px.

Two gaps remain, named in the test (11 cases):

- **Number precision** (`div-style`): `LineHeight::Number` holds an f32
  number, where Blink holds the f32 of the number times 100. For
  `1.4285714` at 14px and 15.75px, and `calc(1 / 3)` at 13.5px, the
  product truncates one layout unit lower than in Chromium. The literal
  cases fail on `main` too. Changing the stored form changes the public
  `LineHeight` type, so it is left to a follow-up.
- **Saturated boxes** (`div-layout`): for `calc(1 / 0)` and
  `calc(1px / 0)`, Chromium reports the saturated line box as 33554428px
  and Div as 33554432px.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| The oracle | `cargo test --locked -p divui --test calc_oracle` | 317 of 328 cases match; the 11 others are the named gaps |
| Parser and evaluation | `cargo test --locked -p div-style math` | 5 passed |
| Used line height | `cargo test --locked -p div-text --lib used_line_height` and `-p div-layout --test sp11_round16_fix_tests` | Pass. `used_line_height_snaps_to_layout_units` is new; the half-leading test now expects Chromium's 18.390625px for 1.15 at 16px instead of at least 19.2px for 1.2 |
| Windows suites | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-text -p div-layout -p div-paint -p div-engine -p divui`, then `-p div-svg`, at `2419d1da` | 9,407 and 36 passed, 0 failed |
| Linux suites (WSL, CI's Skia flags) | `-p div-style -p div-text -p div-layout -p div-paint -p div-svg -p div-engine -p divui` at `2419d1da` | 9,449 passed, 0 failed. The oracle test is Windows-only |
| Generators and ledgers | `check_platform_scoped_tests.py`, the frozen archive, the renderer contract, the SP13-R, SP17 and SP18 closures, CSS Grid coverage port, v0.2, property, FFI and form case generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify --profile all` at the bumped lock | All pass. SP20 was not checked locally: its inventory hashes Chromium's WPT tree, and this machine's checkout is not the pinned one. This PR changes none of its inputs; CI checks it |
| Expanded census, all four profiles, against `main` `c65b1245` | `div-census.sh` (1x run) and `div-census-zoom.sh` (real-zoom oracle above scale 1) under the shared lock, at `2419d1da` | 16,296 of 23,728 exact (1x run) and 10,403 of 17,796 (real zoom), before and after. 0 gains, 0 losses, 0 errors; 0 Div images changed |
| CSS Grid coverage | The same runs | 1,473 of 4,204 exact (1x run) and 894 of 3,153 (real zoom), before and after. 0 changed |

`main` gained no engine change between `c65b1245` and this PR's base
`f7f89890`, and pins the same data, so the earlier base runs stand. The
snapped line heights moved no census or coverage image. The evidence
is div-fidelity-data `4c0031a3`
(`docs/progress/evidence/calc-line-height-div-census-v1.json`, and
`-census-zoom-v1.json`, `-coverage-v1.json` and `-coverage-zoom-v1.json`
beside it); `fidelity-data.lock` pins it.

## Not done here

- **Other properties.** Only `line-height` accepts `calc()`. Lengths,
  percentages and the rest follow in 0.2's later PRs.
- **`min()`, `max()`, `clamp()` and the other math functions**, and units
  other than numbers, lengths and percentages (angles, times, resolutions).
- **The `font` shorthand** does not accept a `calc()` line height.
- **The components fixture** still writes Tailwind's line heights as
  numbers; switching it to `calc()` is the components track's change.
