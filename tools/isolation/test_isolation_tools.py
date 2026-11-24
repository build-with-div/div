"""Filter proof and behavior tests for the isolation tools.

Everything here runs on a synthetic workspace. Its withheld files carry CANARY
markers in comments, strings, private names and notes.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import socket
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.isolation.build_broker import (
    Broker,
    FilterContext,
    classify_path,
    collect_strings,
    contains_guarded_line,
    filter_diagnostic,
    handle_request,
    reads_files_at_compile_time,
    serve,
    significant_lines,
)
from tools.isolation.export_tree import export_tree
from tools.isolation.interface_manifest import build_manifest, flatten_use, strip_rust_noise
from tools.isolation.sandbox_runner import parse_list
from tools.isolation.withheld import IsolationError, load_list, refuse_unsafe_location

SECRET = """//! CANARY_DOC_COMMENT describes this module.
// CANARY_LINE_COMMENT describes the private algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Fast,
    Slow,
    CanaryHiddenVariant,
}

pub struct Widget {
    pub size: u32,
    canary_private_field: u32,
}

#[deprecated(note = "CANARY_DEPRECATION_NOTE see `canary_note_identifier` instead")]
pub fn old_api() -> u32 {
    canary_private_helper(1)
}

pub fn make_widget(size: u32) -> Widget {
    let _canary_text = "CANARY_STRING_LITERAL";
    Widget { size, canary_private_field: canary_private_helper(size) }
}

fn canary_private_helper(value: u32) -> u32 {
    value + 41
}

impl Widget {
    pub fn total(&self) -> u32 {
        self.size + self.canary_private_field
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn passes_in_withheld() {
        assert_eq!(super::canary_private_helper(1), 42);
    }

    #[test]
    fn fails_with_source_text() {
        panic!("CANARY_PANIC_TEXT {}", include_str!("secret.rs"));
    }

    #[test]
    fn writes_source_past_capture() {
        std::io::stdout().write_all(include_bytes!("secret.rs")).unwrap();
    }
}
"""

OTHER_SECRET = """// CANARY_SECOND_FILE_COMMENT explains this helper.
use crate::secret::Mode;

pub fn is_slow(mode: Mode) -> bool {
    matches!(mode, Mode::Slow)
}
"""

CONSUMER = """use owner::other_secret::*;
use owner::secret::{make_widget, Mode};
use owner::Widget;

pub fn consume() -> u32 {
    let widget: Widget = make_widget(3);
    let _ = is_slow(Mode::Slow);
    widget.total() + owner::open_part::open_helper()
}

#[allow(dead_code)]
fn consume_deprecated() -> u32 {
    owner::secret::old_api()
}

#[cfg(test)]
mod tests {
    #[test]
    fn consumer_passes() {
        assert_eq!(super::consume(), 54);
    }
}
"""

FILES = {
    "Cargo.toml": '[workspace]\nresolver = "2"\nmembers = ["crates/owner", "crates/consumer"]\n',
    "Cargo.lock": (
        "version = 3\n\n[[package]]\nname = \"consumer\"\nversion = \"0.1.0\"\n"
        "dependencies = [\n \"owner\",\n]\n\n[[package]]\nname = \"owner\"\nversion = \"0.1.0\"\n"
    ),
    "AGENTS.md": "CANARY_AGENT_INSTRUCTIONS\n",
    "docs/notes.md": "CANARY_DOCS_TEXT\n",
    "crates/owner/Cargo.toml": '[package]\nname = "owner"\nversion = "0.1.0"\nedition = "2021"\n',
    "crates/owner/src/lib.rs": "pub mod open_part;\npub mod other_secret;\npub mod secret;\npub use secret::Widget;\n",
    "crates/owner/src/open_part.rs": "pub fn open_helper() -> u32 {\n    7\n}\n",
    "crates/owner/src/secret.rs": SECRET,
    "crates/owner/src/other_secret.rs": OTHER_SECRET,
    "crates/owner/extra/secret_copy.rs": SECRET,
    "crates/consumer/Cargo.toml": (
        '[package]\nname = "consumer"\nversion = "0.1.0"\nedition = "2021"\n\n'
        '[dependencies]\nowner = { path = "../owner" }\n'
    ),
    "crates/consumer/src/lib.rs": CONSUMER,
}
LISTED = ("crates/owner/src/secret.rs", "crates/owner/src/other_secret.rs")
SECRET_PATH = LISTED[0]

CANDIDATE_MISSING_ITEMS = """#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode { Fast, Slow }
pub struct Widget { pub size: u32 }
impl Widget { pub fn total(&self) -> u32 { self.size } }
"""
CANDIDATE_BREAKS_SIBLING = """#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode { Fast }
pub struct Widget { pub size: u32 }
"""
CANDIDATE_TRIES_TO_ECHO = """compile_error!(include_str!("../extra/secret_copy.rs"));
include!("../extra/secret_copy.rs");
include!("../extra/secret_copy.rs");
"""
CONTAINED_PATH = "crates/owner/src/contained.rs"
CONTAINED_NAMES = (
    "reads_outside_marker",
    "reads_withheld_sibling",
    "writes_source_tree",
    "writes_inbox",
    "writes_result_file",
    "connects_to_host",
    "sees_no_host_directories",
    "sees_no_build_metadata",
    "forges_results",
    "allocates_past_limit",
    "runs_after_limit",
    "removed_by_candidate",
)
CONTAINED_BASE = (
    "pub fn contained_value() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n"
    + "".join(f"    #[test]\n    fn {name}() {{}}\n\n" for name in CONTAINED_NAMES)
    + "}\n"
)
ATTACK_CANDIDATE = """pub fn contained_value() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{SocketAddr, TcpStream};
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn reads_outside_marker() {
        assert!(std::fs::read("@MARKER@").is_err());
    }

