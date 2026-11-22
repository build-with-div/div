# 0.1 PR 4: restyle on state change, 18 November 2026

A state or attribute change now restyles only the elements whose matching
it can change, and marks only the work their changed properties need.

Before this PR:

- any interaction or form state change restyled the whole document at the
  next `update()` whenever an adopted selector read any state;
- an attribute change restyled the element's subtree, its parent's subtree
  when any rule depended on siblings, or the document when any rule used
  `:has()`;
- every rule restyle marked `Subtree`, so a hover color change relaid out
  the page.

This is the fourth PR of item 0.1 (the S1 cascade), and implements RFC
0001's "Invalidation and restyle" section. The behavior matches a full
re-cascade exactly; only the amount of work changes. No sheet-free path
changes, so the census and coverage cannot move (RFC 0001, D4).

## Design

RFC 0001 describes the design after Blink's rule invalidation data
(`core/css/invalidation/`). No Chromium source was read for this PR.

- **The index.** When a sheet is adopted or removed, the rule set records,
  for each class, id, attribute name and state pseudo-class its selectors
  read, where it occurs:
  - in a subject compound: the element itself matches it;
  - in an ancestor compound;
  - in a sibling compound: a preceding sibling, or an ancestor of one,
    matches it;
  - inside `:has()`.

  `:is()`, `:where()` and `:not()` arguments take their compound's position.
  An `:nth-child(… of S)` argument counts as a sibling one, because whether
  an element matches it moves its later siblings' indices. The RFC builds
  the index at compile time; building it at adoption keeps the `StyleSheet`
  API unchanged, and reads the same selectors the matcher reads.
- **The scope of a change** is the widest position among the features it
  touches:

  | Position | Restyles |
  | --- | --- |
  | None | Nothing |
  | Subject | The element alone. If its own style changes, its subtree too |
  | Ancestor | The element's subtree |
  | Sibling | The parent's subtree |
  | `:has()` | The document |

- **Attributes.** `set_attribute` and `remove_attribute` look up the
  attribute's name, and for `class` and `id` each token added or removed.
  Reordering classes touches no token. They restyle at once, as before.
- **Attributes the engine writes itself** (a control's `value`, the `open`
  and `selected` it mirrors, and its `data-divui-` markers) now restyle the
  rules that read them, at the next `update()`. On `main` those rules
  restyled only incidentally, when a state rule triggered a document
  restyle.
- **State.** Each element's style records the state bits it was resolved
  with: one bit per state pseudo-class some selector reads, among `:hover`,
  `:active`, `:focus`, `:focus-visible`, `:focus-within` and the form
  pseudo-classes. At the next `update()` after a state change, elements
  whose bits differ are the candidates, scoped by those states' positions.
  So a pointer move restyles the hover path it changed. This compares
  results rather than trusting each state-change site to name the elements
  it changed, so a site that changes state cannot be missed.
- **Invalidation classes.** A restyle compares each element's old and new
  computed style. It marks the invalidation class of each longhand property
  whose value changed. A shorthand (`border-top`, say) is left out, because
  its longhands carry the change with their own classes. A difference that
  no property accounts for marks `Subtree`, as before, and so does a value
  that does not survive a round trip through its property. To find such a
  difference, both styles are rewritten with the changed values and must
  then be equal.
- **`ComputedStyle` gains `PartialEq`**, with its fields struct and the 19
  value types it holds that lacked it, so a restyle can tell an unchanged
  element from a changed one.

Children changes are unchanged: the parent's subtree, or the document
when a rule uses `:has()`. Media-query flips wait for item 0.5.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `InvalidationIndex`, `RestyleScope`, scoped restyles, state bits and the per-slot state cache, engine-written attributes, invalidation classes from the style diff | `div-engine` | `crates/div-engine/src/rule_cascade.rs` |
| One element resolved without its descendants; the resolved state recorded when a style is published | `div-engine` | `crates/div-engine/src/authored_source.rs` |
| State pseudo-class matching for the state bits | `div-engine` | `crates/div-engine/src/selector_match.rs` |
| `set_attribute` and `remove_attribute` pass the old and new values; the focus marker writes go through the engine-attribute path | `div-engine` | `crates/div-engine/src/lib.rs` |
| A control's `value`, `open`, `selected` and the selection and composition markers go through the engine-attribute path | `div-engine` | `crates/div-engine/src/interaction.rs` |
| `PartialEq` on `ComputedStyle` and its value types | `div-style` | `crates/div-style/src/computed.rs`, `crates/div-style/src/layout_systems.rs` |
| Restyle scope tests | engine qualification | `crates/div-engine/tests/rule_cascade.rs` |

