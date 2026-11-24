#!/usr/bin/env python3
"""Build and test a candidate against the full tree, returning filtered results.

The broker overlays candidate files on a private copy of a commit and runs Cargo.
Results are built from an allowlist of fields: error code, identifiers already
visible to the implementer, visible path and position, test names and outcomes.
Compiler message text, rendered output, source spans, suggestions, test output
and Cargo's stderr are never returned, stored or logged. Candidate tests run in a
bubblewrap sandbox, and their names come from a list fixed before the candidate
is applied.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import posixpath
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import threading
import time
import tomllib
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable, Iterable, Iterator, Mapping

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.isolation.interface_manifest import strip_rust_noise  # noqa: E402
from tools.isolation.sandbox_runner import clean_env, sandbox_prefix  # noqa: E402
from tools.isolation.withheld import (  # noqa: E402
    IsolationError,
    is_excluded_root,
    load_view,
    normalize_repo_path,
    path_key,
    read_blobs,
    refuse_unsafe_location,
)

SCHEMA = "div-build-broker-v1"
MAX_JOBS = 4
MAX_DIAGNOSTICS = 500
MAX_IDENTIFIERS = 16
MIN_GUARDED_LINE = 8

_CODE = re.compile(r"^(?:E\d{4}|[a-z][a-z0-9_]*(?:::[a-z][a-z0-9_]*)*)$")
_QUOTED = re.compile(r"`([^`\n]{1,200})`")
_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
# One token, such as a name, path or selector. The allowlist already limits these,
# and matching them as substrings would abort on harmless paths and test names.
_BARE_LINE = re.compile(r"[A-Za-z0-9_.:/-]+[,;]?")
_PACKAGE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")
_TEST_FILTER = re.compile(r"^[A-Za-z0-9_:]{1,200}$")
_REQUEST_ID = re.compile(r"^[A-Za-z0-9_-]{1,64}$")
_INBOX_FILE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$")
_FILE_READ = re.compile(
    r"\b(?:include|include_str|include_bytes|stylesheet)\s*(?:!|\bas\b)"
    r"|::\s*include(?:_str|_bytes)?\b|\binclude_(?:str|bytes)\b"
)
_PATH_ATTRIBUTE = re.compile(r"#\s*!?\s*\[(?:[^\[\]]|\[[^\]]*\])*\bpath\s*=")
_KEYWORDS = frozenset(
    "as async await break const continue crate dyn else enum extern false fn for if impl in let "
    "loop match mod move mut pub ref return self Self static struct super trait true type unsafe "
    "use where while".split()
)


@dataclass(frozen=True)
class FilterContext:
    """Paths are maps from a case-folded key to the tracked spelling."""

    root: str
    visible: Mapping[str, str]
    candidates: Mapping[str, str]
    vocabulary: frozenset[str]


def normalize_root(path: Path | str) -> str:
    return posixpath.normpath(str(path).replace("\\", "/").removeprefix("//?/")).casefold()


def classify_path(raw: object, context: FilterContext) -> tuple[str, str | None]:
    """Classify a compiler path. Only tracked spellings are ever returned.

    Anything inside the tree that is not a known visible file counts as withheld.
    """
    if not isinstance(raw, str) or not raw:
        return "withheld", None
    value = raw.replace("\\", "/").removeprefix("//?/")
    if value.startswith("<"):
        return "external", None
    if value.startswith("/") or re.match(r"^[A-Za-z]:/", value):
        normalized = posixpath.normpath(value)
        if not normalized.casefold().startswith(context.root + "/"):
            return "external", None
        relative = normalized[len(context.root) + 1 :]
    else:
        relative = posixpath.normpath(value)
        if relative == ".." or relative.startswith("../"):
            return "withheld", None
    key = path_key(relative)
    if key in context.candidates:
        return "candidate", context.candidates[key]
    if key in context.visible:
        return "visible", context.visible[key]
    return "withheld", None


def _walk_span(span: dict) -> Iterator[dict]:
    pending = [span]
    while pending:
        current = pending.pop()
        if not isinstance(current, dict):
            continue
        yield current
        expansion = current.get("expansion")
        if isinstance(expansion, dict):
            pending.extend(s for s in (expansion.get("span"), expansion.get("def_site_span")) if s)


def iter_spans(message: dict) -> Iterator[dict]:
    pending = [message]
    while pending:
        current = pending.pop()
        for span in current.get("spans") or ():
            yield from _walk_span(span)
        pending.extend(c for c in current.get("children") or () if isinstance(c, dict))


def _identifiers(message: dict, context: FilterContext) -> tuple[list[str], int]:
    """Return sorted visible identifiers from quoted names in the top-level message.

    Sorting and de-duplicating prevents a token sequence from rebuilding a line.
    """
    text = message.get("message")
    if not isinstance(text, str):
        return [], 0
    found = {
        token
        for quoted in _QUOTED.findall(text)
        for token in _IDENTIFIER.findall(quoted)
        if token not in _KEYWORDS
    }
    visible = sorted(found & context.vocabulary)
    return visible[:MAX_IDENTIFIERS], len(found - context.vocabulary) + max(0, len(visible) - MAX_IDENTIFIERS)


def filter_diagnostic(message: dict, context: FilterContext) -> dict | None:
    """Reduce one rustc diagnostic to allowlisted fields."""
    raw_level = message.get("level")
    if raw_level == "warning":
        level = "warning"
    elif raw_level in ("error", "error: internal compiler error"):
        level = "error"
    else:
        return None
    raw_code = message.get("code")
    code = raw_code.get("code") if isinstance(raw_code, dict) else None
    code = code if isinstance(code, str) and _CODE.match(code) else None
    top_spans = [s for s in message.get("spans") or () if isinstance(s, dict)]
    if code is None and not top_spans:
        return None

    in_withheld = any(classify_path(s.get("file_name"), context)[0] == "withheld" for s in iter_spans(message))
    location = None
    for span in (s for s in top_spans if s.get("is_primary")):
        kind, relative = classify_path(span.get("file_name"), context)
        if kind in ("visible", "candidate"):
            line, column = span.get("line_start"), span.get("column_start")
            location = (
                relative,
                line if isinstance(line, int) else None,
                column if isinstance(column, int) else None,
            )
            break
    identifiers, withheld_count = _identifiers(message, context)
    return {
        "level": level,
        "code": code,
        "identifiers": identifiers,
        "withheld_identifiers": withheld_count,
        "path": location[0] if location else None,
        "line": location[1] if location else None,
        "column": location[2] if location else None,
        "in_withheld_file": in_withheld,
    }


def reads_files_at_compile_time(data: bytes) -> bool:
    """Detect macros and attributes that pull another file into a candidate."""
    text = strip_rust_noise(data.decode("utf-8", "replace"))
    return bool(_FILE_READ.search(text) or _PATH_ATTRIBUTE.search(text))


def custom_harness_targets(manifest: Path) -> set[str]:
    """Return target names that do not use libtest and so cannot be listed."""
    data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    targets = [*data.get("test", ()), *data.get("bin", ())]
    library = data.get("lib")
    if library is not None:
        default = data.get("package", {}).get("name", "").replace("-", "_")
        targets.append({"name": library.get("name", default), "harness": library.get("harness")})
    return {target.get("name") for target in targets if target.get("harness") is False}


def significant_lines(text: str) -> Iterator[str]:
    """Yield lines that must never appear in a result.

    A lone token is excluded because names and paths are allowed outputs.
    """
    for raw in text.splitlines():
        line = " ".join(raw.split())
        if len(line) >= MIN_GUARDED_LINE and not _BARE_LINE.fullmatch(line):
            yield line


def collect_strings(value: object) -> Iterator[str]:
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from collect_strings(item)
    elif isinstance(value, (list, tuple)):
        for item in value:
            yield from collect_strings(item)


def contains_guarded_line(result: object, guarded: Iterable[str]) -> bool:
    text = "\n".join(collect_strings(result))
    return any(line in text for line in guarded)


def _kill_tree(process: subprocess.Popen) -> None:
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


class Broker:
    def __init__(
        self,
        repo: Path,
        rev: str,
        list_path: Path,
        work_dir: Path,
        *,
        manifest: Path | None = None,
        jobs: int = MAX_JOBS,
        min_free_gb: float = 50,
        census_lock: Path | None = None,
        timeout: float = 7200,
        test_memory_gib: float = 8,
        test_timeout: float = 300,
    ) -> None:
        if not 1 <= jobs <= MAX_JOBS:
            raise IsolationError("jobs_out_of_range")
        self.repo = repo.resolve()
        self.view = load_view(self.repo, rev, list_path)
        self.work_dir = work_dir.resolve()
        self.work_dir.mkdir(parents=True, exist_ok=True)
        self.manifest = manifest
        self.jobs = jobs
        self.min_free_gb = min_free_gb
        self.census_lock = census_lock
        self.timeout = timeout
        self.test_memory_mib = int(test_memory_gib * 1024)
        self.test_timeout = test_timeout
        self.platform = sys.platform
        self._scan_candidates = True
        self._packages: dict[str, dict[str, str]] = {}
        self._by_key = {path_key(e.path): e for e in self.view.entries}
        self._vocabulary: frozenset[str] | None = None
        self._guarded: tuple[str, ...] | None = None

    @property
    def tree(self) -> Path:
        return self.work_dir / f"tree-{self.view.commit[:12]}"

    def _guarded_lines(self) -> tuple[str, ...]:
        if self._guarded is None:
            blobs = read_blobs(self.repo, sorted(self.view.listed_oids))
            lines = {line for blob in blobs.values() for line in significant_lines(blob.decode("utf-8", "replace"))}
            self._guarded = tuple(sorted(lines))
        return self._guarded

    def _base_vocabulary(self) -> frozenset[str]:
        if self._vocabulary is None:
            visible = [e.oid for e in self.view.visible if e.path.endswith(".rs")]
            tokens: set[str] = set()
            for blob in read_blobs(self.repo, visible).values():
                tokens.update(_IDENTIFIER.findall(blob.decode("utf-8", "replace")))
            if self.manifest:
                data = json.loads(self.manifest.read_text(encoding="utf-8"))
                for item in data.get("names", ()):
                    tokens.add(item["name"])
                    tokens.update(item.get("members", ()))
            self._vocabulary = frozenset(tokens - _KEYWORDS)
        return self._vocabulary

    def _preflight(self) -> str | None:
        if self.platform == "win32" and self.census_lock and self.census_lock.exists():
            return "blocked_by_census_lock"
        if shutil.disk_usage(self.work_dir).free < self.min_free_gb * 1024**3:
            return "blocked_by_low_disk"
        lock = self._by_key.get("cargo.lock")
        if lock and not os.environ.get("SKIA_NINJA_COMMAND"):
            if b'name = "skia-bindings"' in read_blobs(self.repo, [lock.oid])[lock.oid]:
                return "blocked_missing_skia_ninja_wrapper"
        return None

    def _validate_candidates(self, candidates: dict[str, Path]) -> dict[str, bytes]:
        overlay: dict[str, bytes] = {}
        for raw_path, source in candidates.items():
            path = normalize_repo_path(raw_path)
            key = path_key(path)
            if key in self._by_key:
                if key in self.view.locked:
                    raise IsolationError("candidate_path_locked", path)
                if key not in self.view.listed:
                    raise IsolationError("candidate_overwrites_visible_file", path)
                path = self._by_key[key].path
            elif is_excluded_root(path) or not path.endswith(".rs") or posixpath.basename(path) == "build.rs":
                raise IsolationError("candidate_path_not_allowed", path)
            try:
                overlay[path] = Path(source).read_bytes()
            except OSError as error:
                raise IsolationError("candidate_unreadable", path) from error
            if self._scan_candidates and path.endswith(".rs") and reads_files_at_compile_time(overlay[path]):
                raise IsolationError("candidate_reads_files_at_compile_time", path)
        return overlay

    def _materialize(self) -> Path:
        tree = self.tree
        marker = self.work_dir / f"{tree.name}.complete"
        if marker.exists():
            return tree
        if tree.exists():
            shutil.rmtree(tree)
        tree.mkdir(parents=True)
        archive = subprocess.Popen(
            ["git", "-C", str(self.repo), "-c", "core.autocrlf=false", "archive", "--format=tar", self.view.commit],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        try:
            with tarfile.open(fileobj=archive.stdout, mode="r|") as stream:
                for member in stream:
                    if not member.isfile():
                        continue
                    target = tree / normalize_repo_path(member.name)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(stream.extractfile(member).read())
                    if member.mode & 0o111:
                        os.chmod(target, 0o755)
        finally:
            archive.stdout.close()
            status = archive.wait()
        if status != 0:
            raise IsolationError("git_failed", "archive")
        marker.write_text(self.view.commit, encoding="ascii")
        return tree

    def _apply_overlay(self, tree: Path, overlay: dict[str, bytes]) -> None:
        state = self.work_dir / "overlay-state.json"
        previous = json.loads(state.read_text(encoding="utf-8")) if state.exists() else []
        state.write_text(json.dumps(sorted({*previous, *overlay})), encoding="utf-8")
        originals = read_blobs(
            self.repo,
            [self._by_key[path_key(p)].oid for p in previous if path_key(p) in self._by_key],
        )
        for path in previous:
            entry = self._by_key.get(path_key(path))
            if entry is not None:
                (tree / path).write_bytes(originals[entry.oid])
            else:
                (tree / path).unlink(missing_ok=True)
        for path, data in overlay.items():
            target = tree / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        state.write_text(json.dumps(sorted(overlay)), encoding="utf-8")

    def _env(self) -> dict[str, str]:
        env = os.environ.copy()
        env.update(
            CARGO_INCREMENTAL="0",
            CARGO_BUILD_JOBS=str(self.jobs),
            CARGO_TARGET_DIR=str(self.work_dir / "target"),
            CARGO_TERM_COLOR="never",
            RUST_BACKTRACE="0",
        )
        env.pop("RUST_TEST_NOCAPTURE", None)
        if os.name == "posix":
            env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
        return env

    def _stream(self, command: list[str], tree: Path, on_line: Callable[[str], None], *, merge: bool) -> int | None:
        options = (
            {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == "nt" else {"start_new_session": True}
        )
        process = subprocess.Popen(
            command,
            cwd=tree,
            env=self._env(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT if merge else subprocess.DEVNULL,
            **options,
        )
        timed_out = threading.Event()

        def expire() -> None:
            timed_out.set()
            _kill_tree(process)

        timer = threading.Timer(self.timeout, expire)
        timer.start()
        try:
            for raw in process.stdout:
                on_line(raw.decode("utf-8", "replace").rstrip("\r\n"))
            code = process.wait()
        finally:
            timer.cancel()
            process.stdout.close()
        return None if timed_out.is_set() else code

    def _workspace_packages(self, tree: Path) -> dict[str, str]:
        completed = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
            cwd=tree,
            env=self._env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if completed.returncode != 0:
            raise IsolationError("cargo_metadata_failed")
        data = json.loads(completed.stdout)
        self._packages = {
            package["id"]: {
                "name": package["name"],
                "version": package["version"],
                "dir": str(Path(package["manifest_path"]).parent),
            }
            for package in data["packages"]
        }
        return {package["id"]: package["name"] for package in data["packages"]}

    def _scope_prefix(self) -> list[str]:
        return [
            "systemd-run",
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            f"--property=MemoryMax={self.test_memory_mib}M",
            "--property=MemorySwapMax=0",
            "--property=TasksMax=2048",
            "--property=OOMPolicy=continue",
            "--",
        ]

    def _sandbox_block(self) -> str | None:
        """Refuse to run candidate code unless the sandbox and memory limit work."""
        if self.platform != "linux" or shutil.which("bwrap") is None:
            return "blocked_sandbox_unavailable"
        if Path("/mnt") in (self.work_dir, *self.work_dir.parents):
            return "blocked_work_dir_on_windows_mount"
        probe = self.work_dir / "sandbox-probe"
        (probe / "tree").mkdir(parents=True, exist_ok=True)
        (probe / "target").mkdir(parents=True, exist_ok=True)
        tree, target = str(probe / "tree"), str(probe / "target")
        quiet = {"stdin": subprocess.DEVNULL, "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
        try:
            sandbox = subprocess.run(
                [*sandbox_prefix(tree, tree, target, target, tree), "/usr/bin/true"],
                env=clean_env({}),
                check=False,
                **quiet,
            )
        except OSError:
            return "blocked_sandbox_unavailable"
        if sandbox.returncode != 0:
            return "blocked_sandbox_unavailable"
        try:
            scope = subprocess.run([*self._scope_prefix(), "/usr/bin/true"], env=self._env(), check=False, **quiet)
        except OSError:
            return "blocked_no_memory_limit"
        return None if scope.returncode == 0 else "blocked_no_memory_limit"

    def _context(self, tree: Path, overlay: dict[str, bytes]) -> FilterContext:
        overlay_paths = {path_key(p): p for p in overlay}
        vocabulary = self._base_vocabulary() | {
            token
            for data in overlay.values()
            for token in _IDENTIFIER.findall(data.decode("utf-8", "replace"))
            if token not in _KEYWORDS
        }
        return FilterContext(
            root=normalize_root(tree.resolve()),
            visible={k: e.path for k, e in ((path_key(e.path), e) for e in self.view.visible) if k not in overlay_paths},
            candidates=overlay_paths,
            vocabulary=frozenset(vocabulary),
        )

    def _test_suites(self, artifacts: list[dict], ids: dict[str, str]) -> tuple[list[dict], list[str]]:
        suites: dict[str, dict] = {}
        unsupported: set[str] = set()
        harness: dict[str, set[str]] = {}
        for artifact in artifacts:
            package = ids.get(artifact.get("package_id"))
            if package is None or not artifact.get("profile", {}).get("test"):
                continue
            info = self._packages[artifact["package_id"]]
            target = artifact["target"]
            suite_id = f"{package}:{target['kind'][0]}:{target['name']}"
            custom = harness.setdefault(package, custom_harness_targets(Path(info["dir"]) / "Cargo.toml"))
            if target["name"] in custom:
                unsupported.add(suite_id)
                continue
            suites[suite_id] = {
                "id": suite_id,
                "exe": artifact["executable"],
                "cwd": info["dir"],
                "env": {
                    "CARGO_MANIFEST_DIR": info["dir"],
                    "CARGO_PKG_NAME": info["name"],
                    "CARGO_PKG_VERSION": info["version"],
                },
            }
        return [suites[key] for key in sorted(suites)], sorted(unsupported)

    def _prepare_runtime(self, tree: Path, overlay: dict[str, bytes], artifacts: list[dict]) -> None:
        """Build the only files a test can see: visible files, candidates and executables."""
        runtime = self.work_dir / "runtime"
        if runtime.exists():
            shutil.rmtree(runtime)
        (runtime / "tree").mkdir(parents=True)
        (runtime / "target").mkdir(parents=True)

        def link(source: Path, destination: Path) -> None:
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists():
                os.link(source, destination)

        for relative in [*(e.path for e in self.view.visible), *overlay]:
            link(tree / relative, runtime / "tree" / relative)
        target_root = self.work_dir / "target"
        for artifact in artifacts:
            executable = Path(artifact["executable"])
            link(executable, runtime / "target" / executable.relative_to(target_root))

    def _execute(self, tree: Path, suites: list[dict]) -> dict:
        plan_path = self.work_dir / "sandbox-plan.json"
        result_path = self.work_dir / "sandbox-result.json"
        result_path.unlink(missing_ok=True)
        plan = {
            "tree": str(tree),
            "runtime_tree": str(self.work_dir / "runtime" / "tree"),
            "target": str(self.work_dir / "target"),
            "runtime_target": str(self.work_dir / "runtime" / "target"),
            "jobs": self.jobs,
            "test_timeout": self.test_timeout,
            "suites": suites,
        }
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        runner = Path(__file__).with_name("sandbox_runner.py")
        command = [*self._scope_prefix(), sys.executable, "-I", str(runner), str(plan_path), str(result_path)]
        exit_code = self._stream(command, tree, lambda _line: None, merge=False)
        if exit_code != 0 or not result_path.exists():
            raise IsolationError("sandbox_runner_failed")
        return json.loads(result_path.read_text(encoding="utf-8"))["suites"]

    def _baseline(self, tree: Path, ids: dict[str, str], selected: set[str], scope: list[str]) -> dict:
        """Record test names and outcomes from the pristine tree, before any candidate."""
        key = hashlib.sha256("\0".join(scope).encode("utf-8")).hexdigest()[:12]
        path = self.work_dir / f"baseline-tests-{self.view.commit[:12]}-{key}.json"
        if path.exists():
            return json.loads(path.read_text(encoding="utf-8"))
        self._apply_overlay(tree, {})
        artifacts: list[dict] = []
        build = ["cargo", "test", *scope, "--tests", "--no-run", "--locked", "--message-format=json", "-j", str(self.jobs)]
        if self._compile(tree, self._context(tree, {}), build, ids, selected, artifacts)["status"] != "ok":
            raise IsolationError("baseline_build_failed")
        suites, unsupported = self._test_suites(artifacts, ids)
        self._prepare_runtime(tree, {}, artifacts)
        results = self._execute(tree, suites)
        listed = {suite: data for suite, data in results.items() if data["listed"] is not None}
        baseline = {"suites": listed, "unsupported": sorted({*unsupported, *(set(results) - set(listed))})}
        path.write_text(json.dumps(baseline), encoding="utf-8")
        return baseline

    @staticmethod
    def _assemble_tests(baseline: dict, results: dict, test_filter: str | None) -> dict:
        """Report only names from the fixed baseline list."""
        suites = []
        counts = {"passed": 0, "failed": 0, "ignored": 0, "missing": 0, "timeout": 0, "regressions": 0}
        field = {"ok": "passed", "FAILED": "failed", "ignored": "ignored", "missing": "missing", "timeout": "timeout"}
        incomplete = set(baseline["unsupported"])
        for suite_id in sorted(baseline["suites"]):
            base = baseline["suites"][suite_id]
            run = results.get(suite_id) or {}
            present = set(run.get("listed") or ())
            ignored = set(base["ignored"])
            tests = []
            for name in base["listed"]:
                if test_filter and test_filter not in name:
                    continue
                before = "ignored" if name in ignored else base["outcomes"].get(name, "FAILED")
                if name in ignored:
                    outcome = "ignored"
                elif name not in present:
                    outcome = "missing"
                else:
                    outcome = run["outcomes"].get(name, "FAILED")
                counts[field[outcome]] += 1
                counts["regressions"] += before == "ok" and outcome != "ok"
                tests.append({"name": name, "outcome": outcome, "baseline": before})
            complete = all(test["outcome"] != "missing" for test in tests)
            if not complete:
                incomplete.add(suite_id)
            suites.append({"suite": suite_id, "complete": complete, "tests": tests})
        if counts["failed"] or counts["timeout"]:
            status = "failed"
        elif incomplete:
            status = "incomplete"
        else:
            status = "passed"
        return {"status": status, "suites": suites, "counts": counts, "incomplete_suites": sorted(incomplete)}

    def _compile(
        self,
        tree: Path,
        context: FilterContext,
        command: list[str],
        ids: dict[str, str],
        selected: set[str],
        artifacts: list[dict] | None = None,
    ) -> dict:
        diagnostics: dict[tuple, dict] = {}
        reached: set[str] = set()
        failed: set[str] = set()

        def on_line(line: str) -> None:
            if not line.startswith("{"):
                return
            try:
                record = json.loads(line)
            except ValueError:
                return
            package = ids.get(record.get("package_id"))
            if record.get("reason") == "compiler-artifact" and package:
                reached.add(package)
                if artifacts is not None and record.get("executable"):
                    artifacts.append(record)
            elif record.get("reason") == "compiler-message" and isinstance(record.get("message"), dict):
                item = filter_diagnostic(record["message"], context)
                if item is not None:
                    key = (item["level"], item["code"], item["path"], item["line"], item["column"], tuple(item["identifiers"]))
                    diagnostics.setdefault(key, item)
                    if item["level"] == "error" and package:
                        failed.add(package)

        exit_code = self._stream(command, tree, on_line, merge=False)
        ordered = sorted(
            diagnostics.values(),
            key=lambda d: (d["level"] != "error", d["path"] or "", d["line"] or 0, d["column"] or 0, d["code"] or ""),
        )
        has_error = any(d["level"] == "error" for d in ordered)
        if exit_code is None:
            status = "timeout"
        elif exit_code == 0:
            status = "ok"
        else:
            status = "build_failed" if has_error else "cargo_failed"
        return {
            "status": status,
            "diagnostics": ordered[:MAX_DIAGNOSTICS],
            "diagnostics_truncated": max(0, len(ordered) - MAX_DIAGNOSTICS),
            "failed_packages": sorted(failed),
            "packages_not_reached": sorted(selected - reached - failed),
        }

    def _finish(self, result: dict) -> dict:
        if contains_guarded_line(result, self._guarded_lines()):
            result = {
                "schema": SCHEMA,
                "base_commit": self.view.commit,
                "command": result.get("command"),
                "status": "filter_abort",
            }
        with (self.work_dir / "broker-log.jsonl").open("a", encoding="utf-8") as log:
            stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
            log.write(json.dumps({"time": stamp, "result": result}, sort_keys=True) + "\n")
        return result

    def run(
        self,
        command: str,
        candidates: dict[str, Path] | None = None,
        packages: Iterable[str] = (),
        test_filter: str | None = None,
    ) -> dict:
        header = {
            "schema": SCHEMA,
            "base_commit": self.view.commit,
            "list_sha256": self.view.list_sha256,
            "command": command,
        }
        try:
            if command not in ("check", "test"):
                raise IsolationError("unknown_command")
            if test_filter is not None and not _TEST_FILTER.match(test_filter):
                raise IsolationError("invalid_test_filter")
            package_list = sorted(set(packages))
            if any(not _PACKAGE.match(p) for p in package_list):
                raise IsolationError("invalid_package")
            blocked = self._preflight() or (self._sandbox_block() if command == "test" else None)
            if blocked:
                return self._finish({**header, "status": blocked})
            overlay = self._validate_candidates(candidates or {})
            header["candidates"] = [
                {"path": path, "sha256": hashlib.sha256(data).hexdigest()} for path, data in sorted(overlay.items())
            ]
            tree = self._materialize()
            ids = self._workspace_packages(tree)
            names = set(ids.values())
            if not set(package_list) <= names:
                raise IsolationError("unknown_package")
            selected = set(package_list) or names
            scope = [arg for p in package_list for arg in ("-p", p)] or ["--workspace"]
            baseline = None
            if command == "test":
                header["sandbox"] = {"runner": "bwrap", "network": False, "memory_limit_mib": self.test_memory_mib}
                baseline = self._baseline(tree, ids, selected, scope)
            self._apply_overlay(tree, overlay)
        except IsolationError as error:
            return self._finish({**header, "status": "rejected", "reason": error.code})

        context = self._context(tree, overlay)
        jobs = ["-j", str(self.jobs)]
        if command == "check":
            cargo = ["cargo", "check", *scope, "--all-targets", "--keep-going", "--locked", "--message-format=json", *jobs]
            return self._finish({**header, **self._compile(tree, context, cargo, ids, selected)})

        if not overlay:
            return self._finish({**header, "diagnostics": [], **self._assemble_tests(baseline, baseline["suites"], test_filter)})
        artifacts: list[dict] = []
        build = ["cargo", "test", *scope, "--tests", "--no-run", "--locked", "--message-format=json", *jobs]
        compiled = self._compile(tree, context, build, ids, selected, artifacts)
        if compiled["status"] != "ok":
            return self._finish({**header, **compiled})
        planned, _ = self._test_suites(artifacts, ids)
        fixed = []
        for suite in planned:
            base = baseline["suites"].get(suite["id"])
            if base is None:
                continue
            ignored = set(base["ignored"])
            allowed = [n for n in base["listed"] if n not in ignored and (not test_filter or test_filter in n)]
            fixed.append({**suite, "allowed": allowed})
        try:
            self._prepare_runtime(tree, overlay, artifacts)
            results = self._execute(tree, fixed)
        except IsolationError as error:
            return self._finish({**header, "status": "rejected", "reason": error.code})
        return self._finish(
            {**header, "diagnostics": compiled["diagnostics"], **self._assemble_tests(baseline, results, test_filter)}
        )


def handle_request(broker: Broker, inbox: Path, request_path: Path) -> dict:
    """Run one inbox request. A bad request gets a fixed reason code only."""
    header = {"schema": SCHEMA, "base_commit": broker.view.commit}
    try:
        request = json.loads(request_path.read_text(encoding="utf-8"))
        if not isinstance(request, dict):
            raise ValueError
        candidates = {}
        for destination, name in (request.get("candidates") or {}).items():
            if not isinstance(name, str) or not _INBOX_FILE.match(name) or not (inbox / name).is_file():
                raise IsolationError("invalid_candidate_file")
            candidates[destination] = inbox / name
        packages = request.get("packages") or []
        if not isinstance(packages, list) or any(not isinstance(p, str) for p in packages):
            raise IsolationError("invalid_package")
        test_filter = request.get("test_filter")
        if test_filter is not None and not isinstance(test_filter, str):
            raise IsolationError("invalid_test_filter")
        return broker.run(str(request.get("command")), candidates, packages, test_filter)
    except IsolationError as error:
        return broker._finish({**header, "command": None, "status": "rejected", "reason": error.code})
    except (ValueError, AttributeError, OSError):
        return broker._finish({**header, "command": None, "status": "rejected", "reason": "invalid_request"})


def serve(broker: Broker, inbox: Path, *, once: bool = False, poll_seconds: float = 2.0) -> None:
    """Answer ``<id>.request.json`` files with ``<id>.result.json`` files."""
    root = refuse_unsafe_location(inbox, broker.repo)
    root.mkdir(parents=True, exist_ok=True)
    while True:
        for request_path in sorted(root.glob("*.request.json")):
            request_id = request_path.name.removesuffix(".request.json")
            if not _REQUEST_ID.match(request_id):
                continue
            result = handle_request(broker, root, request_path)
            partial = root / f"{request_id}.result.partial"
            partial.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
            partial.replace(root / f"{request_id}.result.json")
            request_path.replace(root / f"{request_id}.request.done")
        if once:
            return
        time.sleep(poll_seconds)


def _parse_candidate(value: str) -> tuple[str, Path]:
    destination, separator, source = value.partition("=")
    if not separator or not destination or not source:
        raise argparse.ArgumentTypeError("expected REPO_PATH=FILE")
    return destination, Path(source)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=("check", "test", "serve"))
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--rev", default="main")
    parser.add_argument("--withheld-list", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--candidate", action="append", default=[], type=_parse_candidate, metavar="REPO_PATH=FILE")
    parser.add_argument("--package", action="append", default=[])
    parser.add_argument("--test-filter")
    parser.add_argument("--jobs", type=int, default=MAX_JOBS)
    parser.add_argument("--min-free-gb", type=float, default=50)
    parser.add_argument("--census-lock", type=Path)
    parser.add_argument("--timeout", type=float, default=7200)
    parser.add_argument("--test-memory-gib", type=float, default=8)
    parser.add_argument("--test-timeout", type=float, default=300)
    parser.add_argument("--inbox", type=Path)
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    try:
        broker = Broker(
            args.repo,
            args.rev,
            args.withheld_list,
            args.work_dir,
            manifest=args.manifest,
            jobs=args.jobs,
            min_free_gb=args.min_free_gb,
            census_lock=args.census_lock,
            timeout=args.timeout,
            test_memory_gib=args.test_memory_gib,
            test_timeout=args.test_timeout,
        )
        if args.command == "serve":
            if args.inbox is None:
                parser.error("serve requires --inbox")
            serve(broker, args.inbox, once=args.once)
            return 0
    except IsolationError as error:
        print(f"refused: {error.code}", file=sys.stderr)
        return 2
    result = broker.run(args.command, dict(args.candidate), args.package, args.test_filter)
    text = json.dumps(result, indent=1) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0 if result["status"] in ("ok", "passed") else 1


if __name__ == "__main__":
    raise SystemExit(main())
