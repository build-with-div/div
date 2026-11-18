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

from tools.qualification import census_lock


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


if __name__ == "__main__":
    unittest.main()
