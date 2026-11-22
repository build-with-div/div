#!/usr/bin/env python3
"""Build RFC 0005's per-suite ledger of WPT testharness subtests.

For each suite the ledger has one row per upstream subtest, as pinned
Chromium 147 reports them (`capture_chromium_subtests.py`), with:

- `chromium`: Chromium's own result (PASS, FAIL, ...), from the recording,
  cross-checked against the pinned `-expected.txt` baselines;
- `status`: what Div can do with the subtest today:
  - `translatable`: every web surface it uses maps to a `divui` call;
  - `blocked`: some surface maps to a feature Div lacks (`features`) or to
    nothing yet (`unmapped`);
  - `untranslatable`: the script needs something a straight-line Rust test
    cannot express (`reasons`);
  - `excluded`: the file is not run (manual, reference, wptserve features).

The front end (`translate.mjs`, pinned Acorn) lists each page's subtest
declarations and their surfaces. Helper libraries are intrinsics, known by
path and sha256 (`intrinsics.json`); a changed helper fails until reviewed.
`capabilities.json` maps each surface to its plan feature and its `divui`
call, or to null while the call does not exist. Nothing maps to a near
equivalent.

Usage:

    python tools/wpt_script/ledger.py --wpt-root WPT [--suite S ...] [--check]

`--check` regenerates every ledger and fails on any difference. The WPT
root is `third_party/blink/web_tests/external/wpt` of the pinned Chromium
checkout at 09d377d9 (`WPT_SCRIPT_ROOT` by default), with the suites'
directories checked out. It is kept apart from `CHROMIUM_WPT_ROOT`: the CSS
porter resolves `/resources/` and `/common/` files that exist under its
root, so checking those out there would change pinned porter output. Acorn
comes from `CHROMIUM_ACORN`.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import capture_chromium_subtests as capture  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
TOOL = Path(__file__).resolve().parent
CHROMIUM = ROOT / "tests/wpt-script/chromium"
LEDGERS = ROOT / "tests/wpt-script/ledgers"
# Div's recorded results for the generated tests (codegen.py --record).
EXPECTED = ROOT / "tests/wpt-script/expected"
SUITES = ("html/webappapis/animation-frames", "resize-observer")
ACORN_SHA256 = "402f6759c744ea2ca501c1dc4ded8b1c54c9066cc7a741837057dcb68bc64c93"

# `<script>`, including XHTML/SVG forms such as `<h:script src="..."/>`.
SCRIPT = re.compile(
    r"<(?:[\w-]+:)?script\b([^>]*?)(?:/>|>(.*?)</(?:[\w-]+:)?script\s*>)", re.I | re.S)
SRC = re.compile(r"""\b(?:src|xlink:href|href)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", re.I)
TITLE = re.compile(r"<title\b[^>]*>(.*?)</title\s*>", re.I | re.S)
CDATA = re.compile(r"^\s*(?://)?\s*<!\[CDATA\[(.*?)(?://)?\s*\]\]>\s*$", re.S)
EXPECTED_LINE = re.compile(r"^\[(PASS|FAIL|TIMEOUT|NOTRUN|PRECONDITION_FAILED)\] (.*)$")
# The harness itself: no web surface, nothing to resolve.
HARNESS_SCRIPTS = {"/resources/testharness.js", "/resources/testharnessreport.js"}


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def resolve(src: str, page: str) -> str:
    """A script URL as a path under the WPT root."""
    src = src.split("?", 1)[0].split("#", 1)[0]
    if src.startswith("/"):
        return src.lstrip("/")
    base = page.rsplit("/", 1)[0]
    parts = []
    for part in f"{base}/{src}".split("/"):
        if part in ("", "."):
            continue
        if part == "..":
            parts.pop()
        else:
            parts.append(part)
    return "/".join(parts)


def read_page(wpt_root: Path, relative: str, intrinsics: dict) -> dict:
    """The page's title, inline scripts and resolved helpers."""
    text = (wpt_root / relative).read_text(encoding="utf-8")
    title_match = TITLE.search(text)
    title = html.unescape(re.sub(r"\s+", " ", title_match.group(1)).strip()) if title_match else None
    scripts, helpers, problems = [], [], []
    for match in SCRIPT.finditer(text):
        attributes, body = match.group(1), match.group(2) or ""
        source = SRC.search(attributes)
        if source:
            src = next(group for group in source.groups() if group is not None)
            if src in HARNESS_SCRIPTS:
                continue
            path = resolve(src, relative)
            intrinsic = intrinsics.get(path)
            if intrinsic is None:
                problems.append(f"unrecognised-helper:{path}")
            elif not (wpt_root / path).is_file() or sha256(wpt_root / path) != intrinsic["sha256"]:
                problems.append(f"changed-helper:{path}")
            else:
                helpers.append(path)
            continue
        cdata = CDATA.match(body)
        scripts.append({"source": cdata.group(1) if cdata else body, "offset": match.start(2)})
    return {"file": relative, "title": title, "scripts": scripts,
            "helpers": helpers, "problems": problems, "markup": markup_surfaces(text, relative)}


