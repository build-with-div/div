# S3/S4 PR 1: `@media` in style sheets, 24 November 2026

Tier 0 items S3/S4 (custom properties and media queries), the first
implementation PR of RFC 0014 (#110). A `stylesheet!` sheet can now hold `@media`
rules: at the top level, in a named layer, nested in another `@media`, or
nested in a style rule. The rules inside apply while their queries match,
and they follow the viewport, the device scale factor, the color scheme and
the reduced-motion preference. Queries parse and evaluate as Chromium 147
does: Blink's `MediaQueryParser` and `ConditionalParser`,
`MediaQueryExpValue::Consume` and `MediaQueryEvaluator` (all BSD; the LGPL
`media_query_list.cc` and `media_query_matcher.cc` were not read).

## Change

| Change | Owner | Source |
| --- | --- | --- |
| A CSS Syntax 3 tokenizer and component-value reader, for media query text (and RFC 0014's later unparsed values) | `div-style` (engine) | `crates/div-style/src/css_tokens.rs` |
| Media query data model (RFC 0014 "Data model"): lists, `not`/`only` and media types, `and`/`or`/`not` conditions, `<general-enclosed>`, plain, boolean, `min-`/`max-` and Level 4 range tests of `width`, `height`, `aspect-ratio`, `orientation`, `resolution`, `-webkit-device-pixel-ratio`, `prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `any-hover`, `pointer`, `any-pointer` and `forced-colors`. Parsing is browser-compatible (an invalid query is `not all`, an unknown term is unknown) and lists each problem for strict callers. Evaluation is Blink's three-valued logic, with its comparisons: one layout unit (1/64 px) of slack for `>=`, `<=` and `=` on sizes, the viewport truncated to integers for `aspect-ratio`, the scale factor as an `f32` for `resolution`, and `dpcm` compared at two decimals | `div-style` | `crates/div-style/src/media_query.rs`, `media_query_tests.rs` |
| `StyleRule::with_media`, `media` and `media_matches` | `div-style` | `crates/div-style/src/stylesheet.rs` |
| The sheet parser accepts `@media` at every level above; declarations directly in a nested `@media` keep the parent's selectors, in source order. A query Chromium would read as `not all` or unknown, a media type other than `all`, `screen` and `print`, a feature or unit Div does not evaluate yet (`color`, `ch`, `calc()`, ...), `@media` without a block and `@layer` inside `@media` are compile errors | `div-style` | `crates/div-style/src/stylesheet_parser.rs` |
| `stylesheet!` emits each rule's media lists as typed constructors | `div-macros` | `crates/div-macros/src/stylesheet_macro.rs` |
| The rule set keeps each distinct stack of media lists once with its result, and matching skips rules whose result is false. `set_viewport` (size or scale), `set_prefers_reduced_motion` and the new `Engine::set_color_scheme` evaluate the stacks again; if any result flipped, the document restyles at the next `update()`. Nothing is evaluated without `@media` rules | `div-engine` | `crates/div-engine/src/rule_cascade.rs`, `lib.rs`, `accessibility.rs` |
| `Document::set_color_scheme` and `color_scheme`, and the `ColorScheme` re-export (RFC 0014 "Public API") | `divui` (framework surface, as the RFC lists it) | `crates/divui/src/document.rs`, `lib.rs` |
| The media-queries-v1 oracle: 84 queries, each turning one element opaque, captured in 7 environments | engine qualification | `tools/qualification/capture_media_oracle.py`, `tests/fidelity/fixtures/media-queries-v1/`, `crates/divui/tests/media_oracle.rs` |

## Verification

The census pair was measured on `main` `b7e4f6a0` at `8106a6d2`, and the
branch was then rebased onto `main` `dde50838` as `d5eb930` and `fb3c50f`.
The code diff has the same patch ID on both bases
(`git diff <base> <head> | git patch-id --stable` gives `b1912556` for
`b7e4f6a0..8106a6d2` and `dde50838..fb3c50f`), and the generators' `--check`
passes at `fb3c50f`, so the generated output matches the rebased code.

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `tools/qualification/capture_media_oracle.py` on Windows with pinned Chrome 147.0.7727.50 | 7 environments (200x200, 600x400, 800x600 and 400x600 at 1x; 640x480 at 1.25x and 800x600 at 2x, real zoom; 600x400 dark with reduced motion), 84 elements, 326 matches |
| Oracle test | `cargo test --locked -p divui --test media_oracle` | All 588 values equal Chromium's, with no named gaps, on Linux at `8106a6d2` and `fb3c50f` |
| Touched crates (Linux), measured head | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-macros -p div-engine -p divui` with `b7e4f6a0`'s CI Skia flags, `8106a6d2` | 1,143 passed, 0 failed, 15 ignored; format clean (15 changed files) |
| Touched crates (Linux), rebased head | The same command with `dde50838`'s CI Skia flags (`-O2` added by #142), `fb3c50f` | 1,149 passed, 0 failed, 15 ignored; format clean (15 changed files). Skia's `args.gn` contains `-O2` |
| Touched crates (Windows) | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-macros -p div-layout -p div-engine -p divui` under the shared lock, with `main`'s Windows Skia flags (`args.gn` and `obj\hsw.ninja` contain `/clang:-ffp-contract=off`), `8106a6d2` | 7,115 passed, 0 failed, 21 ignored. Run in the first lock hold for this head (24 November, 07:19 to 07:36); the hold then stopped on a bug in the run script, and the second hold reused this log at the same head |
| Generators and ledgers (Windows) | `check_skia_flags.py`, `check_platform_scoped_tests.py`, the renderer contract, v0.2, property and FFI generators and the SP13-R, SP17, SP18 and SP20 closures with `--check`; rustfmt on each changed file; `fidelity_data.py fetch` and `verify --profile all` | All pass at `fb3c50f`. The renderer contract first reported drift, because `rule_cascade.rs` now reads the viewport's `device_scale_factor` for `@media`; the regenerated inventory lists it |
| Census and coverage | The expanded census (16 jobs) and CSS Grid coverage at 1x and with real zoom, under the shared lock, Linux, base `b7e4f6a0` against head `8106a6d2` | No change in any profile: no Div image changed, no gains, losses or errors, and every Chromium image identical. Census 16,297 of 23,728 exact (1x) and 10,404 of 17,796 (zoom); coverage 1,473 of 4,204 and 894 of 3,153 on both sides. Evidence: `div-fidelity-data` `179feb54`, `archive/docs/progress/evidence/media-queries-div-{census,census-zoom,coverage,coverage-zoom}-v1.json` |

The base results are the ones measured at `b7e4f6a0` for the outline release
pair (#143), reused unchanged. The pair predates #142: its Linux Skia flags
are `b7e4f6a0`'s, without `-O2`.
## Not done here

- `matchMedia` and `MediaQueryList.media` serialization: RFC 0014 step 3.
- Media features Chromium evaluates that Div does not yet: `color`,
  `color-index`, `monochrome`, `grid`, `device-*`, `display-mode`,
  `color-gamut`, `dynamic-range`, `prefers-contrast` and the other
  preferences, `scripting`, `update`, `overflow-*`. They are compile errors
  in `stylesheet!`; at run time (RFC 0014 step 3) they will be unknown.
- Font-relative units other than `em` and `rem` (`ex`, `ch`, `ic`, `cap`,
  `lh`, and their root forms), the small, large and dynamic viewport units,
  and `calc()` in queries: compile errors for now.
- `@layer` inside `@media`, and `@import ... media`.
- Input devices other than a mouse: `hover`, `pointer` and the `any-`
  features read a desktop mouse until the framework reports devices
  (RFC 0014). The oracle machine has a mouse and no touch screen.
- A restyle after a media flip covers the whole document; narrowing it to
  the rules whose result flipped is an optimization for later.
