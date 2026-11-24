# Twelve CSS names Chromium rejects (23 November 2026)

Engine track. Twelve names that pinned Chromium 147.0.7727.50 rejects by default
are no longer CSS names in Div. Each keeps its typed `StyleProperty`, setter, C
constant and animation path. The user approved this removal.

The names are `block-ellipsis`, `hanging-punctuation`, `ruby-overhang`,
`font-synthesis-position`, `filter-blur`, `filter-grayscale`,
`scrollbar-track-color`, `scrollbar-thumb-color`, `scroll-snap-axis`,
`scroll-snap-strictness`, `scroll-snap-align-block` and
`scroll-snap-align-inline`. `line-clamp` stays until the separate
`-webkit-line-clamp` PR.

## What changed

- `tools/style/generate_properties.py` keeps one `INTERNAL_CSS_NAMES` list of 20
  names: the eight radius axes from #134 and these twelve. The generated
  `from_css_name` has no arm for them.
- `style!` or `stylesheet!` with a dropped name is now a compile error. A style
  attribute or stylesheet that uses one drops the declaration, as Chromium does.
- Typed setters, the FFI and `Element::animate` are unchanged.
- The site reference drops from 250 to 238 properties. Its data diff is exactly
  these twelve rows. `values.mdx` gains a section on setters with no CSS name.

## Chromium evidence

`tests/fidelity/fixtures/css-dropped-names-v1/chromium-windows.json` records 42
declarations of the twelve names through `CSS.supports`, `setProperty`, a style
attribute and a stylesheet, on default pinned Windows Chrome. Chromium accepted
0 of 168 route checks and 40 of 40 control checks.

**The census runs Chromium with line-clamp flags on some pages.** Its SP18 pages
launch with `--enable-blink-features=CSSLineClamp,CSSLineClampLineBreakingEllipsis,CSSLineClampLinesAndHeight,CSSListCounterAccounting`.
This removal rests on the default behavior above. With those flags:

| Probe | Platform | Result |
| --- | --- | --- |
| The 42 declarations, four routes | Windows | 8 of 168 accepted: `block-ellipsis: auto` and `no-ellipsis`. The other 11 names stay rejected |
| The 42 declarations, `CSS.supports` only | Linux | The same two values parse. The other 11 names stay rejected |
| `line-clamp`: `4`, `4 no-ellipsis`, `auto`, `none` | Windows and Linux | Parse with the flags, rejected without them |
| `line-clamp`: `4 ""`, `4 "x"`; `block-ellipsis: "x"` | Windows and Linux | Rejected with and without the flags |

Three WPT pages declare these values. This PR changes none of their fixtures:

- `css-overflow/line-clamp/block-ellipsis-023` has `line-clamp: 4 no-ellipsis`.
  It is an SP18 page, flagged Chromium clamps it to 4 lines, and the fixture does
  the same through the typed value.
- `css-overflow/line-clamp/block-ellipsis-024` has `line-clamp: 4 ""`. It is an
  SP18 page, and Chromium rejects the declaration and draws 6 lines. See the
  first open item below.
- `css-overflow/line-clamp/discard/discard-multicol-002` has
  `block-ellipsis: auto`. It is not an SP18 page, so Chromium rejects it, and the
  fixture emits no block ellipsis.

## Open items for the `-webkit-line-clamp` PR

Neither is changed here.

1. **`block-ellipsis-024` matches by accident at 3 of 4 profiles.** The fixture
   sets a clamp that Chromium rejected, and
   `crates/div-layout/src/inline/algorithm.rs:6224` treats an empty ellipsis
   string as no clamp. The match is not evidence that this behavior is right.
2. **`block-ellipsis-023` and `-024` differ at 1.5x.** The cause is not
   established.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, 1x | `5bbcfff` to `d3f4c31` | 16,297 of 23,728 exact before and after, 0 Div images changed |
| Census, real zoom | `5bbcfff` to `d3f4c31` | 10,404 of 17,796 exact before and after, 0 Div images changed |
| CSS Grid coverage, 1x | `5bbcfff` to `d3f4c31` | 1,473 of 4,204 exact before and after, 0 Div images changed |
| CSS Grid coverage, real zoom | `5bbcfff` to `d3f4c31` | 894 of 3,153 exact before and after, 0 Div images changed |
| Linux `div-style`, `div-macros`, `div-engine`, `div-components`, `divui` | `8a29c22` on `main` `db3e244` | 1,160 passed, 0 failed, 15 ignored |
| `css_dropped_names`, including every name through the animation API | `8a29c22` | 5 of 5 pass |
| Generators, Skia flag check, `verify`, format of changed files | `8a29c22` | Pass |
| Site `check-reference`, `npm test`, build | `8a29c22` | Pass |
| WPT ledger unit tests and its CSS capability check | `8a29c22` | Pass |

The receipts are in the data repository at `9c42d2a2`, under
`docs/progress/evidence/css-dropped-names-div-*-v1.json`. Both census stages
finished on their first attempt, and their logs hold no `/mnt/e` memory error.

**How the pair was measured.** The base is the `names` run of #134's code at
`5bbcfff`. The head `d3f4c31` was built on the same commit
with the same two census scripts, byte for byte, which set the same Skia flags
and use the same build directory. The build records differ only in commit,
binary and source hashes. The harness hash differs because this PR adds
`tools/qualification/capture_css_dropped_names_oracle.py`, which is in the
hashed `tools/qualification/*.py` set. Every Chromium image is identical
between the two runs.

**What the pair does not cover.** It was not rerun after the rebase onto
`db3e244`. `main` gained 53 paths in crates, tools, tests, workflows and
Cargo files after `5bbcfff`, including #132's Skia flags, #120,
#136 and #137. The patch at `8a29c22` differs from the measured one in
two ways only:

- `site/src/content/docs/reference/values.mdx` has different patch context
  and identical file text.
- `tools/style/generate_properties.py` differs in 10 comment lines and no
  code. The original comment above the internal names, which names Chromium,
  is restored word for word, and the new lines follow it.

The 15 other paths have identical patches, 2 of them generated files.

**Not run locally.** The full `tools/wpt_script/ledger.py --check` needs WPT
suites this machine's checkout lacks, so CI is its gate. The Windows suites
also run only in CI for this PR.
