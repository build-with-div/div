#!/usr/bin/env python3
"""Allow one full census at a time on this machine.

A full census takes about 75 minutes, 16 jobs and half the host's memory, so
two at once slow both and can exhaust WSL. Every worktree, from Windows or
WSL, shares one lock file in the machine's shared ``out/`` directory
(``E:\\open-ui\\out\\census.lock`` is ``/mnt/e/open-ui/out/census.lock``):
``DIV_SHARED_OUT`` if set, else ``out/`` of the outermost main checkout, so a
clone kept under ``E:\\open-ui\\out\\`` and its worktrees share it too.
The lock is an exclusively created file, which both operating systems see;
OS file locks do not cross the WSL boundary.

    python3 tools/qualification/census_lock.py run [--wait] -- <census command>
    python3 tools/qualification/census_lock.py status
    python3 tools/qualification/census_lock.py release --force [--ticket <name>]

``run`` takes the lock, runs the command, releases the lock and exits with the
command's status. A lock left by a dead process on the same system is
reclaimed automatically; one from the other system needs ``release --force``
after checking that its census has ended.

``run --wait`` queues first come, first served. The waiter writes a ticket,
an exclusively created file in ``census.lock.queue/`` named by the time it
was queued, and takes the lock only when the lock is free and its ticket is
the oldest. ``run`` without ``--wait`` never passes a queued waiter. A
ticket left by a dead waiter on the same system is dropped automatically;
one from the other system needs ``release --force --ticket <name>``.
``status`` lists the holder and the queue in order.

``run`` also guards the machine's disks. Each ``--write-dir`` (repeatable: a
directory the census writes to, such as ``F:\\div-out\\<lane>``, or a WSL
target directory inside ext4) needs 50 GB free on its drive to start, and the
drive holding the shared ``out/`` (E:, which also holds WSL's virtual disk)
needs 20 GB. The checks run before queueing and again once the lock is taken.
While the command runs they repeat every minute, and the command and every
process it started are stopped if a write directory's drive falls below
30 GB or the shared ``out/`` drive below 20 GB. A refused or stopped run
exits 28 (``ENOSPC``). ``disk`` prints the same check without the lock.
A WSL directory inside ext4 reports its virtual disk's free space, which
can exceed E:'s; that disk grows on E:, so for it the shared ``out/``
check is the real limit.

A lock held from WSL cannot stop Windows grandchildren. WSL does not expose
the Windows process id of an ``.exe`` started through interop: killing its
Linux stand-in ends that ``.exe`` (``powershell.exe``), but the ``.exe``'s
own Windows children (``cargo.exe``, ``rustc.exe``) keep running. A run that
builds on Windows takes the lock from Windows Python, whose ``taskkill /T``
ends the whole Windows tree and the Linux processes started through
``wsl.exe``.

    python3 tools/qualification/census_lock.py run --wait --write-dir <dir> -- <census command>
    python3 tools/qualification/census_lock.py disk --write-dir <dir>
"""

from __future__ import annotations

import argparse
import contextlib
import datetime
import json
import os
import platform
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from collections.abc import Mapping
from pathlib import Path

POLL_SECONDS = float(os.environ.get("CENSUS_LOCK_POLL_SECONDS", "30"))
DISK_POLL_SECONDS = float(os.environ.get("CENSUS_LOCK_DISK_POLL_SECONDS", "60"))
# A run refused or stopped for disk space exits with ENOSPC's number.
DISK_EXIT = 28
GIB = 2**30


def native_path(text: str) -> Path:
    """Read a ``gitdir:`` path written by Windows git from either system."""
    match = re.match(r"^([A-Za-z]):[\\/](.*)$", text)
    if match and os.name != "nt":
        drive, rest = match.groups()
        return Path("/mnt") / drive.lower() / rest.replace("\\", "/")
    return Path(text)


def main_checkout(start: Path) -> Path:
    """Find the main checkout from a checkout or any of its worktrees.

    Git is not used: WSL git cannot follow a worktree made by Windows git.
    """
    for directory in [start, *start.parents]:
        dot_git = directory / ".git"
        if dot_git.is_dir():
            return directory
        if dot_git.is_file():
            gitdir = dot_git.read_text(encoding="utf-8").strip().removeprefix("gitdir:").strip()
            # <main>/.git/worktrees/<name>
            return native_path(gitdir).parent.parent.parent
    raise SystemExit(f"census_lock: {start} is not inside a git checkout")


