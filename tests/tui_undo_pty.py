"""Undo/redo through real terminal frames, with isolated DB and separate stdout."""
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

from terminal_screen import TerminalScreen


binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix="qqq-undo-test-") as directory:
    env = dict(os.environ, HOME=directory, TERM="xterm-256color")
    for key in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID", "NO_COLOR"):
        env.pop(key, None)
    if scenario.endswith("no_color"):
        env["NO_COLOR"] = "1"

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=directory, env=env,
                                capture_output=True, timeout=8)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    cli("add", "Seed")
    cli("add", "Other")
    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
    original_terminal = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    child = subprocess.Popen([binary, "tui"], cwd=directory, env=env,
                             stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    visible = TerminalScreen(72, 24)
    screen = bytearray()

    def capture():
        if select.select([master], [], [], 0.05)[0]:
            data = os.read(master, 65536)
            screen.extend(data)
            visible.feed(data)

    def wait(predicate):
        deadline = time.monotonic() + 8
        while not predicate():
            assert time.monotonic() < deadline, f"Frame stalled: {screen[-600:]!r}\n{visible.text()}"
            assert child.poll() is None, f"Editor exited: {screen[-1500:]!r}"
            capture()

    def send(data):
        screen.clear()
        remaining = memoryview(data)
        deadline = time.monotonic() + 8
        while remaining:
            assert time.monotonic() < deadline, "Input timed out"
            readable, writable, _ = select.select([master], [master], [], 0.05)
            if readable:
                capture()
            if writable:
                remaining = remaining[os.write(master, remaining):]

    def paste(value):
        send(b"\x1b[200~" + value.encode() + b"\x1b[201~")

    def editor_row():
        return next((index for index, line in enumerate(visible.text().splitlines())
                     if line.startswith("Task Editor")), None)

    def editor_line():
        row = editor_row()
        return visible.text().splitlines()[row + 1].rstrip() if row is not None else None

    def wait_editor(text, column):
        wait(lambda: editor_line() == text and visible.cursor_visible
             and not visible.pending and not visible.decoder.getstate()[0]
             and (visible.x, visible.y) == (column, editor_row() + 1)
             and screen.endswith(f"\x1b[{editor_row() + 2};{column + 1}H".encode()))

    try:
        wait(lambda: "Task Editor" in visible.text())
        send(b"\x0b1\r")
        wait_editor("Seed", 4)
        before = cli("show", "1")
        paste(" changed")
        wait_editor("Seed changed", 12)
        send(b"\x1a")
        wait_editor("Seed", 4)
        assert cli("show", "1") == before
        assert "[*]" not in visible.text(), visible.text()
        send(b"\x19")
        wait_editor("Seed changed", 12)
        assert cli("show", "1") == before
        assert "[*]" in visible.text(), visible.text()
        send(b"\x13")
        wait(lambda: visible.text().splitlines()[-1].startswith("Saved #1"))
        assert cli("show", "1")["task"]["description"] == "Seed changed"
        saved = cli("show", "1")
        send(b"\x1a")
        wait_editor("Seed", 4)
        assert cli("show", "1") == saved
        assert "[*]" in visible.text(), visible.text()
        send(b"\x19")
        wait_editor("Seed changed", 12)
        assert cli("show", "1") == saved
        assert "[*]" not in visible.text(), visible.text()
        send(b"\x03")
        wait_editor("", 0)
        send(b"\x03")
        stdout, _ = child.communicate(timeout=8)
        assert child.returncode == 0, (child.returncode, stdout)
        assert stdout == b"", stdout
        assert termios.tcgetattr(slave) == original_terminal
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
        os.close(slave)
