#!/usr/bin/env python3
"""Run test binaries in a bubblewrap sandbox and record only their exit status.

The broker starts this script with ``python -I``, so it imports only the standard
library. Each test runs in its own sandbox with no network, no ``/mnt``, no home
directory and a read-only tree. Test output goes to ``/dev/null``. Test names come
from the plan, and an outcome comes from the process exit status, so code under
test has no stream that reaches the result file.
"""

from __future__ import annotations

import json
import os
import re
import signal
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor

MAX_LIST_BYTES = 32 * 1024 * 1024
_LIST_LINE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*): test")


def sandbox_prefix(tree: str, runtime_tree: str, target: str, runtime_target: str, cwd: str) -> list[str]:
    """Return a bwrap command prefix exposing only system files and the runtime copies."""
    args = [
        "bwrap",
        "--unshare-user",
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        "/usr",
        "/usr",
    ]
    for link in ("/bin", "/sbin", "/lib", "/lib32", "/lib64", "/libx32"):
        if os.path.islink(link):
            args += ["--symlink", os.readlink(link), link]
        elif os.path.isdir(link):
            args += ["--ro-bind", link, link]
    for path in ("/etc/ld.so.cache", "/etc/alternatives", "/etc/fonts"):
        args += ["--ro-bind-try", path, path]
    args += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]
    args += ["--ro-bind", runtime_tree, tree, "--ro-bind", runtime_target, target]
    # bwrap's root is a private tmpfs. Make it read-only so that only /tmp is writable.
    args += ["--remount-ro", "/", "--chdir", cwd, "--"]
    return args


def clean_env(extra: dict[str, str]) -> dict[str, str]:
    return {
        "PATH": "/usr/bin:/bin",
        "HOME": "/tmp",
        "TMPDIR": "/tmp",
        "LANG": "C.UTF-8",
        "RUST_BACKTRACE": "0",
        **extra,
    }


def parse_list(output: str) -> list[str]:
    """Accept only exact ``name: test`` lines."""
    names = []
    for line in output.splitlines():
        match = _LIST_LINE.fullmatch(line)
        if match:
            names.append(match.group(1))
    return names


def _kill(process: subprocess.Popen) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def list_names(prefix: list[str], exe: str, env: dict[str, str], timeout: float, *, ignored: bool = False):
    command = [*prefix, exe, "--list", "--format", "terse", *(["--ignored"] if ignored else [])]
    process = subprocess.Popen(
        command,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    timer = threading.Timer(timeout, _kill, (process,))
    timer.start()
    chunks: list[bytes] = []
    total = 0
    try:
        while chunk := process.stdout.read(65536):
            total += len(chunk)
            if total > MAX_LIST_BYTES:
                _kill(process)
                process.wait()
                return None
            chunks.append(chunk)
        code = process.wait()
    finally:
        timer.cancel()
        process.stdout.close()
    if code != 0:
        return None
    return parse_list(b"".join(chunks).decode("utf-8", "replace"))


def run_test(prefix: list[str], exe: str, name: str, env: dict[str, str], timeout: float) -> str:
    process = subprocess.Popen(
        [*prefix, exe, "--exact", name, "--test-threads=1"],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    try:
        code = process.wait(timeout)
    except subprocess.TimeoutExpired:
        _kill(process)
        process.wait()
        return "timeout"
    return "ok" if code == 0 else "FAILED"


def execute(plan: dict) -> dict:
    """Run a plan. A suite with an ``allowed`` list can only report those names."""
    results: dict[str, dict] = {}
    timeout = plan["test_timeout"]
    with ThreadPoolExecutor(max_workers=plan["jobs"]) as pool:
        for suite in plan["suites"]:
            prefix = sandbox_prefix(
                plan["tree"], plan["runtime_tree"], plan["target"], plan["runtime_target"], suite["cwd"]
            )
            env = clean_env(suite["env"])
            listed = list_names(prefix, suite["exe"], env, timeout)
            if listed is None:
                results[suite["id"]] = {"listed": None, "ignored": [], "outcomes": {}}
                continue
            allowed = suite.get("allowed")
            if allowed is None:
                ignored = list_names(prefix, suite["exe"], env, timeout, ignored=True)
                if ignored is None:
                    results[suite["id"]] = {"listed": None, "ignored": [], "outcomes": {}}
                    continue
                names = sorted(set(listed))
                runnable = sorted(set(listed) - set(ignored))
            else:
                ignored = []
                names = sorted(set(listed) & set(allowed))
                runnable = names
            outcomes = pool.map(lambda name: run_test(prefix, suite["exe"], name, env, timeout), runnable)
            results[suite["id"]] = {
                "listed": names,
                "ignored": sorted(set(ignored)),
                "outcomes": dict(zip(runnable, outcomes)),
            }
    return {"suites": results}


def main(argv: list[str]) -> int:
    plan_path, result_path = argv
    with open(plan_path, encoding="utf-8") as handle:
        plan = json.load(handle)
    result = execute(plan)
    partial = result_path + ".partial"
    with open(partial, "w", encoding="utf-8") as handle:
        json.dump(result, handle)
    os.replace(partial, result_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
