"""Regenerate generated files in a freshly converted Div tree.

Renaming crates and moving paths changes generated outputs (the C header and
symbol list, style setters, the renderer contract's path keys, the platform
test ledger, the dependency policy's lockfile hash, the CSS Grid port). Each
is rewritten by its own generator, never by hand, and every `--check` must
then pass. The gates' data files are linked into the tree while the
generators run and removed afterwards.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path


def _run(cmd, cwd, env):
    print("regenerate:", " ".join(str(c) for c in cmd), flush=True)
    proc = subprocess.run([str(c) for c in cmd], cwd=cwd, env=env)
    if proc.returncode != 0:
        raise SystemExit(f"regenerate: failed ({proc.returncode}): {' '.join(map(str, cmd))}")


def _lock_packages(text: str) -> set[tuple[str, str, str, str]]:
    out, cur = set(), {}
    for line in text.splitlines() + ["[[package]]"]:
        if line == "[[package]]":
            if cur.get("source"):
                out.add((cur["name"], cur["version"], cur["source"], cur.get("checksum", "")))
            cur = {}
        elif " = " in line and not line.startswith(" "):
            k, v = line.split(" = ", 1)
            cur[k] = v.strip('"')
    return out


class _Gates:
    """Link the gates profile into the tree for the duration of the block."""

    def __init__(self, tree: Path, data: Path):
        self.tree, self.src = tree, data / "gates"
        self.placed: list[Path] = []

    def __enter__(self):
        for f in sorted(self.src.rglob("*")):
            if f.is_file():
                dest = self.tree / f.relative_to(self.src)
                if dest.exists():
                    raise SystemExit(f"regenerate: data file would overwrite {dest}")
                dest.parent.mkdir(parents=True, exist_ok=True)
                try:
                    os.link(f, dest)
                except OSError:
                    shutil.copyfile(f, dest)
                self.placed.append(dest)
        return self

    def __exit__(self, *exc):
        for dest in self.placed:
            dest.unlink()
        for dest in sorted({p.parent for p in self.placed}, key=lambda p: len(p.parts), reverse=True):
            while dest != self.tree and dest.exists() and not any(dest.iterdir()):
                dest.rmdir()
                dest = dest.parent
        return False


def run_all(tree: Path, data: Path, wpt_root: Path | None, acorn: Path | None) -> None:
    if wpt_root is None or acorn is None:
        raise SystemExit("regenerate: --wpt-root and --acorn are required; without the pinned WPT tree "
                         "the renderer contract silently differs")
    env = dict(os.environ)
    env.update({
        "PYTHONUTF8": "1",
        "CHROMIUM_WPT_ROOT": str(wpt_root),
        "CHROMIUM_WPT_CSS": str(wpt_root / "css"),
        "CHROMIUM_ACORN": str(acorn),
    })
    py = sys.executable

    # 1. The lockfile: cargo re-sorts the renamed packages. Registry packages
    #    must come out exactly as they went in.
    lock = tree / "Cargo.lock"
    before = _lock_packages(lock.read_text(encoding="utf-8"))
    _run(["cargo", "metadata", "--offline", "--format-version", "1", "--no-deps"], tree, env)
    _run(["cargo", "tree", "--offline", "--workspace", "--quiet", "--prefix", "none", "--depth", "0"], tree, env)
    after = _lock_packages(lock.read_text(encoding="utf-8"))
    if before != after:
        raise SystemExit(f"regenerate: registry packages changed in Cargo.lock: "
                         f"-{sorted(before - after)[:5]} +{sorted(after - before)[:5]}")

    with _Gates(tree, data):
        generators = [
            [py, "tools/style/generate_properties.py"],
            [py, "tools/ffi/generate_ffi.py"],
            [py, "tools/ffi/generate_form_owner_cases.py"],
            [py, "tools/ffi/generate_control_keyboard_cases.py"],
            [py, "tools/ci/check_platform_scoped_tests.py", "--write"],
            [py, "tools/release/generate_v02_contract.py"],
            [py, "tools/wpt/port_coverage_area.py", "css-grid", "css_grid"],
            [py, "tools/qualification/generate_renderer_contract.py"],
        ]
        for cmd in generators:
            _run(cmd, tree, env)
        _run(["cargo", "fmt", "--all"], tree, env)
        # The longer C names rewrap lines; format what format-check.yml checks.
        c_sources = _c_sources(tree)
        _run(["clang-format", "-i", *c_sources], tree, env)
        for cmd in generators:
            if cmd[1].endswith("check_platform_scoped_tests.py"):
                check = [py, cmd[1]]  # checks unless --write
            elif cmd[1].endswith("port_coverage_area.py"):
                check = [py, cmd[1], "--check", "css-grid", "css_grid"]
            else:
                check = cmd + ["--check"]
            _run(check, tree, env)
        _run(["cargo", "fmt", "--all", "--check"], tree, env)
        _run(["clang-format", "--dry-run", "--Werror", *c_sources], tree, env)


def format_tree(tree: Path) -> None:
    """Only the formatting passes of run_all: enough for branch conversion
    snapshots to line up with the formatted import."""
    env = os.environ.copy()
    _run(["cargo", "fmt", "--all"], tree, env)
    _run(["clang-format", "-i", *_c_sources(tree)], tree, env)


# Generated headers are checked against their generators instead.
_C_UNFORMATTED = {"include/divui.h", "include/divui_style_properties.h"}


def _c_sources(tree: Path) -> list[str]:
    """The files format-check.yml passes to clang-format, sorted."""
    found = []
    for top in ("include", "examples/c"):
        for path in (tree / top).rglob("*"):
            rel = path.relative_to(tree).as_posix()
            if path.is_file() and path.suffix in (".c", ".cc", ".h") and rel not in _C_UNFORMATTED:
                found.append(rel)
    if not found:
        raise SystemExit("regenerate: no C sources found for clang-format")
    return sorted(found)
