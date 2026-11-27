# Renderer snapshots stop freezing rule-derived values, 27 November 2026

`Engine::set_renderer_style` writes one property into a node's renderer
snapshot (`NativeStyleSource::Snapshot`), which sits below the whole
cascade. It built the snapshot from `unanimated_style`: the native source
with every cascaded declaration resolved over it, from user agent rules,
presentational hints, author rules, the `style` attribute and the node's
own `set_property` declarations. So a value that a matched rule supplied
was frozen into the snapshot and outlived the rule.

The components track's dialog porter found it. Its fixtures write
snapshots while a `<dialog>` is closed, so the user agent's
`dialog:not([open])` rule supplied `display: none`, the snapshot froze it,
and the dialog stayed hidden after `show_modal`. The porter worked around
this by creating dialogs open and closing them at the end; the engine track
rejected that workaround in favour of this fix.

## Change

| Change | Owner | Source |
| --- | --- | --- |
| `set_renderer_style` builds the snapshot from the new `snapshot_base_style`: the native source and the slot's own declarations (`set_property`), resolved as the cascade resolves them. Matched rules, presentational hints and the `style` attribute are left out, so they stay live and cascade above the snapshot, and one whose rule stops matching leaves nothing behind. `unanimated_style` and the new helper share `resolve_native_source` | `div-engine` | `crates/div-engine/src/authored_source.rs`, `crates/div-engine/src/lib.rs` |
| Tests: a closed dialog's `display: none` does not outlive its rule, whether the snapshot is written before or after the dialog is connected, and closing and showing apply and remove it again; author rule values stay live above a snapshot, and a snapshot of the same property shows through once the rule stops matching; the `style` attribute stays live while the node's own declarations are frozen | test | `crates/div-engine/tests/renderer_snapshot_live_rules.rs` (new) |

The three tests fail at `main` `7796adf` and pass with the fix.

The user agent's `width: fit-content` for `dialog` still wins over a
snapshot width, as every user agent rule does over every snapshot. A
fixture that needs an author width writes it with `set_property`.

## Who is affected

`set_renderer_style` is called by pixel-compare's fixture builders
(`tests/fidelity/pixel-compare/src/main.rs` and `damage_coverage.rs`) and
by `div-engine`'s tests. Nothing in `divui`, the components or the
platform crates calls it. The census and coverage pair below shows that
no existing fixture relied on a frozen rule value.

## Verification

| Check | Result |
| --- | --- |
| New tests (`cargo test -p div-engine --test renderer_snapshot_live_rules`) | 3 pass at `3602e20`; all 3 fail at `7796adf` |
| Touched crates' suites (`div-style`, `div-macros`, `div-layout`, `div-engine`, `divui`) | Windows, with `main`'s Windows Skia flags: 7,138 passed, 0 failed, 21 ignored at `3602e20`. Linux (WSL), with CI's Linux flags: 7,140 passed, 0 failed, 17 ignored at `3602e20`. rustfmt: the 3 changed Rust files are clean |
| After rebasing onto `main` `0211362` | On Windows at `eb7a0b2`, the snapshot tests and #158's snapshot users pass: `div-engine` `partial_raster` (7), `paint_damage` (4), `renderer_snapshot_live_rules` (3) and `renderer_snapshot_source_noop` (3), and `divui` `retained_raster_teardown` (1). rustfmt: the 3 changed Rust files are clean |
| Generators and ledgers | On Windows at `eb7a0b2`, every generator and ledger check exits 0, including the renderer and v0.2 contracts, `generate_properties.py --check`, port coverage and `audit --repository-only`, except three that CI runs. The two `wpt_script` checks need `WPT_SCRIPT_ROOT`, whose directories the local WPT checkout lacks. `check_cli_template.py`'s template build compiles Skia from source in a temporary directory and hits the Windows path-length limit here. `python tools/fidelity_data.py fetch --profile all` and `verify --profile all` exit 0 at the bumped lock |
| Census and CSS Grid coverage pair | `3602e20` against `main` `7796adf`, both with `7796adf`'s CI flags, in WSL: no Div image changed in any profile. Expanded census 16,297 of 23,728 exact on both sides (1x) and 10,404 of 17,796 (real zoom); CSS Grid coverage 1,473 of 4,204 (156 errors on both sides) and 894 of 3,153 (117 errors on both sides). Evidence: `div-fidelity-data` `9a83f981`, `archive/docs/progress/evidence/renderer-snapshot-div-{census,coverage}{,-zoom}-v1.json` |
| Licence and freeze | No frozen or off-limits file is touched, and no provenance comment is edited or removed |

**Rebase onto `main` `0211362`.** The branch was measured on `7796adf` and
rebased onto `0211362`, after #157 and #158 merged. The code patch ID is
unchanged (`19f0ec7b`), and no file the PR changes was changed on `main`
apart from this track's log, where both entries are kept. The pair
therefore carries under D4. It was measured without #158's partial raster
code; #158's own pair showed that code changes no census or coverage
image.
