# Fuzz and native consumer lockfile repair, 23 November 2026

`fuzz/Cargo.lock` and the native collapsed-table consumer lock no longer
resolved under `cargo metadata --locked`. `fuzz-smoke` stopped at that command
in both scheduled runs on record, 43869914030 (16 November) and 45197262690
(23 November), so no fuzz target ran in either.

## Change

Each lock gains only what its manifest now needs. Registry packages use the
versions in the root `Cargo.lock`, which this change does not touch. Cargo also
rewrites both files in its sorted order, so their diffs are larger than the
package changes below.

| Package | Version | Licence | `fuzz` | Consumer |
| --- | --- | --- | --- | --- |
| `alloc-no-stdlib` | 2.0.4 | BSD-3-Clause | added | added |
| `brotli-decompressor` | 5.0.3 | BSD-3-Clause/MIT | added | added |
| `bytes` | 1.12.1 | MIT | added | added |
| `wuff` | 0.2.9 | MIT | added | added |
| `divui` (path) | 0.2.0 | Apache-2.0 | | added |
| `openui` (path) | 0.2.0 | | | removed |

Licences come from `cargo metadata`. The consumer's `openui` entry was the
crate's name before the rename to `divui`.

## Verification

Run with cargo 1.94.1 on Windows:

```text
cargo metadata --locked --format-version 1 --manifest-path fuzz/Cargo.toml
```

The consumer has a `Cargo.toml.in` template. It was materialized outside the
workspace as `run_matrix.py` does (root filled in, lock copied, the example as
`src/main.rs`), then checked with the same command. Both pass.

`python3 -B tools/qualification/native_collapsed_tables/test_run_matrix.py`
passes its 5 tests in WSL.

`check_dependency_security.py` reads the root and fuzz locks together. Its
`read_locks` returns the same 396 registry packages before and after this
change, with none added or lost. The four registry packages above were already
read through the root lock at these versions. They were missing only from the
fuzz lock's own entries.

## Not run locally

- The full `cargo audit`: `cargo-audit` is not installed on this machine, so
  this PR's CI job is its first full run against the repaired fuzz lock.
- `run_matrix.py` itself. Only its unit tests ran.

## Not changed

`site/showcase/Cargo.lock` also fails `--locked`. Its repair needs a manifest
patch in `site/`, so it goes to the website track.