SHARED_OUT_ENV = "DIV_SHARED_OUT"


def enclosing_checkout(root: Path) -> Path | None:
    """Return the main checkout whose ``out/`` holds ``root``, if any."""
    for directory in root.parents:
        if directory.name == "out" and (directory.parent / ".git").exists():
            return main_checkout(directory.parent)
    return None


def shared_out(start: Path, environ: Mapping[str, str] | None = None) -> Path:
    """Return the ``out/`` directory every checkout on this machine shares.

    ``DIV_SHARED_OUT`` names it explicitly. Otherwise it is ``out/`` of the
    outermost main checkout: a clone kept under another checkout's ``out/``
    (``E:\\open-ui\\out\\div-fidelity``) and that clone's worktrees use the
    outer checkout's ``out/`` (``E:\\open-ui\\out``), never their own.
    """
    override = (os.environ if environ is None else environ).get(SHARED_OUT_ENV)
    if override:
        return native_path(override)
    root = main_checkout(start)
    while (outer := enclosing_checkout(root)) is not None:
        root = outer
    return root / "out"


def default_lock_path() -> Path:
    return shared_out(Path(__file__).resolve().parent) / "census.lock"


def system_name() -> str:
    if os.name == "nt":
        return "windows"
    if "microsoft" in platform.uname().release.lower():
        return "wsl"
    return sys.platform


