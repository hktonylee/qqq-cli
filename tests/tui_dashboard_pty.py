"""Exercise two-panel qqq tui through a real terminal."""

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
    if scenario in ("scroll", "wheel", "live_refresh_scroll"):
        for index in range(3, 21):
            description = ("\n".join(f"Line{line:02}" for line in range(1, 16))
                           if scenario == "wheel" and index == 20 else f"Task {index}")
            cli("add", description)
    if scenario == "click_editor_scroll":
        cli("add", "\n".join(f"Line{index:02}" for index in range(1, 13)))

    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
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

    def click(column, row):
        send(f"\x1b[<0;{column};{row}M\x1b[<0;{column};{row}m".encode())

    def task_row(label):
        for row, content in enumerate(visible.text().splitlines()[2:7], 3):
            if label in content:
                return row
        raise AssertionError(f"Task row {label!r} not visible:\n{visible.text()}")

    def settle():
        time.sleep(0.1)
        while select.select([master], [], [], 0)[0]:
            capture(os.read(master, 65536))

    try:
        read_until(b"qqq task editor - new task")
        assert "qqq tasks" in visible.text(), visible.text()
        read_until(b"\x1b[?1000h")
        read_until(b"\x1b[?1006h")
        read_until(b"\x1b[?2004h")
        if scenario == "color":
            read_until(b"38;5;81")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"48;5;81")
        elif scenario in ("no_color", "dumb", "pasteboard_no_color", "filter_no_color"):
            assert b"\x1b[38;" not in screen, screen[-2000:]
            assert b"\x1b[48;" not in screen, screen[-2000:]
        if scenario not in ("live_refresh_scroll", "tree_navigation", "scroll", "wheel", "click", "click_filter", "workflow", "workflow_empty", "workflow_status", "filter", "filter_no_color", "archive_hidden", "archive_included", "actions_basic", "actions_rejected", "actions_hidden"):
            read_until(b"Second")
            assert "First" in visible.text() and "Second" in visible.text(), visible.text()
        assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout while open"
        if scenario in ("archive_hidden", "archive_included"):
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
                wait_visible(lambda: visible.x > 0 and visible.y == 9)
            if scenario == "ctrl_c_new_filter":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: first")
            clear_capture()
            send(b"\x03")
            read_until(b"Discard draft? (y/N)")
            assert cli("list") == initial_tasks
            assert child.poll() is None
            settle()
            clear_capture()
            send(b"\x03")
            wait_visible(lambda: bool(screen)
                         and visible.text().splitlines()[-1].startswith("Discard draft? (y/N)"))
            assert child.poll() is None
            if scenario in ("ctrl_c_new_keep", "ctrl_c_new_filter"):
                send(b"n")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save")
                             and visible.text().splitlines()[9].startswith("Unsaved draft"))
                send(b"\x13")
                wait_visible(lambda: "task #3 (" in visible.text().splitlines()[8])
                assert cli("show", "3")["task"]["description"] == "Unsaved draft"
        elif scenario in ("live_title", "live_title_filtered"):
            cli("edit", "2", "--priority", "8")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (New)" in visible.text().splitlines()[8])
            send(b"Draft ")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Draft Second"))
            if scenario == "live_title_filtered":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: visible.text().splitlines()[1].strip() == "Filter: first"
                             and "Second" not in "\n".join(visible.text().splitlines()[2:7]))
            def title_status(status):
                wait_visible(lambda: f"task #2 ({status})" in visible.text().splitlines()[8])
                assert visible.text().splitlines()[9].startswith("Draft Second")
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
            assert "new task" in visible.text().splitlines()[8]
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
            wait_visible(lambda: "task #2 (" in visible.text().splitlines()[8])
            send(b"Unsaved " + CTRL_SLASH + b"second")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Unsaved Second")
                         and visible.text().splitlines()[1].strip() == "Filter: second"
                         and (visible.x, visible.y) == (len("Filter: second"), 1))
            cursor_before = (visible.x, visible.y)
            cli("edit", "2", "--description", "Updated second externally")
            wait_visible(lambda: "Updated second externally" in visible.text().splitlines()[3])
            cli("add", "Second external task")
            wait_visible(lambda: "Second external task" in visible.text()
                         and (visible.x, visible.y) == cursor_before)
            assert "First" not in visible.text()
            assert visible.text().splitlines()[9].startswith("Unsaved Second")
            assert "task #2 (" in visible.text().splitlines()[8]
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
            wait_visible(lambda: "task #20 (" in visible.text().splitlines()[8])
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2])
            cli("edit", "13", "--description", "Refreshed row")
            wait_visible(lambda: "Refreshed row" in visible.text().splitlines()[2])
            assert "task #20 (" in visible.text().splitlines()[8]
            assert visible.text().splitlines()[9].startswith("Task 20")
        elif scenario == "tree_navigation":
            expected = (2, 3, 1)
            for task_id in expected:
                clear_capture()
                send(b"\x1b[1;2A")
                wait_visible(lambda: f"task #{task_id} (" in visible.text().splitlines()[8])
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #3 (" in visible.text().splitlines()[8])
            send(b"\x1b[1;2B")
            wait_visible(lambda: "task #2 (" in visible.text().splitlines()[8])
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8])
        elif scenario in ("escape_selected", "ctrl_c_selected", "escape_dirty_selected", "ctrl_c_dirty_selected"):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #2 (" in visible.text().splitlines()[8])
            if scenario in ("escape_dirty_selected", "ctrl_c_dirty_selected"):
                send(b"Changed ")
                wait_visible(lambda: visible.text().splitlines()[9].startswith("Changed Second"))
            key = b"\x03" if scenario.startswith("ctrl_c") else b"\x1b"
            clear_capture()
            send(key)
            if scenario == "escape_dirty_selected":
                read_until(b"Discard changes and switch? (y/N)")
                send(b"n")
                wait_visible(lambda: visible.text().splitlines()[9].startswith("Changed Second")
                             and visible.text().splitlines()[-1].startswith("Ctrl-S save"))
                clear_capture()
                send(b"\x1b")
                read_until(b"Discard changes and switch? (y/N)")
                send(b"y")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8]
                         and visible.text().splitlines()[9].strip() == "")
            assert cli("list") == initial_tasks
            assert child.poll() is None
        elif scenario == "save_selected":
            send(b"Created\x13")
            wait_visible(lambda: "task #3 (New)" in visible.text().splitlines()[8]
                         and visible.text().splitlines()[9].startswith("Created"))
            assert cli("show", "3")["task"]["description"] == "Created"
            send(b"\x05 updated\x13")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Created updated")
                         and visible.text().splitlines()[-1].startswith("Saved #3")
                         and (visible.x, visible.y) == (0, 9))
            assert len(cli("list")) == 3
            assert cli("show", "3")["task"]["description"] == "Created updated"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8])
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
            read_until(b"task #14")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Target 14"))
            clear_capture()
            click(10, 10)
            send(b" updated")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Target 14 updated"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #14")
            assert cli("show", "14")["task"]["description"] == "Target 14 updated"
            assert child.poll() is None
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout after update"
            send(b"\x1b[1;2B" * 7)
            wait_visible(lambda: "new task" in visible.text().splitlines()[8]
                         and visible.text().splitlines()[9].strip() == "")
            clear_capture()
            send(b"\x13")
            read_until(b"Task description cannot be empty")
            clear_capture()
            send(b"New target ")
            send(b"\x1b[200~details\x1b[201~")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("New target details"))
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
            assert visible.text().splitlines()[8].startswith("qqq task editor - new task")
        elif scenario == "workflow_status":
            wait_visible(lambda: "Completed" in visible.text()
                         and "Error" in visible.text()
                         and "Fresh item" in visible.text())
            assert "New          Fresh item" in visible.text(), visible.text()
            clear_capture()
            send(b"\x1b[1;2A" * 2)
            wait_visible(lambda: "task #2 (Error)" in visible.text().splitlines()[8])
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "task #1 (Completed)" in visible.text().splitlines()[8])
        elif scenario == "actions_basic":
            wait_visible(lambda: "Owned item" in visible.text() and "Hidden item" in visible.text())
            def select_action_task(label, task_id):
                clear_capture()
                click(5, task_row(label))
                wait_visible(lambda: f"task #{task_id}" in visible.text().splitlines()[8])

            def action(letter, prompt):
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "Task actions" in visible.text() and "c Complete" in visible.text())
                clear_capture()
                send(letter.encode())
                read_until(prompt.encode())

            click(5, task_row("Owned item"))
            read_until(b"task #1 (In progress)")
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
            send(b"X")
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
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Fresh item"))

            action("p", "Priority task #4")
            send(b"7\r")
            read_until(b"Priority #4: 7")
            assert cli("show", "4")["task"]["priority"] == 7
            send(b"Y")
            wait_visible(lambda: "YFresh item" in visible.text())
            action("p", "Priority task #4")
            send(b"-5\r")
            wait_visible(lambda: "Set priority task #4?" in visible.text() and
                         "Lose draft?" in visible.text())
            send(b"y")
            read_until(b"Priority #4: -5")
            assert cli("show", "4")["task"]["priority"] == -5
            assert cli("show", "4")["task"]["description"] == "Fresh item"
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Fresh item"))
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
                wait_visible(lambda: f"task #{task_id}" in visible.text().splitlines()[8])
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "c Complete" in visible.text())
                send(letter.encode())
                read_until(prompt.encode())
                clear_capture()
                send(b"y")
                read_until(error.encode())
                send(b"\x1b")
                wait_visible(lambda: f"task #{task_id}" in visible.text().splitlines()[8])
                assert f"task #{task_id}" in visible.text().splitlines()[8]

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
            read_until(b"task #4")
            send(b"\x07p101\r")
            wait_visible(lambda: "Priority must be -100..100" in visible.text())
            assert "101" in visible.text(), visible.text()
            send(b"\x1b")
            wait_visible(lambda: "task #4" in visible.text().splitlines()[8])
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
            wait_visible(lambda: "task #2" in visible.text().splitlines()[8])
            send(b"\x07a")
            read_until(b"Archive task #2?")
            send(b"y")
            wait_visible(lambda: "No matching tasks." in visible.text() and
                         "new task" in visible.text().splitlines()[8] and
                         "Filter: Filtered" in visible.text())
            assert cli("show", "2")["task"]["archived"] is True
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout"
        elif scenario == "actions_narrow":
            click(5, task_row("First"))
            read_until(b"task #1")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 12, 0, 0))
            visible.resize(12, 8)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"qqq tasks")
            read_until(b"\x1b[?25h\x1b[6;1H")
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
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 72, 0, 0))
            visible.resize(72, 16)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            read_until(b"qqq task editor - task #1")
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
            click(6, 16)
            settle()
            assert not screen, f"Inactive click redrew TUI: {screen[-500:]!r}"
            assert cli("list") == initial_tasks

            clear_capture()
            click(5, task_row("Parent detail"))
            read_until(b"task #1")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Parent"))
            clear_capture()
            send(b"\x1b[<65;6;4M")
            wait_visible(lambda: "word" in visible.text().splitlines()[6]
                         and "Child detail" not in visible.text().splitlines()[6])
            clear_capture()
            click(5, 7)
            read_until(b"task #2")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Child start"))
            clear_capture()
            click(7, 10)
            send(b"X")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Child Xstart"))
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
            click(3, 10)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S save"))
            clear_capture()
            send(CTRL_SLASH)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to filter"))
            clear_capture()
            click(6, task_row("Needle 14"))
            read_until(b"Discard changes and switch? (y/N)")
            clear_capture()
            send(b"n")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8]
                         and "Unsaved" in visible.text().splitlines()[9]
                         and visible.text().splitlines()[-1].startswith("Type to filter"))
            assert cli("list") == initial_tasks
            clear_capture()
            click(6, task_row("Needle 14"))
            read_until(b"Discard changes and switch? (y/N)")
            send(b"y")
            read_until(b"task #14")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Needle 14"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"!")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("!Needle 14"))
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
            read_until(b"task #3")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line01"))
            clear_capture()
            send(b"\x1b[<65;6;11M")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line04"))
            clear_capture()
            click(5, 10)
            send(b"X")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("LineX04"))
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
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Unsaved"))
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            clear_capture()
            click(5, 5)
            read_until(b"Task 2 not found")
            assert "new task" in visible.text().splitlines()[8]
            assert visible.text().splitlines()[9].startswith("Unsaved")
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
            read_until(b"task #20")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line01"))

            clear_capture()
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 13" in visible.text().splitlines()[2])
            assert visible.text().splitlines()[9].startswith("Line01"), visible.text()
            clear_capture()
            send(b"\x1b[<65;6;11M")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line04"))
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x11")
            time.sleep(0.1)
            while select.select([master], [], [], 0)[0]:
                capture(os.read(master, 65536))
            assert visible.text().splitlines()[9].startswith("Line04"), visible.text()

            clear_capture()
            send(b"\x1b[<65;6;8M\x1b[<65;6;16M")
            time.sleep(0.1)
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            assert visible.text().splitlines()[9].startswith("Line04"), visible.text()
            clear_capture()
            send(b"\x1b[<64;6;11M" * 10)
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line01"))
            clear_capture()
            send(b"\x1b[<65;6;11M" * 10)
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Line10"))
            assert "Task 13" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x1b[<64;6;4M" * 10)
            wait_visible(lambda: "ID" in visible.text().splitlines()[2])
            assert visible.text().splitlines()[9].startswith("Line10"), visible.text()
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Line11" in visible.text().splitlines()[2])

            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
            visible.resize(72, 24)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: "Line07" in visible.text().splitlines()[2]
                         and visible.text().splitlines()[13].startswith("Line06"))
            time.sleep(0.1)
            clear_capture()
            send(b"!")
            wait_visible(lambda: visible.text().splitlines()[13].startswith("!Line01"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"Discard changes and switch? (y/N)")
            clear_capture()
            send(b"y")
            read_until(b"task #19")
            wait_visible(lambda: "Task 19" in visible.text().splitlines()[2])
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A" * 18)
            wait_visible(lambda: "task #1 (" in visible.text().splitlines()[12])
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Line07" in visible.text().splitlines()[2])
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"No older task")
            wait_visible(lambda: "First" in visible.text().splitlines()[2])
        elif scenario == "filter_shortcuts":
            initial_tasks = cli("list")
            send(b"Draft/path")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("Draft/path"))
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
                assert visible.text().splitlines()[9].startswith("Draft/path"), visible.text()
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
            wait_visible(lambda: visible.text().splitlines()[9].startswith("/")
                         and visible.text().splitlines()[-1].startswith("Ctrl-S save"))
            send(b"\x1b/")
            wait_visible(lambda: visible.text().splitlines()[9].startswith("//"))
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
            wait_visible(lambda: visible.y == 9)
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
            send(CTRL_SLASH)
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
            read_until(b"Saved #2")
            assert cli("show", "2")["task"]["description"] == "Second edited"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8])
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
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == (
                "Prefix \n```pasteboard\n" + payload + "\n```"
            )
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "new task" in visible.text().splitlines()[8])
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"task #3")
            wait_visible(lambda: "task #3" in visible.text().splitlines()[8] and
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
