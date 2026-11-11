#!/usr/bin/env python3
"""Build the Div repository and its fidelity-data repository from an Open UI commit.

    python tools/migrate/migrate.py --source <open-ui checkout> --sha <commit> \
        --out <div tree> --data-out <data tree> [--regenerate ...]

The result depends only on the source commit and this directory (rules,
overlay files and patches): rerunning it at the same commit yields the same
trees and the same commit IDs. Nothing is fetched from the network.

Steps:
 1. Classify every tracked path with rules.PATH_RULES (an unmatched path fails).
 2. Write kept files to --out, rewriting relative paths, repository paths,
    Markdown links and names (rules.TOKEN_RULES); data files go to --data-out.
 3. Copy overlay/ over the tree and apply patches/*.patch in order.
 4. Regenerate generated files with their own generators (--regenerate).
 5. Commit both trees with fixed metadata and pin the data commit in
    fidelity-data.lock.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import posixpath
import re
import shutil
import subprocess
import sys
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fixups  # noqa: E402
import rules  # noqa: E402

OLD_REPO_URL = "https://github.com/zhuowcui/open-ui"
DATA_REPO_URL = "https://github.com/build-with-div/div-fidelity-data"
MAIN_REPO_URL = "https://github.com/build-with-div/div"


def run(cmd, cwd=None, env=None, input=None, capture=True, check=True):
    proc = subprocess.run(cmd, cwd=cwd, env=env, input=input,
                          stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.PIPE if capture else None)
    if check and proc.returncode != 0:
        err = proc.stderr.decode(errors="replace") if capture and proc.stderr else ""
        raise SystemExit(f"command failed ({proc.returncode}): {' '.join(map(str, cmd))}\n{err}")
    return proc


# --------------------------------------------------------------------------- source

def list_tree(source: Path, sha: str):
    out = run(["git", "-C", str(source), "ls-tree", "-r", "-z", "--full-tree", sha]).stdout
    entries = []
    for rec in out.split(b"\0"):
        if not rec:
            continue
        meta, path = rec.split(b"\t", 1)
        mode, kind, obj = meta.decode().split()
        if kind != "blob":
            raise SystemExit(f"unsupported tree entry {kind} at {path.decode()}")
        entries.append((mode, obj, path.decode("utf-8")))
    return entries


def read_blobs(source: Path, objs):
    """Yield blob bytes for objs, in order, through one `git cat-file --batch`."""
    proc = subprocess.Popen(["git", "-C", str(source), "cat-file", "--batch"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE)

    def feed():
        for o in objs:
            proc.stdin.write(o.encode() + b"\n")
        proc.stdin.close()

    t = threading.Thread(target=feed, daemon=True)
    t.start()
    for o in objs:
        header = proc.stdout.readline().split()
        if len(header) != 3 or header[0].decode() != o:
            raise SystemExit(f"cat-file: unexpected header {header!r} for {o}")
        size = int(header[2])
        data = proc.stdout.read(size)
        proc.stdout.read(1)
        yield data
    t.join()
    proc.wait()


# --------------------------------------------------------------------------- classification

COMPILED_RULES = [(re.compile(p), a) for p, a in rules.PATH_RULES]


def classify(path: str):
    for rx, action in COMPILED_RULES:
        m = rx.match(path)
        if m:
            if action[0] == "main":
                return "main", rename_path(m.expand(action[1]))
            return action[0], path
    return None, None


def rename_path(path: str) -> str:
    """File names follow the same token rules as content (openui.h -> divui.h)."""
    head, _, name = path.rpartition("/")
    if any(rx.search(path) for rx in NO_REWRITE_RX):
        return path
    new = apply_tokens(name, rules.TOKEN_RULES)
    return f"{head}/{new}" if head else new


NO_REWRITE_RX = [re.compile(p) for p in rules.NO_REWRITE]
IDENTS_ONLY_RX = [re.compile(p) for p in rules.CRATE_IDENTS_ONLY]


def compile_rules(rule_list):
    return [(re.compile(p), r) for p, r in rule_list]


TOKEN_RX = compile_rules(rules.TOKEN_RULES)
PRODUCT_RX = [re.compile(p) for p in rules.PRODUCT_PATHS]
PRODUCT_TOKEN_RX = compile_rules(rules.PRODUCT_RULES)
IDENT_RX = compile_rules(rules.CRATE_IDENT_RULES)


def apply_tokens(text: str, rule_list) -> str:
    compiled = {id(rules.TOKEN_RULES): TOKEN_RX, id(rules.PRODUCT_RULES): PRODUCT_TOKEN_RX}.get(id(rule_list), IDENT_RX)
    holders: list[str] = []

    def hold(m):
        holders.append(m.group(0))
        return f"\x00P{len(holders) - 1}\x00"

    text = PROTECTED_RX.sub(hold, text)
    for rx, repl in compiled:
        text = rx.sub(repl, text)
    return re.sub(r"\x00P(\d+)\x00", lambda m: holders[int(m.group(1))], text)


PROTECTED_RX = re.compile("|".join(f"(?:{p})" for p in rules.PROTECTED))


# --------------------------------------------------------------------------- rewriting

class Rewriter:
    def __init__(self, files: dict, source_sha: str):
        # old path -> (fate, new path)
        self.files = files
        self.sha = source_sha
        self.dirs = set()
        for p in files:
            parts = p.split("/")
            for i in range(1, len(parts)):
                self.dirs.add("/".join(parts[:i]))
        self.crate_roots = sorted({posixpath.dirname(p) for p in files if p.endswith("/Cargo.toml")},
                                  key=len, reverse=True)
        self.unresolved = []
        self.prefix_rx = re.compile(
            # `fidelity/`: a path the relative pass already moved under tests/fidelity.
            r"(?<![\w.-])(?<!fidelity/)(" + "|".join(re.escape(o) for o, _ in rules.PATH_PREFIXES) + r")(?=$|[/\s\"'`),:;\]>#*]|\\)")
        self.prefix_map = dict(rules.PATH_PREFIXES)

    # A directory's new location: where its files went (longest common dir).
    def map_dir(self, old_dir: str):
        if old_dir == "":
            return ""
        news = [nf for p, (fate, nf) in self.files.items()
                if p.startswith(old_dir + "/") and fate == "main"]
        if not news:
            return None
        common = posixpath.commonpath(news)
        # When the directory itself moved wholesale, the common path is enough.
        return common

    def map_path(self, old: str):
        if old in self.files:
            fate, new = self.files[old]
            return fate, new
        if old in self.dirs:
            # Prefer the explicit prefix map for directories.
            for o, n in rules.PATH_PREFIXES:
                if old == o or old.startswith(o + "/"):
                    return "main", n + old[len(o):]
            fates = {f for p, (f, _) in self.files.items() if p.startswith(old + "/")}
            if fates == {"main"}:
                return "main", self.map_dir(old)
            if "main" in fates:
                return "main", old  # mixed directory keeps its path
            return fates.pop(), old
        return None, None

    def new_dir_of(self, old_dir: str):
        fate, new = self.map_path(old_dir) if old_dir else ("main", "")
        if fate == "main" and new is not None:
            return new
        return None

    def crate_root(self, old_file: str):
        for root in self.crate_roots:
            if old_file.startswith(root + "/"):
                return root
        return None

    REL_RX = re.compile(r"(?<![\w.])((?:\.\./)+)([\w@.\-]+(?:/[\w@.\-*]+)*)")

    def rewrite_relative(self, text: str, old_file: str, new_file: str) -> str:
        old_dir = posixpath.dirname(old_file)
        new_dir = posixpath.dirname(new_file)

        def sub(m):
            dots, rest = m.group(1), m.group(2)
            start = m.start()
            prev = text[start - 1] if start else ""
            candidates = []
            if prev == "/":
                root = self.crate_root(old_file)
                if root is not None:
                    candidates.append((root, self.new_dir_of(root)))
            candidates.append((old_dir, new_dir))
            for base_old, base_new in candidates:
                if base_new is None:
                    continue
                target = posixpath.normpath(posixpath.join(base_old, dots + rest))
                if target.startswith(".."):
                    continue
                probe = target.rstrip("*").rstrip("/")
                fate, new_target = self.map_path(probe)
                if fate is None:
                    continue
                if fate != "main" and fate not in ("gates", "archive"):
                    self.unresolved.append((old_file, m.group(0), f"target {fate}"))
                    return m.group(0)
                new_rel = posixpath.relpath(new_target, base_new or ".")
                if target.endswith("*") or rest.endswith("*"):
                    new_rel += rest[len(rest.rstrip("*")):]
                if not new_rel.startswith("../") and not rest.startswith(".."):
                    # Stays inside the base after the move; keep a ./-free form.
                    pass
                return new_rel
            self.unresolved.append((old_file, m.group(0), "no target"))
            return m.group(0)

        return self.REL_RX.sub(sub, text)

    MD_LINK_RX = re.compile(r"(\]\()([^)\s#]+)(#[^)\s]*)?(\))")

    def rewrite_markdown(self, text: str, old_file: str, new_file: str) -> str:
        old_dir = posixpath.dirname(old_file)
        new_dir = posixpath.dirname(new_file)

        def sub(m):
            link, anchor = m.group(2), m.group(3) or ""
            if re.match(r"^[a-z][a-z0-9+.-]*:", link) or link.startswith("/"):
                return m.group(0)
            target = posixpath.normpath(posixpath.join(old_dir, link))
            if target.startswith(".."):
                return m.group(0)
            fate, new_target = self.map_path(target.rstrip("/"))
            if fate == "main":
                rel = posixpath.relpath(new_target, new_dir or ".")
                return f"{m.group(1)}{rel}{anchor}{m.group(4)}"
            if fate in ("drop", "gates", "archive"):
                kind = "tree" if target in self.dirs else "blob"
                return f"{m.group(1)}{OLD_REPO_URL}/{kind}/{self.sha}/{target}{anchor}{m.group(4)}"
            self.unresolved.append((old_file, link, "markdown link target missing"))
            return m.group(0)

        return self.MD_LINK_RX.sub(sub, text)

    def rewrite_prefixes(self, text: str) -> str:
        return self.prefix_rx.sub(lambda m: self.prefix_map[m.group(1)], text)


def data_ignore_patterns(fates: dict) -> list[str]:
    """The fewest anchored patterns that cover every restored data file."""
    data = {p for p, (f, _) in fates.items() if f in ("gates", "archive")}
    kept_dirs = set()
    for p, (f, new) in fates.items():
        if f == "main":
            parts = new.split("/")
            kept_dirs.update("/".join(parts[:i]) for i in range(1, len(parts)))
            kept_dirs.add(new)
    patterns = set()
    mixed = set()
    for p in data:
        parts = p.split("/")
        for i in range(1, len(parts) + 1):
            prefix = "/".join(parts[:i])
            if prefix not in kept_dirs:
                if i == len(parts):
                    mixed.add("/".join(parts[:-1]))  # a file beside kept files
                else:
                    patterns.add("/" + prefix + "/")
                break
    lines = []
    for parent in sorted(mixed):
        depth = parent.count("/") + 1
        children = sorted({k.split("/")[depth] for k in kept_dirs if k.startswith(parent + "/")})
        loose = sorted({p.split("/")[depth] for p in data if p.startswith(parent + "/")} - set(children))
        if len(loose) <= len(children):
            # Few data entries here: name them.
            for name in loose:
                full = f"{parent}/{name}"
                lines.append(f"/{full}" + ("/" if full not in data else ""))
        else:
            lines.append(f"/{parent}/*")
            lines += [f"!/{parent}/{c}" for c in children]
    # A directory pattern directly under a mixed parent is covered by `parent/*`.
    return sorted(p for p in patterns if posixpath.dirname(p.rstrip("/"))[1:] not in mixed) + lines


def is_text(data: bytes) -> bool:
    if b"\0" in data[:8192]:
        return False
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return False
    return True


_CD_WORKSPACE = ("cd bindings/rust", "cd bindings\\rust")
_CD_BACK = ("cd ../..", "cd ..\\..")


def root_workspace_commands(text: str) -> str:
    """Commands in Markdown code blocks that ran from `bindings/rust` now run
    from the repository root: drop the `cd`, rebase `../../` paths in the
    rest of the block and drop a matching `cd ../..`."""
    out, fenced, rooted = [], False, False
    for line in text.split("\n"):
        stripped = line.strip()
        if stripped.startswith("```"):
            fenced, rooted = not fenced, False
        elif fenced and stripped in _CD_WORKSPACE:
            rooted = True
            continue
        elif rooted and stripped in _CD_BACK:
            rooted = False
            continue
        elif rooted:
            line = line.replace("../../", "").replace("..\\..\\", "")
        out.append(line)
    text = "\n".join(out)
    text = re.sub(r"`cd bindings/rust && ", "`", text)
    return re.sub(r"\b([Ff])rom `bindings/rust`",
                  lambda m: m.group(1) + "rom the repository root", text)


def transform(rw: Rewriter, old: str, new: str, data: bytes) -> bytes:
    if any(rx.search(new) for rx in NO_REWRITE_RX) or not is_text(data):
        return data
    text = data.decode("utf-8")
    if any(rx.search(new) for rx in IDENTS_ONLY_RX):
        return apply_tokens(text, rules.CRATE_IDENT_RULES).encode("utf-8")
    if new.endswith(".md"):
        text = re.sub(r"\bOpenUI\b", "Div", text)
        text = rw.rewrite_markdown(text, old, new)
        text = root_workspace_commands(text)
    else:
        text = rw.rewrite_relative(text, old, new)
    text = rw.rewrite_prefixes(text)
    text = apply_tokens(text, rules.TOKEN_RULES)
    if any(rx.search(new) for rx in PRODUCT_RX):
        text = apply_tokens(text, rules.PRODUCT_RULES)
    if old == "bindings/rust/Cargo.toml":
        text = workspace_members(rw, text)
    return apply_fixups(new, text).encode("utf-8")


FIXUP_USED = {}


def apply_fixups(new: str, text: str) -> str:
    for i, (path, pattern, repl, expected) in enumerate(fixups.FIXUPS):
        if path != new:
            continue
        text, n = re.subn(pattern, repl, text)
        if (n != expected) if expected >= 0 else n == 0:
            raise SystemExit(f"fixup {i} for {path}: {n} matches, expected {expected}: {pattern!r}")
        FIXUP_USED[i] = True
    return text


EXTRA_MEMBERS = ["crates/div-components", "tests/fidelity/components/render"]


def workspace_members(rw: "Rewriter", text: str) -> str:
    """Members of bindings/rust move to their new homes under the root."""
    block = re.search(r'members = \[\n((?:    "[^"]+",\n)+)\]', text)
    if not block:
        raise SystemExit("workspace members list not found")
    old_manifest = rw.old_workspace_members
    news = []
    for m in old_manifest:
        fate, new = rw.map_path("bindings/rust/" + m)
        if fate != "main":
            raise SystemExit(f"workspace member {m} is not kept")
        news.append(new)
    news += EXTRA_MEMBERS
    body = "".join(f'    "{n}",\n' for n in news)
    return text[:block.start(1)] + body + text[block.end(1):]


# --------------------------------------------------------------------------- writing

def write_file(root: Path, rel: str, data: bytes, mode: str):
    dest = root / rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    if mode == "100755":
        try:
            os.chmod(dest, 0o755)
        except OSError:
            pass


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def clean_dir(path: Path):
    if path.exists():
        if (path / ".git").exists() or any(path.iterdir()):
            shutil.rmtree(path, onerror=_force_remove)
    path.mkdir(parents=True, exist_ok=True)


def _force_remove(func, p, _exc):
    os.chmod(p, 0o700)
    func(p)


def git_commit(tree: Path, message: str, author: tuple, date: str, exec_paths=()):
    env = dict(os.environ)
    env.update({
        "GIT_AUTHOR_NAME": author[0], "GIT_AUTHOR_EMAIL": author[1], "GIT_AUTHOR_DATE": date,
        "GIT_COMMITTER_NAME": author[0], "GIT_COMMITTER_EMAIL": author[1], "GIT_COMMITTER_DATE": date,
    })
    g = ["git", "-C", str(tree), "-c", "core.autocrlf=false", "-c", "core.safecrlf=false",
         "-c", "core.longpaths=true", "-c", "commit.gpgsign=false"]
    run(g + ["init", "--quiet", "--initial-branch=main"])
    # Index from a sorted path list so .gitignore and attributes cannot drop files.
    files = sorted(p.relative_to(tree).as_posix() for p in tree.rglob("*")
                   if p.is_file() and ".git" not in p.relative_to(tree).parts[:1])
    run(g + ["add", "--force", "--pathspec-from-file=-", "--pathspec-file-nul"],
        input="\0".join(files).encode())
    for p in exec_paths:
        run(g + ["update-index", "--chmod=+x", p])
    tree_id = run(g + ["write-tree"], env=env).stdout.decode().strip()
    commit = run(g + ["commit-tree", tree_id, "-F", "-"], env=env,
                 input=message.encode()).stdout.decode().strip()
    run(g + ["update-ref", "refs/heads/main", commit])
    run(g + ["reset", "--quiet"])
    return tree_id, commit


# --------------------------------------------------------------------------- main

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--source", type=Path, required=True, help="Open UI git checkout or bare repository")
    ap.add_argument("--sha", required=True, help="Open UI commit to import")
    ap.add_argument("--out", type=Path, required=True, help="output directory for the Div tree (emptied)")
    ap.add_argument("--data-out", type=Path, required=True, help="output directory for the data tree (emptied)")
    ap.add_argument("--regenerate", action="store_true", help="run the generators (see regenerate.py)")
    ap.add_argument("--wpt-root", type=Path, help="Chromium WPT root at the pinned commit")
    ap.add_argument("--acorn", type=Path, help="pinned acorn.mjs")
    ap.add_argument("--skip-data", action="store_true", help="do not write data files (faster iteration)")
    ap.add_argument("--no-commit", action="store_true", help="leave the trees uncommitted")
    ap.add_argument("--report", type=Path, help="write a JSON report of every path's fate")
    ap.add_argument("--link-sha", help="Open UI commit that links to archived files point at "
                    "(default: --sha; branch conversion pins both sides to the import's commit)")
    args = ap.parse_args(argv)
    FIXUP_USED.clear()

    sha = run(["git", "-C", str(args.source), "rev-parse", "--verify", args.sha + "^{commit}"]).stdout.decode().strip()
    meta = run(["git", "-C", str(args.source), "show", "-s", "--format=%cI%n%cn%n%ce", sha]).stdout.decode().splitlines()
    date = meta[0]
    entries = list_tree(args.source, sha)

    fates = {}
    unmatched = []
    for mode, obj, path in entries:
        fate, new = classify(path)
        if fate is None:
            unmatched.append(path)
        fates[path] = (fate, new)
    if unmatched:
        raise SystemExit("unclassified paths:\n  " + "\n  ".join(unmatched[:200]))
    news = {}
    for p, (fate, new) in fates.items():
        if fate == "main":
            if new in news:
                raise SystemExit(f"two sources map to {new}: {news[new]} and {p}")
            news[new] = p

    clean_dir(args.out)
    clean_dir(args.data_out)
    link_sha = sha
    if args.link_sha:
        link_sha = run(["git", "-C", str(args.source), "rev-parse", "--verify",
                        args.link_sha + "^{commit}"]).stdout.decode().strip()
    rw = Rewriter(fates, link_sha)
    ws = run(["git", "-C", str(args.source), "show", f"{sha}:bindings/rust/Cargo.toml"]).stdout.decode()
    rw.old_workspace_members = re.findall(r'^    "([^"]+)",$', re.search(r"members = \[\n(.*?)\]", ws, re.S).group(0), re.M)
    manifest = []
    exec_main = []
    counts = {"main": 0, "gates": 0, "archive": 0, "drop": 0}
    wanted = [e for e in entries if not (args.skip_data and fates[e[2]][0] in ("gates", "archive", "drop"))]
    for (mode, obj, path), data in zip(wanted, read_blobs(args.source, [e[1] for e in wanted])):
        fate, new = fates[path]
        counts[fate] += 1
        if fate == "main":
            out = transform(rw, path, new, data)
            write_file(args.out, new, out, mode)
            if mode == "100755":
                exec_main.append(new)
        elif fate in ("gates", "archive"):
            rel = f"{fate}/{path}"
            if not args.skip_data:
                write_file(args.data_out, rel, data, mode)
            manifest.append((rel, sha256(data), len(data)))

    unused = [i for i in range(len(fixups.FIXUPS)) if i not in FIXUP_USED]
    if unused:
        raise SystemExit(f"fixups never applied (path missing?): {[fixups.FIXUPS[i][:2] for i in unused]}")

    # Overlay files and patches.
    overlay = HERE / "overlay"
    if overlay.is_dir():
        for f in sorted(overlay.rglob("*")):
            if f.is_file():
                rel = f.relative_to(overlay).as_posix()
                write_file(args.out, rel, f.read_bytes(), "100644")
    with open(args.out / ".gitignore", "a", encoding="utf-8", newline="\n") as ignore:
        ignore.write("\n# Restored by tools/fidelity_data.py from the pinned data repository\n")
        ignore.write("".join(f"{p}\n" for p in data_ignore_patterns(fates)))
    # The migration tooling travels with the tree it produced.
    for f in sorted(HERE.rglob("*")):
        if f.is_file() and "__pycache__" not in f.parts:
            rel = "tools/migrate/" + f.relative_to(HERE).as_posix()
            write_file(args.out, rel, f.read_bytes(), "100644")
    for patch in sorted((HERE / "patches").glob("*.patch")):
        run(["git", "apply", "--whitespace=nowarn", "-p1", str(patch)],
            cwd=args.out)

    if args.regenerate:
        import regenerate
        if args.skip_data:
            raise SystemExit("--regenerate needs the data files; drop --skip-data")
        regenerate.run_all(args.out, args.data_out, wpt_root=args.wpt_root, acorn=args.acorn)

    # Data repository: manifest, provenance and README.
    manifest.sort()
    man_text = "".join(f"{h}  {n}  {p}\n" for p, h, n in manifest)
    (args.data_out / "MANIFEST.sha256").write_text(man_text, encoding="utf-8", newline="\n")
    profiles = {}
    for prof in ("gates", "archive"):
        rows = [r for r in manifest if r[0].startswith(prof + "/")]
        profiles[prof] = {"files": len(rows), "bytes": sum(r[2] for r in rows),
                          "sha256": sha256("".join(f"{h}  {n}  {p}\n" for p, h, n in rows).encode())}
    source_json = {
        "source": {"repository": OLD_REPO_URL, "commit": sha, "committed": date},
        "layout": "Each file sits at <profile>/<path>, where <path> is its path in the source repository "
                  "and where tools/fidelity_data.py restores it in a Div checkout.",
        "profiles": profiles,
    }
    (args.data_out / "SOURCE.json").write_text(json.dumps(source_json, indent=2) + "\n", encoding="utf-8", newline="\n")
    data_readme = HERE / "data-README.md"
    if data_readme.is_file():
        shutil.copyfile(data_readme, args.data_out / "README.md")

    report = {
        "source": sha, "counts": counts,
        "unresolved_relative_paths": rw.unresolved,
        "paths": {p: list(v) for p, v in sorted(fates.items())},
    }
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")

    print(f"source {sha}: main {counts['main']}, gates {counts['gates']}, archive {counts['archive']}, "
          f"dropped {counts['drop']}; unresolved relative paths {len(rw.unresolved)}")
    if args.no_commit:
        return 0

    author = ("Div migration", "migration@div.build")
    data_msg = (f"Import fidelity evidence from Open UI {sha[:12]}\n\n"
                f"Source: {OLD_REPO_URL}/commit/{sha}\n")
    data_tree, data_commit = git_commit(args.data_out, data_msg, author, date)
    lock = {
        "repository": DATA_REPO_URL,
        "commit": data_commit,
        "manifest_sha256": sha256(man_text.encode()),
        "profiles": {k: v["sha256"] for k, v in profiles.items()},
        "source": {"repository": OLD_REPO_URL, "commit": sha},
    }
    (args.out / "fidelity-data.lock").write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8", newline="\n")
    main_msg = (f"Import Div from Open UI {sha[:12]}\n\n"
                f"This repository starts from {OLD_REPO_URL}/commit/{sha},\n"
                f"converted by tools/migrate/migrate.py. The history before this commit\n"
                f"stays in {OLD_REPO_URL}; generated evidence is in\n"
                f"{DATA_REPO_URL} at {data_commit}.\n\n"
                f"Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n")
    main_tree, main_commit = git_commit(args.out, main_msg, author, date, exec_main)
    print(f"data commit {data_commit} (tree {data_tree})")
    print(f"div commit  {main_commit} (tree {main_tree})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