def pid_alive(pid: int) -> bool:
    if os.name == "nt":
        import ctypes

        process_query_limited_information = 0x1000
        still_active = 259
        kernel32 = ctypes.windll.kernel32
        handle = kernel32.OpenProcess(process_query_limited_information, False, pid)
        if not handle:
            return False
        try:
            code = ctypes.c_ulong()
            if not kernel32.GetExitCodeProcess(handle, ctypes.byref(code)):
                return True
            return code.value == still_active
        finally:
            kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def read_holder(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except (OSError, ValueError):
        return {"unreadable": True}


def holder_is_stale(holder: dict) -> bool:
    """A holder is stale only if it is provably dead: same system and host."""
    return (
        holder.get("system") == system_name()
        and holder.get("host") == socket.gethostname()
        and isinstance(holder.get("pid"), int)
        and not pid_alive(holder["pid"])
    )


def describe(holder: dict) -> str:
    if holder.get("unreadable"):
        return "an unreadable lock file"
    return (
        f"pid {holder.get('pid')} on {holder.get('system')}/{holder.get('host')} "
        f"since {holder.get('started') or holder.get('queued')} running {holder.get('command')!r} "
        f"from {holder.get('cwd')}"
    )


def process_record(command: list[str], time_key: str) -> dict:
    return {
        "pid": os.getpid(),
        "system": system_name(),
        "host": socket.gethostname(),
        "cwd": os.getcwd(),
        "command": " ".join(command),
        time_key: datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
    }


def create_exclusive(path: Path, record: dict) -> bool:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        return False
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        json.dump(record, handle)
    return True


def try_acquire(path: Path, command: list[str]) -> bool:
    return create_exclusive(path, process_record(command, "started"))


def queue_dir(path: Path) -> Path:
    return path.with_name(path.name + ".queue")


def enqueue(path: Path, command: list[str]) -> Path:
    """Write this process's ticket. Ticket names sort in the order they were
    queued, by wall-clock time; the system and pid break ties. WSL 2 keeps
    its clock in step with the Windows host's, so only waiters queued within
    the clocks' drift of each other (normally well under a second) could
    swap places."""
    record = process_record(command, "queued")
    while True:
        ticket = queue_dir(path) / f"{time.time_ns():020d}-{record['system']}-{record['pid']}.json"
        if create_exclusive(ticket, record):
            return ticket


def read_queue(path: Path) -> list[tuple[Path, dict]]:
    """The live tickets, oldest first. A ticket of a dead waiter on this
    system is dropped; an unreadable ticket keeps its place."""
    queue = []
    for ticket in sorted(queue_dir(path).glob("*.json")):
        waiter = read_holder(ticket)
        if waiter is None:
            continue
        if holder_is_stale(waiter):
            print(f"census_lock: dropping the ticket of dead {describe(waiter)}", file=sys.stderr)
            ticket.unlink(missing_ok=True)
            continue
        queue.append((ticket, waiter))
    return queue


def release(path: Path) -> None:
    holder = read_holder(path)
    if holder and holder.get("pid") == os.getpid() and holder.get("system") == system_name():
        path.unlink(missing_ok=True)


def live_holder(path: Path) -> dict | None:
    """The current holder, after reclaiming a lock left by a dead process."""
    holder = read_holder(path)
    if holder is not None and holder_is_stale(holder):
        print(f"census_lock: reclaiming the lock of dead {describe(holder)}", file=sys.stderr)
        path.unlink(missing_ok=True)
        return None
    return holder


def acquire(path: Path, command: list[str], wait: bool) -> None:
    if not wait:
        holder = live_holder(path)
        if holder is not None:
            raise SystemExit(f"census_lock: a census is running: {describe(holder)}")
        queue = read_queue(path)
        if queue:
            raise SystemExit(
                f"census_lock: {len(queue)} waiter(s) queued, first {describe(queue[0][1])}"
            )
        if not try_acquire(path, command):
            holder = read_holder(path) or {}
            raise SystemExit(f"census_lock: a census is running: {describe(holder)}")
        return
    ticket = enqueue(path, command)
    announced = None
    try:
        while True:
            holder = live_holder(path)
            ahead = [waiter for name, waiter in read_queue(path) if name.name < ticket.name]
            if holder is None and not ahead and try_acquire(path, command):
                return
            state = (describe(holder) if holder else "", len(ahead))
            if state != announced:
                waiting_for = f"waiting for {describe(holder)}" if holder else "waiting"
                print(f"census_lock: {waiting_for}; {len(ahead)} queued ahead", file=sys.stderr)
                announced = state
            time.sleep(POLL_SECONDS)
    finally:
        ticket.unlink(missing_ok=True)


def disk_floors() -> dict[str, float]:
    """Free space floors in GB (GiB, as ``df -BG`` counts). The environment
    overrides are for the tests, whose CI runners have small disks."""
    return {
        "start": float(os.environ.get("CENSUS_LOCK_START_FREE_GB", "50")),
        "stop": float(os.environ.get("CENSUS_LOCK_STOP_FREE_GB", "30")),
        "guard": float(os.environ.get("CENSUS_LOCK_GUARD_FREE_GB", "20")),
    }


def free_gb(path: Path) -> float:
    """Free space on the file system holding ``path`` or, so that a write
    directory need not exist yet, its nearest existing parent."""
    for directory in [path, *path.parents]:
        if directory.exists():
            return shutil.disk_usage(directory).free / GIB
    raise SystemExit(f"census_lock: no existing parent of {path}")


def check_disks(write_dirs: list[Path], guard_dir: Path, starting: bool) -> tuple[list[str], list[str]]:
    """Apply the disk rule. Returns (report lines, problems)."""
    floors = disk_floors()
    write_floor = floors["start"] if starting else floors["stop"]
    report, problems = [], []
    for directory in write_dirs:
        free = free_gb(directory)
        report.append(
            f"write dir {directory}: {free:.1f} GB free "
            f"(needs {floors['start']:g} to start, stops below {floors['stop']:g})"
        )
        if free < write_floor:
            problems.append(f"{directory} has {free:.1f} GB free, under {write_floor:g} GB")
    free = free_gb(guard_dir)
    report.append(f"shared out {guard_dir}: {free:.1f} GB free (needs {floors['guard']:g}, stops below it)")
    if free < floors["guard"]:
        problems.append(f"the shared out drive ({guard_dir}) has {free:.1f} GB free, under {floors['guard']:g} GB")
    return report, problems


def descendants(pid: int) -> list[int]:
    """Every live descendant of ``pid`` (POSIX), from one ``ps`` listing."""
    try:
        listing = subprocess.run(
            ["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True, check=True
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return []
    children: dict[int, list[int]] = {}
    for line in listing.splitlines():
        fields = line.split()
        if len(fields) == 2:
            children.setdefault(int(fields[1]), []).append(int(fields[0]))
    found, stack = [], [pid]
    while stack:
        for child in children.get(stack.pop(), []):
            found.append(child)
            stack.append(child)
    return found


def stop_tree(process: subprocess.Popen) -> None:
    """Stop the command and every process it started."""
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/T", "/F", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        process.wait()
        return
    # List the tree first: once the command exits, its children are reparented.
    tree = [process.pid, *descendants(process.pid)]
    for pid in tree:
        with contextlib.suppress(OSError):
            os.kill(pid, signal.SIGTERM)
    with contextlib.suppress(subprocess.TimeoutExpired):
        process.wait(timeout=30)
    for pid in [*tree, *descendants(process.pid)]:
        with contextlib.suppress(OSError):
            os.kill(pid, signal.SIGKILL)
    process.wait()


def refuse(problems: list[str], outcome: str) -> int:
    for problem in problems:
        print(f"census_lock: {problem}", file=sys.stderr)
    print(f"census_lock: not enough disk space; {outcome}", file=sys.stderr)
    return DISK_EXIT


def cmd_run(path: Path, command: list[str], wait: bool, write_dirs: list[Path]) -> int:
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        raise SystemExit("census_lock: run needs a command after --")
    guard_dir = path.parent
    report, problems = check_disks(write_dirs, guard_dir, starting=True)
    for line in report:
        print(f"census_lock: {line}", file=sys.stderr)
    if problems:
        return refuse(problems, "not started or queued")
    acquire(path, command, wait)
    try:
        # Space may have gone while this run waited in the queue.
        _, problems = check_disks(write_dirs, guard_dir, starting=True)
        if problems:
            return refuse(problems, "not started")
        process = subprocess.Popen(command)
        try:
            while True:
                with contextlib.suppress(subprocess.TimeoutExpired):
                    return process.wait(timeout=DISK_POLL_SECONDS)
                _, problems = check_disks(write_dirs, guard_dir, starting=False)
                if problems:
                    stop_tree(process)
                    return refuse(problems, "the command and its processes were stopped")
        except KeyboardInterrupt:
            stop_tree(process)
            return 130
    finally:
        release(path)


def cmd_disk(path: Path, write_dirs: list[Path]) -> int:
    report, problems = check_disks(write_dirs, path.parent, starting=True)
    for line in report:
        print(line)
    for problem in problems:
        print(f"refused: {problem}")
    if problems:
        return DISK_EXIT
    print("ok to start")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--lock", type=Path, default=None, help="lock file path")
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="run a command while holding the lock")
    run.add_argument("--wait", action="store_true", help="wait instead of failing when held")
    write_dir_help = "a directory the census writes to, whose drive needs free space (repeatable)"
    run.add_argument("--write-dir", action="append", default=[], help=write_dir_help)
    run.add_argument("argv", nargs=argparse.REMAINDER)
    disk = sub.add_parser("disk", help="check the disk rule without taking the lock")
    disk.add_argument("--write-dir", action="append", default=[], help=write_dir_help)
    sub.add_parser("status", help="print the current holder")
    rel = sub.add_parser("release", help="remove a lock whose census has ended")
    rel.add_argument("--force", action="store_true", required=True)
    rel.add_argument("--ticket", help="remove this queued ticket (its file name) instead")
    args = parser.parse_args(argv)
    path = args.lock or default_lock_path()
    if args.command in ("run", "disk"):
        write_dirs = [native_path(directory) for directory in args.write_dir]
        if args.command == "disk":
            return cmd_disk(path, write_dirs)
        return cmd_run(path, args.argv, args.wait, write_dirs)
    holder = read_holder(path)
    if args.command == "status":
        if holder is None:
            print(f"free ({path})")
        else:
            stale = " (stale: its process has exited)" if holder_is_stale(holder) else ""
            print(f"held by {describe(holder)}{stale} ({path})")
        for position, ticket in enumerate(sorted(queue_dir(path).glob("*.json")), 1):
            waiter = read_holder(ticket) or {}
            stale = " (stale: its process has exited)" if holder_is_stale(waiter) else ""
            print(f"  queued {position}: {ticket.name}: {describe(waiter)}{stale}")
        return 0
    if args.ticket:
        ticket = queue_dir(path) / Path(args.ticket).name
        waiter = read_holder(ticket)
        if waiter is None:
            raise SystemExit(f"census_lock: no ticket {ticket.name} in {queue_dir(path)}")
        print(f"census_lock: removing the ticket of {describe(waiter)}", file=sys.stderr)
        ticket.unlink(missing_ok=True)
        return 0
    if holder is not None:
        print(f"census_lock: removing the lock of {describe(holder)}", file=sys.stderr)
        path.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
