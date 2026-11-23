# Internal CSS names (23 November 2026)

Engine track. The eight corner radius axis names are no longer CSS names in
Div. Each keeps its typed `StyleProperty`, setter and C constant.

## What changed

- `tools/style/generate_properties.py` has a new `INTERNAL_CSS_NAMES` list, and
  the generated `from_css_name` has no arm for those names. `style!`, a style
  attribute, a stylesheet and the tools that read names from the schema can no
  longer reach them from CSS text. Pinned Chromium 147.0.7727.50 rejects all
  eight.
- `crates/divui/tests/native_rounded_clip.rs` uses the typed constants.
- The site's style table filters by the generated list, and its generator's path
  to `crates/` is fixed. The published property count drops from 258 to 250,
  because those eight were never CSS properties. The site's data diff is those
  eight rows only.

This does not mean the remaining 250 names are all real CSS. A separate audit
found 24 that pinned Chromium does not know. Twelve of them are removed by the
next PR, which the user approved.

## Verification

| Check | Commit | Result |
| --- | --- | --- |
| Census, 1x | `4e69bdf` to `5bbcfff` | 16,297 of 23,728 exact before and after, 0 Div images changed |
| Census, real zoom | `4e69bdf` to `5bbcfff` | 10,404 of 17,796 exact before and after, 0 Div images changed |
| CSS Grid coverage, 1x | `4e69bdf` to `5bbcfff` | 1,473 of 4,204 exact before and after, 0 Div images changed |
| CSS Grid coverage, real zoom | `4e69bdf` to `5bbcfff` | 894 of 3,153 exact before and after, 0 Div images changed |
| Linux `div-style`, `div-macros`, `div-engine`, `div-components`, `divui` | `d62d081` on `main` `8fbf18e` | 1,154 passed, 0 failed, 15 ignored |
| Generators, Skia flag check, `verify`, format of changed files | `d62d081` | Pass |
| Site `check-reference`, `npm test` (10 of 10), build | `d62d081` | Pass |
| WPT ledger unit tests and its CSS capability check | `d62d081` | Pass |

The receipts are in the data repository at `5d5ef0fb`, under
`docs/progress/evidence/internal-css-names-div-*-v1.json`.

**What the census pair does not cover.** It was measured at `4e69bdf` against
`5bbcfff`, before #132 (Skia flags) and #120 (engine top layer), and was not
rerun on the new Skia flags. The code patch is unchanged by the rebase:

```text
git diff <base> <head> -- . ':!site/src/content/docs/reference/values.mdx' | git patch-id --stable
4e69bdf..5bbcfff  ef97708c7bfc99b24489f0e1720afae78eaecfc5
8fbf18e..d62d081  ef97708c7bfc99b24489f0e1720afae78eaecfc5
```

The only other difference is the sentence in
`site/src/content/docs/reference/values.mdx` about the radius axes, which no
longer says they are in the properties reference.

**Not run locally.** The full `tools/wpt_script/ledger.py --check` needs WPT
suites this machine's checkout lacks, so CI is its gate. The Windows suites also
run only in CI for this PR.
