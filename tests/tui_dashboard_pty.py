"""Exercise two-panel qqq tui through a real terminal."""

import fcntl
import json
import os
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import time


binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix="qqq-dashboard-test-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    for name in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(name, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env,
                                capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    cli("add", "First")
    cli("add", "Second")
    if scenario == "scroll":
        for index in range(3, 21):
            cli("add", f"Task {index}")

    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
    before = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    args = [binary, "--json", "tui"] if scenario in ("empty_json", "save_json") else [binary, "tui"]
    child = subprocess.Popen(args, cwd=folder, env=env, stdin=slave,
                             stderr=slave, stdout=subprocess.PIPE)
    screen = bytearray()

    def read_until(needle):
        deadline = time.monotonic() + 5
        while needle not in screen:
            assert time.monotonic() < deadline, f"Missing {needle!r}: {screen[-2000:]!r}"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    screen.extend(os.read(master, 65536))
                except OSError as error:
                    raise AssertionError(f"Editor exited before {needle!r}") from error
            assert child.poll() is None, f"Editor exited before {needle!r}: {screen[-2000:]!r}"

    def send(data):
        remaining = memoryview(data)
        deadline = time.monotonic() + 5
        while remaining:
            assert time.monotonic() < deadline, "Terminal input timed out"
            readable, writable, _ = select.select([master], [master], [], 0.05)
            if readable:
                screen.extend(os.read(master, 65536))
            if writable:
                remaining = remaining[os.write(master, remaining):]

    try:
        read_until(b"qqq task editor - new task")
        assert b"qqq tasks" in screen, screen[-2000:]
        if scenario != "scroll":
            assert b"First" in screen and b"Second" in screen, screen[-2000:]
        assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout while open"
        if scenario in ("save", "save_json"):
            screen.clear()
            send(b"\x1b[1;2A")
            read_until(b"task #2")
            assert b"> 2" in screen, screen[-2000:]
            screen.clear()
            send(b"\x05 edited\x13")
            read_until(b"Saved #2. New task")
            assert cli("show", "2")["task"]["description"] == "Second edited"
            screen.clear()
            send(b"Third\x13")
            read_until(b"Saved #3. New task")
            screen.clear()
            send(b"\x1b[C")
            read_until(b"Third")
            assert b"3      New" in screen, screen[-2000:]
            assert cli("show", "3")["task"]["description"] == "Third"
        elif scenario == "scroll":
            assert b"Task 20" in screen, screen[-2000:]
            for task_id in range(20, 0, -1):
                screen.clear()
                send(b"\x1b[1;2A")
                read_until(f"qqq task editor - task #{task_id}".encode())
            assert b"> 1" in screen and b"First" in screen, screen[-2000:]
            screen.clear()
            send(b"\x1b[1;2B")
            read_until(b"qqq task editor - task #2")
            assert b"> 2" in screen, screen[-2000:]
        elif scenario == "dirty":
            send(b"Draft")
            screen.clear()
            send(b"\x1b[1;2A")
            read_until(b"Discard changes and switch? (y/N)")
            screen.clear()
            send(b"n")
            read_until(b"qqq task editor - new task")
            read_until(b"Draft")
            screen.clear()
            send(b"\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "Draft"
        elif scenario == "save_error":
            screen.clear()
            send(b"\x1b[1;2A")
            read_until(b"qqq task editor - task #2")
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            screen.clear()
            send(b"\x05 edited\x13")
            read_until(b"Task 2 not found")
            assert child.poll() is None
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("INSERT INTO tasks(id, description) VALUES (2, 'Second')")
            screen.clear()
            send(b"\x13")
            read_until(b"Saved #2. New task")
            assert cli("show", "2")["task"]["description"] == "Second edited"
        elif scenario == "resize":
            send(b"Draft")
            screen.clear()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 7, 10, 0, 0))
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Resize ter")
            screen.clear()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"qqq task editor - new task")
            read_until(b"Draft")
            read_until(b"\x1b[10;6H")
            screen.clear()
            send(b"\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "Draft"
        send(b"\x03" if scenario == "scroll" else b"\x1b")
        deadline = time.monotonic() + 5
        while child.poll() is None:
            assert time.monotonic() < deadline, "TUI failed to exit"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    screen.extend(os.read(master, 65536))
                except OSError:
                    pass
        stdout, _ = child.communicate(timeout=5)
        assert child.returncode == 0, screen[-2000:]
        assert stdout == b"", stdout
        assert before[3] == termios.tcgetattr(slave)[3], "Terminal flags not restored"
        assert b"\x1b[?1049l" in screen, "Alternate screen not restored"
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
