#!/usr/bin/env python3
"""Build RFC 0005's per-suite ledger of WPT testharness subtests.

For each suite the ledger has one row per upstream subtest, as pinned
Chromium 147 reports them (`capture_chromium_subtests.py`), with:

- `chromium`: Chromium's own result (PASS, FAIL, ...), from the recording,
  cross-checked against the pinned `-expected.txt` baselines;
- `status`: what Div can do with the subtest today:
  - `translatable`: every web surface it uses maps to a `divui` call, and
    the page can reach it;
  - `unreachable`: it would be translatable, but it sits behind a link of
    the page's promise chain that cannot run (`reason`:
    `blocked-chain:<link>`). The page stays unmodified, so the test does not
    run: its Div result is NOTRUN, and running must fail the check;
  - `blocked`: some surface maps to a feature Div lacks (`features`) or to
    nothing yet (`unmapped`);
  - `untranslatable`: the script needs something a straight-line Rust test
    cannot express (`reasons`);
  - `disputed`: the capture doesn't reproduce Chromium's own pinned record
    of this run (`reason`), so it can't stand for Chromium's result;
  - `excluded`: the file is not run (manual, reference, wptserve features).

A page with `<meta name=variant>` has one recorded run per variant, and its
rows name the run (`page.html?mouse`).

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
SUITES = ("html/webappapis/animation-frames", "resize-observer", "pointerevents",
          "css/css-transitions", "css/css-animations", "css/css-easing", "web-animations")
ACORN_SHA256 = "402f6759c744ea2ca501c1dc4ded8b1c54c9066cc7a741837057dcb68bc64c93"

# `<script>`, including XHTML/SVG forms such as `<h:script src="..."/>`.
SCRIPT = re.compile(
    r"<(?:[\w-]+:)?script\b([^>]*?)(?:/>|>(.*?)</(?:[\w-]+:)?script\s*>)", re.I | re.S)
SRC = re.compile(r"""\b(?:src|xlink:href|href)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", re.I)
TITLE = re.compile(r"<title\b[^>]*>(.*?)</title\s*>", re.I | re.S)
CDATA = re.compile(r"^\s*(?://)?\s*<!\[CDATA\[(.*?)(?://)?\s*\]\]>\s*$", re.S)
EXPECTED_LINE = re.compile(r"^\[(PASS|FAIL|TIMEOUT|NOTRUN|PRECONDITION_FAILED)\] (.*)$")
# The harness itself: no web surface, nothing to resolve.
HARNESS_SCRIPTS = {"/resources/testharness.js", "/resources/testharnessreport.js",
                   "/resources/testdriver-vendor.js"}


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


def helper_file(wpt_root: Path, path: str, intrinsic: dict) -> Path:
    """A helper's file. Chromium serves the test driver from outside the WPT
    root (`config.tmpl.json`), so its entry names the path in the checkout."""
    if "checkout" in intrinsic:
        return wpt_root.parents[4] / intrinsic["checkout"]
    return wpt_root / path


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
            elif not helper_file(wpt_root, path, intrinsic).is_file() or sha256(
                    helper_file(wpt_root, path, intrinsic)) != intrinsic["sha256"]:
                problems.append(f"changed-helper:{path}")
            else:
                helpers.append(path)
            continue
        cdata = CDATA.match(body)
        scripts.append({"source": cdata.group(1) if cdata else body, "offset": match.start(2)})
    return {"file": relative, "title": title, "scripts": scripts, "helpers": helpers,
            "problems": problems, "markup": markup_surfaces(text, relative),
            "handlers": markup_handlers(text)}


