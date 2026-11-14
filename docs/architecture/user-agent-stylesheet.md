# User-agent stylesheet: engine design

Written 13 November 2026 by the engine track, before Tier 0 item 0.4.
[RFC 0004](../alpha/rfcs/0004-style-presets.md) stands for the public API
(`StylePreset`), the licensing rule and the verification suite. This note
fixes the engine side: what the stylesheet needs from the cascade, how
the sheet is written and compiled, and the order of its parts.

## What exists, and the missing piece

After the selector work (build-with-div/div#16 to #19), the engine has:

- **Parsing:** typed selectors and Blink specificity (`div_style::selector`,
  `selector_parser`).
- **Matching:** selectors are matched on the element tree, including the
  interaction and form pseudo-classes (`Engine::matches_selector`,
  `crates/div-engine/src/selector_match.rs`).

Nothing applies rules yet. Div has no stylesheet or rule type, no cascade
origins, and no ordering of matched declarations. Today's cascade
(`prepare_native_cascade`, `crates/div-engine/src/authored_source.rs`) has
three inputs: each element's own declarations, native defaults and
animations. `matches_selector` has no caller outside its tests.

The user-agent sheet is the first sheet of rules, so the rule cascade has
to land first. The same cascade serves:

- `stylesheet!` (A1's next step, RFC 0001);
- the browser-compatible ingestion mode for third-party sheets (platform
  decision 2);
- `var()` resolution in item 0.5.

It is one engine step, **R0, the rule cascade**. It comes right after the
selector work and before items 0.3 and 0.4, as agreed on 13 November 2026.

## R0: the rule cascade

Blink's shape, reduced to what Div needs now:

- **Rules.**
  - A sheet is `StyleSheet { origin, rules }`, with origin `UserAgent` or
    `Author`. A user origin is not planned.
  - A rule is `StyleRule { selectors, declarations, layer, order }`, where
    declarations are the existing `AuthoredStyleDeclarations`.
- **Collection.** Rules are bucketed by their rightmost compound: id, then
  class, then local name, then universal, as Blink's `RuleSet` does. An
  element tests only the buckets its id, classes and name select, as in
  `ElementRuleCollector`.
- **Order.** Matched declarations are sorted as CSS Cascade 5, section 6
  orders them:
  1. origin and importance;
  2. layer;
  3. specificity (the Blink values from #16);
  4. order of appearance.

  An element's own declarations (`style:`) rank as the style attribute:
  above author rules and below author `!important`. Animations stay above
  all of them, and `revert` rolls back one origin.
- **Integration.** The sorted winners become the element's cascaded
  declarations, the input `prepare_native_cascade` takes today.
  Inheritance, computed values and animation sampling are unchanged.
  Native `display` defaults become the bottom of the user-agent origin,
  until the sheet replaces them.
- **Invalidation.** It starts conservative. A change to an element's id,
  class, attributes, interaction state or children restyles:
  - that element's subtree;
  - its later siblings' subtrees, when a sibling combinator exists.

  These are Blink's "affected by" flags. Invalidation sets are later
  performance work. The gate is the one RFC 0006 sets for paint: an
  incremental restyle must equal a full re-cascade, value for value, over
  a mutation corpus.

R0 must change no census or coverage result (D4). No fixture has a rule
until a sheet is adopted.

## The sheet

**Source.** RFC 0004's rule stands. The sheet is written from the WHATWG
HTML Rendering section, and its values are checked against Chromium 147's
computed style. No text is taken from Chromium's `html.css`, which is
LGPL. Chromium 147's computed style is the gate. Where Chromium's sheet
deviates from WHATWG, the Div sheet follows Chromium and cites the
Chromium source next to the rule. It lives as one CSS file in `div-style`.

**Compilation.** It is compiled at build time by the same compile-time
parser as `stylesheet!`, in strict mode: it is Div's own sheet, so every
rule must compile. The result is a `&'static StyleSheet` with the
`UserAgent` origin. Values that Chromium spells as internal
(`-internal-center` for `th`, quirky `em` units) are accepted only in the
`UserAgent` origin.

**Chromium facts the oracle must settle** (from the 147.0.7727.24
sources; each is confirmed by measurement before it is relied on):

- Div documents are in standards mode, so `quirks.css` does not apply,
  and quirky `em` units are `em`.
- The nested-`h1` rules for `article`, `aside`, `nav` and `section` are
  behind `SpecialRulesForNestedH1Elements`. That feature has no status in
  `runtime_enabled_features.json5`, so it is off in 147, and an `h1` is
  `2em` everywhere.
- Chromium appends a platform sheet (`LayoutTheme::ExtraDefaultStyleSheet`).
  Its rules concern form controls, which come in the forms part.
- Monospace text at `font-size: medium` is 13px in Chrome. That comes from
  the browser's default fixed font size preference; Blink's own
  `defaultFixedFontSize` setting defaults to 0. It is a font setting, not
  a sheet rule, so it belongs to the font defaults, and the oracle
  confirms the value.

## Prerequisites from item 0.3

- **Local names.** Type selectors match a node's `LocalName`, not its
  `ElementTag` (RFC 0004, prerequisite 1). Without them, `h1` and `p` are
  both `div`.
- **An `html` and `body` root.** The sheet's first rules style them:
  `display: block` and the body's 8px margin.
- **The element table** of gap 30, with an `HTMLUnknownElement` fallback.
  It covers every element the sheet names, including `pre`, `code`, `kbd`,
  `b`, `i`, `blockquote`, `dl` and `dialog`.

## Parts, in order

Each part is one PR, measured as described below.

1. **Text and sections:**
   - `html`, `body`;
   - `p`, `h1`–`h6`, `hgroup`;
   - the sectioning and grouping elements (`article` … `search`,
     `address`, `figure`, `figcaption`, `blockquote`, `center`, `hr`,
     `pre`);
   - the text-level elements (`b`, `strong`, `i`, `em`, `cite`, `var`,
     `dfn`, `code`, `kbd`, `samp`, `small`, `sub`, `sup`, `mark`, `u`,
     `ins`, `s`, `del`, `abbr`);
   - `[hidden]`.
2. **Lists:**
   - `ul`, `ol`, `menu`, `li` and the nested-list marker rules;
   - `dl`, `dt`, `dd`.

   This depends on `display: list-item` markers and `list-style-type`
   (item 0.2's literals). Any gap there is fixed first.
3. **Tables:** `table`, `caption`, `colgroup`, `col`, the row groups,
   `tr`, `td` and `th`. These include `border-spacing: 2px`, `th`'s
   centring and the `table > tr` rule.
4. **Forms:** after RFC 0004's prerequisite 3 (`appearance` and the
   styled-control rule), with the platform sheet.

## Verification

Each part has its own gates:

- **RFC 0004's suite.** Pages without author CSS, one element per case.
  Every computed longhand is compared with Chromium 147's
  `getComputedStyle` on Windows and Linux. A longhand that is not compared
  counts as a failure. Pixels must be exact at the census profiles.
- **Census and coverage.** No change while presets default to `None`.
- **End to end.** With the part adopted, removing the WPT porter's
  `_UA_DEFAULTS` entries for those elements leaves the census unchanged.
- **The sample-markdown fixtures.** Today 0 of 48 renders are exact in
  semantic mode. Each part reports the new count.
