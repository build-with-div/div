"""Guards for the Windows Chrome reference tooling that run on any host."""

from __future__ import annotations

from pathlib import Path
import re
import tempfile
import unittest

from tools.qualification import probe_windows_text_oracle as probe
from tools.qualification import windows_chrome


class WindowsChromeToolTests(unittest.TestCase):
    def test_unpinned_binaries_are_rejected_before_launch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            fake = Path(temporary) / "chrome.exe"
            fake.write_bytes(b"not the pinned browser")
            (Path(temporary) / f"{windows_chrome.CHROMIUM_VERSION}.manifest").write_text("")
            with self.assertRaisesRegex(SystemExit, "pinned Windows Chrome"):
                windows_chrome.verify_chrome(fake)

    def test_pages_are_deterministic_and_load_repository_ahem(self) -> None:
        first = probe.page("<div></div>", "div{color:red}")
        self.assertEqual(first, probe.page("<div></div>", "div{color:red}"))
        self.assertIn("@font-face{font-family:Ahem;src:url(data:font/truetype;base64,", first)

    def test_every_scenario_names_an_existing_test_function(self) -> None:
        for name in probe.SCENARIOS:
            path, function = name.split("::")
            source = (windows_chrome.ROOT / path).read_text(encoding="utf-8")
            self.assertRegex(source, rf"\bfn {re.escape(function)}\b", name)

    def test_aliased_scenarios_disable_font_antialiasing(self) -> None:
        for name, (_body, _style, flags, _measure, _scales) in probe.SCENARIOS.items():
            source = (windows_chrome.ROOT / name.split("::")[0]).read_text(encoding="utf-8")
            if "deterministic_aliased" in source and flags:
                self.assertEqual(flags, probe.ALIASED, name)


def _settings_reader(
    smoothing: int | None = 1,
    smoothing_type: int | None = windows_chrome.FE_FONTSMOOTHINGCLEARTYPE,
    orientation: int | None = windows_chrome.FE_FONTSMOOTHINGORIENTATIONRGB,
    display: str | None = "DISPLAY1",
    keys: tuple[str, ...] = (),
):
    parameters = {
        windows_chrome.SPI_GETFONTSMOOTHING: smoothing,
        windows_chrome.SPI_GETFONTSMOOTHINGTYPE: smoothing_type,
        windows_chrome.SPI_GETFONTSMOOTHINGORIENTATION: orientation,
    }
    queried = []

    def key_exists(path: str) -> bool:
        queried.append(path)
        return path in keys

    settings = windows_chrome.read_text_raster_settings(
        parameters.__getitem__, lambda: display, key_exists
    )
    return settings, queried


class TextRasterPinTests(unittest.TestCase):
    """The oracle pin of RFC 0012, with the Windows reads injected."""

    def assert_refused(self, settings, *setting_names: str) -> None:
        with self.assertRaises(SystemExit) as refused:
            windows_chrome.check_text_raster_pin(settings)
        message = str(refused.exception)
        self.assertIn("RFC 0012", message)
        for name in setting_names:
            self.assertIn(name, message)

    def test_cleartype_rgb_without_tuning_is_the_pin(self) -> None:
        settings, queried = _settings_reader()
        self.assertEqual(
            settings,
            windows_chrome.TextRasterSettings(True, 2, 1, "DISPLAY1", False),
        )
        self.assertEqual(queried, [r"SOFTWARE\Microsoft\Avalon.Graphics\DISPLAY1"])
        self.assertEqual(windows_chrome.text_raster_pin_failures(settings), [])
        windows_chrome.check_text_raster_pin(settings)

    def test_font_smoothing_off_is_refused(self) -> None:
        settings, _ = _settings_reader(smoothing=0)
        self.assert_refused(settings, "SPI_GETFONTSMOOTHING) is off")

    def test_standard_smoothing_is_refused(self) -> None:
        settings, _ = _settings_reader(smoothing_type=windows_chrome.FE_FONTSMOOTHINGSTANDARD)
        self.assert_refused(settings, "SPI_GETFONTSMOOTHINGTYPE) is standard, not ClearType")

    def test_bgr_order_is_refused(self) -> None:
        settings, _ = _settings_reader(orientation=windows_chrome.FE_FONTSMOOTHINGORIENTATIONBGR)
        self.assert_refused(settings, "SPI_GETFONTSMOOTHINGORIENTATION) is BGR, not RGB")

    def test_per_display_tuning_key_is_refused(self) -> None:
        key = r"SOFTWARE\Microsoft\Avalon.Graphics\DISPLAY2"
        settings, _ = _settings_reader(display="DISPLAY2", keys=(key,))
        self.assertTrue(settings.per_display_key)
        self.assert_refused(settings, "HKCU\\" + key + " exists")

    def test_only_the_primary_display_key_counts(self) -> None:
        other = r"SOFTWARE\Microsoft\Avalon.Graphics\DISPLAY2"
        settings, queried = _settings_reader(keys=(other,))
        self.assertEqual(queried, [r"SOFTWARE\Microsoft\Avalon.Graphics\DISPLAY1"])
        self.assertEqual(windows_chrome.text_raster_pin_failures(settings), [])

    def test_unread_settings_are_refused(self) -> None:
        settings, queried = _settings_reader(smoothing=None, orientation=None, display=None)
        self.assertEqual(queried, [])
        self.assert_refused(
            settings,
            "SPI_GETFONTSMOOTHING) could not be read",
            "SPI_GETFONTSMOOTHINGORIENTATION) could not be read",
        )
        settings, _ = _settings_reader(smoothing_type=None)
        self.assert_refused(settings, "SPI_GETFONTSMOOTHINGTYPE) could not be read")

    def test_every_failure_is_named_at_once(self) -> None:
        settings, _ = _settings_reader(
            smoothing_type=windows_chrome.FE_FONTSMOOTHINGSTANDARD,
            orientation=windows_chrome.FE_FONTSMOOTHINGORIENTATIONBGR,
            keys=(r"SOFTWARE\Microsoft\Avalon.Graphics\DISPLAY1",),
        )
        self.assertEqual(len(windows_chrome.text_raster_pin_failures(settings)), 3)

    def test_display_names_are_trimmed_like_chromium(self) -> None:
        self.assertEqual(windows_chrome.display_base_name("\\\\.\\DISPLAY1"), "DISPLAY1")
        self.assertEqual(windows_chrome.display_base_name("DISPLAY3"), "DISPLAY3")


if __name__ == "__main__":
    unittest.main()