STYLE_ELEMENT = re.compile(r"<(?:[\w-]+:)?style\b[^>]*>.*?</(?:[\w-]+:)?style\s*>", re.I | re.S)
COMMENT = re.compile(r"<!--.*?-->", re.S)
TAG = re.compile(r"<([a-zA-Z][\w:.-]*)\b([^>]*)>")
STYLE_ATTRIBUTE = re.compile(r"""\sstyle\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", re.I)
HANDLER = re.compile(r"""\s(on[a-z]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))""", re.I)
REL_STYLESHEET = re.compile(r"""\brel\s*=\s*["']?stylesheet""", re.I)
CSS_COMMENT = re.compile(r"/\*.*?\*/", re.S)
CSS_BLOCK = re.compile(r"\{([^{}]*)\}")
# The at-rules that need an animation feature of their own.
AT_RULE = re.compile(r"@(keyframes|starting-style)\b", re.I)
# The property names `stylesheet!` accepts.
PROPERTY_TABLE = ROOT / "crates/div-style/src/generated_properties.rs"
PROPERTY_ARM = re.compile(r'^\s*"([a-z-]+)" => Some\(Self::\w+\),$', re.M)
# Document structure the generated test does not build: `TestApp` provides it.
STRUCTURE = {"html", "head", "body", "title", "meta", "link", "script", "style", "?xml"}


def name_problem(observed: list[str], oracle: list[str], interrupted: bool) -> str | None:
    """How a computed-name page's subtests differ from Chromium's record.

    The names must match exactly. A run the translation stopped may have
    declared fewer of them, never another.
    """
    extra = sorted(set(observed) - set(oracle))
    if extra:
        return f"subtests Chromium did not record: {extra}"
    if not interrupted and sorted(observed) != sorted(oracle):
        return f"subtests differ from Chromium's record: {sorted(observed)} != {sorted(oracle)}"
    return None


def markup_handlers(text: str) -> list[str]:
    """The source of each event handler attribute (`<body onload="run()">`)."""
    body = STYLE_ELEMENT.sub("", SCRIPT.sub("", COMMENT.sub("", text)))
    return [html.unescape(next(group for group in match.groups()[1:] if group is not None))
            for tag in TAG.finditer(body) for match in HANDLER.finditer(tag.group(2))]


def markup_surfaces(text: str, relative: str) -> list[str]:
    """What building the page's static markup needs (RFC 0005 "page setup").

    Each element tag, `<style>` sheets and the properties they declare
    (`css:<name>`: `stylesheet!` refuses a property Div lacks), `style`
    attributes and linked stylesheets. Every subtest on the page needs them.
    """
    surfaces = set()
    for sheet in STYLE_ELEMENT.finditer(text):
        surfaces.add("markup:<style>")
        rules = CSS_COMMENT.sub("", sheet.group(0))
        surfaces.update(f"markup:@{name.lower()}" for name in AT_RULE.findall(rules))
        for block in CSS_BLOCK.findall(CSS_COMMENT.sub("", sheet.group(0))):
            for declaration in block.split(";"):
                if ":" in declaration:
                    surfaces.add("css:" + declaration.split(":", 1)[0].strip().lower())
    body = STYLE_ELEMENT.sub("", SCRIPT.sub("", COMMENT.sub("", text)))
    for match in TAG.finditer(body):
        tag = match.group(1).split(":")[-1].lower()
        attributes = match.group(2)
        if tag == "link" and REL_STYLESHEET.search(attributes):
            href = SRC.search(attributes)
            if href:
                value = next(group for group in href.groups() if group is not None)
                surfaces.add(f"markup:stylesheet:{resolve(value, relative)}")
        surfaces.update(f"markup:{match.group(1).lower()}" for match in HANDLER.finditer(attributes))
        if tag in STRUCTURE:
            continue
        surfaces.add(f"markup:{tag}")
        style = STYLE_ATTRIBUTE.search(attributes)
        if style:
            surfaces.add("markup:style-attribute")
            value = next((group for group in style.groups() if group is not None), "")
            for declaration in html.unescape(value).split(";"):
                if ":" in declaration:
                    surfaces.add("css:" + declaration.split(":", 1)[0].strip().lower())
    return sorted(surfaces)