`Engine::record_selector_restyles` and `Engine::take_selector_restyles`
(`#[doc(hidden)]`) let tests read which elements were resolved again.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Restyle scopes and classes | `cargo test --locked -p div-engine --test rule_cascade` | 22 passed. New tests below |
| Windows suites | `cargo test --locked --no-fail-fast -j 4 -p div-style -p div-text -p div-paint -p div-svg -p div-engine -p divui` at `fadf26f2` | 3,474 passed, 0 failed |
| Linux suites (WSL, CI's Skia flags) | the same crates at `fadf26f2` | 3,477 passed, 0 failed |
| Generators and ledgers | `check_platform_scoped_tests.py`, the frozen archive, the renderer contract, the SP13-R, SP17, SP18 and SP20 closures, CSS Grid coverage port, v0.2, property, FFI and form case generators with `--check`; `cargo fmt --all --check`; `fidelity_data.py verify --profile all` | All pass |
| Expanded census, all four profiles, real-zoom oracle, against `main` `c65b1245` | `div-census.sh` and `div-census-zoom.sh` under the shared lock, at `fadf26f2` | 16,296 of 23,728 exact before and after. 0 gains, 0 losses, 0 errors; 0 Div images changed |
| CSS Grid coverage | The same runs | 1,473 of 4,204 exact (1x run) and 894 of 3,153 (real zoom), before and after. 0 changed |

The census and coverage adopt no style sheet, so no image moves, as
expected. The evidence is div-fidelity-data `f97a017b`
(`docs/progress/evidence/state-invalidation-pr4-div-census-v1.json`,
and `-div-coverage-v1.json` and `-div-coverage-zoom-v1.json` beside
it); `fidelity-data.lock` pins it.

### After rebasing onto `main` `2c25496a`

`main` gained #103 (T5 and T6), #104 (SVG markers) and #105 (T3). T3's
evidence, div-fidelity-data `c4b87acd`, is the parent of this change's
`f97a017b`. The head `92ef89d8` was measured again:

| Check | Result |
| --- | --- |
| Windows suites: `div-style`, `div-text`, `div-layout`, `div-paint`, `div-svg`, `div-engine`, `divui` | 9,469 passed, 0 failed |
| Linux suites, the same seven crates (WSL, CI's Skia flags) | 9,474 passed, 0 failed |
| SVG text corpus, Linux | 24,056 position numbers, none differ; 264 of 290 captures exact, as on `main` |
| Generators, `cargo fmt --check`, `fidelity_data.py verify --profile all` | All pass. SP20 could not be checked locally (neither of this machine's WPT checkouts is the pinned tree now); this change touches none of its inputs |

The census was not rerun: it adopts no style sheet, so this change
cannot move it.

The new tests:

- **Subject `:hover`:** `subject_hover_restyles_the_hover_path_and_repaints_only`.
  - A pointer entering item 1 restyles the list (probed, unchanged), item 1
    and its label.
  - Moving on to item 2 restyles items 1 and 2 and their labels, not the
    list.
  - A color change advances the paint generation and leaves the tree,
    intrinsic and layout generations.
- **Layout properties:** `a_hover_padding_change_marks_layout`. A `padding`
  change marks layout.
- **Ancestor `:hover`:** `ancestor_hover_restyles_the_subtree`.
- **Attribute scopes:** `attribute_changes_restyle_by_where_selectors_read_them`.
  - Restyle nothing: an unread class, an unread attribute, an unread id,
    reordered classes, and removing an unread attribute.
  - Restyle the element and its child: a subject class, an ancestor
    attribute and a subject id.
- **Sibling and `:has()` scopes:**
  `a_sibling_feature_restyles_the_parent_subtree` and
  `a_relational_feature_restyles_the_document`.
- **Engine-written attributes:**
  `engine_written_attributes_restyle_at_the_next_update`. The focus marker
  restyles at the next `update()`, not before.
- **The gate on state restyles:** `scoped_state_restyles_equal_a_full_cascade`.
  - Nine rules: subject, ancestor and sibling `:hover`, `:has(:active)`,
    `:is(...):active`, `:nth-child(2 of :hover)`, `:focus-within` and
    `:not(:hover)`.
  - Nine steps: hover moves, press, release, focus, blur and the pointer
    leaving the window.
  - After each step the result equals a full re-cascade.

The existing gate, `incremental_restyles_equal_a_full_cascade`, and the
pointer, focus and form state tests pass unchanged.

## Not done here

- **`set_data(AttrId, AtomId)` and attribute atoms**, deferred to this PR
  by the PR 3 plan. They are a typed API over `set_attribute` and do not
  change invalidation, so they can land as a small follow-up.
- **Narrower scopes**:
  - a sibling feature restyles the whole parent subtree, where only later
    siblings can change;
  - a `:has()` feature restyles the document, where only the anchors can
    change.

  Blink narrows both with invalidation sets. These are performance
  limits, not fidelity gaps.
- **A performance measurement** (A5). The scope tests count elements; no
  timing was taken.
