#!/usr/bin/env python3
"""List the names that visible Rust files use from withheld files.

Every name and member in the output comes from a visible file. Withheld files
are parsed only to classify those names and are never displayed.
"""

from __future__ import annotations

import argparse
import json
import posixpath
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.isolation.withheld import (  # noqa: E402
    IsolationError,
    Entry,
    load_view,
    path_key,
    read_blobs,
)

CAVEAT = (
    "This manifest is not a complete interface. Imports are matched by name without "
    "type resolution. Entries with status 'unresolved' are modules, re-exports, macros "
    "or similar and were not traced to a definition. Glob imports are approximated by "
    "matching identifiers. Fields and methods reached without a 'Name::member' path "
    "are not listed. Use filtered compiler results for anything missing here."
)

_NOISE = re.compile(
    r"""
    //[^\n]*
    | (?P<block>/\*)
    | \b[bc]?r(?P<hashes>\#*)".*?"(?P=hashes)
    | "(?:\\.|[^"\\])*"
    | '(?:\\.[^']{0,10}|[^\\'\n])'
    """,
    re.S | re.X,
)
_USE = re.compile(r"\buse\s+([^;]*);")
_USE_TOKEN = re.compile(r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*|::|[{},*]")
_PUBLIC_ITEM = re.compile(
    r"^\s*pub(?:\s*\([^)]*\))?\s+"
    r"(?:(?:const|async|unsafe|extern(?:\s+\"\")?)\s+)*"
    r"(enum|struct|type|trait|fn|const|static|union|mod)\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
_PATH_PAIR = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)")
_IDENTIFIER = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
_PACKAGE_NAME = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.M)
_MEMBER_KINDS = {"enum", "struct", "trait", "type", "union", "mod"}


def strip_rust_noise(text: str) -> str:
    """Remove comments and blank string and character literals."""
    output: list[str] = []
    position = 0
    while True:
        match = _NOISE.search(text, position)
        if match is None:
            output.append(text[position:])
            return "".join(output)
        output.append(text[position : match.start()])
        if match.group("block"):
            depth, cursor = 1, match.end()
            while depth and cursor < len(text):
                pair = text[cursor : cursor + 2]
                if pair == "/*":
                    depth, cursor = depth + 1, cursor + 2
                elif pair == "*/":
                    depth, cursor = depth - 1, cursor + 2
                else:
                    cursor += 1
            position = cursor
            output.append(" ")
        else:
            token = match.group(0)
            output.append('""' if '"' in token else " " if token.startswith("/") else "''")
            position = match.end()


def flatten_use(tree: str) -> list[tuple[list[str], str | None, bool]]:
    """Flatten a use tree into (segments, alias, is_glob)."""
    tokens = _USE_TOKEN.findall(tree)
    results: list[tuple[list[str], str | None, bool]] = []
    index = 0

    def parse(prefix: list[str]) -> None:
        nonlocal index
        segments = list(prefix)
        names_self = False
        while index < len(tokens):
            token = tokens[index]
            if token == "::":
                index += 1
            elif token == "{":
                index += 1
                while index < len(tokens) and tokens[index] != "}":
                    parse(segments)
                    if index < len(tokens) and tokens[index] == ",":
                        index += 1
                index += 1
                return
            elif token == "*":
                index += 1
                results.append((segments, None, True))
                return
            elif token in (",", "}"):
                break
            elif token == "as":
                alias = tokens[index + 1] if index + 1 < len(tokens) else None
                index += 2
                results.append((segments, alias, False))
                return
            else:
                name = token.removeprefix("r#")
                if name == "self" and segments:
                    names_self = True
                else:
                    segments = [*segments, name]
                index += 1
        if names_self or segments != prefix:
            results.append((segments, None, False))

    while index < len(tokens):
        before = index
        parse([])
        if index < len(tokens) and tokens[index] in (",", "}"):
            index += 1
        if index == before:
            index += 1
    return results


def crate_for(path: str, packages: dict[str, str]) -> tuple[str | None, str | None]:
    """Return the crate identifier and package directory for a path."""
    directory = posixpath.dirname(path)
    while True:
        if directory in packages:
            return packages[directory].replace("-", "_"), directory
        if not directory:
            return None, None
        directory = posixpath.dirname(directory)


def module_for(path: str, package_dir: str | None) -> list[str] | None:
    prefix = f"{package_dir}/src/" if package_dir else "src/"
    if not path.startswith(prefix) or not path.endswith(".rs"):
        return None
    parts = path[len(prefix) : -3].split("/")
    if parts[-1] in ("lib", "main", "mod"):
        parts = parts[:-1]
    return parts


def _resolve(segments, crate, module, owners):
    head = segments[0]
    if head == "crate":
        return (crate, segments[1:]) if crate and module is not None else None
    if head in ("self", "super"):
        if crate is None or module is None:
            return None
        base = list(module)
        rest = list(segments)
        while rest and rest[0] in ("self", "super"):
            if rest.pop(0) == "super":
                if not base:
                    return None
                base.pop()
        return crate, base + rest
    return (head, segments[1:]) if head in owners else None


