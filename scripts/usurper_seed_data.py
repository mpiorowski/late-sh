#!/usr/bin/env python3
"""Generate Usurper's seed DATA files by driving EDITOR.EXE's Reset Game.

Usurper ships no DATA files: the world data (MONSTER.DAT, NPCS.DAT, GUARDS.DAT,
LEVELS.DAT, OBJDAT*.DAT, ...) is created by the EDITOR's "Reset Game" button, a
FreeVision TUI with no command-line equivalent. This script runs the editor on a
PTY inside the Docker build stage and presses the exact key sequence:

    r  -> the ~R~eset Game button on the main dialog
    y  -> "Are you really sure?" confirm
    y  -> "Reset Usurper?" warning confirm

then waits for the reset to finish writing files and exits. The Dockerfile
asserts the vital files exist afterward (fail-closed), so a UI change in a
future Usurper version breaks the build instead of shipping an empty world.

Usage: usurper_seed_data.py <game-dir> <path-to-EDITOR.EXE>
"""

import os
import pty
import select
import signal
import struct
import sys
import time

import fcntl
import termios

# (seconds-from-start, bytes). The editor needs ~2s to draw its dialog; the
# reset itself (NPC generation is the slow part) finishes well inside the
# budget below on any build machine.
KEYS = [(3.0, b"r"), (6.0, b"y"), (9.0, b"y")]
BUDGET = 300.0
# The reset is done when the editor has been quiet this long after the last
# scripted key went in.
QUIET_AFTER_KEYS = 20.0


def main() -> int:
    game_dir, editor = sys.argv[1], sys.argv[2]

    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(game_dir)
        os.environ["TERM"] = "xterm"
        os.execv(editor, [editor])

    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 25, 80, 0, 0))

    start = time.time()
    sent = 0
    last_output = time.time()
    while time.time() - start < BUDGET:
        r, _, _ = select.select([fd], [], [], 0.2)
        if r:
            try:
                if not os.read(fd, 4096):
                    break
            except OSError:
                break
            last_output = time.time()
        now = time.time()
        if sent < len(KEYS) and now - start >= KEYS[sent][0]:
            os.write(fd, KEYS[sent][1])
            sent += 1
        if sent == len(KEYS) and now - last_output > QUIET_AFTER_KEYS:
            break

    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    os.close(fd)
    return 0


if __name__ == "__main__":
    sys.exit(main())
