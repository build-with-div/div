#!/usr/bin/env python3
"""Carry an unmerged Open UI branch over to the Div repository.

    python tools/migrate/convert_branch.py --source <open-ui checkout> \
        --branch engine/a1-style-macro --div <div checkout> [--onto main] \
        [--work out/convert] [--regenerate --wpt-root ... --acorn ...]

The branch's change is converted, not its text: migrate.py converts both the
branch's merge base with Open UI `main` and the branch head, the difference
between the two converted trees is the change in Div's layout and names, and
that difference is applied with a three-way merge to a new branch
`<branch>` in the Div checkout, starting at --onto. Conflicts are left in the
working tree and reported; nothing is resolved automatically.

Changes to data-repository paths (evidence) are listed in the report: they
belong in a new fidelity-data commit, not in the Div branch.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import migrate  # noqa: E402
import regenerate  # noqa: E402


def git(repo: Path, *args: str, input: bytes | None = None, check: bool = True) -> str:
    proc = subprocess.run(["git", "-C", str(repo), *args], input=input,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and proc.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed:\n{proc.stderr.decode(errors='replace')}")
    return proc.stdout.decode(errors="replace")


def convert(source: Path, sha: str, out: Path, data: Path, regenerate_args: list[str]) -> str:
    args = ["--source", str(source), "--sha", sha, "--out", str(out), "--data-out", str(data),
            "--no-commit", "--report", str(out.parent / f"{out.name}-report.json")]
    if not regenerate_args:
        args.append("--skip-data")
    migrate.main(args + regenerate_args)
    if not regenerate_args:
        regenerate.format_tree(out)  # the import is formatted; so must the snapshots be
    # Snapshot the converted tree as a commit in a scratch repository.
    _, commit = migrate.git_commit(out, f"converted {sha}", ("Div migration", "migration@div.build"),
                                   "2000-01-01T00:00:00Z")
    return commit


def changes_by_fate(source: Path, base: str, head: str, fates: tuple[str, ...]) -> list[str]:
    changed = git(source, "diff", "--name-only", "-z", base, head).split("\0")
    out = []
    for path in filter(None, changed):
        fate, _ = migrate.classify(path)
        if fate in fates:
            out.append(path)
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--source", type=Path, required=True)
    ap.add_argument("--branch", required=True, help="Open UI branch or commit to convert")
    ap.add_argument("--base", help="Open UI commit the branch starts from (default: merge base with origin/main)")
    ap.add_argument("--div", type=Path, required=True, help="Div checkout to create the branch in")
    ap.add_argument("--onto", default="main")
    ap.add_argument("--name", help="branch name in Div (default: same as --branch)")
    ap.add_argument("--work", type=Path, default=Path("out/convert"))
    ap.add_argument("--regenerate", action="store_true")
    ap.add_argument("--wpt-root")
    ap.add_argument("--acorn")
    args = ap.parse_args(argv)

    head = git(args.source, "rev-parse", "--verify", args.branch + "^{commit}").strip()
    base = args.base or git(args.source, "merge-base", head, "origin/main").strip()
    name = args.name or args.branch
    regen = []
    if args.regenerate:
        regen = ["--regenerate", "--wpt-root", args.wpt_root, "--acorn", args.acorn]
    work = args.work.resolve()
    base_commit = convert(args.source, base, work / "base", work / "base-data", regen)
    head_commit = convert(args.source, head, work / "head", work / "head-data", regen)

    # Bring both snapshots into the Div repository so --3way has the blobs.
    for side in ("base", "head"):
        git(args.div, "fetch", "--quiet", "--no-tags", str(work / side), f"+refs/heads/main:refs/convert/{side}")
    diff = subprocess.run(["git", "-C", str(args.div), "diff", "--binary", "--full-index",
                           "refs/convert/base", "refs/convert/head"], stdout=subprocess.PIPE, check=True).stdout
    git(args.div, "switch", "--quiet", "-c", name, args.onto)
    proc = subprocess.run(["git", "-C", str(args.div), "apply", "--3way", "--index", "--whitespace=nowarn"],
                          input=diff, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    conflicts = [l[3:] for l in git(args.div, "status", "--porcelain").splitlines() if l[:2] in ("UU", "AA", "DU", "UD")]
    report = {
        "branch": args.branch, "base": base, "head": head, "div_branch": name, "onto": args.onto,
        "applied": proc.returncode == 0, "conflicts": conflicts,
        "apply_messages": proc.stderr.decode(errors="replace").splitlines()[-40:],
        "data_repository_changes": changes_by_fate(args.source, base, head, ("gates", "archive")),
        # Paths the import rules drop (history such as dated progress logs):
        # the branch's changes to them are not carried over; review each.
        "dropped_changes": changes_by_fate(args.source, base, head, ("drop",)),
        "patch_bytes": len(diff),
    }
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if proc.returncode == 0 and not conflicts:
        log = git(args.source, "log", "--format=%s", f"{base}..{head}").splitlines()
        message = (f"{log[-1] if log else name}\n\nConverted from Open UI {args.branch} "
                   f"({base[:12]}..{head[:12]}) by tools/migrate/convert_branch.py.\n\n"
                   "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n")
        git(args.div, "commit", "--quiet", "-F", "-", input=message.encode())
    print(json.dumps({k: report[k] for k in ("applied", "conflicts", "data_repository_changes",
                                             "dropped_changes", "patch_bytes")},
                     indent=2))
    return 0 if report["applied"] and not conflicts else 1


if __name__ == "__main__":
    sys.exit(main())