STYLE_ELEMENT = re.compile(r"<(?:[\w-]+:)?style\b[^>]*>.*?</(?:[\w-]+:)?style\s*>", re.I | re.S)
COMMENT = re.compile(r"<!--.*?-->", re.S)
TAG = re.compile(r"<([a-zA-Z][\w:.-]*)\b([^>]*)>")
STYLE_ATTRIBUTE = re.compile(r"\sstyle\s*=", re.I)
REL_STYLESHEET = re.compile(r"""\brel\s*=\s*["']?stylesheet""", re.I)
# Document structure the generated test does not build: `TestApp` provides it.
STRUCTURE = {"html", "head", "body", "title", "meta", "link", "script", "style", "?xml"}


def markup_surfaces(text: str, relative: str) -> list[str]:
    """What building the page's static markup needs (RFC 0005 "page setup").

    Each element tag, `<style>` sheets, `style` attributes and linked
    stylesheets. Every subtest on the page needs them.
    """
    surfaces = set()
    if STYLE_ELEMENT.search(text):
        surfaces.add("markup:<style>")
    body = STYLE_ELEMENT.sub("", SCRIPT.sub("", COMMENT.sub("", text)))
    for match in TAG.finditer(body):
        tag = match.group(1).split(":")[-1].lower()
        attributes = match.group(2)
        if tag == "link" and REL_STYLESHEET.search(attributes):
            href = SRC.search(attributes)
            if href:
                value = next(group for group in href.groups() if group is not None)
                surfaces.add(f"markup:stylesheet:{resolve(value, relative)}")
        if tag in STRUCTURE:
            continue
        surfaces.add(f"markup:{tag}")
        if STYLE_ATTRIBUTE.search(attributes):
            surfaces.add("markup:style-attribute")
    return sorted(surfaces)


def front_end(pages: list[dict], intrinsics: dict, acorn: str) -> dict:
    payload = []
    for page in pages:
        names = sorted({name for helper in page["helpers"]
                        for name in intrinsics[helper]["provides"]})
        payload.append({"file": page["file"], "title": page["title"],
                        "scripts": page["scripts"], "helpers": names})
    result = subprocess.run(
        ["node", str(TOOL / "translate.mjs"), acorn],
        input=json.dumps(payload), capture_output=True, text=True, encoding="utf-8", check=True)
    return {record["file"]: record for record in json.loads(result.stdout)}


def expected_baseline(wpt_root: Path, relative: str) -> dict[str, str] | None:
    """Chromium's `-expected.txt` baseline for a page, if it has one."""
    stem = relative.rsplit(".", 1)[0]
    path = wpt_root / f"{stem}-expected.txt"
    if not path.is_file():
        return None
    results = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        match = EXPECTED_LINE.match(line)
        if match:
            results[match.group(2)] = match.group(1)
    return results


def classify(surfaces: list[str], capabilities: dict) -> dict:
    features, unmapped = set(), set()
    for surface in surfaces:
        entry = capabilities.get(surface)
        if entry is None:
            unmapped.add(surface)
        elif entry.get("divui") is None:
            features.add(entry["feature"])
    if unmapped or features:
        row = {"status": "blocked", "features": sorted(features)}
        if unmapped:
            row["unmapped"] = sorted(unmapped)
        return row
    return {"status": "translatable"}


def subtest_surfaces(record: dict, page: dict, intrinsics: dict) -> list[str]:
    """The subtest's own surfaces, with the helpers it uses resolved.

    A helper function it calls contributes the web surfaces the intrinsic
    lists for it; calls to the helper objects' own methods (`methods`) are
    harness surfaces, not web ones.
    """
    used = {surface.removeprefix("harness:") for surface in record["surfaces"]
            if surface.startswith("harness:")}
    used.add(record["declared_by"].removeprefix("harness:"))
    found, methods = set(), set()
    for helper in page["helpers"]:
        intrinsic = intrinsics[helper]
        for name, surfaces in intrinsic["provides"].items():
            if name in used:
                found.update(surfaces)
        for method in intrinsic.get("methods", []):
            methods.update({f".{method}()", f".{method}"})
    for surface in record["surfaces"]:
        if surface in methods:
            found.add(f"harness:{surface}")
        else:
            found.add(surface)
    return sorted(found)


