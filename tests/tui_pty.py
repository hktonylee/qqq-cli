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
    env.pop("NO_COLOR", None)
    color = scenario not in ("no_color", "dumb")
    if scenario == "no_color":
        env["NO_COLOR"] = "1"
        scenario = "blank"
    elif scenario == "dumb":
        env["TERM"] = "dumb"
        scenario = "blank"
    for key in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID"):
        env.pop(key, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env, capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    args = ["add"]
    if scenario in ("edit", "escape_edit"):
        cli("add", "Original\n\nDetails")
        cli("next", "--local", "--session", "worker")
        args = ["edit", "-1"]
    image = Path(folder) / "test image.png"
    image.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="))
    if scenario in ("save", "cancel", "escape_discard", "escape_keep"):
        flagged = Path(folder) / "flag image.png"
        flagged.write_bytes(image.read_bytes())
        args.extend(["--image", str(flagged)])
    master, slave = pty.openpty()
    os.set_blocking(master, False)
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
        # Real terminals drain output while sending input. Keep redraw backpressure
        # from deadlocking a large paste against the PTY's small input buffer.
        remaining = memoryview(data)
        deadline = time.monotonic() + 5
        while remaining:
            assert time.monotonic() < deadline, "Terminal input timed out"
            readable, writable, _ = select.select([master], [master], [], 0.05)
            if readable:
                screen.extend(os.read(master, 65536))
            if writable:
                try:
                    remaining = remaining[os.write(master, remaining):]
                except BlockingIOError:
                    pass

    def paste(text):
        send(b"\x1b[200~" + text.encode() + b"\x1b[201~")

    try:
        read_until(b"Ctrl-S")
        assert b"Whole buffer =" not in screen, "Removed editor hint reappeared"
        if color:
            assert b"\x1b[48;5;236m" in screen, "Editor grey background missing"
            assert b"\x1b[38;5;252m" in screen, "Editor readable foreground missing"
        else:
            assert b"\x1b[48;" not in screen, "Plain editor set background color"
            assert b"\x1b[38;" not in screen, "Plain editor set foreground color"
        if scenario == "blank":
            send(b"\x13")
            read_until(b"Task description cannot be empty")
            send(b"Recovered")
        elif scenario in ("edit", "escape_edit"):
            if scenario == "escape_edit":
                send(b"\x1b")
                read_until(b"Discard draft?")
                assert child.poll() is None
                assert cli("show", "1")["task"]["description"] == "Original\n\nDetails"
                screen.clear()
                send(b"\x1b")
                read_until(b"Ctrl-S")
            cli("add", "Created while editing")
            paste(" amended")
        elif scenario == "escape_empty":
            pass
        elif scenario == "escape_deleted":
            send(b"Draft" + b"\x7f" * 5)
        elif scenario == "escape_whitespace":
            send(b" \r")
        elif scenario in ("escape_image", "escape_narrow"):
            paste("'" + str(image) + "'")
            read_until(b"[Image #1:")
        else:
            send(b"Title\r\r")
            paste("\u754c" * 1001)
            read_until(b"1001 chars")
            send(b"\r")
            paste("'" + str(image) + "'")
            read_until(b"[Image #1:")
        cancelled = scenario in ("cancel", "escape_discard", "escape_image", "escape_whitespace", "escape_narrow", "escape_empty", "escape_deleted")
        if scenario in ("escape_discard", "escape_image", "escape_whitespace", "escape_narrow"):
            send(b"\x1b")
            read_until(b"Discard draft?")
            assert child.poll() is None
            assert cli("list") == []
            if scenario == "escape_narrow":
                screen.clear()
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 4, 12, 0, 0))
                child.send_signal(signal.SIGWINCH)
                read_until(b"Discard? y/N")
            send(b"Y" if scenario == "escape_discard" else b"y")
        elif scenario == "escape_keep":
            for choice in (b"n", b"\r", b"\x1b"):
                screen.clear()
                send(b"\x1b")
                read_until(b"Discard draft?")
                assert child.poll() is None
                assert cli("list") == []
                paste("ignored while confirming")
                send(b"z\x13")
                screen.clear()
                send(choice)
                read_until(b"Ctrl-S")
            send(b"\x13")
        elif scenario in ("escape_empty", "escape_deleted"):
            send(b"\x1b")
        else:
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
        if color:
            assert screen.rfind(b"\x1b[0m") < screen.rfind(b"\x1b[?1049l"), "Colors not reset before leaving alternate screen"
            assert b"\x1b[0m" in screen, "Colors not reset on exit"
        else:
            assert b"\x1b[48;" not in screen and b"\x1b[38;" not in screen
        if cancelled:
            assert child.returncode == 1
            assert stdout == b""
            assert cli("list") == []
            if scenario in ("escape_empty", "escape_deleted"):
                assert b"Discard" not in screen
        else:
            assert child.returncode == 0, screen[-2000:]
            task = json.loads(stdout)
            if scenario in ("save", "escape_keep"):
                assert "title" not in task
                assert task["description"] == "Title\n\n" + "\u754c" * 1001 + "\n[Image: test image.png]"
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
                assert task["description"] == "Recovered"
            else:
                assert task["id"] == 1
                assert task["description"] == "Original\n\nDetails amended"
                assert task["harness_session"] == "worker"
                assert task["status"] == "in_progress"
                assert cli("show", "2")["task"]["description"] == "Created while editing"
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
