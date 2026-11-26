# S3/S4 PR 2: `matchMedia`, engine side, 26 November 2026

Tier 0 items S3/S4, RFC 0014 step 3. `Document::match_media(query)` returns
a `div::MediaQueryList` whose `media()` is the query serialized as Chromium
147 writes it and whose `matches()` says whether it matches the document's
current inputs. The engine registers each list, caches its result and
reports the lists whose result changed at the HTML "evaluate media queries
and report changes" step. Platform item 1.2 (framework) calls that step from
its frame loop and fires the `change` events.

Serialization follows Blink's `MediaQuery::Serialize` and
`MediaQueryExp::Serialize` (`core/css/media_query.cc`, `media_query_exp.cc`)
and `ConditionalParser::ConsumeInner` and `ConsumeGeneralEnclosed`
(`core/css/parser/conditional_parser.cc`), all BSD and already in
`out/chromium-ref` (`SOURCES-engine-s3s4-2026-11-21.txt`). The timing comes
from the new oracle only: the LGPL `media_query_list.cc` and
`media_query_matcher.cc` were removed unread past their headers on
22 November and were not read for this PR.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| CSSOM serialization of media query lists (`impl Display`): restrictor, media type unless a plain `all` with a condition, conditions with the source's grouping, features as `name: value` or ranges with their operators, ratios as `a / b` (a bare number is `n / 1`), keywords and units in lower case, numbers as C's `%g` (six significant digits, `1.23457e+07`, `1e-05`, `-0` as `0`), `<general-enclosed>` as its text, and an invalid query as `not all` | `div-style` | `crates/div-style/src/media_query.rs` |
| `MediaCondition::Nested`: a parenthesized condition, as Blink's `ConditionalExpNode::Nested`. Without it `screen and (not (hover))` and `((width >= 600px))` lost their grouping. It evaluates as its inner condition | `div-style` | `crates/div-style/src/media_query.rs` |
| The data-model `MediaQueryList` is renamed `MediaQueryListData` (framework's review), so the web's name is free for the handle and a later public `StyleRule::with_media` does not have to rename | `div-style`, `div-engine` | `media_query.rs`, `media_query_tests.rs`, `stylesheet.rs`, `stylesheet_parser.rs`, `rule_cascade.rs` and its test |
| `matchMedia` lists: `Engine::match_media`, `media_query_list_media`, `media_query_list_matches`, `release_media_query_list` and `evaluate_media_queries_and_report_changes`, with `MediaQueryListId` (never reused). `set_viewport` (size or scale) and `set_prefers_reduced_motion` evaluate the lists at once; `set_color_scheme` does not | `div-engine` | `crates/div-engine/src/match_media.rs`, `lib.rs`, `accessibility.rs` |
| `Document::match_media` and `div::MediaQueryList` (`media`, `matches`, `Clone`, `Debug` without evaluating, `PartialEq` by identity). The handle holds the document weakly; the last clone's drop releases the engine list; `matches()` on a dropped document is `Err(Engine(StaleHandle))` | `divui` (framework surface) | `crates/divui/src/media_query_list.rs`, `document.rs`, `lib.rs` |
| The match-media-v1 oracle: 236 query texts in the 7 environments of media-queries-v1, and 24 scripted change sequences | engine qualification | `tools/qualification/capture_match_media_oracle.py`, `tests/fidelity/fixtures/match-media-v1/`, `crates/divui/tests/match_media_oracle.rs`, `crates/div-engine/tests/match_media.rs` |
| `stylesheet!` keeps the grouping of parenthesized conditions | test | `crates/divui/tests/stylesheet_macro.rs` (`media_lists_keep_their_parentheses`) |

## Chromium's timing (match-media-v1 sequences)

The CSSOM View specification does not say whether `matches` is live or when
"changed since the last time these steps were run" is measured, so the
oracle pins Chromium 147. Each sequence changes inputs over CDP
(`Emulation.setEmulatedMedia` for the preferences,
`Emulation.setDeviceMetricsOverride` for the viewport width and the device
scale factor), reads `matches` and runs frames. A `requestAnimationFrame`
counter proves no frame ran between a change and a read that is meant to
come before the next frame; every sequence passed on its first attempt.

- `matches` evaluates the current inputs when it is read, and the read
  updates the list's cached result.
- A viewport, scale or reduced-motion change evaluates every list at once
  and queues the lists whose cached result changed, once each, however often
  they flip.
- A color scheme change waits for the frame. A read before the frame moves
  the cache, so no change is reported for that list (`read-before-frame`,
  `flip-read-and-back`); a list with the same query that was not read is
  reported (`same-query-read-one`). Flipping back before the frame without a
  read reports nothing (`flip-and-back-no-read`).
- A list's baseline is its result at creation; a list created after a change
  and before the frame reports nothing at that frame
  (`created-after-change`, `created-after-width-change`).
- The frame reports the queued lists in creation order, each with its result
  at the frame (`motion-flip-and-back` reports one event, `false`).

The scale-factor sequences are CDP-emulated, because real zoom cannot change
while a page runs. They pin when lists are evaluated, not how a scaled page
renders.

## Public API

Proposed, reviewed by the framework track on 26 November, waiting for the
user's approval. See the PR for the final list.

## Gaps

Owned by `div-style`'s media query evaluator. `stylesheet!` rejects each at
compile time; `matchMedia` treats them as unknown, so they do not match.

| Query | Div | Chromium |
| --- | --- | --- |
| `(color)`, `((color))`, `(min-color: 8)` | false | true |
| `(  color  )` | false, `media` `( color )` | true, `(color)` |
| `(min-width: 2ex)`, `2ch`, `2lh`, `2dvw` | false | true |
| `(min-width: calc(10px))` | false | true |
| `screen and (min-width: calc(100px + 2em))` | false, `calc(100px + 2em)` | true, `calc(2em + 100px)` |

One sequence cannot be replayed: `scheme-and-motion-together` changes the
color scheme and reduced motion in one CDP command, which Chromium evaluates
at the next frame. Div has one setter per input, and the reduced-motion
setter evaluates at once.

`<general-enclosed>` text is rebuilt from tokens (one space per whitespace
run, comments dropped, numbers reformatted), where Blink keeps the source
text. Every `<general-enclosed>` case in the oracle agrees; none exercises
the difference.

## Verification

| Check | Result |
| --- | --- |
| match-media-v1 (`cargo test -p divui --test match_media_oracle`, `-p div-engine --test match_media`) | Pass at `1c148d6` on Windows and Linux: 236 `media` texts and 1,652 `matches` values agree outside the 10 named gaps (each gap is asserted to still differ); 23 of 24 sequences replay exactly, one named gap |
| Touched crates' suites | Windows, with `main`'s Windows Skia flags (checked in `args.gn` and the built ninja file): 7,131 passed, 0 failed, 21 ignored at `1c148d6`. Linux (WSL), with CI's Linux flags: 7,133 passed, 0 failed, 17 ignored at `1c148d6`. rustfmt: the 15 changed Rust files are clean |
| Generators and ledgers | Every generator and ledger check exits 0 on Windows at `1c148d6`, including the renderer contract, port coverage, form owner, control keyboard and restore-frozen checks, and `audit --repository-only`. The two `wpt_script` checks need `WPT_SCRIPT_ROOT`, whose directories the local WPT checkout lacks, so they were not run locally; CI runs them. `python tools/fidelity_data.py fetch --profile all` and `verify --profile all` exit 0 at the bumped lock |
| Census and CSS Grid coverage pair | `1c148d6` against `main` `e9b67689`, both with `e9b67689`'s CI flags, in WSL: no Div image changed in any profile. Expanded census 16,297 of 23,728 exact on both sides (1x) and 10,404 of 17,796 (real zoom); CSS Grid coverage 1,473 of 4,204 (156 errors on both sides) and 894 of 3,153 (117 errors on both sides). Evidence: `div-fidelity-data` `26fec8c7`, `archive/docs/progress/evidence/match-media-div-{census,coverage}{,-zoom}-v1.json` |
| Licence and freeze | No provenance comment is edited or removed. No frozen or off-limits file is touched (`div-style/src/lib.rs` is unchanged). The Blink files read are BSD |

**Why the census should not change:** pixel-compare never calls
`match_media`, and the evaluation added to `set_viewport` and
`set_prefers_reduced_motion` runs only over registered lists. The rename and
`MediaCondition::Nested` change no rule's result: the media-queries-v1
oracle still matches all 588 values.