    #[test]
    fn reads_withheld_sibling() {
        assert!(std::fs::read("@TREE@/crates/owner/src/other_secret.rs").is_err());
    }

    #[test]
    fn writes_source_tree() {
        assert!(std::fs::write("@TREE@/crates/owner/src/open_part.rs", "CANARY_WRITE").is_err());
    }

    #[test]
    fn writes_inbox() {
        assert!(std::fs::write("@INBOX@/forged.result.json", "CANARY_INBOX").is_err());
    }

    #[test]
    fn writes_result_file() {
        // The broker's files are not in this view, so a write cannot reach them.
        assert!(!Path::new("@WORK@/sandbox-plan.json").exists());
        let _ = std::fs::write("@WORK@/sandbox-plan.json", "CANARY_RESULT");
        let _ = std::fs::write("@WORK@/sandbox-result.json", "CANARY_RESULT");
    }

    #[test]
    fn connects_to_host() {
        let address: SocketAddr = "127.0.0.1:@PORT@".parse().unwrap();
        assert!(TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn sees_no_host_directories() {
        assert!(!Path::new("/mnt").exists());
        assert!(!Path::new("/home").exists());
        assert!(!Path::new("/init").exists());
    }

    #[test]
    fn sees_no_build_metadata() {
        let exe = std::env::current_exe().unwrap();
        let count = std::fs::read_dir(exe.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().is_some_and(|e| e == "rlib" || e == "rmeta" || e == "d")
            })
            .count();
        assert_eq!(count, 0);
    }

    #[test]
    fn forges_results() {
        let mut out = std::io::stdout();
        out.write_all(b"test forged::canary_forged ... ok\\n{\\"status\\":\\"canary\\"}\\n").unwrap();
        out.flush().unwrap();
    }

    #[test]
    fn allocates_past_limit() {
        let mut block = vec![1u8; 3 << 30];
        for page in block.chunks_mut(4096) {
            page[0] = 2;
        }
        assert_eq!(std::hint::black_box(&block)[0], 2);
    }

    #[test]
    fn runs_after_limit() {}

    #[test]
    fn canary_new_test_name() {}
}
"""
COMPLETE_CANDIDATE = (
    SECRET.replace('include_str!("secret.rs")', '"clean"')
    .replace('include_bytes!("secret.rs")', 'b"clean"')
    .replace("CANARY", "clean")
    .replace("canary", "clean")
    .replace("Canary", "Clean")
)


def _bare_or_trivial(line: str) -> bool:
    return not re.search(r"[A-Za-z]", line) or bool(re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*[,;]?", line))


def leaked_lines(result: object, *sources: str) -> list[str]:
    """Return every non-trivial withheld source line found in a result."""
    text = "\n".join(collect_strings(result))
    leaked = []
    for source in sources:
        for raw in source.splitlines():
            line = raw.strip()
            if line and not _bare_or_trivial(line) and line in text:
                leaked.append(line)
    return leaked


class WorkspaceCase(unittest.TestCase):
    fixture = FILES
    listed = LISTED

    def setUp(self) -> None:
        self.temp = Path(tempfile.mkdtemp(prefix="div-isolation-")).resolve()
        self.addCleanup(shutil.rmtree, self.temp, ignore_errors=True)
        patcher = mock.patch.dict(os.environ)
        patcher.start()
        self.addCleanup(patcher.stop)
        for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "CARGO_BUILD_TARGET"):
            os.environ.pop(name, None)
        self.repo = self.temp / "repo"
        self.write_repo(self.fixture)
        self.list_path = self.temp / "withheld.txt"
        self.list_path.write_text("# synthetic\n" + "\n".join(self.listed) + "\n", encoding="utf-8")

    def write_repo(self, files: dict[str, str]) -> None:
        for name, content in files.items():
            target = self.repo / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content.encode("utf-8"))
        base = ["git", "-C", str(self.repo), "-c", "core.autocrlf=false"]
        if not (self.repo / ".git").exists():
            subprocess.run(["git", "init", "-q", "-b", "main", str(self.repo)], check=True)
        subprocess.run([*base, "add", "-A"], check=True)
        subprocess.run(
            [*base, "-c", "user.name=test", "-c", "user.email=test@example.invalid", "commit", "-q", "-m", "fixture"],
            check=True,
        )

    def broker(self, **options) -> Broker:
        options.setdefault("min_free_gb", 0)
        return Broker(self.repo, "main", self.list_path, self.temp / "broker", **options)

    def candidate(self, text: str, name: str = "candidate.rs") -> Path:
        path = self.temp / name
        path.write_text(text, encoding="utf-8")
        return path

    def assert_no_withheld_text(self, result: object) -> None:
        self.assertEqual(leaked_lines(result, SECRET, OTHER_SECRET), [])
        self.assertNotIn("canary", "\n".join(collect_strings(result)).casefold())


class ExportTests(WorkspaceCase):
    def test_export_omits_listed_files_copies_docs_agent_files_and_git(self) -> None:
        dest = self.temp / "export"
        receipt = export_tree(self.repo, "main", self.list_path, dest)
        exported = {p.relative_to(dest).as_posix() for p in dest.rglob("*") if p.is_file()}
        self.assertIn("crates/consumer/src/lib.rs", exported)
        self.assertIn("crates/owner/src/open_part.rs", exported)
        for path in (*LISTED, "crates/owner/extra/secret_copy.rs", "docs/notes.md", "AGENTS.md"):
            self.assertNotIn(path, exported)
        self.assertFalse((dest / ".git").exists())
        for path in exported:
            self.assertNotIn("CANARY", (dest / path).read_text(encoding="utf-8"))
        self.assertEqual(receipt["exported_files"], len(exported))

    def test_receipt_contains_no_listed_path(self) -> None:
        dest = self.temp / "export"
        export_tree(self.repo, "main", self.list_path, dest)
        receipt = (self.temp / "export.receipt.json").read_text(encoding="utf-8")
        for path in LISTED:
            self.assertNotIn(path, receipt)

    def test_refuses_destination_inside_repository(self) -> None:
        with self.assertRaises(IsolationError) as caught:
            export_tree(self.repo, "main", self.list_path, self.repo / "out" / "export")
        self.assertEqual(caught.exception.code, "location_overlaps_repository")

    def test_refuses_destination_under_project_marker(self) -> None:
        project = self.temp / "project"
        project.mkdir()
        (project / "AGENTS.md").write_text("marker", encoding="utf-8")
        with self.assertRaises(IsolationError) as caught:
            refuse_unsafe_location(project / "export", self.repo)
        self.assertEqual(caught.exception.code, "location_has_project_marker")

    def test_refuses_unknown_listed_path(self) -> None:
        self.list_path.write_text("crates/owner/src/missing.rs\n", encoding="utf-8")
        with self.assertRaises(IsolationError) as caught:
            export_tree(self.repo, "main", self.list_path, self.temp / "export")
        self.assertEqual(caught.exception.code, "listed_path_not_in_tree")

    def test_locked_test_files_are_withheld(self) -> None:
        locked = "crates/consumer/tests/locked.rs"
        self.write_repo({locked: "#[test]\nfn canary_locked() {}\n"})
        with self.list_path.open("a", encoding="utf-8") as handle:
            handle.write(f"locked: {locked}\n")
        dest = self.temp / "export"
        export_tree(self.repo, "main", self.list_path, dest)
        self.assertFalse((dest / locked).exists())

    def test_refuses_list_inside_repository(self) -> None:
        inside = self.repo / "withheld.txt"
        inside.write_text(LISTED[0], encoding="utf-8")
        with self.assertRaises(IsolationError) as caught:
            load_list(inside, self.repo)
        self.assertEqual(caught.exception.code, "list_inside_repository")


class ManifestTests(WorkspaceCase):
    def test_names_come_from_consumers_and_carry_the_caveat(self) -> None:
        manifest = build_manifest(self.repo, "main", self.list_path)
        by_name = {item["name"]: item for item in manifest["names"]}
        self.assertEqual(by_name["make_widget"]["status"], "defined")
        self.assertEqual(by_name["make_widget"]["defined_in"], [SECRET_PATH])
        self.assertIn("Slow", by_name["Mode"]["members"])
        self.assertTrue(by_name["is_slow"]["via_glob"])
        self.assertIn("not a complete interface", manifest["caveat"])
        self.assert_no_withheld_text(manifest)

    def test_private_and_unused_names_are_absent(self) -> None:
        manifest = build_manifest(self.repo, "main", self.list_path)
        text = json.dumps(manifest)
        for name in ("canary_private_helper", "canary_private_field", "CanaryHiddenVariant", "Fast"):
            self.assertNotIn(name, text)

    def test_locked_files_are_neither_definitions_nor_consumers(self) -> None:
        locked = "crates/owner/tests/locked.rs"
        self.write_repo({locked: "use owner::secret::canary_only_in_test;\npub fn canary_locked_item() {}\n"})
        with self.list_path.open("a", encoding="utf-8") as handle:
            handle.write(f"locked: {locked}\n")
        self.assert_no_withheld_text(build_manifest(self.repo, "main", self.list_path))

    def test_flatten_use_handles_groups_aliases_and_globs(self) -> None:
        flat = flatten_use("a::{b, c::{d as e, self}, f::*}")
        self.assertIn((["a", "b"], None, False), flat)
        self.assertIn((["a", "c", "d"], "e", False), flat)
        self.assertIn((["a", "c"], None, False), flat)
        self.assertIn((["a", "f"], None, True), flat)

    def test_noise_stripping_removes_comments_and_literals(self) -> None:
        text = 'a /* x /* y */ z */ b // c\nlet s = "d"; let r = r#"e"#; let l: &\'static str = "f";'
        stripped = strip_rust_noise(text)
        for hidden in ("x", "y", "z", "c", "d", "e", "f"):
            self.assertNotRegex(stripped, rf"\b{hidden}\b")
        self.assertIn("'static", stripped)


class DiagnosticFilterTests(unittest.TestCase):
    context = FilterContext(
        root="/work/tree",
        visible={"crates/b/src/use.rs": "crates/b/src/use.rs"},
        candidates={"crates/a/src/new.rs": "crates/a/src/new.rs"},
        vocabulary=frozenset({"Visible", "visible_name"}),
    )

    @staticmethod
    def span(file_name: str = "crates/b/src/use.rs", **extra) -> dict:
        return {
            "file_name": file_name,
            "is_primary": True,
            "line_start": 4,
            "column_start": 2,
            "text": [],
            "label": None,
            "suggested_replacement": None,
            "expansion": None,
            **extra,
        }

    def message(self, **extra) -> dict:
        return {
            "level": "error",
            "message": "failed on `Visible`",
            "code": {"code": "E0308", "explanation": None},
            "spans": [self.span()],
            "children": [],
            "rendered": None,
            **extra,
        }

    def test_every_compiler_text_channel_is_dropped(self) -> None:
        marker = "CANARY pub fn hidden() -> u32 { 41 }"
        nested = self.span("crates/a/src/secret.rs", text=[{"text": marker}], label=marker)
        channels = {
            "message": {"message": f"failed: {marker}"},
            "quoted message": {"message": f"failed on `{marker}`"},
            "rendered": {"rendered": marker},
            "code explanation": {"code": {"code": "E0308", "explanation": marker}},
            "malformed code": {"code": {"code": marker}},
            "span text": {"spans": [self.span(text=[{"text": marker, "highlight_start": 1}])]},
            "span label": {"spans": [self.span(label=marker)]},
            "suggestion": {"spans": [self.span(suggested_replacement=marker)]},
            "file name": {"spans": [self.span(marker)]},
            "macro name": {"spans": [self.span(expansion={"macro_decl_name": marker, "span": nested})]},
            "expansion span": {"spans": [self.span(expansion={"span": nested, "def_site_span": None})]},
            "definition span": {"spans": [self.span(expansion={"span": None, "def_site_span": nested})]},
            "child message": {"children": [{"level": "note", "message": marker, "spans": [], "children": []}]},
            "child rendered": {"children": [{"level": "help", "message": "x", "rendered": marker, "spans": []}]},
            "child span": {"children": [{"level": "note", "message": "x", "spans": [nested], "children": []}]},
        }
        for name, change in channels.items():
            with self.subTest(channel=name):
                result = filter_diagnostic(self.message(**change), self.context)
                text = "\n".join(collect_strings(result))
                self.assertNotIn("CANARY", text)
                self.assertNotIn("hidden", text)
                self.assertNotIn("41", text)

    def test_result_has_only_allowlisted_fields(self) -> None:
        result = filter_diagnostic(self.message(), self.context)
        self.assertEqual(
            result,
            {
                "level": "error",
                "code": "E0308",
                "identifiers": ["Visible"],
                "withheld_identifiers": 0,
                "path": "crates/b/src/use.rs",
                "line": 4,
                "column": 2,
                "in_withheld_file": False,
            },
        )

    def test_identifiers_not_already_visible_are_withheld(self) -> None:
        message = self.message(message="use `visible_name` and `private_helper`")
        result = filter_diagnostic(message, self.context)
        self.assertEqual(result["identifiers"], ["visible_name"])
        self.assertEqual(result["withheld_identifiers"], 1)

    def test_span_only_in_withheld_file_has_no_location(self) -> None:
        message = self.message(spans=[self.span("crates/a/src/secret.rs")])
        result = filter_diagnostic(message, self.context)
        self.assertIsNone(result["path"])
        self.assertIsNone(result["line"])
        self.assertTrue(result["in_withheld_file"])

    def test_withheld_path_spellings_are_all_recognized(self) -> None:
        for raw in (
            "crates/a/src/secret.rs",
            "crates\\a\\src\\secret.rs",
            "./crates/a/src/secret.rs",
            "crates/b/../a/src/secret.rs",
            "CRATES/A/SRC/SECRET.RS",
            "crates/a/src/untracked_generated.rs",
            "/work/tree/crates/a/src/secret.rs",
            "/WORK/tree/crates/a/src/secret.rs",
            "../outside.rs",
            "",
            None,
        ):
            with self.subTest(path=raw):
                self.assertEqual(classify_path(raw, self.context), ("withheld", None))

    def test_windows_verbatim_paths_are_recognized(self) -> None:
        context = FilterContext("c:/work/tree", {"crates/b/use.rs": "crates/b/use.rs"}, {}, frozenset())
        for prefix in ("C:\\work\\tree\\", "\\\\?\\C:\\work\\tree\\"):
            with self.subTest(prefix=prefix):
                self.assertEqual(classify_path(prefix + "crates\\a\\secret.rs", context), ("withheld", None))
                self.assertEqual(classify_path(prefix + "crates\\b\\use.rs", context), ("visible", "crates/b/use.rs"))

    def test_candidate_and_external_paths(self) -> None:
        self.assertEqual(classify_path("crates/a/src/new.rs", self.context), ("candidate", "crates/a/src/new.rs"))
        self.assertEqual(classify_path("/rustc/abc/library/core/src/x.rs", self.context), ("external", None))
        self.assertEqual(classify_path("<std macros>", self.context), ("external", None))


class LeakDetectorControlTests(unittest.TestCase):
    """The proof must be able to fail."""

    def test_detector_flags_a_passthrough_of_rendered_output(self) -> None:
        line = "pub fn make_widget(size: u32) -> Widget {"
        leaky_result = {"diagnostics": [{"rendered": f"error\n  --> secret.rs:20:1\n20 | {line}\n"}]}
        self.assertIn(line, leaked_lines(leaky_result, SECRET))

    def test_runtime_guard_flags_a_passthrough(self) -> None:
        guarded = tuple(significant_lines(SECRET))
        self.assertTrue(contains_guarded_line({"x": ["fn y", "    value + 41"]}, guarded + ("value + 41",)))
        self.assertTrue(contains_guarded_line({"x": "pub fn make_widget(size: u32) -> Widget {"}, guarded))
        self.assertFalse(contains_guarded_line({"code": "E0308", "identifiers": ["Widget", "Mode"]}, guarded))


class ListParserTests(unittest.TestCase):
    def test_only_exact_list_lines_are_accepted(self) -> None:
        output = "tests::good: test\nbench_one: bench\npub fn leaked(): test\nCANARY text\n2 tests, 0 benchmarks\n"
        self.assertEqual(parse_list(output), ["tests::good"])


class BrokerGuardTests(WorkspaceCase):
    def test_final_guard_replaces_a_leaking_result(self) -> None:
        broker = self.broker()
        leaked = {"command": "check", "status": "ok", "extra": "pub fn make_widget(size: u32) -> Widget {"}
        result = broker._finish(leaked)
        self.assertEqual(result["status"], "filter_abort")
        self.assertNotIn("extra", result)
        log = (self.temp / "broker" / "broker-log.jsonl").read_text(encoding="utf-8")
        self.assertNotIn("make_widget", log)

    def test_candidate_cannot_overwrite_a_visible_file(self) -> None:
        source = self.candidate("pub fn x() {}\n")
        result = self.broker().run("check", {"crates/consumer/src/lib.rs": source})
        self.assertEqual((result["status"], result["reason"]), ("rejected", "candidate_overwrites_visible_file"))

    def test_candidate_cannot_replace_a_copy_of_a_listed_file(self) -> None:
        source = self.candidate("pub fn x() {}\n")
        result = self.broker().run("check", {"crates/owner/extra/secret_copy.rs": source})
        self.assertEqual((result["status"], result["reason"]), ("rejected", "candidate_overwrites_visible_file"))

    def test_new_candidate_paths_are_limited(self) -> None:
        source = self.candidate("fn main() {}\n")
        for path in ("crates/owner/build.rs", "docs/new.rs", "crates/owner/src/new.txt", "../escape.rs"):
            with self.subTest(path=path):
                self.assertEqual(self.broker().run("check", {path: source})["status"], "rejected")

    def test_locked_test_file_cannot_be_replaced(self) -> None:
        locked = "crates/consumer/tests/locked.rs"
        self.write_repo({locked: "#[test]\nfn fixed() {}\n"})
        with self.list_path.open("a", encoding="utf-8") as handle:
            handle.write(f"locked: {locked}\n")
        result = self.broker().run("check", {locked: self.candidate("#[test]\nfn replaced() {}\n")})
        self.assertEqual((result["status"], result["reason"]), ("rejected", "candidate_path_locked"))

    def test_candidate_cannot_pull_in_other_files(self) -> None:
        for text in (
            'const A: &str = include_str!("x.rs");',
            'include!("x.rs");',
            'const A: &[u8] = core::include_bytes!("x");',
            "use std::include_str as read;",
            '#[path = "x.rs"] mod y;',
            '#[cfg_attr(all(), path = "x.rs")] mod y;',
            'div_macros::stylesheet!("x");',
            "use div_macros::stylesheet as sheet;",
        ):
            with self.subTest(text=text):
                result = self.broker().run("check", {SECRET_PATH: self.candidate(text)})
                self.assertEqual(result["reason"], "candidate_reads_files_at_compile_time")

    def test_ordinary_text_is_not_mistaken_for_a_file_read(self) -> None:
        for text in (
            '// include_str!("x")',
            'const A: &str = "include!";',
            "fn f(include: bool, path: u8) {}",
            'fn g() { let path = "x"; }',
            "use crate::stylesheet::Sheet;",
        ):
            with self.subTest(text=text):
                self.assertFalse(reads_files_at_compile_time(text.encode("utf-8")))

    def test_test_command_fails_closed_without_the_sandbox(self) -> None:
        broker = self.broker()
        broker.platform = "win32"
        self.assertEqual(broker.run("test")["status"], "blocked_sandbox_unavailable")

    def test_work_dir_on_a_windows_mount_is_refused(self) -> None:
        broker = self.broker()
        broker.platform = "linux"
        broker.work_dir = Path("/mnt/e/work")
        with mock.patch("shutil.which", return_value="/usr/bin/bwrap"):
            self.assertEqual(broker._sandbox_block(), "blocked_work_dir_on_windows_mount")

    def test_census_lock_blocks_windows_builds(self) -> None:
        lock = self.temp / "census.lock"
        lock.write_text("{}", encoding="utf-8")
        broker = self.broker(census_lock=lock)
        broker.platform = "win32"
        self.assertEqual(broker.run("check")["status"], "blocked_by_census_lock")

    def test_low_disk_blocks_builds(self) -> None:
        self.assertEqual(self.broker(min_free_gb=10**12).run("check")["status"], "blocked_by_low_disk")

    def test_skia_builds_require_the_ninja_wrapper(self) -> None:
        self.write_repo({"Cargo.lock": FILES["Cargo.lock"] + '\n[[package]]\nname = "skia-bindings"\nversion = "0"\n'})
        os.environ.pop("SKIA_NINJA_COMMAND", None)
        self.assertEqual(self.broker().run("check")["status"], "blocked_missing_skia_ninja_wrapper")

    def test_job_limit_is_enforced(self) -> None:
        with self.assertRaises(IsolationError):
            self.broker(jobs=5)

    def test_inbox_rejects_path_traversal_with_a_fixed_reason(self) -> None:
        inbox = self.temp / "inbox"
        inbox.mkdir()
        request = inbox / "r1.request.json"
        request.write_text(
            json.dumps({"command": "check", "candidates": {SECRET_PATH: "../repo/crates/owner/src/secret.rs"}}),
            encoding="utf-8",
        )
        result = handle_request(self.broker(), inbox, request)
        self.assertEqual((result["status"], result["reason"]), ("rejected", "invalid_candidate_file"))
        self.assert_no_withheld_text(result)


class RealCargoFilterTests(WorkspaceCase):
    """Run the real compiler and test harness on the synthetic workspace."""

    @classmethod
    def setUpClass(cls) -> None:
        if shutil.which("cargo") is None:
            if os.environ.get("CI"):
                raise AssertionError("the filter proof requires cargo in CI")
            raise unittest.SkipTest("cargo is not installed")

    def test_consumer_errors_return_code_identifier_and_path_only(self) -> None:
        result = self.broker().run("check", {SECRET_PATH: self.candidate(CANDIDATE_MISSING_ITEMS)})
        self.assertEqual(result["status"], "build_failed")
        errors = [d for d in result["diagnostics"] if d["level"] == "error"]
        self.assertTrue(any(d["path"] == "crates/consumer/src/lib.rs" and d["code"] == "E0432" for d in errors))
        self.assertTrue(any("make_widget" in d["identifiers"] for d in errors))
        self.assert_no_withheld_text(result)

    def test_error_in_a_sibling_withheld_file_hides_its_location(self) -> None:
        result = self.broker().run("check", {SECRET_PATH: self.candidate(CANDIDATE_BREAKS_SIBLING)})
        self.assertEqual(result["status"], "build_failed")
        self.assertEqual(result["failed_packages"], ["owner"])
        self.assertEqual(result["packages_not_reached"], ["consumer"])
        hidden = [d for d in result["diagnostics"] if d["in_withheld_file"]]
        self.assertTrue(hidden)
        self.assertTrue(all(d["path"] is None for d in hidden))
        self.assert_no_withheld_text(result)

    def test_candidate_that_echoes_a_withheld_copy_leaks_nothing(self) -> None:
        broker = self.broker()
        broker._scan_candidates = False
        result = broker.run("check", {SECRET_PATH: self.candidate(CANDIDATE_TRIES_TO_ECHO)})
        self.assertEqual(result["status"], "build_failed")
        self.assertTrue(result["diagnostics"])
        self.assert_no_withheld_text(result)

    def test_deprecation_note_from_a_withheld_file_is_dropped(self) -> None:
        result = self.broker().run("check")
        self.assertEqual(result["status"], "ok")
        deprecated = [d for d in result["diagnostics"] if d["code"] == "deprecated"]
        self.assertTrue(deprecated)
        self.assertTrue(all(d["withheld_identifiers"] >= 1 for d in deprecated))
        self.assert_no_withheld_text(result)

    def test_passing_candidate_and_overlay_restore(self) -> None:
        broker = self.broker(jobs=1)
        good = broker.run("check", {SECRET_PATH: self.candidate(COMPLETE_CANDIDATE)})
        self.assertEqual(good["status"], "ok")
        bad = broker.run("check", {SECRET_PATH: self.candidate(CANDIDATE_MISSING_ITEMS)})
        self.assertEqual(bad["status"], "build_failed")
        restored = broker.run("check")
        self.assertEqual(restored["status"], "ok")
        self.assertEqual((broker.tree / SECRET_PATH).read_text(encoding="utf-8"), SECRET)

    def test_inbox_round_trip(self) -> None:
        inbox = self.temp / "inbox"
        inbox.mkdir()
        (inbox / "secret_candidate.rs").write_text(CANDIDATE_MISSING_ITEMS, encoding="utf-8")
        (inbox / "r1.request.json").write_text(
            json.dumps({"command": "check", "candidates": {SECRET_PATH: "secret_candidate.rs"}}),
            encoding="utf-8",
        )
        serve(self.broker(), inbox, once=True)
        result = json.loads((inbox / "r1.result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["status"], "build_failed")
        self.assertTrue((inbox / "r1.request.done").exists())
        self.assert_no_withheld_text(result)


class ContainedTestRunTests(WorkspaceCase):
    """Run hostile candidate tests in the real sandbox, with a planted marker."""

    fixture = {
        **FILES,
        "crates/owner/src/lib.rs": FILES["crates/owner/src/lib.rs"] + "pub mod contained;\n",
        CONTAINED_PATH: CONTAINED_BASE,
    }
    listed = (*LISTED, CONTAINED_PATH)

    def setUp(self) -> None:
        super().setUp()
        if shutil.which("cargo") is None:
            self.skipTest("cargo is not installed")
        blocked = self.broker()._sandbox_block()
        if blocked:
            self.skipTest(blocked)

    @staticmethod
    def outcomes(result: dict) -> dict[str, str]:
        return {test["name"]: test["outcome"] for suite in result["suites"] for test in suite["tests"]}

    def test_hostile_candidate_is_contained(self) -> None:
        marker = self.temp / "planted-marker.txt"
        marker.write_text("CANARY_PLANTED_MARKER\n", encoding="utf-8")
        inbox = self.temp / "inbox"
        inbox.mkdir()
        broker = self.broker(jobs=1, test_memory_gib=1)
        listener = socket.socket()
        self.addCleanup(listener.close)
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(0.2)
        text = ATTACK_CANDIDATE
        for key, value in {
            "@MARKER@": marker.as_posix(),
            "@TREE@": broker.tree.as_posix(),
            "@INBOX@": inbox.as_posix(),
            "@WORK@": broker.work_dir.as_posix(),
            "@PORT@": str(listener.getsockname()[1]),
        }.items():
            text = text.replace(key, value)

        result = broker.run("test", {CONTAINED_PATH: self.candidate(text)}, ["owner"])

        outcomes = self.outcomes(result)
        for name in CONTAINED_NAMES:
            expected = {"allocates_past_limit": "FAILED", "removed_by_candidate": "missing"}.get(name, "ok")
            with self.subTest(test=name):
                self.assertEqual(outcomes[f"contained::tests::{name}"], expected)
        self.assertEqual(result["sandbox"]["network"], False)
        self.assertEqual(result["sandbox"]["memory_limit_mib"], 1024)
        self.assertEqual(result["counts"]["regressions"], 2)
        self.assert_no_withheld_text(result)
        self.assertEqual(list(inbox.iterdir()), [])
        self.assertNotIn("CANARY_RESULT", json.dumps(result))
        for name in ("sandbox-plan.json", "sandbox-result.json"):
            self.assertNotIn("CANARY_RESULT", (broker.work_dir / name).read_text(encoding="utf-8"))
        self.assertEqual(marker.read_text(encoding="utf-8"), "CANARY_PLANTED_MARKER\n")
        self.assertEqual(
            (broker.tree / "crates/owner/src/open_part.rs").read_text(encoding="utf-8"),
            FILES["crates/owner/src/open_part.rs"],
        )
        with self.assertRaises(TimeoutError):
            listener.accept()

    def test_names_come_only_from_the_pre_candidate_list(self) -> None:
        broker = self.broker(jobs=1)
        baseline = set(self.outcomes(broker.run("test", packages=["owner"])))
        candidate = CONTAINED_BASE.replace(
            "mod tests {\n", "mod tests {\n    #[test]\n    fn canary_encoded_data_0101() {}\n\n", 1
        )
        result = broker.run("test", {CONTAINED_PATH: self.candidate(candidate)}, ["owner"])
        self.assertEqual(set(self.outcomes(result)), baseline)
        self.assert_no_withheld_text(result)

    def test_tests_return_names_and_outcomes_only(self) -> None:
        result = self.broker(jobs=1).run("test")
        self.assertEqual(result["status"], "failed")
        outcomes = self.outcomes(result)
        self.assertEqual(outcomes["secret::tests::passes_in_withheld"], "ok")
        self.assertEqual(outcomes["secret::tests::fails_with_source_text"], "FAILED")
        self.assertEqual(outcomes["secret::tests::writes_source_past_capture"], "ok")
        self.assertEqual(outcomes["tests::consumer_passes"], "ok")
        self.assert_no_withheld_text(result)


if __name__ == "__main__":
    unittest.main()
