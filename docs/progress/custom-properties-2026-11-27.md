# S3/S4 PR 3: custom properties, `var()` and the CSS-wide keywords, 27 November 2026

Tier 0 items S3/S4, RFC 0014 step 4. `--name` declarations,
`var()` with fallbacks and the CSS-wide keywords (`inherit`, `initial`,
`unset`, `revert`, `revert-layer`) work in `style!`, `stylesheet!` and
`css!` blocks, in the `style` attribute and in `Element::set_pseudo_style`.
`ComputedStyle::custom_property(name)` reads a computed custom property as
Chromium's `getPropertyValue` serializes it. Run-time writes
(`set_property`, `apply_style`) take them in step 5; until then they
return an error.

The computed-value behaviour follows Blink's `StyleCascade`
(`core/css/resolver/style_cascade.cc` and `.h`, `css_variable_data.h`,
`style/style_variables.h`, all BSD and already in `out/chromium-ref`,
`SOURCES-engine-s3s4-2026-11-21.txt`). Where those files and the
specifications leave a choice, the new custom-properties-v1 oracle pins
Chromium 147.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| Values: `CssWideKeyword`, `CustomPropertyName`, `UnparsedValue` (checked as a `<declaration-value>` with well-formed `var()`s, and split once into literal runs and references, so the macros' output never tokenizes), `CustomDeclaration` and `CustomProperties` | `div-style` | `crates/div-style/src/custom_property.rs` (new), `custom_property_tests.rs` (new), `css_tokens.rs` |
| `StyleValue::Wide` and `StyleValue::Unparsed`; `Style` keeps a second, ordered list of custom declarations (`with_declaration`, `with_custom_property`, `custom_declarations`) | `div-style` | `property.rs`, `authored_style.rs` |
| The computed field `custom_properties` and `ComputedStyle::custom_property`. Within the licence-rewrite freeze: one field line and one initializer line in `computed.rs`, one row in `internal-style-fields.csv`, and one `mod` and one `pub use` line in `lib.rs`. The row also generates the public accessor `ComputedStyle::custom_properties()` | `div-style` | `computed.rs`, `lib.rs`, `internal-style-fields.csv`, `generated_properties.rs`, `generated_computed_accessors.rs` |
| Parsers: declaration blocks and the `style` attribute take `--name`, `var()` and the keywords; a custom declaration in a sheet may hold `{}` blocks. `parse_literal` stays typed, so `view!`'s `style:`, C and the canvas and component parsers still reject them | `div-style` | `declaration_block.rs`, `inline_style.rs`, `stylesheet_parser.rs` |
| `style!`, `stylesheet!` and `css!` emit declarations in source order through hand-written `ConstructorTokens` | `div-macros` (framework reviews) | `style_macro.rs`, `stylesheet_macro.rs`, `view.rs` |
| Computed-value time: custom properties inherit; references resolve on demand with a lock stack for cycles; substitution writes `/**/` where tokens would merge; a value invalid at computed-value time becomes `unset`; `revert` and `revert-layer` roll back to earlier origins and layers | `div-engine` | `crates/div-engine/src/custom_properties.rs` (new), `rule_cascade.rs`, `authored_source.rs`, `lib.rs`, `control_geometry.rs` |
| Run-time writes before step 5: `Engine::set_property` returns `InvalidInput` for a keyword or `var()`; `Element::apply_style` checks the whole `Style` first and returns `InvalidInput` naming the kind of its first custom property, keyword or `var()`, applying nothing | `div-engine`, `divui` | `lib.rs`, `crates/divui/src/element.rs`, `scroll_button.rs` |
| The `style` attribute's gaps 31 (`var()`) and 32 (CSS-wide keywords) are closed | test | `crates/div-engine/tests/style_attribute.rs` |
| The custom-properties-v1 oracle | engine qualification | `tools/qualification/capture_custom_properties_oracle.py`, `tests/fidelity/fixtures/custom-properties-v1/`, `crates/divui/tests/custom_properties_oracle.rs` |
| Macro output equals the run-time parse, field for field and in order | test | `crates/divui/tests/style_macro.rs` |
| The author-style inventory lists the new values | generated | `docs/renderer/generated/author-style-inventory.json` |

## Chromium's behaviour (custom-properties-v1)

`capture_custom_properties_oracle.py` records `getComputedStyle` in pinned
Windows Chromium 147.0.7727.50 for every element with an `id` in 10 cases:
the standard probes and every custom property a case names, 2,788 values
for 62 elements. Two captures are byte-identical.

- **Inheritance.** Every custom property inherits. Names are
  case-sensitive (`--pad` and `--Pad` are two properties).
- **Cycles.** Every member of a cycle, including `--self: var(--self)`, is
  guaranteed-invalid and serializes as the empty string. A property that
  only references a cycle uses its fallback (`--outside: var(--r, 7px)` is
  `7px`). A property that references its inherited value forms a cycle.
- **Fallbacks.** A fallback is everything after the first comma, so
  `var(--missing, 1px, 2px)` is `1px, 2px`. An empty fallback is valid and
  empty. Fallbacks nest.
- **Invalid at computed-value time.** A standard declaration whose
  substituted value does not parse, or whose reference has no value and no
  fallback, becomes `unset`: an inherited property inherits and the others
  take their initial value, and an earlier valid declaration in the same
  block does not come back.
- **CSS-wide keywords** are ASCII case-insensitive. A custom property set
  to exactly `initial` is guaranteed-invalid; `inherit`, `unset` and
  `revert` inherit (no user agent sheet sets custom properties).
  `revert-layer` rolls back to the previous layer.
- **Importance and layers.** Custom declarations cascade by importance and
  layer as standard ones do, including the reversed layer order for
  `!important`. An `!important` `var()` declaration wins over a later
  normal one.
- **Shorthands.** `var()` in a shorthand is substituted before the
  shorthand is expanded; a later longhand still overrides its part, and a
  shorthand whose substitution fails sets every longhand to `unset`. `em`
  in a substituted value resolves against the element's own font size, and
  in `font-size` against the parent's.
- **Serialization.** A custom property's value keeps its text, case,
  inner whitespace and number form (`1e3`, `+.5`, `-0`), without leading
  and trailing whitespace and comments. A substituted value keeps the
  referenced text as written.
- **The `style` attribute** takes custom properties, `var()` and the
  keywords, and cascades above the sheet as other declarations do.
- **Token merging.** Where substitution puts two runs side by side and
  their edge tokens would merge into one token, Chromium writes `/**/`
  between them (Blink's `TokenSequence::Append`). The `token-merging` case
  measures every pair of 17 left and 15 right edge tokens. The result is
  CSS Syntax 3's serialization table, which `TokenEdge::needs_comment`
  holds.

## Public API

Reviewed by the framework track and approved by the user on 27 November
("approve all 3"), including the doc-hidden items and the generated
`ComputedStyle::custom_properties()`. The list is the one framework
reviewed, with its four changes taken, and with the run-time errors as
`InvalidInput`, which framework accepted. Everything is in `div-style` and
is reached as `div::typed_style::…`; nothing goes at `divui`'s top level.

Values (new file `custom_property.rs`):

1. `CssWideKeyword { Inherit, Initial, Unset, Revert, RevertLayer }`,
   `#[non_exhaustive]`, with `from_css` (ASCII case-insensitive, ignoring
   surrounding whitespace), `as_css` and `Display`.
2. `CustomPropertyName`: opaque. `new(&str) -> Result<Self, LiteralError>`
   accepts exactly one `--` ident; `as_str()` includes the `--`.
3. `UnparsedValue`: opaque. `parse(&str) -> Result<Self, LiteralError>`
   and `as_str()`, the text as Chromium serializes it.
4. `CustomDeclaration`: `name()` and `value()`, with no public
   constructor. It records how many standard declarations precede it.
5. `CustomProperties`: opaque, with `NONE`, `get(name)` and `iter()` in
   name order.

`Style` and computed style:

6. `StyleValue::Wide(CssWideKeyword)` and `StyleValue::Unparsed(UnparsedValue)`.
7. `Style::with_declaration` and `Style::with_custom_property`, which
   append in call order.
8. `Style::custom_declarations() -> &[CustomDeclaration]`, in source order.
9. `ComputedStyleFields.custom_properties: CustomProperties`.
10. `ComputedStyle::custom_property(&self, name: &str) -> Option<&str>`:
    `None` for an unset, guaranteed-invalid or malformed name; `Some("")`
    for an empty value.
11. `ComputedStyle::custom_properties() -> &CustomProperties`, generated
    from the schema row.

Parsers (hidden modules and existing public types):

12. `parse_literal` stays typed and unchanged.
13. `declaration_block::BlockDeclaration` becomes `{ target: BlockTarget,
    important, source }`, with `BlockTarget::Property(StyleProperty,
    StyleValue)` or `BlockTarget::Custom(CustomDeclaration)`, and
    `property()` and `value()` accessors that return `Option`.
14. `InlineCustomDeclaration { declaration, important }` and
    `ParsedStyleAttribute.custom: Vec<InlineCustomDeclaration>`.

Run-time writes before step 5:

15. `Engine::set_property` returns `Err(EngineError::InvalidInput(..))`
    for `Wide` and `Unparsed`, so `Element::set_property` and
    `ScrollButton::set_property` do too. `Element::apply_style` and
    `ScrollButton::apply_style` return it for a `Style` with any custom
    property, keyword or `var()`, before applying anything.

Doc-hidden, not public API:

16. For the macros: `CustomPropertyName::from_checked`,
    `UnparsedValue::from_checked`, `UnparsedSegment` and `TokenEdge`.
17. Plumbing: `parse_substituted`, `CustomValue`,
    `UnparsedValue::{segments, has_references, wide_keyword, substitute}`,
    `CustomProperties::{get_shared, with_changes}`, and the additions to
    the hidden `AuthoredStyleDeclarations`, `AuthoredStyleContribution` and
    `CanonicalStyleFields`.
18. `runtime_value_limit` and `runtime_style_limit`, which exist only for
    the step-4 guards in item 15 and go away in step 5.

## Breaking changes for the alpha notes

- `StyleValue` stays exhaustive and gains `Wide` and `Unparsed`, so an
  exhaustive `match` on it no longer compiles.
- `ParsedStyleAttribute` and `ComputedStyleFields` gain a public field, so
  struct literals of them no longer compile.
- `BlockDeclaration`'s `property` and `value` fields become `target`, with
  `Option` accessors.
- `style!`, `stylesheet!` and `css!` keep declarations in source order, and
  `Style`'s `PartialEq` compares that order.
- `set_property` and `apply_style` can now return `InvalidInput` for a
  keyword, `var()` or (for `apply_style`) a custom property.

## Gaps and left work

| Gap | Owner | Where it shows |
| --- | --- | --- |
| `border: var(--one) solid var(--c)` with a comma-separated `rgb()`: `div-style`'s `border` shorthand rejects the substituted value, so `border-top-width` is `unset` | `div-style` shorthand parser (engine) | custom-properties-v1 `shorthands` `s3`, a named gap |
| `calc()` over lengths in `margin` and `padding` (`calc(var(--n) * 2px)`, `calc(var(--one) + 1px)`) | calc() PR 2, with its owner | `shorthands` `s5` (two probes) and `token-merging` `m2`, named gaps |
| Run-time writes of custom properties, keywords and `var()`, and interleaved CSSOM iteration (`item(i)`; the stored position is there for it) | engine, RFC 0014 step 5 | `set_property` and `apply_style` return `InvalidInput` |
| Keywords and `var()` in `view!`'s `style:` and in C | framework | `parse_literal` stays typed |
| The WPT `css/css-variables` coverage area | engine, a follow-up PR | not ported in this PR |

Each named gap is asserted to still differ, so a fix must remove it.

## Verification

| Check | Result |
| --- | --- |
| custom-properties-v1 (`cargo test -p divui --test custom_properties_oracle`) | Pass at `5470201` on Windows and at `f9ac163` on Linux: the 2,788 values agree outside the 4 named gaps |
| Macro and run-time parse agree (`cargo test -p divui --test style_macro`) | Pass at `5470201`, 11 tests, including declaration order and the step-4 run-time errors |
| The `style` attribute (`cargo test -p div-engine --test style_attribute`) | Pass at `5470201` with no gaps left |
| Touched crates' suites | Windows, with `main`'s Windows Skia flags: 7,172 passed, 0 failed, 21 ignored at `5470201`. Linux (WSL), with CI's Linux flags: 7,174 passed, 0 failed, 17 ignored at `f9ac163`, the code commit (`5470201` adds only Markdown). Both counts include the tests #160 added. rustfmt: the 23 changed Rust files it checks are clean (the two generated `include!` targets are skipped), and it reformats no neighbouring line in a frozen file |
| Generators and ledgers | On Windows at `5470201`, every generator and ledger check exits 0, including `generate_properties.py --check` with the regenerated accessors, the renderer contract and `audit --repository-only`, except three. The two `wpt_script` checks need `WPT_SCRIPT_ROOT`, whose directories the local WPT checkout lacks, so CI runs them. `check_cli_template.py` failed here because its template build compiles Skia from source in a temporary directory and hit the Windows path-length limit (`GetFullPathNameA … too long`); CI runs it. `python tools/fidelity_data.py fetch --profile all` and `verify --profile all` exit 0 at the bumped lock (`91520766`) |
| Census and CSS Grid coverage pair | `f9ac163` against `main` `087de2b`, both with `087de2b`'s CI flags, in WSL: no Div image changed in any profile. Expanded census 16,297 of 23,728 exact on both sides (1x) and 10,404 of 17,796 (real zoom); CSS Grid coverage 1,473 of 4,204 (156 errors on both sides) and 894 of 3,153 (117 errors on both sides). Evidence: `div-fidelity-data` `91520766`, `archive/docs/progress/evidence/var-div-{census,coverage}{,-zoom}-v1.json` |
| Licence and freeze | `computed.rs`: one field line and one initializer line, with no existing line changed or moved. `lib.rs`: one `mod` and one `pub use` line. `internal-style-fields.csv`: one row. No provenance comment is edited or removed, and no off-limits file is touched. The Blink files read are BSD |

## Rebase onto `main` `087de2b`

The branch was first measured at `986a583` on `main` `8e626ab`. #160
(renderer snapshots stop freezing rule-derived values) merged as `087de2b`
first, and both changes edit `crates/div-engine/src/authored_source.rs`. #160
split the native source into `unanimated_style` and `snapshot_base_style`,
which both call `resolve_native_source` with a slice of contributions. This
branch passes whole declaration lists instead, because
`custom_properties::substitute` needs the custom declarations and the
positions of every declaration. The resolution keeps #160's split:
`resolve_native_source` now takes `&AuthoredStyleDeclarations`,
`unanimated_style` passes `&cascaded.declarations`, and `snapshot_base_style`
passes `&slot.authored`. The code's patch ID changed (`cc6e4c6f` to
`33cc5fcd`), so the census and coverage pair, the suites and the generator
checks were run again at the rebased head instead of being carried; the
`986a583` measurements are history.

A renderer snapshot freezes no custom property, for two reasons.

- What it resolves holds none. `snapshot_base_style` resolves `slot.authored`,
  which only `set_property`, `remove_property` and animation staging write.
  `set_property` refuses CSS-wide keywords and `var()` with `InvalidInput`
  until step 5 (`div_style::runtime_value_limit`), staging requires a value of
  the property's own type (`value_matches_property`), and `StyleProperty`
  names no custom property. `divui`'s `apply_style` checks every declaration
  with `runtime_style_limit` before writing any. Custom declarations and
  `var()` reach a node only through rules, the `style` attribute and
  pseudo-element declarations, which stay declarations and are resolved again
  each time.
- What it inherits is replaced on every use. The snapshot source keeps the
  custom properties it inherited from the parent when it was taken, but every
  later resolution from that source, the cascade in `authored_source.rs` and
  `unanimated_style` alike, goes through `resolve_native_declarations`. That
  function first sets the style's custom properties to
  `custom_properties::substitute(declarations, &parent.custom_properties)`,
  computed from the parent's current values, so the inherited copy is never
  read.