def build(suite: str, wpt_root: Path, acorn: str, intrinsics: dict, capabilities: dict) -> dict:
    recording = load_json(CHROMIUM / (suite.replace("/", "--") + ".json"))
    files = recording["files"]
    # The recording must cover exactly the suite's pages at the pinned
    # revision; a page it lacks would drop out of the denominator.
    on_disk = {relative for relative in capture.suite_files(wpt_root, suite)
               if capture.exclusion(relative) != "helper"}
    if on_disk != set(files):
        missing = sorted(on_disk - set(files))[:5]
        extra = sorted(set(files) - on_disk)[:5]
        raise SystemExit(f"{suite}: the Chromium recording is stale "
                         f"(not recorded: {missing}; not on disk: {extra})")
    pages = {relative: read_page(wpt_root, relative, intrinsics)
             for relative, entry in files.items() if "excluded" not in entry}
    analysed = front_end(list(pages.values()), intrinsics, acorn) if pages else {}
    expected_path = EXPECTED / (suite.replace("/", "--") + ".json")
    div = load_json(expected_path) if expected_path.is_file() else {}
    rows = []
    for relative, entry in files.items():
        if "excluded" in entry:
            rows.append({"file": relative, "name": None, "chromium": None,
                         "status": "excluded", "reason": entry["excluded"]})
            continue
        page, record = pages[relative], analysed[relative]
        declared = {}
        for subtest in record["subtests"]:
            if subtest["name_known"]:
                declared.setdefault(subtest["name"], subtest)
        dynamic = [s for s in record["subtests"] if not s["name_known"]]
        baseline = expected_baseline(wpt_root, relative)
        for observed in entry["subtests"]:
            name = observed["name"]
            row = {"file": relative, "name": name, "chromium": observed["status"]}
            if baseline is not None and baseline.get(name, "PASS") != observed["status"]:
                row["chromium_baseline"] = baseline.get(name, "PASS")
            elif baseline is None and observed["status"] != "PASS":
                row["chromium_baseline"] = "PASS"
            subtest = declared.get(name)
            if page["problems"]:
                row.update(status="untranslatable", reasons=sorted(page["problems"]))
            elif "parse_error" in record:
                row.update(status="untranslatable", reasons=["parse-error"])
            elif subtest is None:
                reasons = sorted({r for s in dynamic for r in s.get("untranslatable", [])}
                                 or {"undeclared-subtest"})
                row.update(status="untranslatable", reasons=reasons)
            elif subtest.get("untranslatable"):
                row.update(status="untranslatable", reasons=subtest["untranslatable"])
            else:
                row.update(classify(
                    sorted(set(subtest_surfaces(subtest, page, intrinsics)) | set(page["markup"])),
                    capabilities))
            result = div.get(relative, {}).get("subtests", {}).get(name)
            if row["status"] == "translatable" and result is not None:
                row["div"] = result["status"]
                if "message" in result:
                    row["div_message"] = result["message"]
            rows.append(row)
        if entry["harness"] != "OK":
            rows.append({"file": relative, "name": None, "chromium": entry["harness"],
                         "status": "harness", "message": entry.get("message")})
    counts: dict[str, int] = {}
    blocked_by: dict[str, int] = {}
    div_counts: dict[str, int] = {}
    for row in rows:
        if row["name"] is None:
            continue
        counts[row["status"]] = counts.get(row["status"], 0) + 1
        if "div" in row:
            div_counts[row["div"]] = div_counts.get(row["div"], 0) + 1
        for feature in row.get("features", []):
            blocked_by[feature] = blocked_by.get(feature, 0) + 1
        if row.get("unmapped"):
            blocked_by["unmapped"] = blocked_by.get("unmapped", 0) + 1
    return {
        "suite": suite,
        "chromium": recording["chromium"],
        "wpt_revision": recording["wpt_revision"],
        "subtests": sum(counts.values()),
        "counts": dict(sorted(counts.items())),
        "div": dict(sorted(div_counts.items())),
        "blocked_by": dict(sorted(blocked_by.items())),
        "rows": rows,
    }


def ledger_path(suite: str) -> Path:
    return LEDGERS / (suite.replace("/", "--") + ".json")


def render(ledger: dict) -> str:
    return json.dumps(ledger, indent=1, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--wpt-root", type=Path, default=os.environ.get("WPT_SCRIPT_ROOT"))
    parser.add_argument("--acorn", default=os.environ.get("CHROMIUM_ACORN"))
    parser.add_argument("--suite", action="append")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if not args.wpt_root or not args.acorn:
        raise SystemExit("set --wpt-root/WPT_SCRIPT_ROOT and --acorn/CHROMIUM_ACORN")
    if sha256(Path(args.acorn)) != ACORN_SHA256:
        raise SystemExit(f"{args.acorn} is not the pinned acorn@8.14.0")
    intrinsics = load_json(TOOL / "intrinsics.json")
    capabilities = load_json(TOOL / "capabilities.json")
    failed = False
    for suite in args.suite or SUITES:
        text = render(build(suite, args.wpt_root.resolve(), args.acorn, intrinsics, capabilities))
        path = ledger_path(suite)
        if args.check:
            if not path.is_file() or path.read_text(encoding="utf-8") != text:
                print(f"{path.relative_to(ROOT)} is stale", file=sys.stderr)
                failed = True
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8", newline="\n")
            print(path.relative_to(ROOT))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
