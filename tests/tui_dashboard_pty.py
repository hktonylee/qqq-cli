"""Exercise task-list, selected-details and editor panes through a real terminal."""

import base64
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
import threading
import time
from pathlib import Path


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
# Legacy terminals send this byte for Ctrl+/; Crossterm reads it as Ctrl+7.
CTRL_SLASH = b"\x1f"
SHIFT_ENTER = b"\x1b[13;2u"
CTRL_P = b"\x10"
with tempfile.TemporaryDirectory(prefix="qqq-dashboard-test-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    env.pop("NO_COLOR", None)
    if scenario in ("no_color", "pasteboard_no_color", "filter_no_color", "details_no_color"):
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
    if scenario == "child_open_new":
        cli("config", "tui.after_save_new", "open_new")
    if scenario.startswith("after_save_"):
        cli("config", "tui.after_save_new", "open_saved" if scenario == "after_save_open_saved" else "open_new")
        if scenario == "after_save_default_restored":
            cli("config", "--unset", "tui.after_save_new")
    if scenario == "workflow_empty":
        pass
    elif scenario == "workflow_status":
        cli("add", "Finished item")
        cli("add", "Failed item")
        cli("add", "Fresh item")
        cli("next", "--local", "--session", "worker")
        cli("complete", "1", "--session", "worker")
        cli("next", "--local", "--session", "worker")
        cli("edit", "2", "--set-status", "error", "--reason", "Failed", "--session", "worker")
    elif scenario in ("actions_basic", "actions_rejected"):
        cli("add", "Owned item")
        cli("add", "Failed item")
        cli("add", "Finished item")
        cli("add", "Fresh item")
        cli("add", "Hidden item")
        cli("next", "--local", "--session", "worker")
        cli("next", "--local", "--session", "failed")
        cli("edit", "2", "--set-status", "error", "--reason", "Failed", "--session", "failed")
        cli("next", "--local", "--session", "finished")
        cli("complete", "3", "--session", "finished")
        cli("archive", "5")
    elif scenario == "workflow":
        cli("add", "Other")
        for index in range(2, 21):
            cli("add", f"Target {index}")
        image_path = Path(folder) / "workflow.png"
        image_path.write_bytes(base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="
        ))
    elif scenario == "tree_navigation":
        cli("add", "Parent")
        cli("add", "Other root")
        cli("add", "Child\nWrapped child detail", "--parent", "1")
    elif scenario == "cursor_end":
        cli("add", "First\nOlder tail")
        cli("add", "Second\n" + "\n".join(f"Line {index:02}" for index in range(1, 16)) + "\nNewest tail")
    elif scenario == "click":
        cli("add", "Parent\nParent detail")
        cli("add", "Child start\nChild detail " + "word " * 12, "--parent", "1")
    elif scenario == "click_filter":
        cli("add", "Workspace")
        cli("add", "Needle child", "--parent", "1")
        cli("add", "Other")
        for index in range(4, 21):
            cli("add", f"Needle {index}")
    elif scenario in ("filter", "filter_no_color"):
        cli("add", "Parent")
        cli("add", "Child\nNeEdLe on second line", "--parent", "1")
        cli("add", "Other")
    elif scenario in ("archive_hidden", "archive_included"):
        cli("add", "Visible")
        cli("add", "Hidden")
        cli("archive", "2")
    elif scenario == "actions_hidden":
        cli("add", "Visible")
        cli("add", "Filtered item")
    else:
        cli("add", "First")
        cli("add", "Second")
    if scenario in ("color", "no_color", "dumb"):
        cli("next", "--local", "--session", "worker")
    if scenario == "actions_narrow":
        cli("next", "--local", "--session", "worker")
    if scenario in ("scroll", "wheel", "live_refresh_scroll", "ctrl_c_new_scroll"):
        for index in range(3, 21):
            description = ("\n".join(f"Line{line:02}" for line in range(1, 16))
                           if scenario == "wheel" and index == 20 else f"Task {index}")
            cli("add", description)
    if scenario == "click_editor_scroll":
        cli("add", "\n".join(f"Line{index:02}" for index in range(1, 13)))
    if scenario.startswith("details") and scenario != "details_deleted":
        if scenario == "details_scroll":
            cli("message", "2", "\n".join(f"Message line {index:02}" for index in range(1, 31)), "--session", "reviewer")
        else:
            cli("message", "2", "Earlier message", "--session", "worker")
            cli("message", "2", "Latest message\nMessage continuation", "--session", "reviewer")

    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
    before = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    args = [binary, "--json", "tui"] if scenario in ("empty_json", "save_json") else [binary, "tui"]
    if scenario in ("actions_basic", "actions_rejected", "actions_narrow"):
        args.extend(["--session", "worker"])
    if scenario in ("archive_included", "actions_basic", "actions_rejected"):
        args.append("--include-archived")
    child = subprocess.Popen(args, cwd=folder, env=env, stdin=slave,
                             stderr=slave, stdout=subprocess.PIPE)
    screen = bytearray()
    visible = TerminalScreen(72, 24)
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

    def click(column, row):
        send(f"\x1b[<0;{column};{row}M\x1b[<0;{column};{row}m".encode())

    def list_bottom():
        return next((index for index, row in enumerate(visible.text().splitlines())
                     if row and set(row) == {"─"}), 0)

    def editor_row():
        return next((index for index, row in enumerate(visible.text().splitlines())
                     if row.startswith("qqq task editor"[:visible.width])), None)

    def editor_title():
        row = editor_row()
        return visible.text().splitlines()[row] if row is not None else ""

    def editor_line(offset=0):
        row = editor_row()
        lines = visible.text().splitlines()
        index = row + 1 + offset if row is not None else len(lines)
        return lines[index] if index < len(lines) - 1 else ""

    def details_text():
        row = editor_row()
        return "\n".join(visible.text().splitlines()[list_bottom() + 1:row]) if row is not None else ""

    def click_editor(column, offset=0):
        row = editor_row()
        assert row is not None, visible.text()
        click(column, row + 2 + offset)

    def wheel_editor(down, count=1):
        row = editor_row()
        assert row is not None, visible.text()
        code = 65 if down else 64
        send(f"\x1b[<{code};6;{row + 2}M".encode() * count)

    def task_row(label):
        for row, content in enumerate(visible.text().splitlines()[2:list_bottom()], 3):
            if label in content:
                return row
        raise AssertionError(f"Task row {label!r} not visible:\n{visible.text()}")

    def settle():
        time.sleep(0.1)
        while select.select([master], [], [], 0)[0]:
            capture(os.read(master, 65536))

    try:
        wait_visible(lambda: 'qqq task editor - new task' in editor_title())
        wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save")
                     and (visible.x, visible.y) == (0, editor_row() + 1))
        assert "qqq tasks" in visible.text(), visible.text()
        read_until(b"\x1b[?1000h")
        read_until(b"\x1b[?1006h")
        read_until(b"\x1b[?2004h")
        if scenario == "color":
            read_until(b"38;5;81")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"48;5;24")
            settle()
            assert b"\x1b[4m" not in screen, screen[-2000:]
            assert b"48;5;81" not in screen, screen[-2000:]
        elif scenario in ("no_color", "dumb", "pasteboard_no_color", "filter_no_color"):
            assert b"\x1b[38;" not in screen, screen[-2000:]
            assert b"\x1b[48;" not in screen, screen[-2000:]
            if scenario in ("no_color", "dumb"):
                send(b"\x1b[1;2A")
                wait_visible(lambda: "task #2 (New)" in editor_title()
                             and editor_line().startswith("Second"))
                settle()
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        if scenario not in ("ctrl_c_new_scroll", "live_refresh_scroll", "tree_navigation", "scroll", "wheel", "click", "click_filter", "workflow", "workflow_empty", "workflow_status", "filter", "filter_no_color", "archive_hidden", "archive_included", "actions_basic", "actions_rejected", "actions_hidden"):
            read_until(b"Second")
            assert "First" in visible.text() and "Second" in visible.text(), visible.text()
        assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout while open"
        if scenario == "cursor_end":
            def wait_end(tail):
                def at_end():
                    start = editor_row()
                    if start is None:
                        return False
                    rows = visible.text().splitlines()
                    return any(rows[index].rstrip() == tail and
                               (visible.x, visible.y) == (len(tail), index)
                               for index in range(start + 1, len(rows) - 1))
                wait_visible(at_end)

            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in editor_title())
            wait_end("Newest tail")
            send(b" appended\x1b[H!")
            wait_visible(lambda: "!Newest tail appended" in visible.text()
                         and visible.x == 1)
            send(b"\x13")
            wait_visible(lambda: "Saved #2" in visible.text())
            wait_end("!Newest tail appended")
            assert cli("show", "2")["task"]["description"].endswith("!Newest tail appended")

            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #1 (New)" in editor_title())
            wait_end("Older tail")
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #2 (New)" in editor_title())
            wait_end("!Newest tail appended")
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in editor_title())
            wait_visible(lambda: (visible.x, visible.y) == (0, editor_row() + 1))
            send(b"New text\x1b[H<\x13")
            wait_visible(lambda: "task #3 (New)" in editor_title())
            wait_end("<New text")
            assert cli("show", "3")["task"]["description"] == "<New text"
        elif scenario in ("details", "details_no_color"):
            assert editor_row() == 13, visible.text()
            assert "Select task to view details." in details_text(), visible.text()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in editor_title()
                         and "Latest message" in details_text()
                         and editor_line().startswith("Second"))
            assert editor_row() == 13, visible.text()
            assert "Task #2 | New | Priority 0" in details_text(), visible.text()
            assert "reviewer" in details_text(), visible.text()
            send(b"\x1b[6~")
            wait_visible(lambda: "Message continuation" in details_text())
            assert editor_row() == 13 and editor_line().startswith("Second"), visible.text()
            send(b"\x1b[5~")
            wait_visible(lambda: details_text().startswith("Task #2"))
            assert editor_line().startswith("Second"), visible.text()
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: "new task" in editor_title() and editor_row() == 13
                         and "Latest message" not in visible.text()
                         and visible.text().count("qqq task editor") == 1)
            assert "Select task to view details." in details_text() and "Latest message" not in visible.text(), visible.text()
            send(b"Fresh\x13")
            wait_visible(lambda: "task #3 (New)" in editor_title()
                         and "No messages yet." in details_text()
                         and "Latest message" not in details_text()
                         and editor_line().startswith("Fresh"))
            assert "Latest message" not in details_text(), visible.text()
            assert cli("show", "3")["task"]["description"] == "Fresh"
            if scenario == "details_no_color":
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario == "layout_new":
            payload = "\n".join(f"Draft line {index:02}" for index in range(1, 13))
            send(b"\x1b[200~" + payload.encode() + b"\x1b[201~")
            wait_visible(lambda: editor_line().startswith("Draft line 04") and editor_line(8).startswith("Draft line 12"))
            assert editor_row() == 13 and "Select task to view details." in details_text(), visible.text()
            assert len(cli("list")) == 2
            for height, expected_row in [(18, 10), (30, 17), (24, 13)]:
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, 72, 0, 0))
                visible.resize(72, height)
                os.kill(child.pid, signal.SIGWINCH)
                wait_visible(lambda: editor_row() == expected_row
                             and "Select task to view details." in details_text()
                             and "Draft line 12" in visible.text()
                             and visible.y == height - 2)
            settle()
            send(b"\x13")
            wait_visible(lambda: "task #3" in editor_title() and editor_row() == 13)
            assert cli("show", "3")["task"]["description"] == payload
        elif scenario == "details_scroll":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Message line 01" in details_text()
                         and "task #2 (New)" in editor_title()
                         and editor_line().startswith("Second"))
            initial_tasks = cli("list")
            initial_list = visible.text().splitlines()[:list_bottom() + 1]
            send(b"\x1b[6~")
            wait_visible(lambda: details_text().splitlines()[0].startswith("Message line 02"))
            assert editor_line().startswith("Second"), visible.text()
            assert visible.text().splitlines()[:list_bottom() + 1] == initial_list
            send(f"\x1b[<65;6;{list_bottom() + 2}M".encode())
            wait_visible(lambda: details_text().splitlines()[0].startswith("Message line 05"))
            settle()
            clear_capture()
            click(5, list_bottom() + 2)
            settle()
            assert not screen, f"Read-only details click redrew screen: {screen[-500:]!r}"
            send(b"\x1b[5~" * 3)
            wait_visible(lambda: details_text().startswith("Task #2"))
            assert cli("list") == initial_tasks
            send(b"\x1b[6~" * 10)
            wait_visible(lambda: "Orchestrator:" in details_text())
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #1 (New)" in editor_title()
                         and details_text().startswith("Task #1")
                         and "No messages yet." in details_text())
            assert "No messages yet." in details_text(), visible.text()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #2" in editor_title() and "Message line 01" in details_text())
            assert cli("list") == initial_tasks
        elif scenario == "details_refresh":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Latest message" in details_text())
            send(b"\x01Unsaved ")
            wait_visible(lambda: editor_line().startswith("Unsaved Second"))
            cli("message", "2", "External message", "--session", "reviewer")
            wait_visible(lambda: "External message" in details_text())
            assert editor_line().startswith("Unsaved Second"), visible.text()
            cli("edit", "2", "--priority", "7")
            cli("next", "--local", "--session", "worker")
            cli("edit", "2", "--set-status", "error", "--reason", "Live failure", "--session", "worker")
            wait_visible(lambda: "Task #2 | Error | Priority 7" in details_text() and "Live failure" in details_text())
            assert editor_line().startswith("Unsaved Second"), visible.text()
            assert cli("show", "2")["task"]["description"] == "Second"
        elif scenario == "details_deleted":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "No messages yet." in details_text())
            send(b"\x01Unsaved ")
            wait_visible(lambda: editor_line().startswith("Unsaved Second"))
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            wait_visible(lambda: "Task #2 unavailable." in details_text()
                         and "No messages yet." not in details_text())
            assert editor_line().startswith("Unsaved Second"), visible.text()
            assert "No messages yet." not in details_text(), visible.text()
            send(b"\x1b")
            wait_visible(lambda: "Discard changes and switch?" in visible.text().splitlines()[-1])
            send(b"y")
            wait_visible(lambda: "new task" in editor_title() and editor_row() == 13
                         and "Task #2 unavailable." not in visible.text()
                         and visible.text().count("qqq task editor") == 1)
            assert "Select task to view details." in details_text(), visible.text()
        elif scenario in ("archive_hidden", "archive_included"):
            wait_visible(lambda: "Visible" in visible.text())
            settle()
            if scenario == "archive_hidden":
                assert "Hidden" not in visible.text(), visible.text()
            else:
                assert "[archived] Hidden" in visible.text(), visible.text()
        if scenario.startswith("ctrl_c_new_"):
            initial_tasks = cli("list")
            if scenario == "ctrl_c_new_image":
                image_path = Path(folder) / "unsaved.png"
                image_path.write_bytes(base64.b64decode(
                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="
                ))
                send(b"\x1b[200~" + str(image_path).encode() + b"\x1b[201~")
                wait_visible(lambda: "[Image #1: unsaved.png]" in visible.text())
            else:
                send(b"   " if scenario == "ctrl_c_new_whitespace" else b"Unsaved draft")
                wait_visible(lambda: visible.x > 0 and visible.y == editor_row() + 1)
            if scenario == "ctrl_c_new_filter":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: first")
            if scenario == "ctrl_c_new_scroll":
                send(b"\x1b[<64;6;4M")
                wait_visible(lambda: "Task 13" in visible.text().splitlines()[2]
                             and "Task 17" in visible.text().splitlines()[6])
                scrolled_list = visible.text().splitlines()[:list_bottom() + 1]
            clear_capture()
            send(b"\x03")
            read_until(b"Discard draft? (y/N)")
            if scenario == "ctrl_c_new_filter":
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter:"
                             and "Second" in "\n".join(visible.text().splitlines()[2:list_bottom()])
                             and editor_line().startswith("Unsaved draft"))
            assert cli("list") == initial_tasks
            assert child.poll() is None
            settle()
            if scenario == "ctrl_c_new_scroll":
                assert visible.text().splitlines()[:list_bottom() + 1] == scrolled_list, visible.text()
            clear_capture()
            send(b"\x03")
            wait_visible(lambda: bool(screen)
                         and visible.text().splitlines()[-1].startswith("Discard draft? (y/N)"))
            assert child.poll() is None
            if scenario == "ctrl_c_new_scroll":
                send(b"n")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save")
                             and editor_line().startswith("Unsaved draft"))
                assert visible.text().splitlines()[:list_bottom() + 1] == scrolled_list, visible.text()
                assert cli("list") == initial_tasks
            if scenario in ("ctrl_c_new_keep", "ctrl_c_new_filter"):
                send(b"n")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save")
                             and editor_line().startswith("Unsaved draft"))
                send(b"\x13")
                wait_visible(lambda: "task #3 (" in editor_title())
                assert cli("show", "3")["task"]["description"] == "Unsaved draft"
        elif scenario in ("live_title", "live_title_filtered"):
            cli("edit", "2", "--priority", "8")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in editor_title())
            send(b"\x01Draft ")
            wait_visible(lambda: editor_line().startswith("Draft Second"))
            if scenario == "live_title_filtered":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: first"
                             and "Second" not in "\n".join(visible.text().splitlines()[2:7]))
            def title_status(status):
                wait_visible(lambda: f"task #2 ({status})" in editor_title())
                assert editor_line().startswith("Draft Second")
            assert cli("next", "--local", "--session", "worker")["id"] == 2
            title_status("In progress")
            cli("complete", "2", "--session", "worker")
            title_status("Completed")
            cli("reopen", "2")
            title_status("New")
            cli("next", "--local", "--session", "worker")
            title_status("In progress")
            cli("edit", "2", "--set-status", "error", "--reason", "Failure", "--session", "worker")
            title_status("Error")
            cli("edit", "2", "--set-status", "new")
            title_status("New")
            cli("next", "--local", "--session", "worker")
            title_status("In progress")
            cli("edit", "2", "--set-status", "error", "--reason", "Failure", "--session", "worker")
            title_status("Error")
            cli("archive", "2")
            wait_visible(lambda: "Second" not in "\n".join(visible.text().splitlines()[2:7]))
            cli("edit", "2", "--set-status", "new")
            title_status("New")
            assert cli("show", "2")["task"]["archived"]
            assert cli("show", "2")["task"]["description"] == "Second"
        elif scenario == "live_refresh":
            settle()
            clear_capture()
            cli("add", "External task")
            wait_visible(lambda: "External task" in visible.text())
            assert "new task" in editor_title()
            cli("edit", "2", "--description", "Changed externally")
            wait_visible(lambda: "Changed externally" in visible.text())
            cli("next", "--local", "--session", "worker")
            wait_visible(lambda: "In progress" in visible.text())
            cli("complete", "1", "--session", "worker")
            wait_visible(lambda: "Completed" in visible.text())
            cli("archive", "2")
            wait_visible(lambda: "Changed externally" not in visible.text())
            cli("unarchive", "2")
            wait_visible(lambda: "Changed externally" in visible.text())
            cli("edit", "3", "--set-parent", "1")
            wait_visible(lambda: "External task" in visible.text().splitlines()[4]
                         and "Changed externally" in visible.text().splitlines()[5])
            settle()
            clear_capture()
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("UPDATE tasks SET description='Rolled back' WHERE id=2")
                db.rollback()
            readable, _, _ = select.select([master], [], [], 0.8)
            if readable:
                capture(os.read(master, 65536))
            assert not screen, f"Unchanged DB redrew TUI: {screen[-500:]!r}"
        elif scenario == "live_refresh_filter":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (" in editor_title())
            send(b"\x01Unsaved " + CTRL_SLASH + b"second")
            wait_visible(lambda: editor_line().startswith("Unsaved Second")
                         and visible.text().splitlines()[1].strip() == "Filter: second"
                         and (visible.x, visible.y) == (len("Filter: second"), 1))
            cursor_before = (visible.x, visible.y)
            cli("edit", "2", "--description", "Updated second externally")
            wait_visible(lambda: "Updated second externally" in visible.text().splitlines()[3])
            cli("add", "Second external task")
            wait_visible(lambda: "Second external task" in visible.text()
                         and (visible.x, visible.y) == cursor_before)
            assert "First" not in visible.text()
            assert editor_line().startswith("Unsaved Second")
            assert "task #2 (" in editor_title()
            assert visible.text().splitlines()[1].strip() == "Filter: second"
            assert (visible.x, visible.y) == cursor_before
            assert cli("show", "2")["task"]["description"] == "Updated second externally"
        elif scenario == "live_refresh_motion":
            stop = threading.Event()
            def mouse_motion():
                while not stop.is_set():
                    os.write(master, b"\x1b[<35;6;4M")
                    stop.wait(0.02)
            motion = threading.Thread(target=mouse_motion)
            motion.start()
            try:
                cli("add", "Added during motion")
                wait_visible(lambda: "Added during motion" in visible.text())
                assert motion.is_alive()
            finally:
                stop.set()
                motion.join(timeout=1)
        elif scenario == "live_refresh_scroll":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #20 (" in editor_title())
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2])
            cli("edit", "13", "--description", "Refreshed row")
            wait_visible(lambda: "Refreshed row" in visible.text().splitlines()[2])
            assert "task #20 (" in editor_title()
            assert editor_line().startswith("Task 20")
        elif scenario == "tree_navigation":
            expected = (2, 3, 1)
            for task_id in expected:
                clear_capture()
                send(b"\x1b[1;2A")
                wait_visible(lambda: f"task #{task_id} (" in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #3 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #2 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in editor_title())
        elif scenario in (
            "escape_selected", "ctrl_c_selected", "escape_dirty_selected", "ctrl_c_dirty_selected",
            "ctrl_c_dirty_selected_filter_editor", "ctrl_c_dirty_selected_filter_focused",
            "ctrl_c_selected_filter_menu",
        ):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (" in editor_title())
            if "dirty_selected" in scenario:
                send(b"\x01Changed ")
                wait_visible(lambda: editor_line().startswith("Changed Second"))
            if "_filter_" in scenario:
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: first"
                             and "Second" not in "\n".join(visible.text().splitlines()[2:list_bottom()]))
                if not scenario.endswith("focused"):
                    send(b"\t")
                    wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save"))
                if scenario.endswith("menu"):
                    send(b"\x07")
                    wait_visible(lambda: visible.text().startswith("Task actions"))
            key = b"\x03" if scenario.startswith("ctrl_c") else b"\x1b"
            clear_capture()
            send(key)
            if scenario == "escape_dirty_selected":
                read_until(b"Discard changes and switch? (y/N)")
                send(b"n")
                wait_visible(lambda: editor_line().startswith("Changed Second")
                             and visible.text().splitlines()[-1].startswith("Ctrl-S save"))
                clear_capture()
                send(b"\x1b")
                read_until(b"Discard changes and switch? (y/N)")
                send(b"y")
            wait_visible(lambda: "new task" in editor_title()
                         and editor_line().strip() == ""
                         and ("_filter_" not in scenario
                              or (visible.text().splitlines()[1].strip() == "Filter:"
                                  and "Second" in "\n".join(visible.text().splitlines()[2:list_bottom()]))))
            assert cli("list") == initial_tasks
            assert child.poll() is None
        elif scenario in ("save_selected", "after_save_open_saved", "after_save_default_restored"):
            send(b"Created\x13")
            wait_visible(lambda: "task #3 (New)" in editor_title()
                         and editor_line().startswith("Created"))
            assert cli("show", "3")["task"]["description"] == "Created"
            send(b"\x05 updated\x13")
            wait_visible(lambda: editor_line().startswith("Created updated")
                         and visible.text().splitlines()[-1].startswith("Saved #3")
                         and (visible.x, visible.y) == (len("Created updated"), editor_row() + 1))
            assert len(cli("list")) == 3
            assert cli("show", "3")["task"]["description"] == "Created updated"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in editor_title())
        elif scenario == "shift_enter_text":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in editor_title())
            send(SHIFT_ENTER + b"tail")
            wait_visible(lambda: "task #2 (New)" in editor_title()
                         and editor_line().startswith("Second")
                         and editor_line(1).startswith("tail"))
            send(b"\x13")
            wait_visible(lambda: "Saved #2" in visible.text().splitlines()[-1])
            assert cli("show", "2")["task"]["description"] == "Second\ntail"
            assert len(cli("list")) == 2
        elif scenario.startswith("child"):
            if scenario == "child_no_selection":
                send(CTRL_P)
                wait_visible(lambda: "Select parent task" in visible.text().splitlines()[-1])
                assert len(cli("list")) == 2
                assert editor_line().strip() == ""
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in editor_title())
            if scenario == "child_dirty":
                send(b"\x01Draft ")
                wait_visible(lambda: editor_line().startswith("Draft Second"))
                send(CTRL_P)
                wait_visible(lambda: "Discard changes and switch?" in visible.text().splitlines()[-1])
                send(b"n")
                wait_visible(lambda: editor_line().startswith("Draft Second")
                             and visible.text().splitlines()[-1].startswith("Ctrl-S save"))
                assert cli("show", "2")["task"]["description"] == "Second"
                send(CTRL_P + b"y")
            else:
                send(CTRL_P)
            wait_visible(lambda: "new task (parent #2)" in editor_title()
                         and editor_line().strip() == ""
                         and (visible.x, visible.y) == (0, editor_row() + 1))
            assert len(cli("list")) == 2
            assert "Ctrl-P child" in visible.text().splitlines()[-1], visible.text()
            if scenario == "child_no_selection":
                send(b"\x1b[1;2A")
                wait_visible(lambda: "task #2 (New)" in editor_title())
                send(b"\x1b[1;2B")
                wait_visible(lambda: "new task" in editor_title()
                             and "parent" not in editor_title())
            if scenario == "child_error":
                cli("archive", "2")
            send(b"Child\x13")
            if scenario == "child_error":
                wait_visible(lambda: "archived unfinished parent" in visible.text().splitlines()[-1])
                assert editor_line().startswith("Child")
                assert "parent #2" in editor_title()
                assert len(cli("list", "--include-archived")) == 2
                cli("unarchive", "2")
                send(b"\x13")
            if scenario == "child_open_new":
                wait_visible(lambda: "new task" in editor_title()
                             and "parent" not in editor_title()
                             and editor_line().strip() == ""
                             and visible.text().splitlines()[-1].startswith("Saved #3. New task"))
            else:
                wait_visible(lambda: "task #3 (New)" in editor_title()
                             and visible.text().splitlines()[-1].startswith("Saved #3"))
            task = cli("show", "3")["task"]
            assert task["description"] == "Child"
            assert task["parent_id"] == (None if scenario == "child_no_selection" else 2)
            assert cli("show", "2")["task"]["description"] == "Second"
            if scenario != "child_open_new":
                send(b"\x1b[1;2B")
                wait_visible(lambda: "new task" in editor_title())
            send(b"Root\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #4"))
            assert cli("show", "4")["task"]["parent_id"] is None
        elif scenario in ("after_save_open_new", "after_save_open_new_error"):
            if scenario == "after_save_open_new_error":
                with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                    db.execute("CREATE TRIGGER reject_new BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT, 'New task rejected'); END")
            send(b"Created\x13")
            if scenario == "after_save_open_new_error":
                wait_visible(lambda: "New task rejected" in visible.text().splitlines()[-1]
                             and editor_line().startswith("Created"))
                assert len(cli("list")) == 2
                with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                    db.execute("DROP TRIGGER reject_new")
                send(b"\x13")
            wait_visible(lambda: "new task" in editor_title()
                         and editor_line().strip() == ""
                         and visible.text().splitlines()[-1].startswith("Saved #3. New task")
                         and (visible.x, visible.y) == (0, editor_row() + 1))
            assert cli("show", "3")["task"]["description"] == "Created"
            send(b"Next\x13")
            wait_visible(lambda: "new task" in editor_title()
                         and editor_line().strip() == ""
                         and visible.text().splitlines()[-1].startswith("Saved #4. New task"))
            assert cli("show", "4")["task"]["description"] == "Next"
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #4 (New)" in editor_title())
            send(b"\x05 updated\x13")
            wait_visible(lambda: "task #4 (New)" in editor_title()
                         and editor_line().startswith("Next updated")
                         and visible.text().splitlines()[-1].startswith("Saved #4")
                         and (visible.x, visible.y) == (len("Next updated"), editor_row() + 1))
            assert len(cli("list")) == 4
            assert cli("show", "4")["task"]["description"] == "Next updated"
        elif scenario == "workflow":
            initial_tasks = cli("list")
            clear_capture()
            send(CTRL_SLASH + b"target")
            read_until(b"Filter: target")
            clear_capture()
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Target 13" in visible.text().splitlines()[2])
            settle()
            clear_capture()
            click(6, task_row("Target 14"))
            wait_visible(lambda: 'task #14' in editor_title())
            wait_visible(lambda: editor_line().startswith("Target 14"))
            clear_capture()
            click_editor(10)
            send(b" updated")
            wait_visible(lambda: editor_line().startswith("Target 14 updated"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #14")
            assert cli("show", "14")["task"]["description"] == "Target 14 updated"
            assert child.poll() is None
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout after update"
            send(b"\x1b[1;2B" * 7)
            wait_visible(lambda: "new task" in editor_title()
                         and editor_line().strip() == "")
            clear_capture()
            send(b"\x13")
            read_until(b"Task description cannot be empty")
            clear_capture()
            send(b"New target ")
            send(b"\x1b[200~details\x1b[201~")
            wait_visible(lambda: editor_line().startswith("New target details"))
            clear_capture()
            send(b"\x1b[200~" + str(image_path).encode() + b"\x1b[201~")
            read_until(b"[Image #1: workflow.png]")
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #21")
            saved = cli("show", "21")
            assert saved["task"]["description"] == "New target details![workflow.png](.qqq/images/21/1.png)"
            assert [image["name"] for image in saved["images"]] == ["workflow.png"]
            exported = Path(folder) / "exported-workflow.png"
            cli("show", "21", "--export-image", "1", "--output", str(exported))
            assert exported.read_bytes() == image_path.read_bytes()
            assert child.poll() is None
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout after new task"
        elif scenario == "workflow_empty":
            wait_visible(lambda: "No tasks yet." in visible.text().splitlines()[2])
            assert cli("list") == []
            assert editor_title().startswith("qqq task editor - new task")
        elif scenario == "workflow_status":
            wait_visible(lambda: "Completed" in visible.text()
                         and "Error" in visible.text()
                         and "Fresh item" in visible.text())
            assert "New          Fresh item" in visible.text(), visible.text()
            clear_capture()
            send(b"\x1b[1;2A" * 2)
            wait_visible(lambda: "task #2 (Error)" in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #1 (Completed)" in editor_title())
        elif scenario == "actions_basic":
            wait_visible(lambda: "Owned item" in visible.text() and "Hidden item" in visible.text())
            def select_action_task(label, task_id):
                clear_capture()
                click(5, task_row(label))
                wait_visible(lambda: f"task #{task_id}" in editor_title())

            def action(letter, prompt):
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "Task actions" in visible.text() and "c Complete" in visible.text())
                clear_capture()
                send(letter.encode())
                read_until(prompt.encode())

            click(5, task_row("Owned item"))
            wait_visible(lambda: 'task #1 (In progress)' in editor_title())
            action("c", "Complete task #1?")
            clear_capture()
            send(b"y")
            read_until(b"Completed #1")
            assert cli("show", "1")["task"]["status"] == "completed"

            select_action_task("Failed item", 2)
            action("r", "Retry task #2?")
            clear_capture()
            send(b"y")
            read_until(b"Retried #2")
            assert cli("show", "2")["task"]["status"] == "new"

            select_action_task("Finished item", 3)
            action("o", "Reopen task #3?")
            clear_capture()
            send(b"y")
            read_until(b"Reopened #3")
            assert cli("show", "3")["task"]["status"] == "new"

            select_action_task("Fresh item", 4)
            send(b"\x01X")
            wait_visible(lambda: "XFresh item" in visible.text())
            action("p", "Priority task #4")
            send(b"7\r")
            read_until(b"Set priority task #4?")
            send(b"n")
            wait_visible(lambda: "XFresh item" in visible.text())
            assert cli("show", "4")["task"]["priority"] == 0
            action("c", "Complete task #4?")
            send(b"y")
            wait_visible(lambda: "Task 4 is not claimed by session worker" in visible.text())
            send(b"\x1b")
            wait_visible(lambda: "XFresh item" in visible.text() and
                         "Task 4 is not claimed by session worker" in visible.text().splitlines()[-1])
            assert cli("show", "4")["task"]["status"] == "new"
            send(b"\x7f")
            wait_visible(lambda: editor_line().startswith("Fresh item"))

            action("p", "Priority task #4")
            send(b"7\r")
            read_until(b"Priority #4: 7")
            assert cli("show", "4")["task"]["priority"] == 7
            send(b"\x01Y")
            wait_visible(lambda: "YFresh item" in visible.text())
            action("p", "Priority task #4")
            send(b"-5\r")
            wait_visible(lambda: "Set priority task #4?" in visible.text() and
                         "Lose draft?" in visible.text())
            send(b"y")
            read_until(b"Priority #4: -5")
            assert cli("show", "4")["task"]["priority"] == -5
            assert cli("show", "4")["task"]["description"] == "Fresh item"
            wait_visible(lambda: editor_line().startswith("Fresh item"))
            action("d", "Parent task #4")
            send(b"3\r")
            read_until(b"Parent #4: #3")
            assert cli("show", "4")["task"]["parent_id"] == 3
            action("d", "Parent task #4")
            send(b"none\r")
            read_until(b"Parent cleared #4")
            assert cli("show", "4")["task"]["parent_id"] is None
            action("a", "Archive task #4?")
            send(b"y")
            read_until(b"Archived #4")
            assert cli("show", "4")["task"]["archived"] is True

            select_action_task("Hidden item", 5)
            action("a", "Unarchive task #5?")
            send(b"y")
            read_until(b"Unarchived #5")
            assert cli("show", "5")["task"]["archived"] is False
            assert child.poll() is None
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout"
        elif scenario == "actions_rejected":
            def rejected_action(label, task_id, letter, prompt, error):
                clear_capture()
                click(5, task_row(label))
                wait_visible(lambda: f"task #{task_id}" in editor_title())
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "c Complete" in visible.text())
                send(letter.encode())
                read_until(prompt.encode())
                clear_capture()
                send(b"y")
                read_until(error.encode())
                send(b"\x1b")
                wait_visible(lambda: f"task #{task_id}" in editor_title())
                assert f"task #{task_id}" in editor_title()

            wait_visible(lambda: "Owned item" in visible.text() and "Hidden item" in visible.text())
            rejected_action("Owned item", 1, "a", "Archive task #1?",
                            "Task 1 is in progress and cannot be archived")
            rejected_action("Owned item", 1, "o", "Reopen task #1?",
                            "Task 1 must be completed to reopen")
            rejected_action("Finished item", 3, "r", "Retry task #3?",
                            "Task 3 is no longer in error")
            assert cli("show", "1")["task"]["archived"] is False
            assert cli("show", "3")["task"]["status"] == "completed"

            clear_capture()
            click(5, task_row("Fresh item"))
            wait_visible(lambda: 'task #4' in editor_title())
            send(b"\x07p101\r")
            wait_visible(lambda: "Priority must be -100..100" in visible.text())
            assert "101" in visible.text(), visible.text()
            send(b"\x1b")
            wait_visible(lambda: "task #4" in editor_title())
            send(b"\x07d4\r")
            read_until(b"Task 4 cannot depend on itself")
            send(b"\x1b")
            assert cli("show", "4")["task"]["parent_id"] is None
            assert cli("show", "4")["task"]["priority"] == 0
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout"
        elif scenario == "actions_hidden":
            clear_capture()
            send(CTRL_SLASH + b"Filtered")
            wait_visible(lambda: "Filter: Filtered" in visible.text() and
                         "Filtered item" in visible.text() and "Visible" not in visible.text())
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2" in editor_title())
            send(b"\x07a")
            read_until(b"Archive task #2?")
            send(b"y")
            wait_visible(lambda: "No matching tasks." in visible.text() and
                         "new task" in editor_title() and
                         "Filter: Filtered" in visible.text())
            assert cli("show", "2")["task"]["archived"] is True
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout"
        elif scenario == "actions_narrow":
            click(5, task_row("First"))
            wait_visible(lambda: 'task #1' in editor_title())
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 12, 0, 0))
            visible.resize(12, 8)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"qqq tasks")
            wait_visible(lambda: editor_row() is not None
                         and editor_line().startswith("First")
                         and (visible.x, visible.y) == (len("First"), editor_row() + 1))
            send(b"\x07")
            wait_visible(lambda: "c Complete" in visible.text() and
                         "d Parent" in visible.text() and "Esc cancel" in visible.text())
            send(b"p")
            wait_visible(lambda: "Enter -100" in visible.text())
            send(b"5\r")
            # Narrow footer clips value; wait for committed action before reading DB.
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Priority #1:"))
            assert cli("show", "1")["task"]["priority"] == 5
            send(b"\x07p101\r")
            wait_visible(lambda: "Priority must be -100..100" in
                         " ".join(row.strip() for row in visible.text().splitlines()[3:7]))
            send(b"\x1b")
            wait_visible(lambda: "qqq tasks" in visible.text().splitlines()[0])
            send(b"\x07d0\r")
            wait_visible(lambda: "Parent must be a positive task ID or none" in
                         " ".join(row.strip() for row in visible.text().splitlines()[3:8]))
            send(b"\x1b")
            wait_visible(lambda: "qqq tasks" in visible.text().splitlines()[0])
            send(b"X\x07p-1\r")
            wait_visible(lambda: "Lose draft?" in visible.text())
            send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Priority #1:"))
            assert cli("show", "1")["task"]["priority"] == -1
            send(b"\x07a")
            wait_visible(lambda: "Archive task" in visible.text())
            send(b"y")
            wait_visible(lambda: "Task 1 is in progress and cannot be archived" in
                         " ".join(row.strip() for row in visible.text().splitlines()[1:7]))
            send(b"\x1b")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
            visible.resize(72, 24)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: 'qqq task editor - task #1' in editor_title())
            wait_visible(lambda: "Task 1 is in progress and cannot be archived" in visible.text())
            settle()
        elif scenario == "click":
            initial_tasks = cli("list")
            wait_visible(lambda: "Parent detail" in visible.text() and "Child" in visible.text())
            settle()
            clear_capture()
            click(6, 1)
            click(6, 2)
            click(6, 3)
            click(6, 8)
            click(6, 9)
            click(6, 12)
            click(6, 14)
            settle()
            assert not screen, f"Inactive click redrew TUI: {screen[-500:]!r}"
            assert cli("list") == initial_tasks

            clear_capture()
            click(5, task_row("Parent detail"))
            wait_visible(lambda: 'task #1' in editor_title())
            wait_visible(lambda: editor_line().startswith("Parent"))
            clear_capture()
            send(b"\x1b[<65;6;4M")
            wait_visible(lambda: "word" in visible.text().splitlines()[6]
                         and "Child detail" not in visible.text().splitlines()[6])
            clear_capture()
            click(5, 7)
            wait_visible(lambda: 'task #2' in editor_title())
            wait_visible(lambda: editor_line().startswith("Child start"))
            clear_capture()
            click_editor(7)
            send(b"X")
            wait_visible(lambda: editor_line().startswith("Child Xstart"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #2")
            assert cli("show", "2")["task"]["description"] == (
                "Child Xstart\nChild detail " + "word " * 12
            )
        elif scenario == "click_filter":
            initial_tasks = cli("list")
            clear_capture()
            send(b"Unsaved" + CTRL_SLASH + b"needle")
            read_until(b"Filter: needle")
            clear_capture()
            send(b"\x1b[<64;6;4M" * 10)
            wait_visible(lambda: "Workspace" in visible.text().splitlines()[3]
                         and "Needle child" in visible.text().splitlines()[4]
                         and "Other" not in visible.text())
            clear_capture()
            send(b"\x1b[<65;6;4M" * 4)
            wait_visible(lambda: "Needle 13" in visible.text().splitlines()[2])
            settle()
            clear_capture()
            click_editor(3)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save"))
            clear_capture()
            send(CTRL_SLASH)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to filter"))
            clear_capture()
            click(6, task_row("Needle 14"))
            read_until(b"Discard changes and switch? (y/N)")
            clear_capture()
            send(b"n")
            wait_visible(lambda: "new task" in editor_title()
                         and "Unsaved" in editor_line()
                         and visible.text().splitlines()[-1].startswith("Type to filter"))
            assert cli("list") == initial_tasks
            clear_capture()
            click(6, task_row("Needle 14"))
            read_until(b"Discard changes and switch? (y/N)")
            send(b"y")
            wait_visible(lambda: 'task #14' in editor_title())
            wait_visible(lambda: editor_line().startswith("Needle 14"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x01!")
            wait_visible(lambda: editor_line().startswith("!Needle 14"))
            clear_capture()
            click(6, task_row("Needle 14"))
            settle()
            assert "Discard changes" not in visible.text(), visible.text()
            assert "!Needle 14" in visible.text(), visible.text()
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #14")
            assert cli("show", "14")["task"]["description"] == "!Needle 14"
        elif scenario == "click_editor_scroll":
            clear_capture()
            click(5, task_row("Line01"))
            wait_visible(lambda: 'task #3' in editor_title())
            wheel_editor(False, 10)
            wait_visible(lambda: editor_line().startswith("Line01"))
            clear_capture()
            wheel_editor(True, 1)
            wait_visible(lambda: editor_line().startswith("Line04"))
            clear_capture()
            click_editor(5)
            send(b"X")
            wait_visible(lambda: editor_line().startswith("LineX04"))
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #3")
            expected = "\n".join("LineX04" if index == 4 else f"Line{index:02}"
                                 for index in range(1, 13))
            assert cli("show", "3")["task"]["description"] == expected
        elif scenario == "click_error":
            settle()
            clear_capture()
            click(6, 7)
            settle()
            assert not screen, f"Blank list click redrew TUI: {screen[-500:]!r}"
            send(b"Unsaved")
            wait_visible(lambda: editor_line().startswith("Unsaved"))
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            clear_capture()
            click(5, 5)
            read_until(b"Task 2 not found")
            assert "new task" in editor_title()
            assert editor_line().startswith("Unsaved")
            assert cli("list")[0]["id"] == 1
        elif scenario == "wheel_error":
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DROP TABLE tasks")
            clear_capture()
            send(b"\x1b[<65;6;4M")
            deadline = time.monotonic() + 5
            while child.poll() is None:
                assert time.monotonic() < deadline, "TUI failed to exit after DB error"
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        capture(os.read(master, 65536))
                    except OSError:
                        pass
            assert child.returncode != 0
            assert b"\x1b[?1000l" in screen and b"\x1b[?1006l" in screen, screen[-2000:]
            assert screen.index(b"\x1b[?1006l") < screen.index(b"\x1b[?1049l")
            assert before[3] == termios.tcgetattr(slave)[3], "Terminal flags not restored after error"
        elif scenario == "wheel":
            initial_tasks = cli("list")
            assert "Line01" in visible.text(), visible.text()
            time.sleep(0.1)
            while select.select([master], [], [], 0)[0]:
                capture(os.read(master, 65536))
            clear_capture()
            send(b"\x1b[<35;6;4M" * 20)
            time.sleep(0.1)
            while select.select([master], [], [], 0)[0]:
                capture(os.read(master, 65536))
            assert not screen, f"Mouse movement redrew TUI: {screen[-500:]!r}"
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'task #20' in editor_title())
            wheel_editor(False, 10)
            wait_visible(lambda: editor_line().startswith("Line01"))

            clear_capture()
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2])
            assert editor_line().startswith("Line01"), visible.text()
            clear_capture()
            wheel_editor(True, 1)
            wait_visible(lambda: editor_line().startswith("Line04"))
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x11")
            time.sleep(0.1)
            while select.select([master], [], [], 0)[0]:
                capture(os.read(master, 65536))
            assert editor_line().startswith("Line04"), visible.text()

            clear_capture()
            send(b"\x1b[<65;6;8M\x1b[<65;6;13M")
            time.sleep(0.1)
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            assert editor_line().startswith("Line04"), visible.text()
            clear_capture()
            wheel_editor(False, 10)
            wait_visible(lambda: editor_line().startswith("Line01"))
            clear_capture()
            wheel_editor(True, 10)
            wait_visible(lambda: editor_line().startswith("Line07"))
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x1b[<64;6;4M" * 10)
            wait_visible(lambda: "ID" in visible.text().splitlines()[2])
            assert editor_line().startswith("Line07"), visible.text()
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Task 18" in visible.text().splitlines()[2]
                         and "Line03..." in visible.text().splitlines()[6])

            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 72, 0, 0))
            visible.resize(72, 36)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2]
                         and editor_line().startswith("Line02")
                         and (visible.x, visible.y) == (6, 34))
            settle()
            clear_capture()
            send(b"!")
            wait_visible(lambda: "Line15!" in "\n".join(visible.text().splitlines()[editor_row() + 1:-1]))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"Discard changes and switch? (y/N)")
            clear_capture()
            send(b"y")
            wait_visible(lambda: 'task #19' in editor_title())
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2]
                         and editor_line().startswith("Task 19"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A" * 18)
            wait_visible(lambda: "task #1 (" in editor_title())
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2])
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"No older task")
            wait_visible(lambda: "First" in visible.text().splitlines()[2])
        elif scenario == "filter_shortcuts":
            initial_tasks = cli("list")
            send(b"Draft/path")
            wait_visible(lambda: editor_line().startswith("Draft/path"))
            for shortcut in (
                CTRL_SLASH,
                b"\x1b[47;5u",       # CSI-u Ctrl+/.
                b"\x1b[95;6u",       # CSI-u Ctrl+Shift+_ (same legacy byte).
            ):
                clear_capture()
                send(shortcut + b"First")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: First"
                             and visible.text().splitlines()[-1].startswith("Type to filter"))
                assert "Second" not in visible.text(), visible.text()
                assert editor_line().startswith("Draft/path"), visible.text()
                send(shortcut + b"/")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: First/")
                assert cli("list") == initial_tasks
                send(b"\x1b")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter:")
                send(b"\x1b")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save")
                             and "Ctrl+/ filter" in visible.text().splitlines()[-1])
            send(b"\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft/path"
        elif scenario == "slash_edit":
            send(b"/")
            wait_visible(lambda: editor_line().startswith("/")
                         and visible.text().splitlines()[-1].startswith("Ctrl-S save"))
            send(b"\x1b/")
            wait_visible(lambda: editor_line().startswith("//"))
            send(b"path\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "//path"
        elif scenario in ("filter", "filter_no_color"):
            initial_tasks = cli("list")
            send(b"Unsaved")
            clear_capture()
            send(CTRL_SLASH + b"needle")
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
            wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: needle/zzzz"
                         and "No matching tasks." in visible.text())
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter:"
                         and "Other" in visible.text())
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: visible.y == editor_row() + 1)
            send(b"!")
            wait_visible(lambda: "Unsaved!" in visible.text())

            clear_capture()
            send(CTRL_SLASH + b"needle\t")
            read_until(b"Filter: needle")
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #4")
            assert cli("show", "4")["task"]["description"] == "Unsaved!"
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'task #2' in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'task #1' in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: 'task #2' in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            read_until(b"new task")
            assert cli("list")[0:3] == initial_tasks

            clear_capture()
            send(CTRL_SLASH)
            wait_visible(lambda: visible.y == 1 and visible.text().splitlines()[1].strip() == "Filter: needle")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 7, 10, 0, 0))
            visible.resize(10, 7)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Resize ter")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
            visible.resize(72, 24)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"Filter: needle")
            read_until(b"\x1b[2;15H")
            time.sleep(0.1)
            clear_capture()
            send(b"\rx")
            wait_visible(lambda: editor_line().startswith("x"))
            send(b"\x7f")
            wait_visible(lambda: editor_line().strip() == "")
        elif scenario in ("save", "save_json"):
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'task #2' in editor_title())
            assert "> 2" in visible.text(), visible.text()
            clear_capture()
            send(b"\x05 edited\x13")
            read_until(b"Saved #2")
            assert cli("show", "2")["task"]["description"] == "Second edited"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in editor_title())
            clear_capture()
            send(b"Third\x13")
            read_until(b"Saved #3")
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
            wait_visible(lambda: 'qqq task editor - task #2' in editor_title())
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
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == (
                "Prefix \n```pasteboard\n" + payload + "\n```"
            )
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'task #3' in editor_title())
            wait_visible(lambda: "task #3" in editor_title() and
                         "[Pasted Content 1001 chars]" in visible.text() and
                         visible.text().splitlines()[-1].startswith("Ctrl-S save"))
            if scenario == "pasteboard":
                assert b"38;5;222" in screen, "Reloaded paste accent missing"
            else:
                assert b"\x1b[38;" not in screen, screen[-2000:]
            clear_capture()
            send(b"\x1b[C" * (len("Prefix \n") + 1) + b"\x17\x13")
            read_until(b"Saved #3")
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
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft"
        elif scenario == "save_error":
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'qqq task editor - task #2' in editor_title())
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
            read_until(b"Saved #2")
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
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
            visible.resize(72, 24)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: 'qqq task editor - new task' in editor_title())
            read_until(b"Draft")
            wait_visible(lambda: (visible.x, visible.y) == (5, editor_row() + 1))
            settle()
            clear_capture()
            assert not (termios.tcgetattr(slave)[0] & termios.IXON), termios.tcgetattr(slave)
            send(b"\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft"
        if scenario != "wheel_error":
            if scenario in ("ctrl_c_new_discard", "ctrl_c_new_image", "ctrl_c_new_whitespace"):
                send(b"y")
                assert cli("list") == initial_tasks
            else:
                send(b"\x1b" if scenario in ("empty", "empty_json", "workflow_empty", "escape_selected", "escape_dirty_selected") else b"\x03\x03y")
        deadline = time.monotonic() + 5
        while child.poll() is None:
            assert time.monotonic() < deadline, f"TUI failed to exit: {screen[-1000:]!r}\n{visible.text()}"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    capture(os.read(master, 65536))
                except OSError:
                    pass
        stdout, _ = child.communicate(timeout=5)
        # Child exit can precede final PTY read; collect terminal cleanup bytes.
        while select.select([master], [], [], 0)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            capture(chunk)
        if scenario != "wheel_error":
            assert child.returncode == 0, screen[-2000:]
        assert stdout == b"", stdout
        if scenario.startswith("child"):
            assert b"\x1b[>1u" in screen, "Modified-key reporting not requested"
            assert b"\x1b[<1u" in screen, "Keyboard reporting mode not restored"
        assert before[3] == termios.tcgetattr(slave)[3], "Terminal flags not restored"
        assert b"\x1b[?1049l" in screen, "Alternate screen not restored"
        assert b"\x1b[?1000l" in screen and b"\x1b[?1006l" in screen, "Mouse capture not restored"
        assert b"\x1b[?2004l" in screen, "Bracketed paste not restored"
        assert screen.index(b"\x1b[?2004l") < screen.index(b"\x1b[?1049l")
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
