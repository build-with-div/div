from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tools.ci import msvc_toolset

GN_ARGS = 'extra_cflags += [ "-msse3" ]'


class SkiaToolsetTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def vc(self, name: str, *toolsets: str) -> Path:
        vc = self.root / name / "VC"
        for toolset in toolsets:
            (vc / "Tools" / "MSVC" / toolset).mkdir(parents=True)
        vc.mkdir(parents=True, exist_ok=True)
        return vc

    def test_rust_skia_lookup_order(self) -> None:
        first = self.root / "2022" / "VC"
        second = self.vc("2019")
        candidates = [str(first), str(second)]
        self.assertEqual(msvc_toolset.rust_skia_vc({}, candidates), second)
        self.vc("2022")
        self.assertEqual(msvc_toolset.rust_skia_vc({}, candidates), first)
        self.assertEqual(
            msvc_toolset.rust_skia_vc({"VCINSTALLDIR": "D:\\VS\\VC\\"}, candidates), Path("D:\\VS\\VC")
        )
        self.assertIsNone(msvc_toolset.rust_skia_vc({}, [str(self.root / "none")]))

    def test_app_build_takes_the_highest_toolset_in_its_vc(self) -> None:
        both = self.vc("vs18", "14.44.35207", "14.52.37025", "not-a-toolset")
        view = self.vc("view", "14.44.35207")
        env = {"SKIA_GN_ARGS": GN_ARGS}
        self.assertEqual(msvc_toolset.skia_toolset(env, "app", [str(both)]), "14.52.37025")
        self.assertEqual(msvc_toolset.skia_toolset(env, "app", [str(view)]), "14.44.35207")
        self.assertIsNone(msvc_toolset.skia_toolset(env, "app", [str(self.root / "none")]))

    def test_prebuilt_build_takes_the_pin(self) -> None:
        both = self.vc("vs18", "14.44.35207", "14.52.37025")
        env = {
            "SKIA_GN_ARGS": GN_ARGS,
            "VCToolsInstallDir": str(both / "Tools" / "MSVC" / "14.44.35207") + "\\",
        }
        self.assertEqual(msvc_toolset.skia_toolset(env, "prebuilt", [str(both)]), "14.44.35207")


if __name__ == "__main__":
    unittest.main()
