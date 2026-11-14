#!/usr/bin/env python3
"""Fetch and verify the pinned fidelity evidence for this checkout.

The comparison history (census results, the frozen 5,731-image archive,
ported-test ledgers and older evidence) lives in a separate repository,
pinned by commit and SHA-256 in fidelity-data.lock at the repository root.
Each file there sits at <profile>/<path>; this script restores it at <path>
in the checkout, where the tools and the hash-pinned ledgers expect it.

    python3 tools/fidelity_data.py fetch            # profile `gates`: what CI reads
    python3 tools/fidelity_data.py fetch --profile all
    python3 tools/fidelity_data.py verify           # re-hash the restored files
    python3 tools/fidelity_data.py status

Any mismatch (commit, manifest or file hash, or a restored path that is
tracked in git) fails; nothing is downloaded from anywhere else.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "fidelity-data.lock"
STATE = ROOT / "out" / "fidelity-data"
STAMP = STATE / "restored.json"
PROFILES = ("gates", "archive")
CLONE_CONFIG = (("core.autocrlf", "false"), ("core.longpaths", "true"))


def fail(message: str) -> None:
    raise SystemExit(f"fidelity_data: {message}")


def git(*args: str, cwd: Path | None = None) -> str:
    proc = subprocess.run(["git", *args], cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if proc.returncode != 0:
        fail(f"git {' '.join(args)} failed:\n{proc.stderr}")
    return proc.stdout


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock() -> dict:
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    for key in ("repository", "commit", "manifest_sha256", "profiles"):
        if key not in lock:
            fail(f"{LOCK.name} has no {key!r}")
    return lock


def selected(profile: str) -> tuple[str, ...]:
    return PROFILES if profile == "all" else (profile,)


def parse_manifest(text: str, profiles: tuple[str, ...]) -> list[tuple[str, str, int]]:
    rows = []
    for line in text.splitlines():
        digest, size, path = line.split("  ", 2)
        if path.split("/", 1)[0] in profiles:
            rows.append((path, digest, int(size)))
    return rows


def fetch(profile: str, source: str | None) -> None:
    lock = load_lock()
    profiles = selected(profile)
    clone = STATE / "repository"
    url = source or os.environ.get("DIV_FIDELITY_DATA_URL") or lock["repository"]
    if not (clone / ".git").is_dir():
        clone.parent.mkdir(parents=True, exist_ok=True)
        git("clone", "--quiet", "--no-checkout", "--filter=blob:none", "--sparse",
            *[f"--config={k}={v}" for k, v in CLONE_CONFIG], url, str(clone))
    # The files are hashed byte for byte, so every git call that writes them,
    # including `sparse-checkout set` when a later fetch adds a profile, must
    # skip line-ending conversion whatever the user's global config says.
    # Set it in the clone's own config, which also repairs older clones.
    for key, value in CLONE_CONFIG:
        git("config", key, value, cwd=clone)
    git("sparse-checkout", "set", "--no-cone", "/MANIFEST.sha256", "/SOURCE.json",
        "/README.md", *[f"/{p}/" for p in profiles], cwd=clone)
    if git("rev-parse", "--verify", "--quiet", f"{lock['commit']}^{{commit}}", cwd=clone).strip() != lock["commit"]:
        git("fetch", "--quiet", "--filter=blob:none", "origin", lock["commit"], cwd=clone)
    git("checkout", "--quiet", "--detach", lock["commit"], cwd=clone)
    head = git("rev-parse", "HEAD", cwd=clone).strip()
    if head != lock["commit"]:
        fail(f"data checkout is at {head}, the lock pins {lock['commit']}")

    manifest_text = (clone / "MANIFEST.sha256").read_bytes()
    if hashlib.sha256(manifest_text).hexdigest() != lock["manifest_sha256"]:
        fail("MANIFEST.sha256 does not match fidelity-data.lock")
    manifest_text = manifest_text.decode("utf-8")
    for p in profiles:
        rows = parse_manifest(manifest_text, (p,))
        listing = "".join(f"{d}  {n}  {path}\n" for path, d, n in rows)
        if hashlib.sha256(listing.encode()).hexdigest() != lock["profiles"][p]:
            fail(f"profile {p!r} listing does not match fidelity-data.lock")

    rows = parse_manifest(manifest_text, profiles)

    def matches(path: str, digest: str, size: int) -> bool:
        src = clone / path
        return src.is_file() and src.stat().st_size == size and sha256_file(src) == digest

    # A clone made before core.autocrlf was set in its config may hold files
    # written with CRLF; git considers them current, so write them again.
    stale = [path for path, digest, size in rows if not matches(path, digest, size)]
    if stale:
        for path in stale:
            (clone / path).unlink(missing_ok=True)
        subprocess.run(["git", "checkout", "--quiet", "HEAD", "--pathspec-from-file=-", "--pathspec-file-nul"],
                       cwd=clone, input="\0".join(stale), text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    tracked = set(git("ls-files", "-z", cwd=ROOT).split("\0"))
    restored = []
    for path, digest, size in rows:
        src = clone / path
        if not matches(path, digest, size):
            fail(f"{path} does not match MANIFEST.sha256")
        rel = path.split("/", 1)[1]
        if rel in tracked:
            fail(f"{rel} is tracked in this repository; the data repository must not overwrite it")
        dest = ROOT / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        if dest.exists():
            if dest.stat().st_size == size and sha256_file(dest) == digest:
                restored.append(rel)
                continue
            dest.unlink()
        try:
            os.link(src, dest)
        except OSError:
            shutil.copyfile(src, dest)
        restored.append(rel)
    STAMP.write_text(json.dumps({"commit": lock["commit"], "profiles": list(profiles), "files": len(restored)},
                                indent=2) + "\n", encoding="utf-8")
    print(f"fidelity_data: restored {len(restored)} files from {lock['repository']} at {lock['commit'][:12]} "
          f"({', '.join(profiles)})")


def verify(profile: str) -> None:
    lock = load_lock()
    manifest = STATE / "repository" / "MANIFEST.sha256"
    if not manifest.is_file():
        fail("nothing restored yet; run `fetch` first")
    text = manifest.read_bytes()
    if hashlib.sha256(text).hexdigest() != lock["manifest_sha256"]:
        fail("restored data is from another commit; run `fetch` again")
    bad = []
    rows = parse_manifest(text.decode("utf-8"), selected(profile))
    for path, digest, size in rows:
        dest = ROOT / path.split("/", 1)[1]
        if not dest.is_file() or dest.stat().st_size != size or sha256_file(dest) != digest:
            bad.append(dest.relative_to(ROOT).as_posix())
    if bad:
        fail(f"{len(bad)} restored files differ, first: {bad[0]}")
    print(f"fidelity_data: {len(rows)} files match {lock['commit'][:12]}")


def status() -> None:
    lock = load_lock()
    print(f"pinned: {lock['repository']} @ {lock['commit']}")
    if STAMP.is_file():
        print("restored:", STAMP.read_text(encoding="utf-8").strip())
    else:
        print("restored: nothing")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch", help="clone the pinned commit and restore its files")
    f.add_argument("--profile", choices=(*PROFILES, "all"), default="gates")
    f.add_argument("--source", help="clone from this URL or path instead of the locked repository")
    v = sub.add_parser("verify", help="re-hash the restored files")
    v.add_argument("--profile", choices=(*PROFILES, "all"), default="gates")
    sub.add_parser("status", help="show the pin and what is restored")
    args = parser.parse_args(argv)
    if args.command == "fetch":
        fetch(args.profile, args.source)
    elif args.command == "verify":
        verify(args.profile)
    else:
        status()
    return 0


if __name__ == "__main__":
    sys.exit(main())
