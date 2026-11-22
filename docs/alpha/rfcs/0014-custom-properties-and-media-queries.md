# RFC 0014: Custom properties and media queries

Status: Approved by the user on 21 November 2026. Owner: the engine track
(`div-style`, `div-macros` style parsing, `div-engine` cascade). Consumers:
the framework track (`divui` wrappers, `matchMedia` events, platform item
1.2), the components track (shadcn and Geist tokens), the SVG and canvas
lanes, the fidelity lane (UA origin, the ingestion mode) and the samples.
Platform plan: Tier 0 item 0.5, rows S3 and S4.

## Problem

RFC 0001 reserves `var()` ("a later engine RFC (A2 tokens) specifies
them") and models `@media` without implementing it. On `main`:

- `--x` declarations are rejected by `style!` and `stylesheet!`
  (`declaration_block.rs`) and dropped from `style` attributes
  (`inline_style.rs`), as are `var()`, `env()`, `attr()` and the CSS-wide
  keywords (`inherit`, `initial`, `unset`, `revert`, `revert-layer`).
- `@media` is a compile error in `stylesheet!`. The only environment input is
  `Document::set_prefers_reduced_motion`, and nothing reads it in a
  selector or rule. There is no color scheme input and no `matchMedia`.
- `div-components/src/theme.rs` stands in for shadcn's tokens.

The samples need both:

- Tailwind v4 composes utilities through internal variables;
- shadcn themes are token sheets switched by `.dark` or
  `prefers-color-scheme`;
- Radix, cmdk and react-spring write custom properties at runtime
  (decision 2 of 13 November);
- charts and motion gate animation on `prefers-reduced-motion`, both in CSS
  and through `matchMedia`.

## Proposal

The design follows Blink's `StyleCascade` (Chromium 147,
`core/css/resolver/style_cascade.cc`, BSD): a value that contains `var()`
is kept as text and substituted when the element's style is computed. Only
then is it parsed, with the property's grammar. Strict `stylesheet!` keeps
checking everything it can at compile time.

### Values (`div-style`)

```rust
pub enum StyleValue { /* existing variants */,
    Wide(CssWideKeyword),       // inherit, initial, unset, revert, revert-layer
    Unparsed(UnparsedValue) }   // a value with var() in it, or a custom property's value
pub struct UnparsedValue { /* original text, token boundaries, referenced names */ }
impl UnparsedValue { pub fn parse(text: &str) -> Result<Self, ValueError>; pub fn as_str(&self) -> &str; }
pub struct CustomPropertyName(/* interned, starts with "--" */);
```

- **CSS-wide keywords.** They become a typed declaration value for every
  property, as RFC 0001 planned. Substitution can produce them, and custom
  properties take them, so they land here first.
- **Custom property declarations.** `--name: <declaration-value>` is valid
  for any balanced token sequence that Chromium 147 accepts, `!important`
  included, and is stored as an `UnparsedValue`. `Style` keeps a single
  declaration list in source order, so `color: red; color: var(--c)` lets
  the second declaration win at specified-value time, as in Chromium.
- **Standard properties with `var()`.** A longhand stores
  `StyleValue::Unparsed`. A shorthand gives each of its longhands a
  pending substitution that names the shorthand, as Blink's
  `CSSPendingSubstitutionValue` does. The shorthand's text is substituted
  and parsed once per element.
- **Compile time.** The macros check token balance and the `var()` syntax,
  including fallbacks, and that every `--name` is well formed. They do not
  type a value that contains `var()`, because its result is not known until
  substitution.

### Computed-value time (`div-engine`, `authored_source.rs`)

