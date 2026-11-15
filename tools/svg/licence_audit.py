#!/usr/bin/env python3
"""Classify Chromium source files by their licence header only.

RFC 0009 ports only permissively licensed Chromium files, and nobody on the
SVG work reads LGPL or GPL source. This tool decides which is which without
anyone opening a file: it fetches each file at a pinned tag from
chromium.googlesource.com, keeps only the leading comment block, classifies
it, and discards the rest of the file unread. Its output is the file path,
the class and the first copyright line, one file per row, `.h` and `.cc`
separately. It never prints or stores code.

Classes:

- `LGPL` or `GPL`: the header names the GNU (Library|Lesser) General Public
  License, or the GNU General Public License. Not read; behaviour is
  written from the W3C specifications.
- `BSD`: the header names Chromium's BSD-style licence or carries the BSD
  redistribution clauses, and no GNU licence. May be ported.
- `NONE`: no licence text in the header. Treated as not cleared.

    licence_audit.py --tag 147.0.7727.50 --output tools/svg/licence-audit-147.0.7727.50.tsv
"""

from __future__ import annotations

import argparse
import base64
import concurrent.futures
import json
import re
import sys
import time
import urllib.error
import urllib.request

HOST = "https://chromium.googlesource.com/chromium/src/+/refs/tags/{tag}/{path}"
BLINK = "third_party/blink/renderer/"

# Every directory, recursively, whose files RFC 0009 might port or must avoid.
DIRECTORIES = [
    BLINK + "core/layout/svg",
    BLINK + "core/svg",
]
# Files outside those directories that RFC 0009 names, by prefix.
PREFIXES = {
    BLINK + "core/paint": ("svg_", "clip_path_clipper", "text_fragment_painter",
                           "outline_painter"),
    BLINK + "core/style": ("style_svg_resource", "svg_paint"),
    BLINK + "core/css": ("svg.css", "css_properties.json5"),
    BLINK + "platform/geometry": ("path.", "path_builder", "stroke_data"),
    BLINK + "platform/graphics": ("graphics_context.", "gradient.", "dash_array"),
}
SUFFIXES = (".h", ".cc", ".css", ".json5")


def fetch(url: str) -> bytes:
    delay = 2.0
    for attempt in range(8):
        try:
            with urllib.request.urlopen(url, timeout=60) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code not in (429, 500, 502, 503, 504) or attempt == 7:
                raise
        except (urllib.error.URLError, TimeoutError):
            if attempt == 7:
                raise
        time.sleep(delay)
        delay = min(delay * 2, 60)
    raise RuntimeError("unreachable")


def listing(tag: str, directory: str) -> list[tuple[str, str]]:
    raw = fetch(HOST.format(tag=tag, path=directory + "/") + "?format=JSON")
    data = json.loads(raw.decode("utf-8").split("\n", 1)[1])
    return [(entry["name"], entry["type"]) for entry in data.get("entries", [])]


def files_under(tag: str, directory: str) -> list[str]:
    out = []
    for name, kind in listing(tag, directory):
        path = f"{directory}/{name}"
        if kind == "tree":
            out += files_under(tag, path)
        elif name.endswith(SUFFIXES):
            out.append(path)
    return out


def leading_comment(text: str) -> str:
    """The comment block before the first line of code; nothing after it."""
    kept = []
    in_block = False
    for line in text.splitlines():
        stripped = line.strip()
        if in_block:
            kept.append(stripped)
            if "*/" in stripped:
                in_block = False
            continue
        if not stripped:
            continue
        if stripped.startswith(("//", "#")) and not stripped.startswith(("#include", "#ifndef",
                                                                        "#define", "#if")):
            kept.append(stripped)
            continue
        if stripped.startswith("/*"):
            kept.append(stripped)
            in_block = "*/" not in stripped
            continue
        break
    return "\n".join(kept)


def classify(tag: str, path: str) -> tuple[str, str, str]:
    raw = fetch(HOST.format(tag=tag, path=path) + "?format=TEXT")
    header = leading_comment(base64.b64decode(raw).decode("utf-8", "replace"))
    if re.search(r"GNU (Library|Lesser) General Public|\bLGPL", header):
        kind = "LGPL"
    elif re.search(r"GNU General Public|\bGPL", header):
        kind = "GPL"
    elif re.search(r"BSD-style license|Redistribution and use in source and binary", header):
        kind = "BSD"
    else:
        kind = "NONE"
    first = next((line for line in header.splitlines() if "Copyright" in line), "")
    first = re.sub(r"^[/*#\s]+", "", first).strip()
    return path, kind, first


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()

    paths: list[str] = []
    for directory in DIRECTORIES:
        paths += files_under(args.tag, directory)
    for directory, prefixes in PREFIXES.items():
        paths += [f"{directory}/{name}" for name, kind in listing(args.tag, directory)
                  if kind == "blob" and name.startswith(prefixes) and name.endswith(SUFFIXES)]
    paths = sorted(set(paths))

    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        rows = sorted(pool.map(lambda p: classify(args.tag, p), paths))

    counts: dict[str, int] = {}
    with open(args.output, "w", encoding="utf-8", newline="\n") as out:
        out.write(f"# Licence headers at Chromium {args.tag}, by tools/svg/licence_audit.py.\n")
        out.write("# Header only: no file body was printed, stored or read.\n")
        out.write("path\tclass\tfirst_copyright\n")
        for path, kind, first in rows:
            counts[kind] = counts.get(kind, 0) + 1
            out.write(f"{path}\t{kind}\t{first}\n")
    summary = ", ".join(f"{k} {v}" for k, v in sorted(counts.items()))
    print(f"{len(rows)} files at {args.tag}: {summary}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
