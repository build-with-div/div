# R0b: the `style` attribute, 17 November 2026

A `style` attribute now styles its element, read as Chromium 147 reads one. This is step R0b of the rule cascade in the [user-agent stylesheet design](../architecture/user-agent-stylesheet.md), the second PR of Tier 0 item 0.1. It builds on R0a's cascade ([record](rule-cascade-r0a-2026-11-15.md)). It closes Md MD-24 and editor gap 22, where a `style` attribute was ignored.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `parse_style_attribute`: splits a declaration list as CSS Syntax 3 does. Each declaration Div cannot use is dropped and reported with its text and reason, and the rest apply. Dropped: unknown properties, invalid values, bad strings (a newline inside), stray closing brackets and at-rules. The end of the text closes open comments, strings and brackets. Importance is kept per declaration | `div-style` | `crates/div-style/src/inline_style.rs` |
| Values go through the same `parse_literal` as `style!`, with ASCII case-insensitive keywords and units. `style!` keeps its strict mode, which rejects a whole block for one bad declaration | `div-style` | `inline_style.rs` |
| `parse_literal` follows Chromium where the oracle found it too lenient or too strict: a number and its unit are one token (`10 px` and `10.px` are not lengths); opacity and the other number properties take only CSS numbers; `opacity` takes a percentage (CSS Color 4); `normal` is a value of `letter-spacing` and `word-spacing` only | `div-style` | `crates/div-style/src/property.rs` |
| Setting, changing or removing the attribute applies it. Its normal declarations rank at the `style` attribute's level, just below the element's own declarations (`view!`'s `style:` and the typed setters), which override it. Its `!important` declarations rank above author `!important` rules and below user-agent ones. A clone keeps it | `div-engine` | `crates/div-engine/src/rule_cascade.rs`, `lib.rs` |
| `Engine::dropped_style_declarations` reports what the attribute dropped, in source order. Chromium drops the same declarations silently | `div-engine` | `rule_cascade.rs` |
| The census harness ignores the recorded `style` text. The translator already lowers each element's cascaded style, inline style included, into typed setters. Applying the text again would rank it above values the cascade gave to `!important` rules | `pixel-compare`, tests | `tests/fidelity/pixel-compare/src/main.rs`, `crates/div-engine/tests/fragmented_grid_positioned_ownership.rs` |

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `tools/qualification/capture_style_attribute_oracle.py` on Windows with pinned Chrome 147.0.7727.50, written to `tests/fidelity/fixtures/style-attribute-v1/chromium-windows.json` | 54 `style` attributes; for 9 probe properties, the value and priority Chromium keeps |
| Oracle test | `cargo test --locked -p div-engine --test style_attribute` (Windows) | All 54 cases keep what Chromium keeps, apart from 2 named gaps (below) |
| Workspace tests | CI on this PR's head: `rust-parity` (Linux) and the `windows-msvc` headless workspace tests | Reported in the PR. A local Windows run of the touched crates was held back: no Windows build runs during a census on this machine |
| Generators and ledgers | `check_platform_scoped_tests.py`, the renderer, v0.2, property, FFI and WPT closure generators with `--check`, `cargo fmt --all --check`, `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `72ee8e74` against R0a at `e57ec047` | 16,287 of 23,728 exact before and after; Chromium identical in all 23,728; 0 Div images changed ([delta](evidence/style-attribute-r0b-div-census-v1.json)) |
| CSS Grid coverage, all four profiles (WSL) | coverage matrix, same head and base | 1,473 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/style-attribute-r0b-div-coverage-v1.json)) |

The census measures `72ee8e74`, on R0a's pre-merge head `e57ec047`. This PR is rebased onto `main` at `457eea1a`, which adds R0a's state-hook commits (#70) and #71 to #74; none of those was in either measured commit. The R0b commit itself differs from `72ee8e74` in three ways, none of which any census or coverage page reaches:
- it removes an SVG presentation-attribute parser mode that nothing called, with its test (moved to E2, below);
- it drops that mode's 23 oracle cases;
- it adds three `opacity` style cases to the oracle (`50%`, `normal`, `0.5e`).

Chromium keeps `opacity: 50%` as 0.5 and drops the other two, as Div does. The first 51 style cases are byte-identical to the earlier capture.

The tests:
- `style_attributes_keep_what_chromium_keeps`: the oracle.
- `the_attribute_applies_restyles_and_reports_what_it_dropped`.
- `the_attribute_ranks_as_cascade_5_places_it`: author normal < the attribute < the element's own declarations < author `!important` < the attribute's `!important` < user-agent `!important`.
- `without_sheets_the_attribute_alone_applies`.
- Parser unit tests in `inline_style.rs`: dropping, end-of-text closing, stray brackets, importance and CSS numbers.

## Named gaps

| Case | Chromium | Div | Owner |
| --- | --- | --- | --- |
| `color: var(--x)` (case 31) | keeps it | drops it, reported | item 0.5 (custom properties and `var()`) |
| `color: inherit` (case 32) | keeps it | drops it, reported: `StyleValue` cannot hold CSS-wide keywords | item 0.5 |

## Not done here

- **SVG presentation attributes:** moved to E2, the SVG properties PR after 0.1. There they land with their callers and a per-attribute Chromium oracle. The parser mode and its 23 measured cases are kept on a local branch as E2's starting point. Two rules from the SVG lane are pending that oracle: an unparsable length used as 0 (measured for `rx` only), and f32 digit-by-digit numbers (measured only for path data, points and transforms; `stroke-dasharray` measures as CSS-style double parsing).
- **The divui API for the dropped list:** the framework track owns it.
- **`<style>` elements and stylesheet text:** S2, and 0.1 PR 3 (`stylesheet!`).
