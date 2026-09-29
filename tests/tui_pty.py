"""Real PTY checks: terminal input/stderr, separate JSON stdout, isolated HOME/DB."""
import base64
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

binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix="qqq-tui-test-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    for key in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID"):
        env.pop(key, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env, capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    args = ["add"]
    if scenario == "edit":
        cli("add", "Original", "-d", "Details")
        cli("next", "--local", "--session", "worker")
        args = ["edit", "-1"]
    image = Path(folder) / "test image.png"
    image.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="))
    if scenario in ("save", "cancel"):
        flagged = Path(folder) / "flag image.png"
        flagged.write_bytes(image.read_bytes())
        args.extend(["--image", str(flagged)])
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 60, 0, 0))
    before = termios.tcgetattr(slave)

    # Keep session leader alive through terminal-restoration assertions. macOS
    # revokes a controlling PTY when its session leader exits.
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    child = subprocess.Popen([binary, "--json", *args], cwd=folder, env=env,
        stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    screen = bytearray()

    def read_until(needle, timeout=5):
        deadline = time.monotonic() + timeout
        while needle not in screen:
            if time.monotonic() > deadline:
                raise AssertionError(f"Missing {needle!r}: {screen[-2000:]!r}")
            if select.select([master], [], [], 0.05)[0]:
                try:
                    screen.extend(os.read(master, 65536))
                except OSError:
                    raise AssertionError(f"Editor exited early: {screen!r}")
            if child.poll() is not None and needle not in screen:
                raise AssertionError(f"Editor exited early: {screen!r}")

    def send(data):
        os.write(master, data)

    def paste(text):
        send(b"\x1b[200~" + text.encode() + b"\x1b[201~")

    try:
        read_until(b"Ctrl-S")
        if scenario == "blank":
            send(b"\x13")
            read_until(b"Task title cannot be empty")
            send(b"Recovered")
        elif scenario == "edit":
            cli("add", "Created while editing")
            paste(" amended")
        else:
            send(b"Title\r\r")
            paste("\u754c" * 1001)
            read_until(b"1001 chars")
            send(b"\r")
            paste("'" + str(image) + "'")
            read_until(b"[Image #1:")
        send(b"\x03" if scenario == "cancel" else b"\x13")
        # Keep draining terminal redraws while process exits; a PTY has a small
        # output buffer and can block editor writes before save reaches stdout.
        deadline = time.monotonic() + 5
        pending_stdout = bytearray()
        while child.poll() is None:
            assert time.monotonic() < deadline, f"Save/cancel timed out: {screen[-2000:]!r}"
            readable, _, _ = select.select([master, child.stdout], [], [], 0.05)
            for stream in readable:
                if stream == master:
                    screen.extend(os.read(master, 65536))
                else:
                    pending_stdout.extend(os.read(child.stdout.fileno(), 65536))
        stdout, _ = child.communicate(timeout=5)
        stdout = bytes(pending_stdout) + stdout
        read_until(b"\x1b[?2004l")
        after = termios.tcgetattr(slave)
        assert before[3] == after[3], "Terminal flags not restored"
        assert b"\x1b[?1049l" in screen, "Alternate screen not restored"
        if scenario == "cancel":
            assert child.returncode == 1
            assert stdout == b""
            assert cli("list") == []
        else:
            assert child.returncode == 0, screen[-2000:]
            task = json.loads(stdout)
            if scenario == "save":
                assert task["title"] == "Title"
                assert task["description"] == "\u754c" * 1001 + "\n[Image: test image.png]"
                attachments = cli("show", "1")["images"]
                assert [item["name"] for item in attachments] == ["test image.png", "flag image.png"]
                exported = Path(folder) / "export.png"
                cli("show", "1", "--export-image", "1", "--output", str(exported))
                assert exported.read_bytes() == image.read_bytes()
                flagged.unlink()
                flagged_export = Path(folder) / "flag export.png"
                cli("show", "1", "--export-image", "2", "--output", str(flagged_export))
                assert flagged_export.read_bytes() == image.read_bytes()
            elif scenario == "blank":
                assert task["title"] == "Recovered"
            else:
                assert task["id"] == 1
                assert task["description"] == "Details amended"
                assert task["assignee"] == "worker"
                assert task["status"] == "in_progress"
                assert cli("show", "2")["task"]["title"] == "Created while editing"
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
