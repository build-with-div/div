#!/usr/bin/env python3
"""Export a commit without its withheld files, to a directory outside the project.

The export omits every listed file, any tracked copy of one, symlinks, ``docs/``,
agent instruction files and ``.git``. The receipt holds counts and digests only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.isolation.withheld import (  # noqa: E402
    IsolationError,
    TreeView,
    blob_oid,
    load_view,
    normalize_repo_path,
    path_key,
    refuse_unsafe_location,
)


def _write_visible_files(repo: Path, view: TreeView, target: Path) -> dict[str, str]:
    expected = {path_key(e.path) for e in view.visible}
    exported: dict[str, str] = {}
    archive = subprocess.Popen(
        ["git", "-C", str(repo), "-c", "core.autocrlf=false", "archive", "--format=tar", view.commit],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    try:
        with tarfile.open(fileobj=archive.stdout, mode="r|") as stream:
            for member in stream:
                if member.isdir():
                    continue
                name = normalize_repo_path(member.name)
                key = path_key(name)
                if key in view.withheld:
                    continue
                if key not in expected or not member.isfile():
                    raise IsolationError("unexpected_archive_entry", name)
                data = stream.extractfile(member).read()
                oid = blob_oid(data, view.object_format)
                normalized = blob_oid(data.replace(b"\r\n", b"\n"), view.object_format)
                if oid in view.listed_oids or normalized in view.listed_oids:
                    raise IsolationError("withheld_content_in_export", name)
                destination = target / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
                if member.mode & 0o111:
                    os.chmod(destination, 0o755)
                exported[name] = oid
    finally:
        archive.stdout.close()
        status = archive.wait()
    if status != 0:
        raise IsolationError("git_failed", "archive")
    if {path_key(name) for name in exported} != expected:
        raise IsolationError("export_incomplete")
    return exported


def export_tree(repo: Path, rev: str, list_path: Path, dest: Path, receipt: Path | None = None) -> dict:
    view = load_view(repo, rev, list_path)
    resolved = refuse_unsafe_location(dest, repo)
    if resolved.exists() and any(resolved.iterdir()):
        raise IsolationError("destination_not_empty", str(dest))
    receipt_path = receipt or resolved.with_name(resolved.name + ".receipt.json")
    partial = resolved.with_name(resolved.name + ".partial")
    if partial.exists():
        raise IsolationError("partial_export_exists", str(partial))
    partial.mkdir(parents=True)
    try:
        exported = _write_visible_files(repo, view, partial)
        if resolved.exists():
            resolved.rmdir()
        partial.rename(resolved)
    except BaseException:
        shutil.rmtree(partial, ignore_errors=True)
        raise
    tree_digest = hashlib.sha256()
    for name in sorted(exported):
        tree_digest.update(f"{name}\0{exported[name]}\n".encode("utf-8", "surrogateescape"))
    result = {
        "schema": "div-isolated-export-v1",
        "base_commit": view.commit,
        "list_sha256": view.list_sha256,
        "listed_files": len(view.listed),
        "withheld_files": len(view.withheld),
        "exported_files": len(exported),
        "export_tree_sha256": tree_digest.hexdigest(),
    }
    receipt_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--rev", default="main")
    parser.add_argument("--withheld-list", type=Path, required=True)
    parser.add_argument("--dest", type=Path, required=True)
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args(argv)
    try:
        result = export_tree(args.repo, args.rev, args.withheld_list, args.dest, args.receipt)
    except IsolationError as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
