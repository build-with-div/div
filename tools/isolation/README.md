# Isolation tools

These tools let someone work on replacement files without seeing the current
text of a chosen set of paths. They take that set as an input file kept outside
the repository, with one repo-relative path per line. An unknown path is an
error.

| Tool | Purpose |
| --- | --- |
| `export_tree.py` | Exports a commit without the listed files, tracked copies of them, symlinks, `docs/`, agent instruction files and `.git`. Refuses a destination inside or under a project. |
| `interface_manifest.py` | Lists the names visible Rust files use from the listed files. It is not a complete interface: read its `caveat` field. |
| `build_broker.py` | Overlays candidate files on a private copy of the commit and runs `cargo check` or contained tests. |
| `sandbox_runner.py` | Started by the broker. Runs each test in its own `bwrap` sandbox and records its exit status. |

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
- **Tests:** suite names, baseline test names and `ok`, `FAILED`, `ignored`,
  `missing` or `timeout`, with each test's baseline outcome. A suite that
  cannot be listed or has a missing test is reported as incomplete.
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

## Contained tests

`test` needs Linux, `bwrap` and a user systemd manager. It refuses to run
otherwise, and refuses a work directory under `/mnt`.

- **Fixed names.** Before a candidate is applied, the broker builds the pristine
  commit and records its test names and outcomes. Only those names are reported.
  A test the candidate adds is never named, and one it removes is `missing`.
- **Containment.** Each test runs in its own `bwrap` sandbox with no network, no
  `/mnt` or home directory, and read-only views holding only visible files,
  candidates and test executables. Only a private `/tmp` is writable.
- **Memory.** A transient `systemd-run --user` scope sets `MemoryMax` (default
  8 GiB, `--test-memory-gib`) with swap off.
- **Results.** Test output goes to `/dev/null`. The outcome is the process exit
  status, recorded by a runner outside the sandbox.
- **Compile time.** Candidates that use `include!`, `include_str!`,
  `include_bytes!`, `stylesheet!` or a `path` attribute are refused. This is a
  static check, not a proof.

Custom-harness test targets cannot be listed and are reported as incomplete.

**Remaining channel.** Pass or fail cannot be fully closed. Test binaries contain
compiled code from listed files, so a hostile candidate could read that code and
encode bits in which fixed-name tests pass, or in whether a build succeeds. The
channel is bounded per run by the number of baseline tests plus the build
outcome. Treat a candidate as untrusted, and rate-limit and log broker runs.

## List format

One repo-relative path per line. `#` starts a comment. A line `locked: <path>`
withholds a file that a candidate may not replace, such as a test.
