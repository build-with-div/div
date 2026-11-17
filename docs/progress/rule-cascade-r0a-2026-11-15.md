# R0a: the rule cascade, 15 November 2026

Div can now apply style rules. An application adopts a `StyleSheet` of
`StyleRule`s, each a selector list with normal and `!important`
declarations, and the engine orders whatever matches an element as CSS
Cascade 5 orders it. This is step R0a of the rule cascade in the
[user-agent stylesheet design](../architecture/user-agent-stylesheet.md).
It is what the user-agent sheet (item 0.4), `stylesheet!`, the `style`
attribute (R0b) and SVG presentation attributes build on.

Before this, the selector work (build-with-div/div#16 to #19) could match
selectors, but nothing applied rules: an element's style came only from its
own declarations, native defaults and animations.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `StyleSheet`, `StyleRule` and `CascadeOrigin` (user agent, author). A rule has a selector list, normal declarations, `!important` declarations and an optional named layer. A sheet records its layers in order of first appearance, and `declare_layers` stands for `@layer a, b;` | `div-style` | `crates/div-style/src/stylesheet.rs` |
| `Engine::adopt_style_sheet` and `remove_style_sheet`. Adoption validates every declaration and rejects selectors with `:invalid`, `:open` or `:modal`, which the matcher does not read yet, so nothing silently fails to match | `div-engine` | `crates/div-engine/src/rule_cascade.rs` |
| Rules bucketed by the rightmost compound of each selector (id, then class, then local name, then the rest), as in Blink's `RuleSet`; an element tests only the buckets its id, classes and name select, as in `ElementRuleCollector` | `div-engine` | `crates/div-engine/src/rule_cascade.rs` |
| Cascade order. In ascending precedence: user-agent normal; presentational hints; author normal; the element's own declarations, ranked as the `style` attribute; author `!important`; user-agent `!important`. Within each, layer, then the specificity of the most specific matching selector of the rule, then order of appearance. Normal layered rules rank below unlayered ones in layer order, and `!important` reverses that order | `div-engine` | `crates/div-engine/src/rule_cascade.rs` |
| Animations stay above normal declarations and now rank below `!important` ones, as Cascade 5 places them | `div-engine` | `crates/div-engine/src/authored_source.rs` |
| The presentational-hint level, for SVG presentation attributes (RFC 0009, E2): `Engine::set_presentational_hint` and `remove_presentational_hint`, hidden until the SVG lane parses presentation attributes | `div-engine` | `crates/div-engine/src/rule_cascade.rs`, `lib.rs` |
| The cascade, the scrollbar-width capture and `Engine::cursor` read the cascaded declarations instead of the element's own | `div-engine` | `authored_source.rs`, `control_geometry.rs`, `lib.rs` |
| Conservative invalidation (below) | `div-engine` | `lib.rs`, `interaction.rs`, `rule_cascade.rs` |

### Invalidation

Nothing below runs until a sheet is adopted, so documents without sheets do
exactly what they did before.

- **An attribute** (including `id` and `class`) changes: the element's
  subtree is restyled. If a rule depends on siblings (a sibling combinator,
  `:first-child`, `:last-child`, `:only-child` or `:nth-child`), its parent's
  subtree is restyled instead.
- **Children** are inserted, moved or removed: the parent's subtree is
  restyled, and so is the former parent's.
- **`:has()`** anywhere: the whole document is restyled.
- **Interaction and form state** (hover, active, focus, focus-visible,
  checked, indeterminate, the text that decides `:placeholder-shown`): the
  whole document is restyled at the next `update()`, the way Blink
  recalculates style lazily. `computed_style` reads the last restyle until
  then.

These are Blink's "affected by" flags at their coarsest. Invalidation sets
are later performance work. The gate is the one the design sets: an
incremental restyle must equal a full re-cascade, value for value.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Cascade order and the gate | `cargo test --locked -p div-engine --test rule_cascade` (Windows) | 9 passed (below) |
| Touched crates, on `main` at `7bbddf8c` | `cargo test --locked -p div-style -p div-layout -p div-engine --no-fail-fast`, then `-p divui -p div-compositor -p div-paint` (Windows), head `510c975d` | 6,600 and 1,626 passed, 0 failed, 8 and 9 ignored |
| Touched crates, rebased on `main` at `6e7f5f02` | the same two commands, head `68e9077` | 6,607 and 1,627 passed, 0 failed, 8 and 9 ignored |
| Generators and ledgers | `check_platform_scoped_tests.py`; `generate_renderer_contract.py`, `generate_v02_contract.py`, `generate_properties.py`, `generate_ffi.py` and the four `tools/wpt` closure generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify --profile all` | all pass once `author-style-inventory.json` lists `rule_cascade.rs` (its `Iterator::position` and `HashMap::clear` calls match the generator's name search) |
| Expanded census, all four profiles (WSL) | `run_renderer_matrix.py --suite expanded` under `census_lock.py`, real-zoom oracle (RFC 0010), head `510c975d` (the later commit only regenerates the inventory), against the fidelity lane's run of `main` at `7bbddf8c` | 16,287 of 23,728 exact before and after, the real-zoom baseline's count; Chromium identical in all 23,728; 0 Div images changed ([delta](evidence/rule-cascade-r0a-div-census-v1.json)) |
| CSS Grid coverage, all four profiles (WSL) | coverage matrix, same head, against a run of `main` at `7bbddf8c` (the fidelity lane's run of the same commit matches it page for page) | 1,459 of 4,204 exact before and after; 0 statuses or Div images changed ([delta](evidence/rule-cascade-r0a-div-coverage-v1.json)) |

The census and coverage measure head `510c975d` on `7bbddf8c`; the rebased
head was not re-measured. Without an adopted sheet the cascade reads each
element's own declarations, as before, and no census or coverage page adopts
one, so the rebase onto the grid auto-placement (build-with-div/div#14) and
wheel (build-with-div/div#63) changes brings in their frames, not new ones
from R0a.

The tests in `crates/div-engine/tests/rule_cascade.rs`:

- specificity beats order of appearance, and a later rule wins a tie;
- a selector list ranks by its most specific matching selector (`div, #x`
  outranks a later `div:first-child` on `#x`);
