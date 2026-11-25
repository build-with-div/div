"""Tests for the machine-wide census lock."""

from __future__ import annotations

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from tools.qualification import census_lock

# The queue and lock tests run on CI runners with small disks. The disk rule
# has its own tests below, with the real floors and a stand-in for free space.
_DISK_FLOORS_OFF = {
    "CENSUS_LOCK_START_FREE_GB": "0",
    "CENSUS_LOCK_STOP_FREE_GB": "0",
    "CENSUS_LOCK_GUARD_FREE_GB": "0",
}
_saved_environment: dict[str, str | None] = {}
REAL_FREE_GB = census_lock.free_gb


def setUpModule() -> None:
    for key, value in _DISK_FLOORS_OFF.items():
        _saved_environment[key] = os.environ.get(key)
        os.environ[key] = value


def tearDownModule() -> None:
    for key, value in _saved_environment.items():
        if value is None:
            os.environ.pop(key, None)
        else:
            os.environ[key] = value


class CensusLockTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.lock = Path(self.temporary.name) / "out" / "census.lock"

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_lock(self, *argv: str) -> int:
        return census_lock.main(["--lock", str(self.lock), *argv])

    def test_run_holds_the_lock_and_releases_it(self) -> None:
        probe = f"import pathlib,sys; sys.exit(0 if pathlib.Path({str(self.lock)!r}).exists() else 7)"
        self.assertEqual(self.run_lock("run", "--", sys.executable, "-c", probe), 0)
        self.assertFalse(self.lock.exists())

    def test_run_returns_the_command_status(self) -> None:
        self.assertEqual(self.run_lock("run", "--", sys.executable, "-c", "raise SystemExit(3)"), 3)
        self.assertFalse(self.lock.exists())

    def test_live_holder_blocks_a_second_run(self) -> None:
        self.assertTrue(census_lock.try_acquire(self.lock, ["census"]))
        with self.assertRaisesRegex(SystemExit, "a census is running"):
            self.run_lock("run", "--", sys.executable, "-c", "pass")
        self.assertTrue(self.lock.exists())

    def test_dead_holder_on_this_system_is_reclaimed(self) -> None:
        self.lock.parent.mkdir(parents=True)
        dead = {
            "pid": 2**22 + 12345,
            "system": census_lock.system_name(),
            "host": census_lock.socket.gethostname(),
        }
        self.lock.write_text(json.dumps(dead), encoding="utf-8")
        self.assertTrue(census_lock.holder_is_stale(dead))
        self.assertEqual(self.run_lock("run", "--", sys.executable, "-c", "pass"), 0)
        self.assertFalse(self.lock.exists())

    def test_holder_on_the_other_system_is_never_reclaimed(self) -> None:
        other = {
            "pid": 2**22 + 12345,
            "system": "elsewhere",
            "host": census_lock.socket.gethostname(),
        }
        self.assertFalse(census_lock.holder_is_stale(other))

    def test_worktrees_share_the_main_checkout_lock(self) -> None:
        main = Path(self.temporary.name) / "main"
        (main / ".git" / "worktrees" / "track").mkdir(parents=True)
        tree = main / ".claude" / "worktrees" / "track"
        (tree / "tools").mkdir(parents=True)
        (tree / ".git").write_text(
            f"gitdir: {(main / '.git' / 'worktrees' / 'track').as_posix()}\n", encoding="utf-8"
        )
        self.assertEqual(census_lock.main_checkout(tree / "tools").resolve(), main.resolve())
        self.assertEqual(census_lock.main_checkout(main).resolve(), main.resolve())

    def make_worktree(self, main: Path, name: str, at: Path) -> Path:
        (main / ".git" / "worktrees" / name).mkdir(parents=True)
        (at / "tools").mkdir(parents=True)
        (at / ".git").write_text(
            f"gitdir: {(main / '.git' / 'worktrees' / name).as_posix()}\n", encoding="utf-8"
        )
        return at

    def test_a_clone_under_a_checkouts_out_shares_the_outer_out(self) -> None:
        # E:\open-ui (checkout), E:\open-ui\out\div-fidelity (clone) and a
        # worktree of the clone kept under the outer checkout's worktrees.
        outer = Path(self.temporary.name) / "open-ui"
        (outer / ".git").mkdir(parents=True)
        clone = outer / "out" / "div-fidelity"
        (clone / ".git").mkdir(parents=True)
        (clone / "tools").mkdir()
        tree = self.make_worktree(
            clone, "track", outer / ".claude" / "worktrees" / "setup" / "out" / "div-track"
        )
        own = outer / ".claude" / "worktrees" / "setup"
        self.make_worktree(outer, "setup", own)
        for start in (outer, clone / "tools", tree / "tools", own / "tools"):
            with self.subTest(start=start):
                self.assertEqual(
                    census_lock.shared_out(start, environ={}).resolve(), (outer / "out").resolve()
                )

    def test_a_standalone_checkout_uses_its_own_out(self) -> None:
        main = Path(self.temporary.name) / "main"
        (main / ".git").mkdir(parents=True)
        tree = self.make_worktree(main, "track", main / ".claude" / "worktrees" / "track")
        self.assertEqual(
            census_lock.shared_out(tree / "tools", environ={}).resolve(), (main / "out").resolve()
        )

    def test_the_environment_names_the_shared_out(self) -> None:
        main = Path(self.temporary.name) / "main"
        (main / ".git").mkdir(parents=True)
        shared = Path(self.temporary.name) / "shared"
        self.assertEqual(
            census_lock.shared_out(main, environ={census_lock.SHARED_OUT_ENV: str(shared)}),
            shared,
        )

    def test_windows_gitdir_maps_to_wsl_mount(self) -> None:
        path = census_lock.native_path("E:/open-ui/.git/worktrees/track")
        if census_lock.os.name == "nt":
            self.assertEqual(path, Path("E:/open-ui/.git/worktrees/track"))
        else:
            self.assertEqual(path, Path("/mnt/e/open-ui/.git/worktrees/track"))

    def test_forced_release_removes_the_lock(self) -> None:
        self.assertTrue(census_lock.try_acquire(self.lock, ["census"]))
        self.assertEqual(self.run_lock("release", "--force"), 0)
        self.assertFalse(self.lock.exists())