def build_manifest(repo: Path, rev: str, list_path: Path) -> dict:
    view = load_view(repo, rev, list_path)
    blobs = read_blobs(repo, [e.oid for e in view.entries if e.path.endswith((".rs", "Cargo.toml"))])

    packages: dict[str, str] = {}
    for entry in view.entries:
        if posixpath.basename(entry.path) == "Cargo.toml" and path_key(entry.path) not in view.withheld:
            text = blobs[entry.oid].decode("utf-8", "replace")
            section = text.split("[package]", 1)
            match = _PACKAGE_NAME.search(section[1]) if len(section) == 2 else None
            if match:
                packages[posixpath.dirname(entry.path)] = match.group(1)

    rust = [e for e in view.entries if e.path.endswith(".rs")]
    stripped = {e.path: strip_rust_noise(blobs[e.oid].decode("utf-8", "replace")) for e in rust}

    listed_defs: dict[str, dict[str, list[tuple[str, str, list[str] | None]]]] = defaultdict(lambda: defaultdict(list))
    other_defs: dict[str, dict[str, list[list[str] | None]]] = defaultdict(lambda: defaultdict(list))
    for entry in rust:
        crate, package_dir = crate_for(entry.path, packages)
        if crate is None:
            continue
        module = module_for(entry.path, package_dir)
        for kind, name in _PUBLIC_ITEM.findall(stripped[entry.path]):
            if path_key(entry.path) in view.listed:
                listed_defs[crate][name].append((entry.path, kind, module))
            elif path_key(entry.path) not in view.withheld:
                other_defs[crate][name].append(module)
    owners = {
        crate
        for crate in (crate_for(e.path, packages)[0] for e in view.listed_entries if e.path.endswith(".rs"))
        if crate
    }

    records: dict[tuple[str, str], dict] = {}

    def record(crate: str, name: str, consumer: str, *, via_glob: bool, module_path: list[str] | None):
        definitions = listed_defs.get(crate, {}).get(name, [])
        others = other_defs.get(crate, {}).get(name, [])
        if definitions and others and module_path is not None:
            exact = [d for d in definitions if d[2] == module_path]
            if exact:
                definitions, others = exact, []
            elif module_path in others:
                return None
        elif others and not definitions:
            return None
        item = records.setdefault(
            (crate, name),
            {
                "crate": crate,
                "name": name,
                "status": "defined" if definitions else "unresolved",
                "kinds": set(),
                "defined_in": set(),
                "members": set(),
                "consumers": set(),
                "via_glob": False,
                "ambiguous": False,
            },
        )
        item["kinds"].update(d[1] for d in definitions)
        item["defined_in"].update(d[0] for d in definitions)
        item["consumers"].add(consumer)
        item["via_glob"] = item["via_glob"] or via_glob
        item["ambiguous"] = item["ambiguous"] or bool(definitions and others)
        return item

    glob_modules: dict[str, set[str]] = defaultdict(set)
    for entry in view.visible:
        if not entry.path.endswith(".rs"):
            continue
        text = stripped[entry.path]
        crate, package_dir = crate_for(entry.path, packages)
        module = module_for(entry.path, package_dir)
        local: dict[str, list[dict]] = defaultdict(list)
        globs: list[tuple[str, list[str]]] = []
        for tree in _USE.findall(text):
            for segments, alias, is_glob in flatten_use(tree):
                if not segments:
                    continue
                resolved = _resolve(segments, crate, module, owners)
                if resolved is None or resolved[0] not in owners:
                    continue
                target_crate, rest = resolved
                if is_glob:
                    globs.append((target_crate, rest))
                    glob_modules["::".join([target_crate, *rest])].add(entry.path)
                elif rest:
                    item = record(target_crate, rest[-1], entry.path, via_glob=False, module_path=rest[:-1])
                    if item is not None:
                        local[alias or rest[-1]].append(item)
        if globs:
            identifiers = set(_IDENTIFIER.findall(text))
            for target_crate, rest in globs:
                for name, definitions in listed_defs.get(target_crate, {}).items():
                    if name in identifiers and (not rest or any(d[2] == rest for d in definitions)):
                        item = record(target_crate, name, entry.path, via_glob=True, module_path=rest or None)
                        if item is not None:
                            local[name].append(item)
        for left, right in _PATH_PAIR.findall(text):
            for item in local.get(left, ()):
                if item["status"] == "unresolved" or item["kinds"] & _MEMBER_KINDS:
                    item["members"].add(right)

    names = [
        {
            "crate": item["crate"],
            "name": item["name"],
            "status": item["status"],
            "kinds": sorted(item["kinds"]),
            "defined_in": sorted(item["defined_in"]),
            "members": sorted(item["members"]),
            "consumer_files": len(item["consumers"]),
            "via_glob": item["via_glob"],
            "ambiguous": item["ambiguous"],
        }
        for _, item in sorted(records.items())
    ]
    return {
        "schema": "div-interface-manifest-v1",
        "base_commit": view.commit,
        "list_sha256": view.list_sha256,
        "caveat": CAVEAT,
        "counts": {
            "names": len(names),
            "defined": sum(1 for n in names if n["status"] == "defined"),
            "unresolved": sum(1 for n in names if n["status"] == "unresolved"),
            "ambiguous": sum(1 for n in names if n["ambiguous"]),
            "via_glob": sum(1 for n in names if n["via_glob"]),
        },
        "names": names,
        "glob_imports": [
            {"module": module, "consumer_files": len(files)} for module, files in sorted(glob_modules.items())
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--rev", default="main")
    parser.add_argument("--withheld-list", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        manifest = build_manifest(args.repo, args.rev, args.withheld_list)
    except IsolationError as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    args.out.write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(manifest["counts"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
