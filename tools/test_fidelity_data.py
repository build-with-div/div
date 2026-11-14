#!/usr/bin/env python3
"""Tests for tools/fidelity_data.py against a local data repository."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fidelity_data  # noqa: E402

FILES = {"gates/ledger.txt": b"gate one\ngate two\n", "archive/old/receipt.txt": b"archived\nreceipt\n"}


def git(*args: str, cwd: Path) -> str:
    return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", *args], cwd=cwd,
                          check=True, capture_output=True, text=True).stdout


class FetchLineEndingTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        base = Path(tmp.name)
        # Every git call, the tool's included, sees a global autocrlf=true.
        config = base / "gitconfig"
        config.write_text("[core]\n\tautocrlf = true\n", encoding="utf-8")
        env = mock.patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config), "GIT_CONFIG_NOSYSTEM": "1"})
        env.start()
        self.addCleanup(env.stop)

        data = base / "data"
        data.mkdir()
        git("init", "--quiet", cwd=data)
        rows = []
        for path, content in sorted(FILES.items()):
            (data / path).parent.mkdir(parents=True, exist_ok=True)
            (data / path).write_bytes(content)
            rows.append((path, hashlib.sha256(content).hexdigest(), len(content)))
        manifest = "".join(f"{d}  {n}  {p}\n" for p, d, n in rows).encode()
        (data / "MANIFEST.sha256").write_bytes(manifest)
        git("-c", "core.autocrlf=false", "add", "-A", cwd=data)
        git("commit", "--quiet", "-m", "data", cwd=data)
        profiles = {}
        for profile in fidelity_data.PROFILES:
            listing = "".join(f"{d}  {n}  {p}\n" for p, d, n in rows if p.startswith(profile + "/"))
            profiles[profile] = hashlib.sha256(listing.encode()).hexdigest()

        self.root = base / "checkout"
        self.root.mkdir()
        git("init", "--quiet", cwd=self.root)
        lock = self.root / "fidelity-data.lock"
        lock.write_text(json.dumps({"repository": str(data), "commit": git("rev-parse", "HEAD", cwd=data).strip(),
                                    "manifest_sha256": hashlib.sha256(manifest).hexdigest(),
                                    "profiles": profiles}), encoding="utf-8")
        state = self.root / "out" / "fidelity-data"
        for name, value in (("ROOT", self.root), ("LOCK", lock), ("STATE", state),
                            ("STAMP", state / "restored.json")):
            patcher = mock.patch.object(fidelity_data, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def restored(self, path: str) -> bytes:
        return (self.root / path.split("/", 1)[1]).read_bytes()

    def test_adding_a_profile_keeps_bytes_under_global_autocrlf(self):
        fidelity_data.fetch("gates", None)
        fidelity_data.fetch("all", None)
        for path, content in FILES.items():
            self.assertEqual(self.restored(path), content)
        fidelity_data.verify("all")

    def test_files_written_with_crlf_by_an_older_fetch_are_repaired(self):
        fidelity_data.fetch("all", None)
        clone = fidelity_data.STATE / "repository"
        receipt = clone / "archive/old/receipt.txt"
        receipt.write_bytes(FILES["archive/old/receipt.txt"].replace(b"\n", b"\r\n"))
        fidelity_data.fetch("all", None)
        self.assertEqual(receipt.read_bytes(), FILES["archive/old/receipt.txt"])


if __name__ == "__main__":
    unittest.main()
