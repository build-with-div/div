"""Shared handling of a withheld-path list for the isolation tools.

The list is an input file kept outside the repository, with one repo-relative
path per line. Blank lines and lines starting with ``#`` are ignored. A line
``locked: <path>`` withholds a file that a candidate may not replace, such as a
test. The tools may process withheld files, but must never display their text.
"""

from __future__ import annotations

import hashlib
import subprocess
import threading
from dataclasses import dataclass
from pathlib import Path

# Never exported, in addition to the listed files. Agent instruction files are
# included because a session started in the export would load them.
EXCLUDED_ROOTS = ("docs", "AGENTS.md", "CLAUDE.md", ".claude")
AGENT_MARKERS = (".git", "AGENTS.md", "CLAUDE.md", ".claude")
EMPTY_BLOBS = frozenset(
    {
        "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
        "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813",
    }
)
SYMLINK_MODE = "120000"
LOCKED_PREFIX = "locked:"


class IsolationError(Exception):
    """A refused operation. ``code`` is safe to return to an implementer."""

    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}" if detail else code)
        self.code = code


def path_key(path: str) -> str:
    return path.casefold()


def normalize_repo_path(raw: str) -> str:
    value = raw.strip().replace("\\", "/")
    parts = value.split("/")
    is_absolute = value.startswith("/") or (len(value) > 1 and value[1] == ":")
    if not value or is_absolute or any(p in ("", ".", "..") for p in parts):
        raise IsolationError("invalid_path", repr(raw))
    return "/".join(parts)


def is_inside(path: Path, parent: Path) -> bool:
    return path == parent or parent in path.parents


def load_list(list_path: Path, repo: Path) -> tuple[frozenset[str], frozenset[str], str]:
    """Return the listed paths, the locked subset and a digest of the list."""
    resolved = list_path.resolve()
    if is_inside(resolved, repo.resolve()):
        raise IsolationError("list_inside_repository", str(list_path))
    paths: set[str] = set()
    locked: set[str] = set()
    for line in resolved.read_text(encoding="utf-8-sig").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith(LOCKED_PREFIX):
            path = normalize_repo_path(stripped[len(LOCKED_PREFIX) :])
            locked.add(path)
        else:
            path = normalize_repo_path(stripped)
        paths.add(path)
    if not paths:
        raise IsolationError("empty_list", str(list_path))
    canonical = "\n".join(sorted(f"{LOCKED_PREFIX}{p}" if p in locked else p for p in paths)).encode("utf-8")
    return frozenset(paths), frozenset(locked), hashlib.sha256(canonical).hexdigest()


def git(repo: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repo), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if completed.returncode != 0:
        raise IsolationError("git_failed", args[0])
    return completed.stdout.decode("utf-8", "surrogateescape")


def read_blobs(repo: Path, oids: list[str]) -> dict[str, bytes]:
    unique = list(dict.fromkeys(oids))
    if not unique:
        return {}
    process = subprocess.Popen(
        ["git", "-C", str(repo), "cat-file", "--batch"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )

    def feed() -> None:
        process.stdin.write("".join(f"{oid}\n" for oid in unique).encode("ascii"))
        process.stdin.close()

    feeder = threading.Thread(target=feed, daemon=True)
    feeder.start()
    blobs: dict[str, bytes] = {}
    for oid in unique:
        header = process.stdout.readline().split()
        if len(header) != 3:
            process.kill()
            raise IsolationError("git_failed", "cat-file")
        blobs[oid] = process.stdout.read(int(header[2]))
        process.stdout.read(1)
    feeder.join()
    process.stdout.close()
    if process.wait() != 0:
        raise IsolationError("git_failed", "cat-file")
    return blobs


@dataclass(frozen=True)
class Entry:
    mode: str
    oid: str
    path: str


@dataclass(frozen=True)
class TreeView:
    """A commit's files, split into withheld and visible."""

    commit: str
    object_format: str
    list_sha256: str
    entries: tuple[Entry, ...]
    listed: frozenset[str]
    locked: frozenset[str]
    listed_oids: frozenset[str]
    withheld: frozenset[str]

    @property
    def visible(self) -> tuple[Entry, ...]:
        return tuple(e for e in self.entries if path_key(e.path) not in self.withheld)

    @property
    def listed_entries(self) -> tuple[Entry, ...]:
        return tuple(e for e in self.entries if path_key(e.path) in self.listed)

    @property
    def replaceable_entries(self) -> tuple[Entry, ...]:
        return tuple(e for e in self.listed_entries if path_key(e.path) not in self.locked)


def is_excluded_root(path: str) -> bool:
    return path.split("/", 1)[0] in EXCLUDED_ROOTS


def read_tree(repo: Path, commit: str) -> tuple[Entry, ...]:
    output = git(repo, "ls-tree", "-r", "-z", "--full-tree", commit)
    entries = []
    for record in output.split("\0"):
        if not record:
            continue
        meta, path = record.split("\t", 1)
        mode, kind, oid = meta.split()
        if kind == "blob":
            entries.append(Entry(mode, oid, path))
    return tuple(entries)


def load_view(repo: Path, rev: str, list_path: Path) -> TreeView:
    """Resolve the list against a commit. Any unknown listed path is an error."""
    listed, locked, digest = load_list(list_path, repo)
    commit = git(repo, "rev-parse", "--verify", f"{rev}^{{commit}}").strip()
    object_format = git(repo, "rev-parse", "--show-object-format").strip()
    entries = read_tree(repo, commit)
    by_key = {path_key(e.path): e for e in entries}
    listed_keys = frozenset(path_key(p) for p in listed)
    missing = sorted(p for p in listed if path_key(p) not in by_key)
    if missing:
        raise IsolationError("listed_path_not_in_tree", ", ".join(missing))
    listed_oids = frozenset(by_key[k].oid for k in listed_keys) - EMPTY_BLOBS
    withheld = {
        path_key(e.path)
        for e in entries
        if path_key(e.path) in listed_keys
        or e.oid in listed_oids
        or e.mode == SYMLINK_MODE
        or is_excluded_root(e.path)
    }
    return TreeView(
        commit=commit,
        object_format=object_format,
        list_sha256=digest,
        entries=entries,
        listed=listed_keys,
        locked=frozenset(path_key(p) for p in locked),
        listed_oids=listed_oids,
        withheld=frozenset(withheld),
    )


def blob_oid(data: bytes, object_format: str) -> str:
    digest = hashlib.new(object_format)
    digest.update(b"blob %d\0" % len(data))
    digest.update(data)
    return digest.hexdigest()


def refuse_unsafe_location(location: Path, repo: Path) -> Path:
    """Refuse a directory that a session would link to the project."""
    resolved = location.resolve()
    root = repo.resolve()
    if is_inside(resolved, root) or is_inside(root, resolved):
        raise IsolationError("location_overlaps_repository", str(location))
    home = Path.home().resolve()
    for parent in (resolved, *resolved.parents):
        for marker in AGENT_MARKERS:
            # ~/.claude is user-level configuration, not a project marker.
            if marker == ".claude" and parent == home:
                continue
            if (parent / marker).exists():
                raise IsolationError("location_has_project_marker", str(parent / marker))
    return resolved
