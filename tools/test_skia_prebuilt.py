"""Tests for the archive handling of tools/skia_prebuilt.py's `cargo` command."""

from __future__ import annotations

import os
import tempfile
import time
import unittest
from pathlib import Path

from tools import skia_prebuilt

KEY = "a31b86ba3b767344d39a-x86_64-pc-windows-msvc-jpegd-jpege-pdf-textlayout"


def write_skia(skia_dir: Path, seed: bytes) -> None:
    skia_dir.mkdir(parents=True)
    for name in ("skia.lib", "skparagraph.lib", "icudtl.dat", "bindings.rs"):
        (skia_dir / name).write_bytes(seed + name.encode())


def write_run(target: Path, unit: str, log: str, seed: bytes) -> Path:
    build = target / "debug" / "build" / f"skia-bindings-{unit}"
    write_skia(build / "out" / "skia", seed)
    (build / "output").write_text(log, encoding="utf-8")
    return build / "out" / "skia"


class SkiaPrebuiltTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def test_target_dir_follows_cargo(self) -> None:
        env = {"CARGO_TARGET_DIR": "from-env"}
        self.assertEqual(skia_prebuilt.target_dir(["build", "--target-dir", "t"], env), Path("t"))
        self.assertEqual(skia_prebuilt.target_dir(["build", "--target-dir=u"], env), Path("u"))
        self.assertEqual(skia_prebuilt.target_dir(["build"], env), Path("from-env"))
        self.assertEqual(
            skia_prebuilt.target_dir(["build"], {}), skia_prebuilt.WORKSPACE / "target"
        )

    def test_runs_since_finds_profile_and_triple_layouts(self) -> None:
        target = self.root / "target"
        write_run(target, "old", "built", b"a")
        old = target / "debug" / "build" / "skia-bindings-old" / "output"
        os.utime(old, (time.time() - 60, time.time() - 60))
        since = time.time() - 1
        write_run(target, "new", "built", b"b")
        triple = target / "x86_64-pc-windows-msvc" / "release" / "build" / "skia-bindings-t"
        write_skia(triple / "out" / "skia", b"c")
        (triple / "output").write_text("built", encoding="utf-8")
        found = sorted(path.parent.parent.name for path, _ in skia_prebuilt.skia_runs_since(target, since))
        self.assertEqual(found, ["skia-bindings-new", "skia-bindings-t"])

    def test_installed_archive_must_equal_its_source_build(self) -> None:
        source = self.root / "source" / "skia"
        write_skia(source, b"built")
        cache = self.root / "cache"
        archive = skia_prebuilt.write_archive(cache, "0.94.0", KEY, {}, ["build"], source)
        log = (
            f"TRYING TO DOWNLOAD AND INSTALL SKIA BINARIES: 0.94.0/{KEY}\n"
            f"  FROM: file://{archive.as_posix()}\n{skia_prebuilt.INSTALLED}\n"
        )
        same = write_run(self.root / "target", "same", log, b"built")
        skia_prebuilt.verify_installed(same, log)

        changed = write_run(self.root / "target", "changed", log, b"built")
        (changed / "skia.lib").write_bytes(b"other bytes")
        with self.assertRaises(SystemExit):
            skia_prebuilt.verify_installed(changed, log)


if __name__ == "__main__":
    unittest.main()