def front_end(pages: list[dict], intrinsics: dict, acorn: str) -> dict:
    payload = []
    for page in pages:
        names = sorted({name for helper in page["helpers"]
                        for name in intrinsics[helper]["provides"]})
        payload.append({"file": page["file"], "title": page["title"],
                        "scripts": page["scripts"], "handlers": page["handlers"],
                        "helpers": names})
    result = subprocess.run(
        ["node", str(TOOL / "translate.mjs"), acorn],
        input=json.dumps(payload), capture_output=True, text=True, encoding="utf-8", check=True)
    return {record["file"]: record for record in json.loads(result.stdout)}


def expand_intrinsics(intrinsics: dict, capabilities: dict, wpt_root: Path, acorn: str) -> None:
    """Fill in what each `analyse` helper's functions use, from its pinned source.

    Each function's surfaces follow its calls, across the analysed helpers
    too. `page` is what the helper's top-level code uses, which every subtest
    on a page loading it needs. `declares` lists the functions that declare
    subtests for the page.

    The runtime doesn't implement these helpers yet, so every page loading
    one is blocked on `T3:helper-intrinsic` (`helper:<path>`). Of a helper's
    surfaces, only those the capability table names are kept, to attribute
    the block to a feature: the rest are the helper's private fields.
    """
    paths = [path for path, entry in intrinsics.items() if entry.get("analyse")
             and (wpt_root / path).is_file() and sha256(wpt_root / path) == entry["sha256"]]
    if not paths:
        return
    payload = [{"file": path, "source": (wpt_root / path).read_text(encoding="utf-8")}
               for path in paths]
    result = subprocess.run(
        ["node", str(TOOL / "translate.mjs"), acorn, "--helpers"],
        input=json.dumps(payload), capture_output=True, text=True, encoding="utf-8", check=True)
    records = json.loads(result.stdout)
    functions: dict[str, set[str]] = {}
    declaring: set[str] = set()
    for record in records:
        reasons = set(record["page"]["reasons"])
        for name, found in record["functions"].items():
            reasons.update(found["reasons"])
            functions.setdefault(name, set()).update(found["surfaces"])
            if found["declares"]:
                declaring.add(name)
        if reasons:
            raise SystemExit(f"{record['file']} uses {sorted(reasons)}: review this helper")
    # A call into another helper stands for what that function uses.
    changed = True
    while changed:
        changed = False
        for surfaces in functions.values():
            for surface in list(surfaces):
                other = surface[:-2] if surface.endswith("()") else None
                if other in functions and not functions[other] <= surfaces:
                    surfaces |= functions[other]
                    changed = True
    named = set(capabilities)
    for record in records:
        path = record["file"]
        entry = intrinsics[path]
        page = set(record["page"]["surfaces"])
        for surface in list(page):
            other = surface[:-2] if surface.endswith("()") else None
            if other in functions:
                page |= functions[other]
        entry["page"] = sorted((page & named) | {f"helper:{path}"})
        entry["provides"] = {name: sorted(functions[name] & named) for name in record["functions"]}
        entry["declares"] = sorted(name for name in record["functions"] if name in declaring)
        entry["page_declares"] = record["page_declares"]
        capabilities[f"helper:{path}"] = {"feature": "T3:helper-intrinsic", "divui": None}
        for name in entry["provides"]:
            capabilities.setdefault(f"harness:{name}",
                                    {"feature": "T3:helper-intrinsic", "divui": None})


def helper_declared(record: dict, page: dict, intrinsics: dict) -> bool:
    """Whether a helper the page loads declares subtests when it loads, or
    the page calls one of its functions that does."""
    if any(intrinsics[helper].get("page_declares") for helper in page["helpers"]):
        return True
    names = {name for helper in page["helpers"]
             for name in intrinsics[helper].get("declares", [])}
    used = {surface.removeprefix("harness:") for surface in record["whole_page"]["surfaces"]
            if surface.startswith("harness:")}
    return bool(names & used)


def expected_baseline(wpt_root: Path, relative: str) -> dict[str, str] | None:
    """Chromium's `-expected.txt` baseline for a page, if it has one."""
    path = capture.baseline_path(wpt_root, relative)
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
        found.update(intrinsic.get("page", []))
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


