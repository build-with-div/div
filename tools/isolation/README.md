# Isolation tools

These tools let someone work on replacement files without seeing the current
text of a chosen set of paths. They take that set as an input file kept outside
the repository, with one repo-relative path per line. An unknown path is an
error.

| Tool | Purpose |
| --- | --- |
| `export_tree.py` | Exports a commit without the listed files, tracked copies of them, symlinks, `docs/`, agent instruction files and `.git`. Refuses a destination inside or under a project. |
| `interface_manifest.py` | Lists the names visible Rust files use from the listed files. It is not a complete interface: read its `caveat` field. |
| `build_broker.py` | Overlays candidate files on a private copy of the commit and runs `cargo check` or `cargo test`. |

```powershell
python tools/isolation/export_tree.py --withheld-list <list> --dest <outside-project>
```

```powershell
python tools/isolation/interface_manifest.py --withheld-list <list> --out <manifest.json>
```

```powershell
python tools/isolation/build_broker.py check --withheld-list <list> --work-dir <private-dir> --manifest <manifest.json> --census-lock <census.lock> --candidate crates/example/src/file.rs=<candidate.rs>
```

The export deliberately omits `AGENTS.md`, `CLAUDE.md` and `.claude`. A session
started in the export would load them automatically, and they point to project
history and documents that the export withholds.

## What the broker returns

- **Builds:** level, error code, identifiers, visible path, line and column,
  failed packages and packages Cargo did not reach.
- **Tests:** suite names, test names and `ok`, `FAILED` or `ignored`. A suite
  whose results cannot be fully parsed is reported as incomplete.
- **Never:** compiler message text, rendered output, source spans, suggestions,
  test output or Cargo's stderr.

An identifier is returned only if it already appears in a visible Rust file, a
candidate or the manifest. Others are counted in `withheld_identifiers`. As a
last check, a result containing a line from a listed file is replaced by
`filter_abort`.

## Running it

The operator runs the broker, because its work directory holds the full tree.
An implementer should not be given that path. For requests without it, run
`build_broker.py serve --inbox <dir>`. The implementer writes candidate files and
`<id>.request.json` to the inbox and reads `<id>.result.json`:

```json
{"command": "check", "candidates": {"crates/example/src/file.rs": "candidate.rs"}}
```

The broker limits Cargo to 4 jobs, sets `CARGO_INCREMENTAL=0`, and refuses to
build below 50 GB free. It also refuses a Skia workspace unless
`SKIA_NINJA_COMMAND` is set. It cannot check that the command limits ninja, so
point it at a wrapper that runs `ninja -j4 "$@"`. On Windows, `--census-lock`
blocks builds while the lock file exists.

These tools filter output. They do not sandbox the implementer's file access,
so the session must still be started without access to the work directory.
The `test` command also runs candidate code with the broker's own file and
network access, so a candidate test could bypass the filter. Do not run it on a
candidate until that step is contained.
