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
| A CSS Syntax 3 tokenizer and component-value reader for media query text, private to `div-style` | `div-style` (engine) | `crates/div-style/src/css_tokens.rs` |
| Media query data model (RFC 0014 "Data model"): lists, `not`/`only` and media types, `and`/`or`/`not` conditions, `<general-enclosed>`, plain, boolean, `min-`/`max-` and Level 4 range tests of `width`, `height`, `aspect-ratio`, `orientation`, `resolution`, `-webkit-device-pixel-ratio`, `prefers-color-scheme`, `prefers-reduced-motion`, `hover`, `any-hover`, `pointer`, `any-pointer` and `forced-colors`. Parsing is browser-compatible (an invalid query is `not all`, an unknown term is unknown) and lists each problem for strict callers. Evaluation is Blink's three-valued logic, with its comparisons: one layout unit (1/64 px) of slack for `>=`, `<=` and `=` on sizes, the viewport truncated to integers for `aspect-ratio`, the scale factor as an `f32` for `resolution`, and `dpcm` compared at two decimals | `div-style` | `crates/div-style/src/media_query.rs`, `media_query_tests.rs` |
| `StyleRule::with_media` and `media` | `div-style` | `crates/div-style/src/stylesheet.rs` |
| The sheet parser accepts `@media` at every level above; declarations directly in a nested `@media` keep the parent's selectors, in source order. A query Chromium would read as `not all` or unknown, a media type other than `all`, `screen` and `print`, a feature or unit Div does not evaluate yet (`color`, `ch`, `calc()`, ...), `@media` without a block and `@layer` inside `@media` are compile errors | `div-style` | `crates/div-style/src/stylesheet_parser.rs` |
| `stylesheet!` emits each rule's media lists as typed constructors | `div-macros` | `crates/div-macros/src/stylesheet_macro.rs` |
| The rule set keeps each distinct stack of media lists once with its result, and matching skips rules whose result is false. `set_viewport` (size or scale), `set_prefers_reduced_motion` and the new `Engine::set_color_scheme` evaluate the stacks again; if any result flipped, the document restyles at the next `update()` and the paint generation is bumped, so a frame loop that compares generations redraws. Nothing is evaluated without `@media` rules | `div-engine` | `crates/div-engine/src/rule_cascade.rs`, `lib.rs`, `accessibility.rs` |
| `Document::set_color_scheme` and `color_scheme`, and the `ColorScheme` re-export (RFC 0014 "Public API") | `divui` (framework surface, as the RFC lists it) | `crates/divui/src/document.rs`, `lib.rs` |
| The media-queries-v1 oracle: 84 queries, each turning one element opaque, captured in 7 environments | engine qualification | `tools/qualification/capture_media_oracle.py`, `tests/fidelity/fixtures/media-queries-v1/`, `crates/divui/tests/media_oracle.rs` |

## Public API, decided by the user on 24 November

The orchestrator put the new public items to the user with a narrower
version. The user's answer, verbatim: "yes approve".

- `css_tokens` is a private module of `div-style`.
- `media_query` is `#[doc(hidden)] pub mod`. Its types stay public because
  `stylesheet!` output builds them at `::div::typed_style::media_query` and
  `div-engine` uses them. `parse_media_query_list` and `ParsedMediaQueryList`
  stay public inside it for `matchMedia` (RFC 0014 step 3).
- `MediaQuery::matches` and `MediaQuery::not_all` are `pub(crate)`.
- `StyleRule::media_matches`, which had no caller, is removed.
  `StyleRule::with_media` and `media` stay public.
- Documented API: `Document::color_scheme()`, `Document::set_color_scheme()`
  and the root `div::ColorScheme` re-export. `Engine::color_scheme` and
  `Engine::set_color_scheme` stay public because `divui` calls them.

## Constraints on later steps

From the framework track's review:
- A future value type for the CSS `color-scheme` property must not be named
  `ColorScheme`. That name is the `prefers-color-scheme` input here; Chromium
  calls it `PreferredColorScheme`.
- Before `matchMedia` (RFC 0014 step 3) takes the name `MediaQueryList` for
  its public handle, rename the data-model `MediaQueryList` (for example to
  `MediaQueryListData`) or keep its module hidden.

## Known existing gap (not changed here)

`Engine::note_selector_state_change` sets the paint flag without bumping a
dirty generation, the pattern this PR fixes for media flips. A state change
from outside input handling may therefore not request a frame on its own.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `tools/qualification/capture_media_oracle.py` on Windows with pinned Chrome 147.0.7727.50 | 7 environments (200x200, 600x400, 800x600 and 400x600 at 1x; 640x480 at 1.25x and 800x600 at 2x, real zoom; 600x400 dark with reduced motion), 84 elements, 326 matches |
| Redraw after a media flip | `cargo test --locked -p div-engine --test rule_cascade` (Linux, `dde50838`'s CI Skia flags), at `8b5bec1` | The new `a_media_flip_bumps_a_dirty_generation_before_update` fails without the fix (paint generation stays 0) and passes with it; 26 of 26 pass |
| Generators and ledgers (Windows), final code | `check_skia_flags.py`, `check_platform_scoped_tests.py`, the renderer contract, v0.2, property and FFI generators and the SP13-R, SP17, SP18 and SP20 closures with `--check`; rustfmt on each changed file | All pass at `9c04e2f` on `main` `4dc219ed`; 15 changed files are clean |
| Touched crates (Linux and Windows), final head | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-macros -p div-layout -p div-engine -p divui` | SUITES-PENDING: run when E: is back above its 50 GB free-space floor |
| Census and coverage, final head | One full pair, base `main` `4dc219ed` against the final head, both sides with `4dc219ed`'s `ci.yml` flags | CENSUS-PENDING: waits for E: to clear and for the orchestrator's go-ahead |

**Expected census result (not evidence):** no change. The code that reacts to
media changes runs only when an adopted sheet has `@media` rules, and the
census adopts none. pixel-compare never calls `adopt_style_sheet`,
`stylesheet!` or `with_media`, and the dialog user agent sheet has no media
lists. The pair above is what confirms this.

**Earlier measurements (history; superseded).**
- **Census pair:** measured at `8106a6d2` on `main` `b7e4f6a0`. No profile
  changed: 16,297 of 23,728, 10,404 of 17,796, 1,473 of 4,204 and 894 of
  3,153. Evidence: `179feb54`.
- **Why it is superseded:** it measured the code before the redraw fix and
  the API changes, so the code patch ID moved on from `b1912556`.
- **Linux suites:** 1,143 passed at `8106a6d2`, and 1,149 passed at
  `fb3c50f` with `-O2`, 0 failed each time.
- **Windows suites:** 7,115 passed, 0 failed, at `8106a6d2`.
- **Oracle:** all 588 values matched at both heads.

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