def classify_computed(record: dict, dynamic: list[dict], page: dict, intrinsics: dict,
                      capabilities: dict) -> dict:
    """A subtest whose name the page computes while it runs.

    It can't be tied to one declaration, so it needs the whole page: every
    script surface and all the markup must map to `divui`, and nothing on the
    page may be outside the subset. A name no declaration on the page can
    produce stays `undeclared-subtest`; a helper function that declares
    subtests for the page counts as a declaration.
    """
    if not dynamic and not helper_declared(record, page, intrinsics):
        return {"status": "untranslatable", "reasons": ["undeclared-subtest"]}
    whole = record["whole_page"]
    reasons = set(whole["reasons"]) | {reason for subtest in dynamic
                                       for reason in subtest.get("untranslatable", [])
                                       if reason != "runtime-subtest-name"}
    if reasons:
        return {"status": "untranslatable", "reasons": sorted(reasons)}
    surfaces = subtest_surfaces({"surfaces": whole["surfaces"], "declared_by": "harness:page"},
                                page, intrinsics)
    return classify(sorted(set(surfaces) | set(page["markup"])), capabilities)


def chain_stops(record: dict, page: dict, intrinsics: dict, capabilities: dict) -> dict[str, str]:
    """The subtests behind a chain link that cannot run, and that link.

    A link that needs a surface Div lacks throws or never settles, so no
    later link runs, on the unmodified page. A subtest a later link declares
    or finishes is not reachable.
    """
    stops: dict[str, str] = {}
    for chain in record.get("chains", []):
        broken = None
        for link in chain:
            if broken is not None:
                for name in link["subtests"] + link["finishes"]:
                    stops.setdefault(name, broken)
                continue
            surfaces = subtest_surfaces({"surfaces": link["surfaces"],
                                         "declared_by": "harness:chain"}, page, intrinsics)
            if link["reasons"] or classify(surfaces, capabilities)["status"] != "translatable":
                broken = link["label"]
    return stops


def div_css_properties(text: str) -> set[str]:
    """The names in `StyleProperty::from_css_name`, from its source."""
    start = text.index("pub fn from_css_name")
    return set(PROPERTY_ARM.findall(text[start:text.index("_ => None", start)]))


def check_css_capabilities(capabilities: dict, known: set[str]) -> None:
    """Each `css:` surface is mapped exactly when Div has the property, so a
    shorthand that lands fails the check until its row is updated."""
    stale = sorted(surface for surface, entry in capabilities.items()
                   if surface.startswith("css:")
                   and (entry.get("divui") is not None) != (surface[4:] in known))
    if stale:
        raise SystemExit("capabilities.json disagrees with StyleProperty::from_css_name on: "
                         + ", ".join(stale))


def check_reachability(relative: str, name: str, status: str, recorded: str) -> None:
    """The classifier and the recorded run must agree on what the page reaches."""
    if (status == "unreachable") != (recorded == "NOTRUN"):
        raise SystemExit(f"{relative}: {name!r} is {status} but its recorded Div result is "
                         f"{recorded}: update the chain analysis or re-record")


def check_stopped(relative: str, name: str, status: str, stopped: str | None) -> None:
    """A row behind a stopped chain needs the run's reason for the stop."""
    if status == "unreachable" and not stopped:
        raise SystemExit(f"{relative}: {name!r} is unreachable but the recorded run names no "
                         "reason its chain stopped: re-record")