- author normal beats user-agent normal whatever the specificity; the
  `style` attribute beats author normal rules; author `!important` beats
  the `style` attribute; user-agent `!important` beats author `!important`;
  removing sheets restores the order;
- an unlayered rule beats layered ones of higher specificity; a later layer
  beats an earlier one; for `!important`, the earliest layer wins over later
  layers and unlayered rules;
- presentational hints rank above user-agent rules and below author rules;
- matched declarations inherit, and stop when a class change unmatches;
- a sheet using `:invalid` is rejected;
- an animation overrides a normal rule and not an `!important` one;
- the gate: a sheet with child, sibling, structural, attribute, `:has()`,
  `:empty`, `:checked`, `:focus` and `:focus-within` rules, and 14
  mutations (classes, ids, attributes, detaching, inserting, removing,
  moving, checking, focusing and blurring). After each, every element's
  computed style equals the one a full re-cascade gives. While it was
  written it caught `set_checked` bypassing the state hook.

## Not done here

- `revert` and the other CSS-wide keywords: `StyleValue` cannot hold them
  yet.
- Sublayers (`a.b`) and `@layer` blocks nested in other at-rules: a layer
  name is one flat name.
- The `!important` form of the `style` attribute: `view!`'s `style:` has no
  importance. R0b's parser brings it.
- Pseudo-element rules (`::before`): the selector model has no
  pseudo-elements.
- Finer invalidation (invalidation sets, `:hover` restyling only the hover
  path), and `:hover` in the gate's corpus, which needs hit testing.
