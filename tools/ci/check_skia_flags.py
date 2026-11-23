#!/usr/bin/env python3
"""Keep Skia's AVX-512 raster path out of every Div Skia build.

Chromium 147 builds Skia's x86 raster pipeline for SSE2 and AVX2 only
(`skia/BUILD.gn`, `skia_opts_hsw`). Upstream Skia's `BUILD.gn` always defines
`SK_ENABLE_AVX512_OPTS`, and on an AVX-512 CPU that selects a path whose
reciprocal estimate differs (`_mm512_rcp14_ps` against `_mm256_rcp_ps`), so Div
would render some blend modes differently from Chromium on those CPUs.

Skia's compile rules put `{{defines}}` before `{{cflags}}` on both its MSVC and
GCC-like toolchains, so `-USK_ENABLE_AVX512_OPTS` in `extra_cflags` wins. This
check fails if any `extra_cflags` list in a tracked file omits it.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
REQUIRED = '"-USK_ENABLE_AVX512_OPTS"'
FORBIDDEN = "-DSK_ENABLE_AVX512_OPTS"
FLAG_LIST = re.compile(r"extra_cflags\s*\+=\s*\[.*?\]", re.DOTALL)
# Unit-test fixtures for other tools, and this check and its tests.
EXEMPT = frozenset(
    {
        "tools/ci/check_skia_flags.py",
        "tools/ci/test_check_skia_flags.py",
        "tools/ci/test_msvc_toolset.py",
        "tools/test_skia_prebuilt.py",
    }
)
# Dated records describe builds as they were.
EXEMPT_PREFIXES = ("docs/progress/",)


def problems_in(name: str, text: str) -> tuple[int, list[str]]:
    """The number of flag lists in `text` and what is wrong with them."""
    lists = list(FLAG_LIST.finditer(text))
    problems = []
    for match in lists:
        flags = match.group()
        line = text.count("\n", 0, match.start()) + 1
        if REQUIRED not in flags:
            problems.append(f"{name}:{line}: extra_cflags omits {REQUIRED}")
        if FORBIDDEN in flags:
            problems.append(f"{name}:{line}: extra_cflags sets {FORBIDDEN}")
    return len(lists), problems


def tracked_files() -> list[str]:
    output = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, check=True, capture_output=True
    ).stdout
    return [name for name in output.decode("utf-8").split("\0") if name]


def main() -> int:
    total = 0
    problems: list[str] = []
    for name in tracked_files():
        if name in EXEMPT or name.startswith(EXEMPT_PREFIXES):
            continue
        path = ROOT / name
        if not path.is_file():
            continue
        data = path.read_bytes()
        if b"extra_cflags" not in data:
            continue
        count, found = problems_in(name, data.decode("utf-8", errors="replace"))
        total += count
        problems.extend(found)
    if total == 0:
        problems.append("no extra_cflags list found; the check is looking in the wrong place")
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"{total} Skia flag lists exclude the AVX-512 raster path")
    return 0


if __name__ == "__main__":
    sys.exit(main())
