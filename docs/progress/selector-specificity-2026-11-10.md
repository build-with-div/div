# Selectors and their specificity, 10 November 2026

Open UI has a typed selector model and a macro-time selector parser, the
third A1 step of [RFC 0001](../alpha/rfcs/0001-typed-style-rules.md) (plan
unit A1, Workstream A). Specificity is computed as Chromium 147 computes it
and checked against the values the pinned Chromium reports. Nothing matches
selectors yet and no macro emits them: `stylesheet!`, `css!`, matching and
the cascade build on this in later steps.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `selector` module: `Selector` (a complex selector stored right to left from its subject compound), `Compound` (simple selectors in source order), `SimpleSelector`, `AttributeSelector`, `PseudoClass` (the RFC's list), `RelativeSelector` for `:has()`, and `Specificity`. | `div-style` | `crates/div-style/src/selector.rs` |
| `Selector::specificity` follows Blink's `CSSSelector::Specificity()` and `SpecificityForOneSelector()` (`third_party/blink/renderer/core/css/css_selector.cc`): contributions are added from the subject compound leftwards, in source order within a compound, and a component that would overflow is clamped. `:where()` adds 0; `:is()`, `:not()` and `:has()` add their most specific argument; `:nth-child(An+B of S)` adds one class-like count plus `S`'s most specific selector. | `div-style` | same |
| `selector_parser::parse_selector_list`, behind the internal `constructor-tokens` feature like the declaration block parser. It reads CSS escapes (Tailwind class names such as `.hover\:bg-muted`), attribute selectors with the `i` flag, `An+B` and comments, and applies CSS Nesting: with a parent list, `&` becomes `:is(<parent>)`, and a selector with no `&` anywhere, including inside `:is()` or `:nth-child(… of S)`, is relative to the parent. | `div-style` | `crates/div-style/src/selector_parser.rs` |
| Pseudo-elements, namespaces, the `s` attribute flag, `:has()` inside `:has()`, a leading combinator outside nesting, `&` outside nesting, empty `:is()`, `:where()` and `:not()`, and every pseudo-class outside the RFC's list are errors. | `div-style` | same |

Compounds keep their simple selectors as a list rather than the RFC's bit
set of states, because Blink counts repeated simple selectors (`:hover:hover`
is 0-2-0) and its clamping depends on the order of contributions. The RFC
allows field layouts to change during A1.

Chromium 147 drops a selector with the `s` attribute flag
(`[data-state="open" s]`), so the parser rejects it rather than accept CSS
the pinned browser ignores.

## Verification

The change was made and measured in Open UI (`zhuowcui/open-ui`) before the
move to Div. Its last Open UI head, `aa8c18d8` on `main` at `29104c15`, was
never pushed there. The commands below use Open UI's crate names
(`openui-*`, `openui`); the Div rows at the end are from this branch in Div.
The oracle is the tracked fixture
`tests/fidelity/fixtures/selector-specificity-v1/chromium-windows.json`; the
census and coverage evidence is in
[div-fidelity-data](https://github.com/build-with-div/div-fidelity-data);
`python3 tools/fidelity_data.py fetch --profile all` restores it here.

`tools/qualification/capture_selector_specificity_oracle.py` gives each
case its own rule in one page loaded by the pinned Windows
Chrome-for-Testing 147.0.7727.50, and reads Blink's specificity for each
selector through DevTools' `CSS.getMatchedStylesForNode`
([oracle](../../tests/fidelity/fixtures/selector-specificity-v1/chromium-windows.json)). Each
rule is `X, * { … }`, or nested, `P1, * { P2, * { X, & { … } } }`, so it
matches the probe element whatever X is. The oracle has 93 accepted cases:

- every supported simple selector, combinator and pseudo-class;
- shadcn and Tailwind v4 patterns, escaped class names among them;
- overflow at 300 classes, IDs and types;
- an order-dependent clamp: `.c` × 255 followed by ` :is(.d e)` is 0-255-1
  in Chromium, because the subject's `:is()` is added first;
- 18 nesting cases, including two levels and `&` inside `:is()`, `:where()`,
  `:not()` and `:nth-child(… of S)`.

It also has 18 selectors that Chromium drops.

| Check | Command | Result |
| --- | --- | --- |
| Chromium oracle | `python tools/qualification/capture_selector_specificity_oracle.py --chrome E:\open-ui\out\chrome-win\win-147.0.7727.50\chrome-win64\chrome.exe` | 93 accepted, 18 rejected |
| Parser against the oracle | `cargo test -p openui-macros --test selector_specificity` (Windows) | 2 passed: every accepted case parses to Chromium's specificities, every rejected case is an error. With the clamping order reversed (leftmost compound first), the first test fails on the order-dependent case, 0-255-0 against Chromium's 0-255-1 |
| Parser unit tests | `cargo test -p openui-style --features constructor-tokens --lib selector` (Windows) | 7 passed |
| Touched crates | `cargo test -p openui-macros -p openui-style -p openui-style-derive -p openui` (Windows) | 457 passed, 0 failed, 12 ignored |
| `openui-style` with the feature | `cargo test -p openui-style --features constructor-tokens --lib` (Windows) | 124 passed, 0 failed |
| Generators | `generate_properties.py --check`; `generate_v02_contract.py --check` (the dependency policy hash follows the lockfile, which gains `serde_json` as a dev-dependency of `openui-macros`); `generate_renderer_contract.py --check`; `check_platform_scoped_tests.py` | no drift |
| `cargo fmt --all --check` | in `bindings/rust` | pass |
| Paired expanded census (WSL) | `wsl-branch-census-a8.sh`, base `9d7bccf6` | 22,517 of 23,728 exact before and after; 0 changed |
| CSS Grid coverage (WSL) | coverage matrix, base `04d1eeab` | 2,218 of 4,204 exact before and after; 0 changed |
| Div: touched crates | `cargo test --locked -p div-macros -p div-style -p div-style-derive -p divui --no-fail-fast` (Windows) | 457 passed, 0 failed, 12 ignored (as in Open UI) |
| Div: `div-style` with the feature | `cargo test --locked -p div-style --features constructor-tokens --lib` (Windows) | 124 passed, 0 failed (as in Open UI) |
| Div: generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check` | pass |
| Div: expanded census (WSL) | `run_renderer_matrix.py --suite expanded`, head `9d9ecbb3` (#16–#19 together, on `5a315f42`), against the run of `5b7b8e47`, whose engine code was `main`'s | 22,519 of 23,728 exact before and after; Chromium identical in all comparisons; 0 Div images changed ([delta](evidence/selector-stack-div-census-v1.json)) |
| Div: CSS Grid coverage (WSL) | coverage matrix, same head and base | 2,227 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/selector-stack-div-coverage-v1.json)) |

The Div census measured the four selector PRs together, at the head of
#19, which contains this change. Nothing renders from selectors yet, so it
checks that rendering is unchanged. The stack was then rebased on
`dd8ba04d`, which adds build-with-div/div#33, #39 and #41; none of them
touches selectors. The receipts are in div-fidelity-data `1c158de0`.

The paired expanded census in WSL has no change. The base is `9d7bccf6`,
the recorded census base; the head is `4022b7e9`, this change. Both are
built with the CI `SKIA_GN_ARGS` and share one Chromium cache. Chromium
and Open UI are each identical in all 23,728 comparisons: 22,517 exact
before and after, with nothing gained or lost. Nothing renders from
selectors yet, so this checks that rendering is unchanged. The delta is in
the [census evidence](evidence/selector-specificity-census-v1.json).

The CSS Grid coverage suite has no change either
([coverage evidence](evidence/selector-specificity-coverage-v1.json)). The
base is `04d1eeab`, the recorded coverage baseline; the head is `4022b7e9`.
In all 4,204 measured comparisons the status and the Open UI image are
unchanged: 2,218 exact before and after. The two references that the
baseline lists as unmeasured, whose renderer memory is unbounded, are left
out of both runs.

## Not done here

- Matching selectors against elements, the cascade, invalidation, and the
  `stylesheet!` and `css!` macros that emit these types.
- The parser is stricter than Chromium where Chromium is forgiving: an empty
  or invalid argument in `:is()` or `:where()` is an error here, and a
  top-level `&` (which Chromium treats as `:scope`) is an error.
- `*|a` and other namespace forms Chromium accepts are errors here; Open UI
  has no namespaces.
