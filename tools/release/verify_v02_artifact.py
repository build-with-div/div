#!/usr/bin/env python3
"""Verify structure, normalization, and sidecars of a Div v0.2 SDK."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tarfile
from pathlib import Path, PurePosixPath


ARCHIVE_RE = re.compile(
    r"^divui-sdk-0\.2\.0-(x86_64|aarch64)-unknown-linux-gnu\.tar\.gz$"
)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def member_json(archive: tarfile.TarFile, name: str) -> dict:
    member = archive.getmember(name)
    source = archive.extractfile(member)
    if source is None:
        raise SystemExit(f"not a regular JSON file: {name}")
    return json.loads(source.read())


def verify(archive_path: Path) -> None:
    if not ARCHIVE_RE.match(archive_path.name):
        raise SystemExit(f"unexpected SDK filename: {archive_path.name}")
    checksum_path = archive_path.with_suffix(archive_path.suffix + ".sha256")
    expected_checksum_line = f"{digest(archive_path)}  {archive_path.name}"
    if not checksum_path.is_file() or checksum_path.read_text(encoding="utf-8").strip() != expected_checksum_line:
        raise SystemExit("missing or invalid archive checksum sidecar")

    top = archive_path.name.removesuffix(".tar.gz")
    required = {
        f"{top}/include/divui/divui.h",
        f"{top}/include/divui/divui_style_properties.h",
        f"{top}/lib/libopenui.a",
        f"{top}/lib/libdivui.so",
        f"{top}/lib/libdivui.so.0",
        f"{top}/lib/libdivui.so.0.2.0",
        f"{top}/lib/pkgconfig/divui.pc",
        f"{top}/lib/cmake/DivUi/DivUiConfig.cmake",
        f"{top}/debug/libdivui.so.debug",
        f"{top}/share/divui/examples/c/hello.c",
        f"{top}/share/divui/examples/c/native/window.c",
        f"{top}/share/divui/examples/c/native/window.cc",
        f"{top}/share/divui/licenses/Apache-2.0.txt",
        f"{top}/share/divui/divui.spdx.json",
        f"{top}/share/divui/BUILD-METADATA.json",
        f"{top}/share/divui/MIGRATION.md",
        f"{top}/share/divui/UNSUPPORTED.md",
    }
    with tarfile.open(archive_path, "r:gz") as archive:
        members = archive.getmembers()
        names = {member.name for member in members}
        missing = sorted(required - names)
        if missing:
            raise SystemExit("missing SDK members: " + ", ".join(missing))
        metadata = member_json(archive, f"{top}/share/divui/BUILD-METADATA.json")
        sbom = member_json(archive, f"{top}/share/divui/divui.spdx.json")
        epoch = metadata["source_date_epoch"]
        if metadata["version"] != "0.2.0" or metadata["target"] not in archive_path.name:
            raise SystemExit("build metadata does not match the SDK name")
        if sbom.get("spdxVersion") != "SPDX-2.3" or not sbom.get("packages"):
            raise SystemExit("SDK does not contain a populated SPDX 2.3 SBOM")
        for member in members:
            pure = PurePosixPath(member.name)
            if pure.is_absolute() or ".." in pure.parts:
                raise SystemExit(f"unsafe archive path: {member.name}")
            if member.uid != 0 or member.gid != 0 or member.uname != "root" or member.gname != "root":
                raise SystemExit(f"non-normalized archive ownership: {member.name}")
            if member.mtime != epoch:
                raise SystemExit(f"non-normalized archive timestamp: {member.name}")
            expected_mode = 0o755 if member.isdir() else 0o777 if member.issym() else 0o644
            if member.mode != expected_mode:
                raise SystemExit(f"non-normalized archive mode: {member.name} {oct(member.mode)}")
        if archive.getmember(f"{top}/lib/libdivui.so").linkname != "libdivui.so.0":
            raise SystemExit("invalid unversioned shared-library symlink")
        if archive.getmember(f"{top}/lib/libdivui.so.0").linkname != "libdivui.so.0.2.0":
            raise SystemExit("invalid ABI-major shared-library symlink")

    provenance = archive_path.parent / f"divui-0.2.0-{metadata['target']}.intoto.jsonl"
    if not provenance.is_file():
        raise SystemExit("missing provenance sidecar")
    statement = json.loads(provenance.read_text(encoding="utf-8"))
    subjects = {item["name"]: item["digest"]["sha256"] for item in statement["subject"]}
    if subjects.get(archive_path.name) != digest(archive_path):
        raise SystemExit("provenance does not bind the SDK digest")
    print(
        f"v0.2 SDK verified: {archive_path.name} "
        f"members={len(members)} packages={len(sbom['packages'])}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    verify(args.archive.resolve())


if __name__ == "__main__":
    main()