def build(suite: str, wpt_root: Path, acorn: str, intrinsics: dict, capabilities: dict) -> dict:
    recording = load_json(CHROMIUM / (suite.replace("/", "--") + ".json"))
    files = recording["files"]
    # The recording must cover exactly the suite's pages at the pinned
    # revision; a page it lacks would drop out of the denominator.
    on_disk = {relative for relative in capture.suite_files(wpt_root, suite)
               if capture.exclusion(relative) != "helper"}
    recorded = {key.split("?", 1)[0] for key in files}
    if on_disk != recorded:
        missing = sorted(on_disk - recorded)[:5]
        extra = sorted(recorded - on_disk)[:5]
        raise SystemExit(f"{suite}: the Chromium recording is stale "
                         f"(not recorded: {missing}; not on disk: {extra})")
    pages = {path: read_page(wpt_root, path, intrinsics)
             for path in sorted({key.split("?", 1)[0] for key, entry in files.items()
                                 if "excluded" not in entry})}
    analysed = front_end(list(pages.values()), intrinsics, acorn) if pages else {}
    expected_path = EXPECTED / (suite.replace("/", "--") + ".json")
    div = load_json(expected_path) if expected_path.is_file() else {}
    rows = []
    for relative, entry in files.items():
        if "excluded" in entry:
            rows.append({"file": relative, "name": None, "chromium": None,
                         "status": "excluded", "reason": entry["excluded"]})
            continue
        path = relative.split("?", 1)[0]
        page, record = pages[path], analysed[path]
        oracle = entry.get("oracle", {"status": "agrees"})
        declared = {}
        for subtest in record["subtests"]:
            if subtest["name_known"]:
                declared.setdefault(subtest["name"], subtest)
        dynamic = [s for s in record["subtests"] if not s["name_known"]]
        baseline = expected_baseline(wpt_root, relative)
        stops = chain_stops(record, page, intrinsics, capabilities)
        unrecorded = set(div.get(relative, {}).get("subtests", {})) - {
            observed["name"] for observed in entry["subtests"]}
        if unrecorded:
            raise SystemExit(f"{relative}: Div recorded subtests Chromium did not: "
                             f"{sorted(unrecorded)}")
        for observed in entry["subtests"]:
            name = observed["name"]
            row = {"file": relative, "name": name, "chromium": observed["status"]}
            if baseline is not None and baseline.get(name, "PASS") != observed["status"]:
                row["chromium_baseline"] = baseline.get(name, "PASS")
            elif baseline is None and observed["status"] != "PASS":
                row["chromium_baseline"] = "PASS"
            subtest = declared.get(name)
            if oracle["status"] == "disputed":
                row.update(status="disputed", reason=oracle["reason"])
            elif page["problems"]:
                row.update(status="untranslatable", reasons=sorted(page["problems"]))
            elif "parse_error" in record:
                row.update(status="untranslatable", reasons=["parse-error"])
            elif subtest is None:
                row.update(classify_computed(record, dynamic, page, intrinsics, capabilities))
                if dynamic or helper_declared(record, page, intrinsics):
                    row["computed_name"] = True
            elif subtest.get("untranslatable"):
                row.update(status="untranslatable", reasons=subtest["untranslatable"])
            else:
                row.update(classify(
                    sorted(set(subtest_surfaces(subtest, page, intrinsics)) | set(page["markup"])),
                    capabilities))
            stop = stops.get(name)
            if stop is not None and row["status"] == "translatable":
                row.update(status="unreachable", reason=f"blocked-chain:{stop}")
            elif stop is not None and row["status"] == "blocked":
                row["blocked_chain"] = stop
            result = div.get(relative, {}).get("subtests", {}).get(name)
            if row["status"] in ("translatable", "unreachable") and result is not None:
                check_reachability(relative, name, row["status"], result["status"])
                check_stopped(relative, name, row["status"], div[relative].get("stopped"))
                row["div"] = result["status"]
                if "message" in result:
                    row["div_message"] = result["message"]
                # Chromium's harness finished this run; Div's reported an error.
                if div[relative].get("harness_error") and entry["harness"] == "OK":
                    row["div_harness_error"] = div[relative]["harness_error"]
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
    check_css_capabilities(capabilities,
                           div_css_properties(PROPERTY_TABLE.read_text(encoding="utf-8")))
    expand_intrinsics(intrinsics, capabilities, args.wpt_root.resolve(), args.acorn)
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