For each element, after the cascade has chosen the winning declarations
(RFC 0001's priority order, unchanged):

1. **Custom properties.**
   - Each element starts from its parent's computed custom properties:
     every unregistered custom property inherits.
   - The winning declaration for each name applies on top.
   - A value that references other custom properties is resolved on
     demand. A lock set detects cycles, as Blink's `CascadeResolver`
     does.
   - Every property in a cycle becomes guaranteed-invalid.
   - A value that, after substitution, is exactly a CSS-wide keyword acts
     as that keyword (`ResolveCustomProperty`).
2. **Substitution.** Each `var(--x, fallback)` is replaced by `--x`'s
   computed value. A guaranteed-invalid value uses the fallback, or makes
   the declaration invalid when there is none (`ResolveVarInto`).
3. **Parse.** The substituted text goes through the runtime parser that the
   `style` attribute already uses (`inline_style::parse_value`). The
   property's grammar is the same as at compile time.
4. **Invalid at computed-value time.** A failure at step 2 or 3 makes the
   declaration `unset`: inherited properties inherit and the rest take
   their initial value (`ResolveVariableReference` returns
   `CSSUnsetValue`).
   - A value Chromium accepts but Div's parser does not yet accept also
     lands here.
   - Such a value is a named fidelity gap with its owning property, never
     a silent difference.

`ComputedStyle` gains `custom_properties`: a persistent map shared with the
parent while an element sets none of its own (Blink's `StyleVariables`
trie). It is read with `ComputedStyle::custom_property(&self, name) ->
Option<&str>`. The text is the computed value as Chromium 147's
`getPropertyValue` serializes it; the oracle pins that serialization.

### Registered custom properties (`@property`)

- `@property --x { syntax; inherits; initial-value }` in `stylesheet!`.
- A registered property's computed value is typed by its syntax (`<length>`,
  `<color>`, `<number>`, `<percentage>`, `<length-percentage>`, `<angle>`,
  `<time>`, `<integer>`, `*` and lists of them).
- Registered properties that do not inherit start from their initial value.
- A registered `<length>` that uses `em` depends on `font-size`, so its
  cycle rules follow Blink's (`HasFontSizeDependency`).
- Registered properties interpolate in transitions and animations.

`CSS.registerProperty()` is out of scope until a runtime API asks for it.

### Runtime writes (`divui` wrappers, engine implementation)

```rust
impl Element {
    pub fn set_custom_property(&self, name: &str, value: &str) -> Result<(), Error>;
    pub fn remove_custom_property(&self, name: &str) -> Result<bool, Error>;
}
```

- These are element-attached, like `set_property`, and form the
  `style.setProperty('--x', …)` path of decision 2.
- An invalid name or an unbalanced value returns an error.
- A standard property can take `var()` at runtime through the existing
  `set_property(property, StyleValue::Unparsed(UnparsedValue::parse(..)?))`.
- The `style` attribute accepts `--x` declarations, `var()` and the CSS-wide
  keywords.
- **Invalidation (RFC 0001, PR 4 machinery):**
  - A write restyles the element and the descendants that inherit the
    property.
  - The style diff marks only the invalidation class of the properties
    whose computed values changed, so a color token repaints without
    layout.
  - An index of which elements read each `--name` may narrow this further.
    That is an internal detail, not API.

### Media queries

- **Data model.** RFC 0001's `MediaFeature`, plus:
  - `Resolution` (dppx, with `x`, `dppx`, `dpi` and `dpcm`);
  - `Orientation`, `AspectRatio`, `AnyHover` and `AnyPointer`;
  - the Level 4 range syntax (`(width >= 40rem)`,
    `(400px <= width <= 700px)`);
  - `and`, `or`, `not` and `only`, and the media types `all`, `screen` and
    `print`. Div is always `screen`.
- **Where queries appear.**
  - Top-level and nested `@media` in `stylesheet!` and `css!`, inside
    `@layer` too.
  - `StyleRule::with_media(MediaQueryList)` for sheets built in code.
- **Errors.**
  - An unknown feature or value is a compile error in strict mode.
  - In the runtime parser it makes the query false, as Chromium's
    "not all".
