"""Exercise two-panel qqq tui through a real terminal."""

import codecs
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


class TerminalScreen:
    """Track visible cells across Ratatui's partial-frame terminal updates."""

    def __init__(self, width, height):
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""
        self.resize(width, height)

    def resize(self, width, height):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0

    def feed(self, data):
        text = self.pending + self.decoder.decode(data)
        self.pending = ""
        index = 0
        while index < len(text):
            char = text[index]
            if char == "\x1b":
                if index + 1 >= len(text):
                    break
                if text[index + 1] != "[":
                    index += 2
                    continue
                end = index + 2
                while end < len(text) and not ("@" <= text[end] <= "~"):
                    end += 1
                if end == len(text):
                    break
                self.csi(text[index + 2:end], text[end])
                index = end + 1
                continue
            if char == "\r":
                self.x = 0
            elif char == "\n":
                self.y = min(self.height - 1, self.y + 1)
            elif char >= " ":
                # Task tree adds single-cell box drawing glyphs. Other wide
                # Unicode needs a fuller screen emulator.
                assert char.isascii() or char in "└├─│", f"Unsupported screen character {char!r}"
                if self.x >= self.width:
                    self.x = 0
                    self.y = min(self.height - 1, self.y + 1)
                self.cells[self.y][self.x] = char
                self.x += 1
            index += 1
        self.pending = text[index:]

    def csi(self, parameters, command):
        values = [int(part) if part.isdigit() else 0 for part in parameters.split(";")]
        if command in ("H", "f"):
            self.y = max(0, min(self.height - 1, (values[0] or 1) - 1))
            self.x = max(0, min(self.width - 1, (values[1] if len(values) > 1 else 1) - 1))
        elif command == "J" and values[0] == 2:
            self.cells = [[" "] * self.width for _ in range(self.height)]
        elif command == "K":
            self.cells[self.y][self.x:] = [" "] * (self.width - self.x)

    def text(self):
        return "\n".join("".join(row) for row in self.cells)


binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix="qqq-dashboard-test-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    env.pop("NO_COLOR", None)
    if scenario in ("no_color", "pasteboard_no_color", "filter_no_color"):
        env["NO_COLOR"] = "1"
    elif scenario == "dumb":
        env["TERM"] = "dumb"
    for name in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(name, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env,
                                capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    if scenario in ("filter", "filter_no_color"):
        cli("add", "Parent")
        cli("add", "Child\nNeEdLe on second line", "--parent", "1")
        cli("add", "Other")
    else:
        cli("add", "First")
        cli("add", "Second")
    if scenario in ("color", "no_color", "dumb"):
        cli("next", "--local", "--session", "worker")
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
    visible = TerminalScreen(72, 16)
    visible_at_clear = [visible.text()]

    def clear_capture():
        screen[:] = b""
        visible_at_clear[0] = visible.text()

    def capture(data):
        screen.extend(data)
        visible.feed(data)

    def read_until(needle):
        deadline = time.monotonic() + 5
        def found():
            return needle in screen or (
                bool(screen) and b"\x1b" not in needle and
                needle.decode("utf-8", "replace") not in visible_at_clear[0] and
                needle.decode("utf-8", "replace") in visible.text()
            )
        while not found():
            assert time.monotonic() < deadline, (
                f"Missing {needle!r}: {screen[-2000:]!r}\nVisible:\n{visible.text()}\nTasks: {cli('list')}"
            )
            if select.select([master], [], [], 0.05)[0]:
                try:
                    capture(os.read(master, 65536))
                except OSError as error:
                    raise AssertionError(f"Editor exited before {needle!r}") from error
            assert child.poll() is None, f"Editor exited before {needle!r}: {screen[-2000:]!r}"

    def wait_visible(predicate):
        deadline = time.monotonic() + 5
        while not predicate():
            assert time.monotonic() < deadline, f"Visible screen stalled, cursor={visible.x},{visible.y}, bytes={screen[-500:]!r}:\n{visible.text()}"
            if select.select([master], [], [], 0.05)[0]:
                capture(os.read(master, 65536))
            assert child.poll() is None, "Editor exited before visible state"

    def send(data):
        remaining = memoryview(data)
        deadline = time.monotonic() + 5
        while remaining:
            assert time.monotonic() < deadline, "Terminal input timed out"
            readable, writable, _ = select.select([master], [master], [], 0.05)
            if readable:
                capture(os.read(master, 65536))
            if writable:
                remaining = remaining[os.write(master, remaining):]

    try:
        read_until(b"qqq task editor - new task")
        assert "qqq tasks" in visible.text(), visible.text()
        if scenario == "color":
            read_until(b"38;5;81")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"48;5;81")
        elif scenario in ("no_color", "dumb", "pasteboard_no_color", "filter_no_color"):
            assert b"\x1b[38;" not in screen, screen[-2000:]
            assert b"\x1b[48;" not in screen, screen[-2000:]
        if scenario not in ("scroll", "filter", "filter_no_color"):
            read_until(b"Second")
            assert "First" in visible.text() and "Second" in visible.text(), visible.text()
        assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout while open"
        if scenario == "slash_edit":
            send(b"\x1b/")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("/"))
            send(b"path\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "/path"
        elif scenario in ("filter", "filter_no_color"):
            initial_tasks = cli("list")
            send(b"Unsaved")
            clear_capture()
            send(b"/needle")
            read_until(b"Filter: needle")
            clear_capture()
            send(b"\x7f")
            wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: needl")
            clear_capture()
            send(b"e")
            wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: needle")
            assert "Parent" in visible.text(), visible.text()
            assert "Child" in visible.text(), visible.text()
            assert "Other" not in visible.text(), visible.text()
            assert "Unsaved" in visible.text(), visible.text()
            assert "new task" in visible.text(), visible.text()
            assert cli("list") == initial_tasks
            if scenario == "filter_no_color":
                assert b"\x1b[38;" not in screen, screen[-2000:]

            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"Discard changes and switch? (y/N)")
            send(b"n")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to filter"))
            assert "Unsaved" in visible.text(), visible.text()
            clear_capture()
            send(b"/zzzz")
            read_until(b"No matching tasks.")
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter:"
                         and "Other" in visible.text())
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: visible.y == 9)
            send(b"!")
            wait_visible(lambda: "Unsaved!" in visible.text())

            clear_capture()
            send(b"/needle\t")
            read_until(b"Filter: needle")
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #4. New task")
            assert cli("show", "4")["task"]["description"] == "Unsaved!"
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"task #2")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"task #1")
            clear_capture()
            send(b"\x1b[1;2B")
            read_until(b"task #2")
            clear_capture()
            send(b"\x1b[1;2B")
            read_until(b"new task")
            assert cli("list")[0:3] == initial_tasks

            clear_capture()
            send(b"/")
            wait_visible(lambda: visible.y == 1 and visible.text().splitlines()[1].strip() == "Filter: needle")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 7, 10, 0, 0))
            visible.resize(10, 7)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Resize ter")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
            visible.resize(72, 16)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Filter: needle")
            read_until(b"\x1b[2;15H")
            time.sleep(0.1)
            clear_capture()
            send(b"\rx")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("x"))
            send(b"\x7f")
            wait_visible(lambda: visible.text().splitlines()[9].strip() == "")
        elif scenario in ("save", "save_json"):
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"task #2")
            assert "> 2" in visible.text(), visible.text()
            clear_capture()
            send(b"\x05 edited\x13")
            read_until(b"Saved #2. New task")
            assert cli("show", "2")["task"]["description"] == "Second edited"
            clear_capture()
            send(b"Third\x13")
            read_until(b"Saved #3. New task")
            assert "3      New" in visible.text(), visible.text()
            assert cli("show", "3")["task"]["description"] == "Third"
        elif scenario == "scroll":
            assert "Task 20" in visible.text(), visible.text()
            for task_id in range(20, 0, -1):
                clear_capture()
                send(b"\x1b[1;2A")
                read_until(f"qqq task editor - task #{task_id}".encode())
            assert "> 1" in visible.text() and "First" in visible.text(), visible.text()
            clear_capture()
            send(b"\x1b[1;2B")
            read_until(b"qqq task editor - task #2")
            assert "> 2" in visible.text(), visible.text()
        elif scenario in ("pasteboard", "pasteboard_no_color"):
            payload = "x" * 1001
            clear_capture()
            send(b"Prefix \x1b[200~" + payload.encode() + b"\x1b[201~")
            read_until(b"[Pasted Content 1001 chars]")
            if scenario == "pasteboard":
                assert b"38;5;222" in screen, "Paste accent missing"
            else:
                assert b"\x1b[38;" not in screen, screen[-2000:]
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == (
                "Prefix \n```pasteboard\n" + payload + "\n```"
            )
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"task #3")
            read_until(b"[Pasted Content 1001 chars]")
            if scenario == "pasteboard":
                assert b"38;5;222" in screen, "Reloaded paste accent missing"
            else:
                assert b"\x1b[38;" not in screen, screen[-2000:]
            send(b"\x1b[C" * (len("Prefix \n") + 1) + b"\x17\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "Prefix \n"
        elif scenario == "dirty":
            send(b"Draft")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"Discard changes and switch? (y/N)")
            clear_capture()
            send(b"n")
            read_until(b"Ctrl-S save")
            assert "qqq task editor - new task" in visible.text(), visible.text()
            assert "Draft" in visible.text(), visible.text()
            assert "Discard changes" not in visible.text(), visible.text()
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "Draft"
        elif scenario == "save_error":
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"qqq task editor - task #2")
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            clear_capture()
            send(b"\x05 edited\x13")
            read_until(b"Task 2 not found")
            assert b"38;5;1" in screen, screen[-2000:]
            assert child.poll() is None
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("INSERT INTO tasks(id, description) VALUES (2, 'Second')")
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #2. New task")
            assert cli("show", "2")["task"]["description"] == "Second edited"
        elif scenario == "resize":
            send(b"Draft")
            clear_capture()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 7, 10, 0, 0))
            visible.resize(10, 7)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Resize ter")
            clear_capture()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
            visible.resize(72, 16)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"qqq task editor - new task")
            read_until(b"Draft")
            read_until(b"\x1b[10;6H")
            clear_capture()
            assert not (termios.tcgetattr(slave)[0] & termios.IXON), termios.tcgetattr(slave)
            send(b"\x13")
            read_until(b"Saved #3. New task")
            assert cli("show", "3")["task"]["description"] == "Draft"
        send(b"\x03" if scenario in ("scroll", "color") else b"\x1b")
        deadline = time.monotonic() + 5
        while child.poll() is None:
            assert time.monotonic() < deadline, f"TUI failed to exit: {screen[-1000:]!r}\n{visible.text()}"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    capture(os.read(master, 65536))
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
