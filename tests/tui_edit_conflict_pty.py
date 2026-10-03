"""Guard single-task builtin saves while preserving local draft and terminal state."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

# Reuse cell emulator without executing dashboard scenario setup.
namespace = {}
source = Path(__file__).with_name("tui_dashboard_pty.py").read_text()
exec(source.split("binary, scenario = sys.argv[1:]")[0], namespace)
TerminalScreen = namespace["TerminalScreen"]

binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    env = dict(os.environ, HOME=directory, TERM="xterm-256color")
    for key in ("QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID", "EDITOR"):
        env.pop(key, None)
    env["NO_COLOR"] = "1"

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=root, env=env, capture_output=True, check=True)
        return json.loads(result.stdout)

    cli("init")
    original = "Original text\n\nFull original details"
    cli("add", original)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
    before = termios.tcgetattr(slave)

    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)

    child = subprocess.Popen([binary, "--json", "edit", "1"], cwd=root, env=env,
                             stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    visible = TerminalScreen(72, 24)
    output = bytearray()

    def wait(predicate):
        deadline = time.monotonic() + 5
        while not predicate():
            assert time.monotonic() < deadline, f"cursor={visible.x},{visible.y}\n{visible.text()}"
            if select.select([master], [], [], 0.05)[0]:
                data = os.read(master, 65536)
                output.extend(data)
                visible.feed(data)
            assert child.poll() is None, visible.text()

    def send(data):
        os.write(master, data)

    try:
        wait(lambda: "Task Editor - Task #1" in visible.text() and "Full original details" in visible.text())
        send(b" local")
        wait(lambda: "Full original details local" in visible.text()
             and (visible.x, visible.y) == (len("Full original details local"), 3))
        caret = (visible.x, visible.y)
        if scenario == "removed":
            cli("archive", "1")
            cli("delete", "1", "--yes")
        else:
            newer = cli("edit", "1", "-d", "Newer DB text\nComplete DB details")
        send(b"\x13")
        wait(lambda: "Content conflict #1" in visible.text() and "Current DB" in visible.text())
        if scenario == "reload_removed":
            send(b"r")
            wait(lambda: "Reload current DB text?" in visible.text())
            cli("archive", "1")
            cli("delete", "1", "--yes")
            send(b"y")
            wait(lambda: "Current DB: task removed" in visible.text() and "Content conflict #1" in visible.text()
                 and (visible.x, visible.y) == (13, 10))
            send(b"o")
            # Known deletion offers no overwrite confirmation.
            deadline = time.monotonic() + 0.2
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.02)[0]:
                    data = os.read(master, 65536)
                    output.extend(data)
                    visible.feed(data)
            assert "Overwrite DB text?" not in visible.text(), visible.text()
        send(b"\t")
        wait(lambda: "Local draft" in visible.text() and "Full original details local" in visible.text())
        send(b"\x1b")
        wait(lambda: "Content conflict #1" not in visible.text() and "Full original details local" in visible.text()
             and (visible.x, visible.y) == caret)
        if scenario in ("removed", "reload_removed"):
            assert cli("list") == []
            send(b"\x03")
        else:
            assert cli("show", "1")["task"] == newer
            send(b"\x13")
            wait(lambda: "Content conflict #1" in visible.text())
            send(b"o")
            wait(lambda: "Overwrite DB text?" in visible.text())
            send(b"y")
        deadline = time.monotonic() + 5
        while child.poll() is None:
            assert time.monotonic() < deadline, "Single editor failed to exit"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    output.extend(os.read(master, 65536))
                except OSError:
                    pass
        stdout, _ = child.communicate(timeout=5)
        if scenario in ("removed", "reload_removed"):
            assert child.returncode == 1 and stdout == b"", output
            assert cli("list") == []
        else:
            assert child.returncode == 0, output
            saved = json.loads(stdout)
            assert saved["description"] == original + " local"
            assert cli("show", "1")["task"] == saved
        assert before[3] == termios.tcgetattr(slave)[3], "Terminal flags not restored"
        assert b"\x1b[?1049l" in output and b"\x1b[?2004l" in output
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