DEAD_PID = 2**22 + 12345


class CensusLockQueueTests(unittest.TestCase):
    """``run --wait`` queues first come, first served."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.lock = Path(self.temporary.name) / "out" / "census.lock"
        self.queue = census_lock.queue_dir(self.lock)
        self.log = Path(self.temporary.name) / "order.txt"
        self.waiters: list[subprocess.Popen] = []

    def tearDown(self) -> None:
        for waiter in self.waiters:
            if waiter.poll() is None:
                waiter.kill()
            waiter.wait()
        self.temporary.cleanup()

    def run_lock(self, *argv: str) -> int:
        return census_lock.main(["--lock", str(self.lock), *argv])

    def start_waiter(self, name: str) -> subprocess.Popen:
        """A ``run --wait`` in its own process whose census appends its name
        to the order log."""
        census = f"open({str(self.log)!r}, 'a').write({name!r} + '\\n')"
        environment = dict(os.environ, CENSUS_LOCK_POLL_SECONDS="0.05")
        waiter = subprocess.Popen(
            [sys.executable, census_lock.__file__, "--lock", str(self.lock),
             "run", "--wait", "--", sys.executable, "-c", census],
            env=environment,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.waiters.append(waiter)
        return waiter

    def tickets_of(self, *waiters: subprocess.Popen) -> list[str]:
        pids = {waiter.pid for waiter in waiters}
        return [
            ticket.name
            for ticket in sorted(self.queue.glob("*.json"))
            if json.loads(ticket.read_text(encoding="utf-8"))["pid"] in pids
        ]

    def wait_for(self, condition, what: str) -> None:
        deadline = time.monotonic() + 30
        while not condition():
            if time.monotonic() > deadline:
                self.fail(f"timed out waiting for {what}")
            time.sleep(0.02)

    def finish(self, waiter: subprocess.Popen) -> None:
        _, stderr = waiter.communicate(timeout=30)
        self.assertEqual(waiter.returncode, 0, stderr)

    def order(self) -> list[str]:
        return self.log.read_text(encoding="utf-8").split()

    def write_ticket(self, name: str, pid: int, system: str | None = None) -> Path:
        ticket = self.queue / name
        record = {
            "pid": pid,
            "system": system or census_lock.system_name(),
            "host": census_lock.socket.gethostname(),
        }
        self.queue.mkdir(parents=True, exist_ok=True)
        ticket.write_text(json.dumps(record), encoding="utf-8")
        return ticket

    def test_waiters_take_the_lock_in_the_order_they_queued(self) -> None:
        self.assertTrue(census_lock.try_acquire(self.lock, ["census"]))
        first = self.start_waiter("first")
        self.wait_for(lambda: self.tickets_of(first), "the first ticket")
        second = self.start_waiter("second")
        self.wait_for(lambda: len(self.tickets_of(first, second)) == 2, "the second ticket")
        self.assertEqual(self.tickets_of(first, second), self.tickets_of(first) + self.tickets_of(second))
        self.assertEqual(self.run_lock("release", "--force"), 0)
        self.finish(first)
        self.finish(second)
        self.assertEqual(self.order(), ["first", "second"])
        self.assertFalse(self.lock.exists())
        self.assertEqual(list(self.queue.glob("*.json")), [])

    def test_a_free_lock_waits_for_an_older_live_ticket(self) -> None:
        # This test process is alive, so its ticket keeps its place.
        ours = census_lock.enqueue(self.lock, ["census"])
        waiter = self.start_waiter("waiter")
        self.wait_for(lambda: self.tickets_of(waiter), "the waiter's ticket")
        time.sleep(0.5)
        self.assertIsNone(waiter.poll())
        self.assertFalse(self.lock.exists())
        ours.unlink()
        self.finish(waiter)
        self.assertEqual(self.order(), ["waiter"])

    def test_run_without_wait_never_passes_a_queued_waiter(self) -> None:
        census_lock.enqueue(self.lock, ["census"])
        with self.assertRaisesRegex(SystemExit, "1 waiter"):
            self.run_lock("run", "--", sys.executable, "-c", "pass")
        self.assertFalse(self.lock.exists())

    def test_a_crashed_waiters_ticket_is_dropped(self) -> None:
        self.assertTrue(census_lock.try_acquire(self.lock, ["census"]))
        crashed = self.start_waiter("crashed")
        self.wait_for(lambda: self.tickets_of(crashed), "the crashed waiter's ticket")
        crashed.kill()
        crashed.communicate()
        # Killed, it never removed its ticket.
        self.assertEqual(len(list(self.queue.glob("*.json"))), 1)
        self.assertEqual(self.run_lock("release", "--force"), 0)
        self.assertEqual(self.run_lock("run", "--", sys.executable, "-c", "pass"), 0)
        self.assertEqual(list(self.queue.glob("*.json")), [])

    def test_a_dead_waiters_older_ticket_does_not_block_a_waiter(self) -> None:
        self.write_ticket("00000000000000000001-dead.json", DEAD_PID)
        waiter = self.start_waiter("waiter")
        self.finish(waiter)
        self.assertEqual(self.order(), ["waiter"])
        self.assertEqual(list(self.queue.glob("*.json")), [])

    def test_a_ticket_from_the_other_system_keeps_its_place(self) -> None:
        self.write_ticket("00000000000000000001-elsewhere.json", DEAD_PID, "elsewhere")
        with self.assertRaisesRegex(SystemExit, "1 waiter"):
            self.run_lock("run", "--", sys.executable, "-c", "pass")

    def test_forced_release_of_a_ticket_removes_only_that_ticket(self) -> None:
        other = self.write_ticket("00000000000000000001-elsewhere.json", DEAD_PID, "elsewhere")
        ours = census_lock.enqueue(self.lock, ["census"])
        self.assertTrue(census_lock.try_acquire(self.lock, ["census"]))
        self.assertEqual(self.run_lock("release", "--force", "--ticket", other.name), 0)
        self.assertFalse(other.exists())
        self.assertTrue(ours.exists())
        self.assertTrue(self.lock.exists())
        with self.assertRaisesRegex(SystemExit, "no ticket"):
            self.run_lock("release", "--force", "--ticket", other.name)

    def test_status_lists_the_queue_in_order(self) -> None:
        self.write_ticket("00000000000000000002-dead.json", DEAD_PID)
        self.write_ticket("00000000000000000001-elsewhere.json", DEAD_PID, "elsewhere")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(self.run_lock("status"), 0)
        lines = output.getvalue().splitlines()
        self.assertTrue(lines[0].startswith("free"))
        self.assertIn("queued 1: 00000000000000000001-elsewhere.json", lines[1])
        self.assertNotIn("stale", lines[1])
        self.assertIn("queued 2: 00000000000000000002-dead.json", lines[2])
        self.assertIn("stale", lines[2])


class CensusLockDiskTests(unittest.TestCase):
    """``run`` and ``disk`` apply the disk rule: 50 GB on each write
    directory's drive to start, 30 GB to keep running, and 20 GB on the
    shared ``out/`` drive throughout."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        root = Path(self.temporary.name)
        self.lock = root / "out" / "census.lock"
        self.lock.parent.mkdir(parents=True)
        self.write = root / "lane"
        self.marker = root / "ran.txt"
        # Free space in GB by directory, changed by the tests while a run goes.
        self.free = {self.lock.parent: 100.0, self.write: 100.0}
        patches = [
            mock.patch.dict(os.environ, {key: "" for key in _DISK_FLOORS_OFF}),
            mock.patch.object(census_lock, "free_gb", lambda path: self.free[path]),
            mock.patch.object(census_lock, "DISK_POLL_SECONDS", 0.05),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        for key in _DISK_FLOORS_OFF:
            os.environ.pop(key)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_lock(self, *argv: str) -> int:
        with contextlib.redirect_stderr(io.StringIO()) as self.stderr:
            return census_lock.main(["--lock", str(self.lock), *argv])

    def touch_command(self) -> list[str]:
        return ["--", sys.executable, "-c", f"open({str(self.marker)!r}, 'w').write('ran')"]

    def assert_refused(self, status: int) -> None:
        self.assertEqual(status, census_lock.DISK_EXIT)
        self.assertFalse(self.marker.exists())
        self.assertFalse(self.lock.exists())
        self.assertEqual(list(census_lock.queue_dir(self.lock).glob("*.json")), [])

    def test_a_write_dir_under_50_gb_is_refused_before_queueing(self) -> None:
        self.free[self.write] = 49.0
        self.assert_refused(self.run_lock("run", "--wait", "--write-dir", str(self.write), *self.touch_command()))
        self.assertIn("under 50 GB", self.stderr.getvalue())

    def test_a_shared_out_drive_under_20_gb_is_refused(self) -> None:
        self.free[self.lock.parent] = 19.0
        self.assert_refused(self.run_lock("run", "--write-dir", str(self.write), *self.touch_command()))
        self.assert_refused(self.run_lock("run", *self.touch_command()))

    def test_enough_space_runs_the_command(self) -> None:
        self.free[self.write] = 50.0
        self.free[self.lock.parent] = 20.0
        self.assertEqual(self.run_lock("run", "--write-dir", str(self.write), *self.touch_command()), 0)
        self.assertTrue(self.marker.exists())
        self.assertFalse(self.lock.exists())

    def test_a_running_command_continues_between_30_and_50_gb(self) -> None:
        def shrink(path: Path) -> float:
            if path == self.write and self.marker.exists():
                return 31.0
            return self.free[path]

        census = f"import time; open({str(self.marker)!r}, 'w').write('ran'); time.sleep(0.5)"
        with mock.patch.object(census_lock, "free_gb", shrink):
            status = self.run_lock("run", "--write-dir", str(self.write), "--", sys.executable, "-c", census)
        self.assertEqual(status, 0)

    def stop_case(self, low: Path, free: float) -> None:
        pids = Path(self.temporary.name) / "pids.txt"
        # The command starts a child process; both must be stopped.
        census = (
            "import os, subprocess, sys, time\n"
            "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])\n"
            f"open({str(pids)!r}, 'w').write(f'{{os.getpid()}} {{child.pid}}')\n"
            "time.sleep(60)\n"
        )

        def shrink(path: Path) -> float:
            if path == low and pids.exists() and pids.read_text().strip():
                return free
            return self.free[path]

        started = time.monotonic()
        with mock.patch.object(census_lock, "free_gb", shrink):
            status = self.run_lock("run", "--write-dir", str(self.write), "--", sys.executable, "-c", census)
        self.assertEqual(status, census_lock.DISK_EXIT)
        self.assertLess(time.monotonic() - started, 50)
        self.assertIn("were stopped", self.stderr.getvalue())
        self.assertFalse(self.lock.exists())
        for pid in map(int, pids.read_text().split()):
            deadline = time.monotonic() + 10
            while census_lock.pid_alive(pid) and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertFalse(census_lock.pid_alive(pid), f"process {pid} still runs")

    def test_a_write_dir_under_30_gb_stops_the_command_and_its_children(self) -> None:
        self.stop_case(self.write, 29.0)

    def test_a_shared_out_drive_under_20_gb_stops_the_command(self) -> None:
        self.stop_case(self.lock.parent, 19.0)

    def test_disk_reports_every_drive_without_taking_the_lock(self) -> None:
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(self.run_lock("disk", "--write-dir", str(self.write)), 0)
        text = output.getvalue()
        self.assertIn(f"write dir {self.write}: 100.0 GB free", text)
        self.assertIn(f"shared out {self.lock.parent}: 100.0 GB free", text)
        self.assertIn("ok to start", text)
        self.free[self.write] = 10.0
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(self.run_lock("disk", "--write-dir", str(self.write)), census_lock.DISK_EXIT)
        self.assertIn("refused", output.getvalue())
        self.assertFalse(self.lock.exists())

    def test_free_space_of_a_missing_directory_is_its_parents(self) -> None:
        missing = Path(self.temporary.name) / "not" / "yet"
        free = census_lock.shutil.disk_usage(self.temporary.name).free / census_lock.GIB
        self.assertAlmostEqual(REAL_FREE_GB(missing), free, delta=1.0)


if __name__ == "__main__":
    unittest.main()