- **Values** (Blink's `MediaValues` and `media_query_evaluator.cc`):
  - `width` and `height` are the viewport size in CSS px from
    `ViewportMetrics`, measured as Blink's `ViewportSizeForMediaQueries`.
  - `resolution` is the device scale factor as an `f32`. `dpcm` is compared
    at two decimals (`EvalResolution`).
  - `prefers-color-scheme` comes from `Document::set_color_scheme`
    (default `light`).
  - `prefers-reduced-motion` comes from the existing
    `set_prefers_reduced_motion`.
  - `hover`, `any-hover`, `pointer` and `any-pointer` default to a
    desktop mouse (`hover`, `fine`) until the framework reports other
    input devices.
  - `forced-colors` is `none`.
  - Relative lengths in queries resolve as Blink's media values do; the
    oracle pins the font size they use.
- **Evaluation.**
  - When an input changes (viewport size, scale factor, color scheme,
    reduced motion), the next `update()` evaluates each distinct query once.
  - It restyles under the rules whose result flipped, as RFC 0001's
    "Invalidation and restyle" section specifies.
  - Nothing is evaluated while no sheet with `@media` is adopted.

### `matchMedia` (engine side; the framework owns the event API)

```rust
impl Document { pub fn match_media(&self, query: &str) -> MediaQueryList; }
impl MediaQueryList { pub fn media(&self) -> String; pub fn matches(&self) -> bool; }
```

- `media()` returns the serialized query.
- The runtime parser takes `query`. An invalid query serializes as
  `not all` and never matches.
- A list stays registered while any handle to it is alive.
- **Changes.** The engine evaluates registered lists at the HTML
  "evaluate media queries and report changes" step, which the framework's
  frame loop calls once per frame.
  - It reports the lists whose result flipped, in creation order.
  - The framework turns that feed into `change` events
    (`MediaQueryListEvent`) and an `on_change` wrapper in platform item
    1.2. Item 1.2 also wires the OS color scheme, reduced motion and
    `devicePixelRatio` into the inputs above.

## Affected tracks

- **Framework.**
  - `divui` re-exports the wrappers above.
  - `view!`'s `style:` accepts `--x` and `var()`.
  - Item 1.2 delivers `change` events and the OS inputs right after the
    `matchMedia` PR, ahead of the rest of T3: charts are blocked on
    reduced motion.
  - The final shape and documentation of the wrappers stay with the
    framework, as in RFC 0001.
- **Components.**
  - `theme.rs` is replaced by real token sheets (shadcn, Geist), switched
    by `.dark` or `prefers-color-scheme`.
  - No component API changes.
- **SVG lane.** The V1 paint properties (`fill`, `stroke` and the rest
  listed in RFC 0009 §4) join the `div-style` schema, so `var()` reaches
  them. The engine adds them and wires `div-svg` to read the computed
  style; the SVG lane reviews the `div-svg` part.
- **Canvas lane.**
  - `fillStyle` and `strokeStyle` take no `var()`: Chromium parses them
    without an element.
  - The separate colour PR (`hsl()`, `hwb()`, the rest of modern `rgb()`)
    reaches `parse_css_color` unchanged.
- **Fidelity lane.**
  - The UA origin enters the same cascade. Origin insertion stays separate
    from rule storage, so these PRs rebase cleanly.
  - The browser-compatible ingestion mode (decision 2) reuses this RFC's
    runtime parser and its drop report.
- **Breaking change.** `StyleValue` gains two variants. Exhaustive matches
  outside `div-style` (`div-engine`, `div-components`, tests) add arms in
  the PR that adds the variants. No behavior changes for code that does
  not use the new syntax.
- **Out of scope:**
  - `env()`, `attr()`, `if()`, `@function` and mixins;
  - `@container`, `@import` media lists and runtime `@supports`;
  - `CSS.registerProperty()`;
  - `var()` in `@keyframes`, which lands with A2's `@keyframes` (Tier 1.7).
    The existing `Element::animate` takes typed values and needs no
    substitution.

## Migration

Each PR is green on `main` by itself, has its own record, and runs the full
census and CSS Grid coverage. The census and coverage fixtures adopt no
sheet and set no custom property, so neither may move (D4).

1. This RFC, approved on 21 November 2026.
2. **`@media` in sheets**, the evaluator and `Document::set_color_scheme`.
   RFC 0001 already covers these, so this PR may start before this RFC is
   approved.
3. **`matchMedia`, engine side.** The framework's item 1.2 follows.
4. **CSS-wide keywords, custom properties and `var()`** in `style!`,
   `stylesheet!`, `css!` and the `style` attribute.
5. **Runtime writes** and their invalidation.
6. **`@property`.**
7. **SVG paint properties** in the cascade, and `var()` in them.

The colour-parser PR has no dependency and lands whenever it is ready.

## Verification

- **New oracles** under `tests/fidelity/fixtures/`, in `cascade-v1`'s
  format (one HTML and one CSS file per case; `getComputedStyle` dumps from
  pinned Chromium 147, custom properties included):
  - **`media-queries-v1`:** the four census profiles with real zoom; two
    viewport widths; both color schemes; both reduced-motion values.
  - **`custom-properties-v1`:** inheritance, fallbacks, cycles, values
    invalid at computed-value time, CSS-wide keywords and shorthands.
  - **Scripted cases:** runtime writes and `matchMedia` change sequences,
    driven over CDP and through the public API.
- **WPT reftests** ported as coverage areas: `css-variables`, `mediaqueries`
  and `css-properties-values-api`. Their testharness tests follow RFC 0005's
  script translation.
- **Unit tests** for the parser, cycle detection and restyle scope, as PR 4
  has.
- **Census and coverage nonregression** at all four profiles for every PR.
