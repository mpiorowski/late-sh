"""GPL-2.0-or-later. Added 2026-10-09: real-terminal door persistence tests.

Run: uv run --no-project vendor/frotz/src/curses/ux_door_test.py
Requires the door-capable curses build and tmux. Uses a private tmux server.
"""
import json
import os
from pathlib import Path
import shutil
import shlex
import struct
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[4]
BIN = Path(os.environ.get("LATE_ZORK_TEST_BIN", ROOT / "vendor/frotz/frotz"))
STORIES = Path(os.environ.get("LATE_ZORK_TEST_STORIES", ROOT / "assets/zork"))


class DoorTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="late-zork-test-")
        self.data = Path(self.tmp.name)
        self.socket = self.data / "tmux.sock"
        self.live = False

    def tearDown(self):
        self.tmux("kill-server", check=False)
        self.tmp.cleanup()

    def tmux(self, *args, check=True):
        return subprocess.run(
            ["tmux", "-S", str(self.socket), *map(str, args)],
            check=check, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        ).stdout

    def launch(self, edition=1, mode="new", cols=100, rows=30, story=None, directory=None, prompt=">"):
        if self.live:
            self.tmux("kill-session", "-t", "game")
        directory = directory or self.data / f"zork{edition}"
        directory.mkdir(exist_ok=True)
        self.story = story or STORIES / f"zork{edition}.z3"
        self.directory = directory
        self.tmux("new-session", "-d", "-s", "game", "-x", cols, "-y", rows,
                  "-c", directory, "-e", f"LATE_FROTZ_DOOR={mode}",
                  "-e", "TERM=xterm-256color", "-e", "LC_ALL=en_US.UTF-8" if sys.platform == "darwin" else "LC_ALL=C.UTF-8",
                  "-e", "TMUX_TMPDIR=" , "sleep", "30")
        self.tmux("set-option", "-t", "game", "remain-on-exit", "on")
        self.tmux("set-option", "-t", "game", "window-size", "manual")
        # Capture the actual child status. Under x86 emulation tmux 3.3a can
        # report a dead direct-exec pane without a pane_dead_status value.
        self.exit_file = self.data / "child-exit"
        self.pid_file = self.data / "child-pid"
        self.exit_file.unlink(missing_ok=True)
        command = shlex.join([str(BIN), "-d", "-s", "1234", str(self.story)])
        wrapper = (f"{command} < /dev/tty & child=$!; printf '%s' \"$child\" > {shlex.quote(str(self.pid_file))}; "
                   f"wait \"$child\"; result=$?; printf '%s' \"$result\" > {shlex.quote(str(self.exit_file))}; exit \"$result\"")
        self.tmux("respawn-pane", "-k", "-t", "game", "/bin/sh", "-c", wrapper)
        self.live = True
        self.wait(lambda: self.screen().rstrip().endswith(prompt) or self.exited() is not None)

    def screen(self, styles=False):
        return self.tmux("capture-pane", "-t", "game", "-p", *( ["-e"] if styles else []))

    def exited(self):
        if self.exit_file.is_file():
            value = self.exit_file.read_text()
            return int(value) if value else None
        return None

    def wait(self, condition, timeout=4):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if condition():
                return
            time.sleep(.03)
        self.fail("Timed out\n" + self.screen())

    def keys(self, *keys):
        self.tmux("send-keys", "-t", "game", *keys)

    def command(self, command, expect=None):
        before = self.screen()
        self.tmux("send-keys", "-t", "game", "-l", command)
        self.keys("Enter")
        self.wait(lambda: self.screen() != before and (
            expect in self.screen() if expect else self.screen().rstrip().endswith(">")))

    def inspect(self, manual=False, story=None):
        result = subprocess.run([str(BIN), str(story or self.story)], cwd=self.directory,
                                env={**os.environ, "LATE_FROTZ_DOOR": "inspect-manual" if manual else "inspect-auto"},
                                capture_output=True, text=True, check=True)
        return json.loads(result.stdout)

    def save(self, description="checkpoint"):
        self.command("save", "One manual slot")
        self.tmux("send-keys", "-t", "game", "-l", description)
        self.keys("Enter")
        self.wait(lambda: self.screen().rstrip().endswith(">"))
        self.assertEqual(self.inspect(True)["description"], description)

    def return_menu(self):
        self.command("RESTORE ignored", "Return to this Zork's menu?")
        self.keys("y")
        # tmux can mark a pane dead before publishing its exit status.
        self.wait(lambda: self.exited() == 20)
        self.assertEqual(self.exited(), 20)

    def test_trilogy_switch_and_screen_fidelity(self):
        expected = {}
        for edition in (1, 2, 3):
            self.launch(edition)
            self.command("look")
            self.command("inventory")
            self.save(f"../RESTORE zork{edition}")
            expected[edition] = self.screen(styles=True)
            self.return_menu()
        for edition in (1, 2, 3):
            self.launch(edition, "auto")
            self.assertEqual(self.exited(), None)
            self.assertEqual(self.screen(styles=True), expected[edition])
            self.command("look")
            self.assertEqual(set(p.name for p in self.directory.iterdir()), {"autosave.lz", "manual.lz"})
            self.return_menu()

    def test_restore_cancellation_preserves_state_and_prompt(self):
        self.launch()
        before = (self.directory / "autosave.lz").read_bytes()
        for answer in ("n", "Enter", "Escape"):
            self.command("  ReStOrE anything", "Return to this Zork's menu?")
            self.keys(answer)
            self.wait(lambda: "Return to this Zork's menu?" not in self.screen())
            self.assertIn("Moves: 0", self.screen())
            self.assertNotIn("Failed.", self.screen())
            self.assertEqual((self.directory / "autosave.lz").read_bytes(), before)
        self.command("north", "Moves: 1")

    def test_manual_restore_new_and_cancel(self):
        self.launch()
        self.command("north", "Moves: 1")
        self.save("deliberate fallback")
        manual = (self.directory / "manual.lz").read_bytes()
        self.command("east", "Moves: 2")
        self.return_menu()
        self.launch(mode="manual")
        self.assertIn("Moves: 1", self.screen())
        self.command("look")
        self.return_menu()
        self.launch(mode="new")
        self.assertIn("Moves: 0", self.screen())
        self.assertEqual((self.directory / "manual.lz").read_bytes(), manual)
        self.command("save", "One manual slot")
        self.keys("Escape")
        self.wait(lambda: self.screen().rstrip().endswith(">"))
        self.assertEqual((self.directory / "manual.lz").read_bytes(), manual)

    def test_unicode_description_limit_and_empty(self):
        self.launch()
        description = "界🗝" * 64
        self.save(description)
        self.assertEqual(len(self.inspect(True)["description"]), 128)
        self.save("")

    def test_confirmation_continuation_and_resize(self):
        self.launch()
        self.command("quit", "Do you wish to leave the game?")
        self.wait(lambda: (self.directory / "autosave.lz").is_file())
        # A game-owned question is itself a READ continuation, not a command loop.
        self.return_menu()
        self.launch(mode="auto", cols=80, rows=24)
        self.assertIn("Do you wish to leave the game?", self.screen())
        self.keys("n", "Enter")
        self.wait(lambda: self.screen().rstrip().endswith(">"))
        self.command("look")
        self.tmux("resize-window", "-t", "game", "-x", 110, "-y", 35)
        self.command("inventory")

    def test_death_states_and_terminal_ending(self):
        # These are real story routes, not a replay or a synthetic LOOK after
        # restore. A death can legitimately be the newest automatic progress.
        for edition in (1, 2, 3):
            self.launch(edition, rows=60)
            if edition == 1:
                route = ("north", "east", "open window", "west", "west", "take sword", "kill me with sword")
            elif edition == 2:
                route = ("take sword", "kill me with sword")
            else:
                route = ("south", "west") # The fixed RNG seed gives a grue here.
            for command in route:
                self.command(command)
            self.assertIn("You have died", self.screen())
            expected = self.screen(styles=True)
            self.return_menu()
            self.launch(edition, "auto", rows=60)
            self.assertEqual(self.screen(styles=True), expected)
            self.command("inventory")
            self.return_menu()
        # Zork III's fourth death ends the interpreter without another READ.
        # The last committed slot remains coherent and may be a losing state.
        self.launch(3, rows=60)
        # Repeatedly move between two dark rooms; death returns to the stair.
        for index in range(40):
            ending_command = ("south", "south", "north")[index % 3]
            before = self.screen()
            self.keys(ending_command, "Enter")
            self.wait(lambda: self.exited() is not None or (
                self.screen() != before and self.screen().rstrip().endswith(">")))
            if self.exited() is not None:
                break
        self.assertEqual(self.exited(), 0, self.screen())
        self.assertEqual(self.inspect()["status"], "ready")
        self.launch(3, "auto", rows=60)
        self.assertIn("pitch black", self.screen())
        self.keys(ending_command, "Enter")
        self.wait(lambda: self.exited() is not None)
        self.assertEqual(self.exited(), 0)

    def test_invalid_slots_and_failed_write_preserve_previous(self):
        self.launch()
        self.save()
        auto = self.directory / "autosave.lz"
        manual = self.directory / "manual.lz"
        old_auto, old_manual = auto.read_bytes(), manual.read_bytes()
        # Make replacement fail even when the test runs as root.
        auto.unlink()
        auto.mkdir()
        self.command("north", "automatic save failed")
        self.assertEqual(manual.read_bytes(), old_manual)
        self.command("restore", "Return to this Zork's menu?")
        self.keys("y")
        self.wait(lambda: "Cannot return" in self.screen())
        self.assertIsNone(self.exited())
        auto.rmdir()
        auto.write_bytes(old_auto)
        self.return_menu()
        good = auto.read_bytes()
        auto.write_bytes(good[:70])
        self.assertEqual(self.inspect()["status"], "invalid")
        self.launch(mode="auto")
        self.assertEqual(self.exited(), 21)
        auto.write_bytes(good)
        self.assertEqual(self.inspect(story=STORIES / "zork2.z3")["status"], "invalid")
        corrupt = bytearray(good)
        corrupt[-1] ^= 1
        auto.write_bytes(corrupt)
        self.assertEqual(self.inspect()["status"], "invalid")

    def test_unwritable_directory_preserves_both_slots(self):
        if os.geteuid() == 0:
            self.skipTest("run as the non-root door user to exercise directory permissions")
        self.launch()
        self.save("original")
        before = {p.name: p.read_bytes() for p in self.directory.iterdir()}
        self.directory.chmod(0o500)
        try:
            self.command("save", "One manual slot")
            self.keys("replacement", "Enter")
            self.wait(lambda: "Manual save failed" in self.screen())
            self.assertEqual({p.name: p.read_bytes() for p in self.directory.iterdir()}, before)
        finally:
            self.directory.chmod(0o700)

    def test_rng_continuation_and_abrupt_shutdown(self):
        self.launch()
        self.command("north")
        checkpoint = (self.directory / "autosave.lz").read_bytes()
        commands = ("north", "east", "south", "west", "look", "inventory")
        for command in commands:
            self.command(command)
        expected = (self.directory / "autosave.lz").read_bytes()[52:]
        pid = int(self.pid_file.read_text())
        os.kill(pid, 9)
        self.wait(lambda: self.exited() is not None)
        (self.directory / "autosave.lz").write_bytes(checkpoint)
        self.launch(mode="auto")
        for command in commands:
            self.command(command)
        self.assertEqual((self.directory / "autosave.lz").read_bytes()[52:], expected)

    def test_decoded_stack_operands_resume_once(self):
        # A tiny V3 story READs two operands from stack variable zero. Once READ
        # has decoded them, the stack is empty: decoding a second time is wrong.
        story = bytearray(4096)
        story[0] = 3
        for offset, value in ((2, 1), (6, 0x600), (8, 0x300), (10, 0x200),
                              (12, 0x400), (14, 0x800), (24, 0x100), (26, 2048)):
            struct.pack_into(">H", story, offset, value)
        story[18:24] = b"261009"
        struct.pack_into(">H", story, 0x400, 1)
        struct.pack_into(">H", story, 0x200 + 62 + 7, 0x2c0)
        story[0x2c0] = 1
        struct.pack_into(">H", story, 0x2c1, 0x8000 | (7 << 10) | (20 << 5) | 29)
        story[0x500] = 32
        code = bytes.fromhex("e57f3e e87f00 e83f0500 e4af0000 e57f2a ba")
        story[0x600:0x600 + len(code)] = code
        struct.pack_into(">H", story, 28, sum(story[64:]) & 65535)
        path = self.data / "stack.z3"
        path.write_bytes(story)
        self.launch(story=path)
        self.return_menu()
        self.launch(mode="auto", story=path)
        # This fixture echoes/ends for every line. A non-exact RESTORE token
        # must reach it (the production V3 dictionaries themselves truncate
        # words, independently of the door's first-token rule).
        self.keys("RESTOREfoo", "Enter")
        self.wait(lambda: self.exited() is not None)
        self.assertEqual(self.exited(), 0)


if __name__ == "__main__":
    if not BIN.exists():
        raise SystemExit("Build first: make -C vendor/frotz curses SOUND_TYPE=none ZORK_DOOR=1")
    if not shutil.which("tmux"):
        raise SystemExit("tmux is required for real-terminal regression tests")
    unittest.main(verbosity=2)
