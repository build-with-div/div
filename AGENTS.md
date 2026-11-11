# Div workspace instructions

## Repository

`main` is the source of truth. Div was imported from
[zhuowcui/open-ui](https://github.com/zhuowcui/open-ui) by
`tools/migrate/migrate.py`; that archived repository keeps the earlier
history, the retired C++/Blink backend, the sprint plans, the dated progress
logs and the agent memory. Link to it for history; do not copy it back.

Work is split into tracks (decision D2 in the [alpha plan](docs/alpha/plan.md)).
Each track works in its own git worktree on its own branch, with one focused
PR open at a time. Before merging, rebase the PR on the latest `main` and
rerun its checks; merge it once CI is green, then start the next PR from the
new `main`. Do not create PR stacks, merge another track's PR, force-push
`main` or leave integration work for the user.

Keep temporary builds and comparison output under the ignored `out/`
directory, and give each worktree its own `CARGO_TARGET_DIR` there (for
example `out\cargo-<track>`): concurrent builds in one target directory block
and invalidate each other.

## Track ownership

| Track | Owns |
| --- | --- |
| setup | `AGENTS.md`, `docs/alpha/plan.md`, `docs/progress/tracks/README.md`, `tools/skia_prebuilt.py`, `tools/qualification/census_lock.py`, `tools/fidelity_data.py`, `tools/migrate/` |
| engine | `crates/div-{style,style-derive,layout,paint,text,engine,geometry,dom,compositor}`, `tests/fidelity/pixel-compare` and the qualification tooling (`tools/qualification`, `tools/coverage`, `tools/wpt`, `tools/accountability`) |
| framework | `crates/divui`, `crates/div-platform`, `crates/div-macros`, `crates/div-cli`, `crates/div-ffi` |
| components | `crates/div-components` and `tests/fidelity/components`, using the public `divui` crate only |
| website | `site/` and `brand/` |
| docs (later) | `docs/guide/` and the API reference |

Paths no track owns (the workspace `Cargo.toml` and `Cargo.lock`,
`.github/workflows`, `examples`, `vendor`) may be changed by any track in a
focused PR; keep such edits minimal and rebase before merging.

A change to an API or behavior that another track depends on goes through a
one-page RFC under `docs/alpha/rfcs/` (problem, proposal, affected tracks,
migration). Merge the RFC first, or with the change once the affected tracks
have agreed.

## Shared machine resources

- Only one full census runs at a time on a machine (about 75 minutes, 16
  jobs in WSL). Run it under the lock:
  `python3 tools/qualification/census_lock.py run --wait -- <census command>`.
  `census_lock.py status` names the holder. Use `release --force` only after
  checking that the holder's census has ended. Focused tests need no lock.
- On Windows, build Skia once per set of build inputs with
  `tools/skia_prebuilt.py` and install it in each worktree, as described in
  [the development guide](docs/DEVELOPMENT.md).
- The census, the frozen-archive audit and several ledger checks read the
  evidence in [div-fidelity-data](https://github.com/build-with-div/div-fidelity-data).
  Run `python3 tools/fidelity_data.py fetch` first (`--profile all` for the
  older evidence). The files it restores are ignored by git and never edited;
  new evidence goes into a new data commit and a new pin in
  `fidelity-data.lock`.

## Read before continuing

- `docs/alpha/plan.md` and your track's file in `docs/progress/tracks/`.
- `docs/status.md`, `docs/renderer/contract.md`, `docs/engineering-principles.md`.
- `CONTRIBUTING.md` and `docs/DEVELOPMENT.md`.

## Progress and review

Each track records its progress in `docs/progress/tracks/<track>.md`.
Detailed records of a single change go in their own dated files under
`docs/progress/`.

Document each accepted behavior change and each remaining failure with its
owning subsystem, exact source, verification command and result. Distinguish
tests of the working branch from historical or private-source measurements.
Do not add selected results to a full census or claim unmeasured
qualification. Keep generated evidence reproducible and never weaken an
exactness gate. Engine changes must show no census or coverage regression
(decision D4).

Keep each PR focused on one repair or coherent feature. Stop feature work at
a requested review checkpoint; do not grow a paused PR with unrelated work.
Keep the known native clipping failure documented; this experimental
integration does not establish renderer or release qualification.
