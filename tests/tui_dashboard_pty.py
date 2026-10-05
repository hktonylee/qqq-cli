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
        self.cursor_visible = True
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
                assert char.isascii() or char in "┌┐└┘├─│╔╗╚╝═║·✓×", f"Unsupported screen character {char!r}"
                if self.x >= self.width:
                    self.x = 0
                    self.y = min(self.height - 1, self.y + 1)
                self.cells[self.y][self.x] = char
                self.x += 1
            index += 1
        self.pending = text[index:]

    def csi(self, parameters, command):
        if parameters == "?25" and command in ("h", "l"):
            self.cursor_visible = command == "h"
            return
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
    if scenario in ("no_color", "pasteboard_no_color", "filter_no_color", "details_no_color", "actions_popup_no_color", "wide_layout_no_color", "menu_arrows_no_color", "filter_escape_empty_no_color", "filter_ctrl_c_empty_no_color", "compact_layout_no_color", "handoff_hint_no_color", "menu_retry_new_no_color", "menu_retry_error_no_color", "content_conflict_no_color", "details_assignment_no_color"):
        env["NO_COLOR"] = "1"
    elif scenario == "dumb":
        env["TERM"] = "dumb"
    if scenario.startswith(("buffers_", "long_description_", "prerequisites", "dirty_marker", "completed_toggle", "jump", "menu_error_", "force_complete", "list_selection")) and scenario.endswith("no_color"):
        env["NO_COLOR"] = "1"
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
    elif scenario.startswith("force_complete"):
        cli("add", "Foreign item")
        cli("next", "--local", "--session", "foreign")
        cli("add", "Failed item")
        cli("next", "--local", "--session", "failed")
        cli("edit", "2", "--set-status", "error", "--reason", "Failed", "--session", "failed")
        cli("add", "Fresh item")
        cli("add", "Owned item", "--priority", "10")
        if scenario == "force_complete_native":
            env["CODEX_THREAD_ID"] = "native-key"
            env["CODEX_SESSION_ID"] = "native-display"
            cli("next", "--local")
        else:
            cli("next", "--local", "--session", "worker")
        cli("add", "Finished item", "--priority", "20")
        cli("next", "--local", "--session", "finished")
        cli("complete", "5", "--session", "finished")
        if scenario == "force_complete_owner_db_error":
            with sqlite3.connect(Path(folder) / ".qqq" / "qqq.db") as conn:
                conn.execute("INSERT INTO herdr_links(task_id,link_json) VALUES (4,'broken')")
            fake = Path(folder) / "herdr"
            fake.write_text('''#!/usr/bin/env python3
import json
print(json.dumps({"result": {"pane": {
    "pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1",
    "terminal_id": "test-terminal", "agent": "codex"
}}}))
''')
            fake.chmod(0o755)
            env["PATH"] = os.pathsep.join((folder, os.defpath))
            env["HERDR_PANE_ID"] = "w1:p1"
    elif scenario in ("actions_basic", "actions_rejected") or scenario.startswith(("menu_retry_", "menu_error_")):
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
    elif scenario.startswith("wide_layout"):
        cli("add", "First")
        cli("add", "Second " + "word " * 24 + "TAIL")
        cli("message", "2", "\n".join(f"Message line {index:02}" for index in range(1, 31)))
        for index in range(3, 21):
            cli("add", f"Task {index}")
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
    elif scenario.startswith("completed_toggle"):
        cli("add", "Finished parent")
        cli("add", "Needle child", "--parent", "1")
        cli("add", "Needle other")
        cli("next", "--local", "--session", "worker")
        cli("complete", "1", "--session", "worker")
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
    if scenario in ("details_assignment", "details_assignment_no_color"):
        cli("next", "--local", "--session", "older")
        cli("next", "--local", "--session", "worker", "--harness-name", "codex",
            "--harness-session", "00000000-0000-0000-0000-000000000000",
            "--orchestrator-name", "herdr", "--orchestrator-session", "default")
    if scenario in ("scroll", "wheel", "live_refresh_scroll", "ctrl_c_new_scroll"):
        for index in range(3, 21):
            description = ("\n".join(f"Line{line:02}" for line in range(1, 16))
                           if scenario == "wheel" and index == 20 else f"Task {index}")
            if scenario == "scroll" and index == 20:
                description += "\nNewest preview second\nNewest preview tail"
            cli("add", description)
    if scenario == "click_editor_scroll":
        cli("add", "\n".join(f"Line{index:02}" for index in range(1, 13)))
    if scenario in ("prerequisites", "prerequisites_no_color"):
        cli("edit", "2", "--depends-on", "1")
    if scenario == "jump_archived":
        cli("archive", "2")
        for index in range(3, 21):
            cli("add", f"Task {index}")
    if scenario == "jump_completed":
        cli("next", "--local", "--session", "completed", "--filter", "id == 2")
        cli("complete", "2", "--session", "completed")
    if scenario.startswith("dirty_marker"):
        cli("edit", "1", "--description", "First " + "unchanged clean words " * 8 + "\nClean tail")
        cli("edit", "2", "--description", "Second " + "wrapped saved words " * 8 + "\nContinuation\nTail")
    if scenario.startswith("content_conflict"):
        stored_image = Path(folder) / "stored.png"
        stored_image.write_bytes(b"\x89PNG\r\n\x1a\nstored")
        cli("edit", "2", "--image", str(stored_image))
        pending_image = Path(folder) / "pending.png"
        pending_image.write_bytes(b"\x89PNG\r\n\x1a\npending")
    if scenario.startswith("details") and scenario != "details_deleted":
        if scenario == "details_scroll":
            cli("message", "2", "\n".join(f"Message line {index:02}" for index in range(1, 31)), "--session", "reviewer")
        else:
            cli("message", "2", "Earlier message", "--session", "worker")
            message = "Latest message\nMessage continuation"
            if scenario in ("details", "details_no_color"):
                message += "\n" + "W" * 66 + "TAIL"
            cli("message", "2", message, "--session", "reviewer")

    if scenario.startswith("long_description_"):
        long_body = "Start marker\n" + "Ordinary row with visible editable text\n" * 60 + "End marker"
        stored_description = (f"```text\n{long_body}\n```" if scenario.endswith("text_fence") else
                              f"```pasteboard\n{long_body}\n```" if scenario.endswith("pasteboard") else long_body)
        cli("edit", "2", "--description", stored_description)

    if scenario == "buffers_scroll":
        cli("edit", "2", "--description", "\n".join(f"Line{index:02}" for index in range(1, 21)))

    if scenario.startswith("handoff_"):
        if scenario == "handoff_scroll":
            cli("edit", "2", "--description", "Second\n" + "\n".join(f"Line {index:02}" for index in range(1, 21)))
            cli("message", "2", "\n".join(f"Message {index:02}" for index in range(1, 31)))
            for index in range(3, 21):
                cli("add", f"Task {index}")
        live_pane = {
            "pane_id": "live:p2", "workspace_id": "live", "tab_id": "live:t1",
            "agent": "codex", "terminal_id": "live-terminal",
            "agent_session": {"agent": "codex", "kind": "id", "value": "linked-agent"},
        }
        identity = (live_pane["agent_session"] if scenario != "handoff_terminal"
                    else {"agent": "codex", "kind": "terminal", "value": "live-terminal"})
        cached_pane = dict(live_pane, pane_id="cached:p1")
        if scenario not in ("handoff_missing", "handoff_no_selection"):
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                connection.execute("INSERT INTO herdr_links(task_id,link_json) VALUES(?,?)", (
                    2, json.dumps({"server": "named", "identity": identity, "pane": cached_pane}),
                ))
        fake = Path(folder) / "herdr"
        fake.write_text(f"#!{sys.executable}\n" + r'''
import json, os, sys, termios
from pathlib import Path
args = sys.argv[1:]
with open("herdr-calls", "a") as log:
    log.write(json.dumps(args) + "\n")
assert args[:2] == ["--session", "named"], args
args = args[2:]
scenario = os.environ["HANDOFF_SCENARIO"]
pane = json.loads(os.environ["HANDOFF_PANE"])
if args == ["agent", "list"]:
    agents = ([] if scenario == "handoff_stale" else [pane, pane]
              if scenario == "handoff_ambiguous" else [pane])
    result = {"agents": agents}
elif args == ["agent", "focus", "live:p2"]:
    if scenario == "handoff_focus_error":
        print("focus denied", file=sys.stderr)
        sys.exit(1)
    if scenario == "handoff_launch_error":
        Path(sys.argv[0]).unlink()
    result = {}
elif not args:
    assert all(os.isatty(fd) for fd in (0, 1, 2)), "client lacks terminal streams"
    flags = termios.tcgetattr(0)[3]
    assert flags & termios.ICANON and flags & termios.ECHO, "client inherits raw mode"
    Path("herdr-client-opened").write_text("cooked")
    print("\x1b[2JHERDR_CLIENT", flush=True)
    sys.exit(1 if scenario == "handoff_client_error" else 0)
else:
    raise AssertionError(args)
print(json.dumps({"result": result}))
''')
        fake.chmod(0o755)
        # Prevent launch-failure regression from falling through to real Herdr.
        env["PATH"] = os.pathsep.join((folder, os.defpath))
        env["HANDOFF_SCENARIO"] = scenario
        env["HANDOFF_PANE"] = json.dumps(live_pane)

    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
    before = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    args = [binary, "--json", "tui"] if scenario in ("empty_json", "save_json", "handoff_json") else [binary, "tui"]
    if scenario == "force_complete_native":
        args.extend(["--session", "native-display"])
    elif scenario in ("actions_basic", "actions_rejected", "actions_narrow") or scenario.startswith(("menu_retry_", "menu_error_")) or (scenario.startswith("force_complete") and scenario not in ("force_complete_sessionless", "force_complete_owner_db_error")):
        args.extend(["--session", "other" if scenario == "menu_error_rejected" else "worker"])
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
            assert child.poll() is None, f"Editor exited before visible state: {screen[-2000:]!r}\n{visible.text()}"

    def wait_frame(predicate):
        def ready():
            if not predicate() or visible.pending or visible.decoder.getstate()[0]:
                return False
            # Toggle scenario keeps ASCII query/editor carets at text end.
            # Match expected final cursor, never an intermediate paint cursor.
            if visible.text().splitlines()[-1].startswith("Type to Filter"):
                query = filter_text().removeprefix("Filter:").lstrip()
                cursor = (8 + len(query), 0)
            else:
                cursor = (len(editor_line().rstrip()), editor_row() + 1)
            return ((visible.x, visible.y) == cursor
                    and screen.endswith(f"\x1b[{cursor[1] + 1};{cursor[0] + 1}H".encode()))
        wait_visible(ready)

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
        if visible.width < 60 or visible.width >= 150:
            row = editor_row()
            return row - 1 if row is not None else 3
        # Details box starts immediately after final task-list row.
        # Compact panes lack borders; minimum layout retains four list rows.
        return next((index - 1 for index, row in enumerate(visible.text().splitlines())
                     if row.startswith("╔")), 3)

    def list_width():
        return (visible.width - (visible.width * 40 + 50) // 100
                if visible.width >= 150 else visible.width)

    def filter_text():
        width = list_width()
        button_width = 13 if width >= 24 else 3
        return visible.text().splitlines()[0][:width - button_width - 1].rstrip()

    def list_text():
        return "\n".join(row[:list_width()] for row in visible.text().splitlines()[:list_bottom() + 1])

    def completed_button_text():
        width = list_width()
        return visible.text().splitlines()[0][width - (13 if width >= 24 else 3):width]

    def editor_row():
        return next((index for index, row in enumerate(visible.text().splitlines())
                     if row.startswith("Task Editor"[:visible.width])), None)

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
        if row is None:
            return ""
        if visible.width >= 150:
            start = visible.width - (visible.width * 40 + 50) // 100
            return "\n".join(line[start + 3:-3].rstrip()
                             for line in visible.text().splitlines()[1:row - 1])
        lines = visible.text().splitlines()[list_bottom() + 1:row]
        return "\n".join(line[3:-3].rstrip() if line.startswith("║") else line[2:].rstrip()
                         for line in lines if not line.startswith(("╔", "╚")))

    def scroll_details_to(predicate, up=False):
        for _ in range(50):
            settle()
            if predicate(details_text()):
                return
            send(b"\x1b[5~" if up else b"\x1b[6~")
        settle()
        assert predicate(details_text()), visible.text()

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
        lines = visible.text().splitlines()
        start = int(lines[0].startswith("Filter:"))
        for row, content in enumerate(lines[start:list_bottom() + 1], start + 1):
            if label in content:
                return row
        raise AssertionError(f"Task row {label!r} not visible:\n{visible.text()}")

    def settle():
        time.sleep(0.1)
        while select.select([master], [], [], 0)[0]:
            capture(os.read(master, 65536))

    try:
        wait_visible(lambda: 'Task Editor - New Task' in editor_title())
        wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                     and (visible.x, visible.y) == (0, editor_row() + 1))
        assert visible.text().splitlines()[0].strip(), visible.text()
        assert not visible.text().splitlines()[0].startswith("Filter:"), visible.text()
        assert not any(line.split()[:3] == ["ID", "STATUS", "TASK"]
                       for line in visible.text().splitlines()[:list_bottom() + 1]), visible.text()
        assert "qqq tasks" not in visible.text(), visible.text()
        read_until(b"\x1b[?1000h")
        read_until(b"\x1b[?1006h")
        read_until(b"\x1b[?2004h")
        if scenario == "color":
            read_until(b"38;5;81")
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"48;2;15;51;62")
            settle()
            assert b"\x1b[4m" not in screen, screen[-2000:]
            assert b"48;5;81" not in screen, screen[-2000:]
            assert b"48;5;17" not in screen, screen[-2000:]
        elif scenario in ("no_color", "dumb", "pasteboard_no_color", "filter_no_color"):
            assert b"\x1b[38;" not in screen, screen[-2000:]
            assert b"\x1b[48;" not in screen, screen[-2000:]
            if scenario in ("no_color", "dumb"):
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #2 (New)" in editor_title()
                             and editor_line().startswith("Second"))
                settle()
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        if not scenario.startswith(("wide_layout", "menu_retry_", "menu_error_", "long_description_", "completed_toggle", "force_complete")) and scenario not in ("buffers_scroll", "handoff_scroll", "ctrl_c_new_scroll", "live_refresh_scroll", "tree_navigation", "scroll", "wheel", "click", "click_filter", "workflow", "workflow_empty", "workflow_status", "filter", "filter_no_color", "archive_hidden", "archive_included", "actions_basic", "actions_rejected", "actions_hidden", "jump_archived"):
            read_until(b"Second")
            assert "First" in visible.text() and "Second" in visible.text(), visible.text()
        assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout while open"
        if scenario.startswith("content_conflict"):
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title() and editor_line().startswith("Second"))
            send(b" local")
            wait_visible(lambda: editor_line().startswith("Second local"))
            payload = "Local paste\n" + "P" * 1010 + "\nLocal tail"
            send(b"\x1b[200~" + payload.encode() + b"\x1b[201~")
            wait_visible(lambda: "chars]" in visible.text())
            send(b"\x1b[200~" + str(pending_image).encode() + b"\x1b[201~")
            wait_visible(lambda: "[Image #1: pending.png]" in visible.text())
            settle()
            caret = (visible.x, visible.y)
            # Park original text/revision plus paste/image atoms while another editor saves.
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (New)" in editor_title() and editor_line().startswith("First"))
            send(b" retained")
            wait_visible(lambda: editor_line().startswith("First retained"))
            assert "[*]" in visible.text().splitlines()[task_row("Second") - 1], visible.text()
            other_master, other_slave = pty.openpty()
            fcntl.ioctl(other_slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))

            def other_terminal():
                os.setsid()
                fcntl.ioctl(0, termios.TIOCSCTTY, 0)

            other = subprocess.Popen([binary, "tui"], cwd=folder, env=env, stdin=other_slave,
                                     stderr=other_slave, stdout=subprocess.PIPE, preexec_fn=other_terminal)
            other_visible = TerminalScreen(72, 24)

            def other_wait(predicate):
                deadline = time.monotonic() + 5
                while not predicate():
                    assert time.monotonic() < deadline, other_visible.text()
                    if select.select([other_master], [], [], 0.05)[0]:
                        other_visible.feed(os.read(other_master, 65536))
                    assert other.poll() is None, other_visible.text()

            newer_text = "Newer DB text\n" + "\n".join(f"DB row {index:02}" for index in range(30)) + "\nDB tail"
            try:
                other_wait(lambda: "Task Editor - New Task" in other_visible.text())
                os.write(other_master, b"\x1b[1;2A")
                other_wait(lambda: "Task #2 (New)" in other_visible.text() and "Second" in other_visible.text())
                os.write(other_master, b"\x7f" * len("Second"))
                os.write(other_master, b"\x1b[200~" + newer_text.encode() + b"\x1b[201~")
                other_wait(lambda: "DB tail" in other_visible.text())
                os.write(other_master, b"\x13")
                other_wait(lambda: "Saved #2" in other_visible.text())
                newer = cli("show", "2")["task"]
                assert newer["description"] == newer_text
                os.write(other_master, b"\x03\x03")
                deadline = time.monotonic() + 5
                while other.poll() is None:
                    assert time.monotonic() < deadline, "Second editor failed to exit"
                    if select.select([other_master], [], [], 0.05)[0]:
                        try:
                            other_visible.feed(os.read(other_master, 65536))
                        except OSError:
                            pass
                stdout, _ = other.communicate(timeout=5)
                assert other.returncode == 0 and stdout == b"", other_visible.text()
            finally:
                if other.poll() is None:
                    other.kill()
                os.close(other_master)
                os.close(other_slave)
                other.wait(timeout=2)
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (New)" in editor_title() and "[Image #1: pending.png]" in visible.text()
                         and (visible.x, visible.y) == caret)
            assert "[*]" in visible.text().splitlines()[task_row("First") - 1], visible.text()
            send(b"\x13")
            wait_visible(lambda: "Content conflict #2" in visible.text())
            wait_visible(lambda: "Current DB revision" in visible.text() and "Newer DB text" in visible.text())
            assert cli("show", "2")["task"] == newer
            assert len(cli("show", "2")["images"]) == 1
            assert (Path(folder) / ".qqq/images/2/1.png").read_bytes() == stored_image.read_bytes()
            assert not (Path(folder) / ".qqq/images/2/2.png").exists()
            send(b"\x1b[F")
            wait_visible(lambda: "DB tail" in visible.text())
            send(b"\t")
            wait_visible(lambda: "Local draft" in visible.text() and "Second local" in visible.text())
            send(b"\x1b[F")
            wait_visible(lambda: "[Image: pending.png]" in visible.text())
            send(b"\x1b")
            wait_visible(lambda: "Content conflict #2" not in visible.text() and "[Image #1: pending.png]" in visible.text()
                         and (visible.x, visible.y) == caret)
            send(b"\x13")
            wait_visible(lambda: "Content conflict #2" in visible.text())
            send(b"o")
            wait_visible(lambda: "Overwrite DB text?" in visible.text())
            send(b"n")
            wait_visible(lambda: "Content conflict #2" in visible.text())
            assert cli("show", "2")["task"] == newer
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 59, 0, 0))
            visible.resize(59, 24)
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: "Content conflict #2" in visible.text() and "Current DB revision" in visible.text()
                         and "First" in visible.text().splitlines()[0]
                         and " New " not in visible.text().splitlines()[0]
                         and (visible.x, visible.y) == (6, 3))
            settle()
            send(b"o")
            wait_visible(lambda: "Overwrite DB text?" in visible.text())
            latest = cli("edit", "2", "-d", "Another newer DB text")
            send(b"y")
            wait_visible(lambda: "Content conflict #2" in visible.text()
                         and f"Current DB revision {latest['content_revision']}" in visible.text()
                         and "Another newer DB text" in visible.text())
            assert cli("show", "2")["task"] == latest
            send(b"o")
            wait_visible(lambda: "Overwrite DB text?" in visible.text())
            send(b"y")
            wait_visible(lambda: "Content conflict #2" not in visible.text() and "Saved #2" in visible.text())
            saved = cli("show", "2")
            assert payload in saved["task"]["description"]
            assert "```pasteboard" in saved["task"]["description"]
            assert ".qqq/images/2/2.png" in saved["task"]["description"]
            assert len(saved["images"]) == 2
            assert (Path(folder) / ".qqq/images/2/2.png").read_bytes() == pending_image.read_bytes()
            assert (Path(folder) / ".qqq/images/2/1.png").read_bytes() == stored_image.read_bytes()
            assert cli("show", "1")["task"]["description"] == "First"
            assert "[*]" in visible.text().splitlines()[task_row("First") - 1], visible.text()
            send(b" more")
            wait_visible(lambda: "more" in visible.text())
            cli("message", "2", "Progress without content change")
            cli("edit", "2", "--priority", "50")
            cli("next", "--local", "--session", "content-owner")
            cli("complete", "2", "--session", "content-owner")
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #2")
            wait_visible(lambda: "Task #2 (Completed)" in editor_title())
            assert "more" in cli("show", "2")["task"]["description"]
            send(b" reload-discard")
            wait_visible(lambda: "reload-discard" in visible.text())
            restored = cli("edit", "2", "-d", "Reload current text\nFull current details")
            send(b"\x13")
            wait_visible(lambda: "Content conflict #2" in visible.text())
            send(b"r")
            wait_visible(lambda: "Reload current DB text?" in visible.text())
            send(b"n")
            wait_visible(lambda: "Content conflict #2" in visible.text())
            send(b"r")
            wait_visible(lambda: "Reload current DB text?" in visible.text())
            send(b"y")
            wait_visible(lambda: "Content conflict #2" not in visible.text() and editor_line().startswith("Reload current text")
                         and editor_line(1).startswith("Full current details"))
            assert cli("show", "2")["task"] == restored
            # Reload affects only selected buffer; other task's draft remains saveable.
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (New)" in editor_title() and editor_line().startswith("First retained"))
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #1"))
            assert cli("show", "1")["task"]["description"] == "First retained"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (Completed)" in editor_title() and editor_line(1).startswith("Full current details"))
            send(b" removed-local")
            wait_visible(lambda: "removed-local" in visible.text())
            send(b"\x1b[200~" + str(pending_image).encode() + b"\x1b[201~")
            wait_visible(lambda: "[Image #1: pending.png]" in visible.text())
            settle()
            removed_caret = (visible.x, visible.y)
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (New)" in editor_title())
            cli("archive", "2")
            cli("delete", "2", "--yes")
            wait_visible(lambda: "Reload current text" not in "\n".join(visible.text().splitlines()[:editor_row()]))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title())
            send(b"\x03")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #2?"))
            send(b"n")
            wait_visible(lambda: "Task #2 (" in editor_title() and "[Image #1: pending.png]" in visible.text()
                         and (visible.x, visible.y) == removed_caret)
            send(b"\x13")
            wait_visible(lambda: "Content conflict #2" in visible.text() and "task removed" in visible.text()
                         and visible.text().splitlines()[-1].startswith("Content conflict for task 2"))
            send(b"\t")
            wait_visible(lambda: "Local draft" in visible.text() and "removed-local" in visible.text())
            send(b"\x1b")
            wait_visible(lambda: "Content conflict #2" not in visible.text() and "removed-local" in visible.text())
            assert [task["id"] for task in cli("list")] == [1]
            assert not (Path(folder) / ".qqq/images/2/2.png").exists()
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen
        elif scenario.startswith("compact_layout"):
            initial_tasks = cli("list")

            def resize_compact(width, height=24, dirty=False):
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
                visible.resize(width, height)
                os.kill(child.pid, signal.SIGWINCH)
                editor_y = (height + 1) // 2 if width < 60 else 13
                wait_visible(lambda: editor_row() == editor_y
                             and (visible.x, visible.y) == (8 if dirty else 0, editor_y + 1)
                             and (not dirty or editor_line().startswith("Changed Second")))
                settle()

            resize_compact(59)
            upper = "\n".join(visible.text().splitlines()[:editor_row()])
            assert "First" in upper and "Second" in upper and "New" not in upper, visible.text()
            assert not any(symbol in visible.text() for symbol in "╔╚║"), visible.text()
            click(6, task_row("Second"))
            wait_visible(lambda: "Task #2 (New)" in editor_title() and editor_line().startswith("Second"))
            send(b"\x01Changed ")
            wait_visible(lambda: editor_line().rstrip() == "Changed Second"
                         and (visible.x, visible.y) == (8, 13))
            send(CTRL_SLASH)
            wait_visible(lambda: filter_text() == "Filter:"
                         and (visible.x, visible.y) == (8, 0))
            click(6, 1)
            settle()
            assert "Task #2 (New)" in editor_title() and editor_line().startswith("Changed Second"), visible.text()
            send(b"first")
            wait_visible(lambda: filter_text() == "Filter: first"
                         and "Second" not in "\n".join(visible.text().splitlines()[1:editor_row()]))
            send(b"\x1b")
            wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                         and editor_line().startswith("Changed Second") and (visible.x, visible.y) == (8, 13))
            resize_compact(58, height=25, dirty=True)
            resize_compact(50, dirty=True)
            assert "New" not in "\n".join(visible.text().splitlines()[:editor_row()]), visible.text()
            assert not any(symbol in visible.text() for symbol in "╔╚║"), visible.text()
            resize_compact(60, dirty=True)
            assert "New" in "\n".join(visible.text().splitlines()[:8]), visible.text()
            assert "╔" in visible.text(), visible.text()
            resize_compact(150, dirty=True)
            resize_compact(59, dirty=True)
            assert "New" not in "\n".join(visible.text().splitlines()[:editor_row()]), visible.text()
            assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario.startswith("wide_layout"):
            initial_tasks = cli("list")

            def resize_layout(width, selected=False):
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
                visible.resize(width, 24)
                os.kill(child.pid, signal.SIGWINCH)
                split = width - (width * 40 + 50) // 100 if width >= 150 else 0
                border_row = 0 if width >= 150 else 8
                final_cursor = f"\x1b[15;{9 if selected else 1}H".encode()
                wait_visible(lambda: editor_row() == 13
                             and visible.text().splitlines()[border_row][split] == "╔"
                             and (width < 150 or not visible.text().splitlines()[12][:split].strip())
                             and (not selected or editor_line().startswith("Changed Second"))
                             and visible.text().splitlines()[-1].rstrip() ==
                             "Ctrl-S Save  Ctrl-L Go to Task  Ctrl-P Create Child  Ctrl-G Menu  Shift-Up/Dn Switch Tasks  Ctrl+/ Filter"
                             and (visible.x, visible.y) == (8 if selected else 0, 14)
                             and not visible.pending and screen.endswith(final_cursor))
                settle()

            resize_layout(149)
            assert "Select task to view details." in details_text(), visible.text()
            resize_layout(150)
            assert "Select task to view details." in details_text(), visible.text()
            before_padding = visible.text()
            click(6, 13)
            send(b"\x1b[<65;6;13M")
            settle()
            assert visible.text() == before_padding, (
                f"Before padding input:\n{before_padding}\nAfter padding input:\n{visible.text()}\n"
                f"Cursor={visible.x},{visible.y}, pending={visible.pending!r}, bytes={screen[-1600:]!r}"
            )
            send(b"\x1b[<64;6;2M" * 20)
            wait_visible(lambda: "First" in visible.text().splitlines()[0][:90]
                         and "TAIL" in "\n".join(line[:90] for line in visible.text().splitlines()[:4]))
            settle()
            assert not visible.text().splitlines()[12][:90].strip(), visible.text()
            click(6, 2)
            wait_visible(lambda: "Task #2 (New)" in editor_title()
                         and editor_line().startswith("Second ") and details_text().startswith("#2 · New"))
            send(b"\x01Changed ")
            expected_draft = "Changed " + cli("show", "2")["task"]["description"]
            wait_visible(lambda: editor_line().rstrip() == expected_draft
                         and (visible.x, visible.y) == (8, 14))
            settle()
            saved_draft = editor_line()
            before_details = details_text()
            send(b"\x1b[<65;95;2M")
            wait_visible(lambda: details_text() != before_details
                         and (visible.x, visible.y) == (8, 14))
            settle()
            assert editor_line() == saved_draft and (visible.x, visible.y) == (8, 14), visible.text()
            before_details = details_text()
            click(95, 2)
            send(b"\x1b[<65;91;2M")
            settle()
            assert details_text() == before_details and editor_line() == saved_draft, visible.text()
            send(CTRL_SLASH)
            wait_visible(lambda: filter_text() == "Filter:"
                         and visible.text().splitlines()[0][90] == "╔"
                         and (visible.x, visible.y) == (8, 0))
            send(b"Second")
            wait_visible(lambda: visible.text().splitlines()[0].startswith("Filter: Second")
                         and visible.text().splitlines()[0][90] == "╔"
                         and (visible.x, visible.y) == (14, 0))
            send(b"\x1b")
            wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                         and (visible.x, visible.y) == (8, 14))
            resize_layout(151, selected=True)
            resize_layout(200, selected=True)
            resize_layout(149, selected=True)
            resize_layout(150, selected=True)
            assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario == "cursor_end":
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
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            wait_end("Newest tail")
            send(b" appended\x1b[H!")
            wait_visible(lambda: "!Newest tail appended" in visible.text()
                         and visible.x == 1)
            send(b"\x13")
            wait_visible(lambda: "Saved #2" in visible.text())
            wait_end("!Newest tail appended")
            assert cli("show", "2")["task"]["description"].endswith("!Newest tail appended")

            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (New)" in editor_title())
            wait_end("Older tail")
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            wait_end("!Newest tail appended")
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title())
            wait_visible(lambda: (visible.x, visible.y) == (0, editor_row() + 1))
            send(b"New text\x1b[H<\x13")
            wait_visible(lambda: "Task #3 (New)" in editor_title())
            wait_end("<New text")
            assert cli("show", "3")["task"]["description"] == "<New text"
        elif scenario.startswith("list_selection"):
            click(5, task_row("First"))
            wait_frame(lambda: "Task #1 (New)" in editor_title() and editor_line().startswith("First"))
            for width in (72, 50, 150, 72):
                if visible.width != width:
                    clear_capture()
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
                    visible.resize(width, 24)
                    os.kill(child.pid, signal.SIGWINCH)
                    wait_frame(lambda: "Task #1 (New)" in editor_title()
                               and editor_line().startswith("First"))
                available = list_width() - (8 if width < 60 else 21)
                description = "X" * (available - 2) + ">Z"
                cli("edit", "2", "--description", description)
                wait_frame(lambda: description in list_text() and "Task #1 (New)" in editor_title())
                initial_tasks = cli("list")
                first_row = visible.text().splitlines()[task_row("First") - 1][:list_width()]
                second_row = visible.text().splitlines()[task_row(description) - 1][:list_width()]
                assert first_row.startswith(" 1"), visible.text()
                assert second_row.startswith(" 2") and second_row.endswith(">Z"), visible.text()
                click(5, task_row(description))
                wait_frame(lambda: "Task #2 (New)" in editor_title() and editor_line().startswith(description))
                selected_row = visible.text().splitlines()[task_row(description) - 1][:list_width()]
                assert selected_row == second_row, visible.text()
                assert selected_row[1] == "2" and selected_row[-1] == "Z", visible.text()
                assert cli("list") == initial_tasks
                send(b"\x1b[1;2A")
                wait_frame(lambda: "Task #1 (New)" in editor_title() and editor_line().startswith("First"))
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen
        elif scenario.startswith("dirty_marker"):
            def marker_frame_ready(cursor_x):
                final_cursor=f"\x1b[{visible.y+1};{cursor_x+1}H".encode()
                return ("Task #2 (" in editor_title()
                        and visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                        and visible.x==cursor_x and editor_row()<visible.y<visible.height-1
                        and not visible.pending and screen.endswith(final_cursor))
            send(b"\x1b[1;2A")
            wait_visible(lambda: marker_frame_ready(4))
            for width in (72, 50, 150, 72):
                if visible.width != width:
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
                    visible.resize(width, 24)
                    child.send_signal(signal.SIGWINCH)
                    wait_visible(lambda: marker_frame_ready(4) and "Second" in visible.text())
                list_before=visible.text().splitlines()[:list_bottom()+1]
                first=task_row("Second")-1
                column=8 if width<60 else 21
                send(b"!")
                wait_visible(lambda: marker_frame_ready(5) and "[*]" in visible.text().splitlines()[first])
                list_after=visible.text().splitlines()[:list_bottom()+1]
                assert len(list_after)==len(list_before), visible.text()
                assert list_after[first][:column]==list_before[first][:column], visible.text()
                assert list_after[first][column:column+4]=="[*] ", visible.text()
                assert all(list_after[index]==row for index,row in enumerate(list_before) if index!=first), visible.text()
                send(b"\x7f")
                wait_visible(lambda: marker_frame_ready(4) and "[*]" not in visible.text())
                assert visible.text().splitlines()[:list_bottom()+1]==list_before, visible.text()
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen
        elif scenario in ("prerequisites", "prerequisites_no_color"):
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            send(b"!")
            wait_visible(lambda: editor_line().startswith("Second!"))
            scroll_details_to(lambda text: "#1 · new · blocks claim" in text)
            cli("next", "--local", "--session", "prereq", "--filter", "id == 1")
            cli("complete", "1", "--session", "prereq")
            wait_visible(lambda: "#1 · completed" in details_text())
            assert editor_line().startswith("Second!"), visible.text()
            assert cli("show", "2")["task"]["description"] == "Second"
            cli("reopen", "1")
            wait_visible(lambda: "#1 · new · blocks claim" in details_text())
            if scenario == "prerequisites_no_color":
                assert b"\x1b[38;" not in screen
        elif scenario.startswith("jump"):
            initial_tasks = cli("list")

            def jump_frame_ready():
                return ("Enter go  Esc cancel" in visible.text()
                        and 0 <= visible.x < visible.width and 0 <= visible.y < visible.height
                        and not visible.pending
                        and screen.endswith(f"\x1b[{visible.y + 1};{visible.x + 1}H".encode()))

            def open_jump():
                send(b"\x0c")
                wait_visible(lambda: "Go to task" in visible.text() and jump_frame_ready())

            def go_to(task_id, description, cursor=None, status="New"):
                open_jump()
                send(str(task_id).encode() + b"\r")
                wait_visible(lambda: f"Task #{task_id} ({status})" in editor_title()
                             and editor_line().startswith(description)
                             and (cursor is None or (visible.x, visible.y) == (cursor, editor_row() + 1))
                             and not visible.pending
                             and screen.endswith(f"\x1b[{visible.y + 1};{visible.x + 1}H".encode()))
                assert "Go to task" not in visible.text(), visible.text()

            if scenario in ("jump", "jump_no_color"):
                send(b"New draft")
                wait_visible(lambda: editor_line().startswith("New draft"))
                open_jump()
                for value in (b"", b"0", b"-1", b"abc", b"9223372036854775808"):
                    send(b"\x15" + value + b"\r")
                    wait_visible(lambda value=value: "Enter a positive task ID" in visible.text()
                                 and (not value or "> " + value.decode() in visible.text()))
                send(b"\x15\x1b[200~1\n2\x1b[201~\r")
                wait_visible(lambda: "> 1 2" in visible.text()
                             and "Enter a positive task ID" in visible.text())
                send(b"\x15" + b"9999\r")
                wait_visible(lambda: "9999" in visible.text() and "not found" in visible.text())
                send(b"\x15\x1b[200~ 2 \n\x1b[201~\r")
                wait_visible(lambda: "Task #2 (New)" in editor_title()
                             and editor_line().startswith("Second")
                             and (visible.x, visible.y) == (6, editor_row() + 1))
                for cancel_key in (b"\x1b", b"\x03"):
                    open_jump()
                    send(cancel_key)
                    wait_visible(lambda: "Go to task" not in visible.text()
                                 and "Task #2 (New)" in editor_title()
                                 and (visible.x, visible.y) == (6, editor_row() + 1))
                    assert child.poll() is None
                send(b"\x1b[1;2B")
                wait_visible(lambda: "New Task" in editor_title()
                             and editor_line().startswith("New draft")
                             and (visible.x, visible.y) == (9, editor_row() + 1))
            elif scenario == "jump_filter":
                send(b"Draft" + CTRL_SLASH + b"zzz")
                wait_visible(lambda: "No matching tasks" in visible.text()
                             and (visible.x, visible.y) == (11, 0))
                for cancel_key in (b"\x1b", b"\x03"):
                    open_jump()
                    send(b"1")
                    send(cancel_key)
                    wait_visible(lambda: "Go to task" not in visible.text()
                                 and visible.text().splitlines()[0].startswith("Filter: zzz")
                                 and (visible.x, visible.y) == (11, 0))
                    assert child.poll() is None
                go_to(1, "First", 5)
                assert not visible.text().splitlines()[0].startswith("Filter:"), visible.text()
                send(b"\x1b[1;2B\x1b[1;2B")
                wait_visible(lambda: "New Task" in editor_title()
                             and editor_line().startswith("Draft")
                             and (visible.x, visible.y) == (5, editor_row() + 1))
            elif scenario == "jump_drafts":
                go_to(2, "Second", 6)
                send(b" local\x1b[D\x1b[D")
                wait_visible(lambda: editor_line().startswith("Second local")
                             and (visible.x, visible.y) == (10, editor_row() + 1))
                go_to(1, "First", 5)
                send(b" local")
                wait_visible(lambda: editor_line().startswith("First local"))
                go_to(2, "Second local", 10)
                go_to(2, "Second local", 10)
                send(CTRL_P + b"Child draft")
                wait_visible(lambda: "parent #2" in editor_title()
                             and editor_line().startswith("Child draft"))
                go_to(1, "First local", 11)
                go_to(2, "Second local", 10)
                send(CTRL_P)
                wait_visible(lambda: "parent #2" in editor_title()
                             and editor_line().startswith("Child draft")
                             and (visible.x, visible.y) == (11, editor_row() + 1))
            elif scenario == "jump_archived":
                assert "Second" not in "\n".join(visible.text().splitlines()[:list_bottom() + 1])
                go_to(1, "First", 5)
                assert "First" in "\n".join(visible.text().splitlines()[:list_bottom() + 1])
                go_to(2, "Second", 6)
                assert "Second" in "\n".join(visible.text().splitlines()[:list_bottom() + 1])
                assert cli("show", "2")["task"]["archived"]
                send(b"\x1b[1;2A\x1b[1;2B")
                wait_visible(lambda: "Task #2 (New)" in editor_title()
                             and editor_line().startswith("Second"))
            elif scenario == "jump_completed":
                send(b"Draft" + CTRL_SLASH + b"\x14")
                wait_frame(lambda: completed_button_text() == "[× Completed]"
                           and "Second" not in list_text() and editor_line().startswith("Draft"))
                open_jump()
                send(b"2\x1b")
                wait_frame(lambda: "Go to task" not in visible.text()
                           and completed_button_text() == "[× Completed]"
                           and "Second" not in list_text() and "New Task" in editor_title())
                go_to(1, "First", 5)
                assert completed_button_text() == "[× Completed]" and "Second" not in list_text(), visible.text()
                go_to(2, "Second", 6, status="Completed")
                assert "Second" in list_text(), visible.text()
                assert not visible.text().splitlines()[0].startswith("Filter:"), visible.text()
                send(b"\x1b[1;2B")
                wait_frame(lambda: "New Task" in editor_title() and editor_line().startswith("Draft")
                           and (visible.x, visible.y) == (5, editor_row() + 1))
            elif scenario == "jump_narrow":
                wait_visible(lambda: not visible.pending and screen.endswith(b"\x1b[15;1H"))
                clear_capture()
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 32, 0, 0))
                visible.resize(32, 12)
                os.kill(child.pid, signal.SIGWINCH)
                wait_visible(lambda: "New Task" in editor_title()
                             and (visible.x, visible.y) == (0, editor_row() + 1)
                             and visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                             and not visible.pending and screen.endswith(b"\x1b[8;1H"))
                open_jump()
                send(b"9" * 40 + b"\r")
                wait_visible(lambda: "positive" in visible.text() and jump_frame_ready())
                assert 0 <= visible.x < 32 and 0 <= visible.y < 12
                send(b"\x15" + b"2\r")
                wait_visible(lambda: "Task #2 (New)" in editor_title()
                             and editor_line().startswith("Second")
                             and (visible.x, visible.y) == (6, editor_row() + 1))
            assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario in ("details_assignment", "details_assignment_no_color"):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (In progress)" in editor_title()
                         and editor_line().startswith("Second")
                         and (visible.x, visible.y) == (6, editor_row() + 1))
            scroll_details_to(lambda text: "Harness" in text)
            lines = details_text().splitlines()
            index = next(index for index, row in enumerate(lines) if row.strip().startswith("Harness"))
            assert lines[index][25:] + lines[index + 1] == "00000000-0000-0000-0000-000000000000 (codex)", visible.text()
            assert lines[index][:25].strip() == "Harness", visible.text()
            assert lines[index + 1] == "ex)", visible.text()
            assert lines[index + 2][:25].strip() == "Orchestrator", visible.text()
            assert lines[index + 2][25:] == "default (herdr)", visible.text()
            assert "Harness name:" not in details_text() and "Harness session:" not in details_text()
            assert editor_line().startswith("Second") and (visible.x, visible.y) == (6, editor_row() + 1)
            assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario in ("details", "details_no_color"):
            assert editor_row() == 13, visible.text()
            assert "Select task to view details." in details_text(), visible.text()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title()
                         and details_text().startswith("#2 · New\n\nDescription:")
                         and editor_line().startswith("Second")
                         and (visible.x, visible.y) == (len("Second"), editor_row() + 1))
            scroll_details_to(lambda text: "Messages: (2)" in text)
            assert details_text().splitlines() == ["  Second", "", "Messages: (2)"], visible.text()
            scroll_details_to(lambda text: "Earlier message" in text)
            assert "  #1 · worker · " in details_text(), visible.text()
            scroll_details_to(lambda text: "Latest message" in text)
            assert "  #2 · reviewer · " in details_text(), visible.text()
            assert "    Message continuation" in details_text(), visible.text()
            scroll_details_to(lambda text: "TAIL" in text)
            assert details_text().split("\n") == ["    " + "W" * 62, "W" * 4 + "TAIL", ""], visible.text()
            scroll_details_to(lambda text: "Details:" in text)
            scroll_details_to(lambda text: "Priority:" in text)
            for label, value in (("Parent:", "-"), ("Priority:", "0"), ("Archived:", "no")):
                scroll_details_to(lambda text, label=label: label in text)
                row = next(row for row in details_text().splitlines() if row.strip().startswith(label))
                assert row.startswith("  " + label) and row[25:] == value, visible.text()
            selected_task = cli("show", "2")["task"]
            scroll_details_to(lambda text: "Created:" in text and "Updated:" in text)
            for label, key in (("Created:", "created_at"), ("Updated:", "updated_at")):
                row = next(row for row in details_text().splitlines() if row.strip().startswith(label))
                assert row[25:] == selected_task[key], visible.text()
            scroll_details_to(lambda text: "Assignment:" in text)
            scroll_details_to(lambda text: "Harness" in text)
            row = next(row for row in details_text().splitlines() if row.strip().startswith("Harness"))
            assert row[25:] == "-", visible.text()
            for empty in ("Images: None", "History: None", "Herdr: Not linked"):
                scroll_details_to(lambda text, empty=empty: empty in text)
            assert editor_row() == 13 and editor_line().startswith("Second"), visible.text()
            assert (visible.x, visible.y) == (len("Second"), editor_row() + 1), visible.text()
            scroll_details_to(lambda text: text.startswith("#2 · New"), up=True)
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: "New Task" in editor_title() and editor_row() == 13
                         and "Select task to view details." in details_text()
                         and visible.text().count("Task Editor") == 1)
            send(b"Fresh\x13")
            wait_visible(lambda: "Task #3 (New)" in editor_title()
                         and details_text().startswith("#3 · New")
                         and editor_line().startswith("Fresh"))
            scroll_details_to(lambda text: "Messages: None" in text)
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
            wait_visible(lambda: "Task #3" in editor_title() and editor_row() == 13)
            assert cli("show", "3")["task"]["description"] == payload
        elif scenario == "details_scroll":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title()
                         and details_text().startswith("#2 · New")
                         and editor_line().startswith("Second"))
            scroll_details_to(lambda text: "Messages: (1)" in text)
            assert details_text().splitlines() == ["  Second", "", "Messages: (1)"], visible.text()
            scroll_details_to(lambda text: "Message line 01" in text)
            assert "  #1 · reviewer · " in details_text(), visible.text()
            initial_tasks = cli("list")
            initial_list = visible.text().splitlines()[:list_bottom() + 1]
            send(b"\x1b[6~")
            wait_visible(lambda: details_text().splitlines()[0] == "    Message line 03")
            send(b"\x1b[6~")
            wait_visible(lambda: details_text().splitlines()[0] == "    Message line 06")
            assert editor_line().startswith("Second"), visible.text()
            assert visible.text().splitlines()[:list_bottom() + 1] == initial_list
            send(f"\x1b[<65;6;{list_bottom() + 3}M".encode())
            wait_visible(lambda: details_text().splitlines()[0] == "    Message line 09")
            settle()
            clear_capture()
            click(5, list_bottom() + 3)
            settle()
            assert not screen, f"Read-only details click redrew screen: {screen[-500:]!r}"
            scroll_details_to(lambda text: text.startswith("#2 · New"), up=True)
            assert cli("list") == initial_tasks
            scroll_details_to(lambda text: "Herdr: Not linked" in text)
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (New)" in editor_title()
                         and details_text().startswith("#1 · New"))
            scroll_details_to(lambda text: "Messages: None" in text)
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2" in editor_title() and details_text().startswith("#2 · New"))
            scroll_details_to(lambda text: "Message line 01" in text)
            assert cli("list") == initial_tasks
        elif scenario == "details_refresh":
            send(b"\x1b[1;2A")
            wait_visible(lambda: details_text().startswith("#2 · New"))
            scroll_details_to(lambda text: "Messages: (2)" in text)
            send(b"\x01Unsaved ")
            wait_visible(lambda: editor_line().startswith("Unsaved Second"))
            cli("message", "2", "External message", "--session", "reviewer")
            wait_visible(lambda: "Messages: (3)" in details_text())
            scroll_details_to(lambda text: "Earlier message" in text)
            assert editor_line().startswith("Unsaved Second"), visible.text()
            scroll_details_to(lambda text: "External message" in text)
            cli("edit", "2", "--priority", "7")
            cli("next", "--local", "--session", "worker")
            cli("edit", "2", "--set-status", "error", "--reason", "Live failure", "--session", "worker")
            wait_visible(lambda: "Task #2 (Error)" in editor_title())
            scroll_details_to(lambda text: text.startswith("#2 · Error"), up=True)
            scroll_details_to(lambda text: "Priority:" in text)
            row = next(row for row in details_text().splitlines() if row.strip().startswith("Priority:"))
            assert row[25:] == "7", visible.text()
            scroll_details_to(lambda text: "Live failure" in text, up=True)
            assert editor_line().startswith("Unsaved Second"), visible.text()
            assert cli("show", "2")["task"]["description"] == "Second"
        elif scenario == "details_deleted":
            send(b"\x1b[1;2A")
            wait_visible(lambda: details_text().startswith("#2 · New"))
            scroll_details_to(lambda text: "Messages: None" in text)
            send(b"\x01Unsaved ")
            wait_visible(lambda: editor_line().startswith("Unsaved Second"))
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            wait_visible(lambda: "Task #2 unavailable." in details_text()
                         and "Messages: None" not in details_text())
            assert editor_line().startswith("Unsaved Second"), visible.text()
            send(b"\x1b")
            wait_visible(lambda: "Discard changes and switch?" in visible.text().splitlines()[-1])
            send(b"y")
            wait_visible(lambda: "New Task" in editor_title() and editor_row() == 13
                         and "Task #2 unavailable." not in visible.text()
                         and visible.text().count("Task Editor") == 1)
            assert "Select task to view details." in details_text(), visible.text()
        elif scenario in ("archive_hidden", "archive_included"):
            wait_visible(lambda: "Visible" in visible.text())
            settle()
            if scenario == "archive_hidden":
                assert "Hidden" not in visible.text(), visible.text()
            else:
                assert "[archived] Hidden" in visible.text(), visible.text()
        if scenario.startswith("long_description_"):
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            if scenario.endswith("pasteboard"):
                wait_visible(lambda: f"[Pasted Content {len(long_body)} chars]" in editor_line())
                expected = stored_description
            else:
                wait_visible(lambda: "End marker" in "\n".join(visible.text().splitlines()[editor_row() + 1:-1]))
                assert "[Pasted Content" not in visible.text(), visible.text()
                assert cli("show", "2")["task"]["description"] == stored_description
                send(b"\x01" + (b"\x1b[A" if scenario.endswith("text_fence") else b"") + b"X")
                wait_visible(lambda: "XEnd marker" in visible.text())
                expected = stored_description.replace("End marker", "XEnd marker")
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #2"))
            assert cli("show", "2")["task"]["description"] == expected
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-1000:]
        elif scenario.startswith("buffers_exit_"):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x05 two")
            wait_visible(lambda: editor_line().startswith("Second two"))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title())
            send(b"\x05 one")
            wait_visible(lambda: editor_line().startswith("First one"))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second two"))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and not editor_line().strip())
            if scenario == "buffers_exit_deleted":
                with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                    db.execute("DELETE FROM tasks WHERE id=1")
            if scenario == "buffers_exit_active_new":
                send(b"New unsaved")
                wait_visible(lambda: editor_line().startswith("New unsaved"))
            if scenario == "buffers_exit_filter":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: filter_text() == "Filter: first")
                send(b"\x03")
                wait_visible(lambda: filter_text() == "Filter:")
                assert "Discard" not in visible.text(), visible.text()
                send(b"\x03")
                wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:"))
                assert "Discard" not in visible.text(), visible.text()
            send(b"\x1b" if scenario == "buffers_exit_escape" else b"\x03")
            if scenario == "buffers_exit_active_new":
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard draft?"))
                send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #1?"))
            assert "First one" in visible.text(), visible.text()
            if scenario == "buffers_exit_resize":
                for width, height in ((12, 8), (150, 36), (72, 24)):
                    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
                    visible.resize(width, height)
                    os.kill(child.pid, signal.SIGWINCH)
                    wait_visible(lambda: visible.text().splitlines()[-1].startswith(
                        "Drop #1? y/N" if width == 12 else "Discard task #1?"))
                    assert child.poll() is None
                assert "First one" in visible.text(), visible.text()
            if scenario == "buffers_exit_mouse":
                settle()
                before_click = visible.text()
                click(6, task_row("Second"))
                click(1, 20)
                settle()
                assert child.poll() is None and visible.text() == before_click, visible.text()
            send(b"\x03\x03")
            settle()
            assert child.poll() is None and visible.text().splitlines()[-1].startswith("Discard task #1?"), visible.text()
            send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #2?"))
            assert "Second two" in visible.text(), visible.text()
            send({"buffers_exit_enter": b"\r", "buffers_exit_escape": b"\x1b"}.get(scenario, b"n"))
            wait_visible(lambda: not visible.text().splitlines()[-1].startswith("Discard"))
            assert child.poll() is None
            if scenario == "buffers_exit_active_new":
                assert editor_line().startswith("New unsaved"), visible.text()
            if scenario != "buffers_exit_deleted":
                assert cli("list") == initial_tasks
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second two"))
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #1 (" in editor_title() and editor_line().startswith("First one"))
                # Both approvals aborted: task #1 remains despite earlier y.
                send(b"\x1b[1;2B")
                wait_visible(lambda: "Task #2 (" in editor_title())
                send(b"\x1b[1;2B")
                wait_visible(lambda: "New Task" in editor_title())
            send(b"\x03")
            if scenario == "buffers_exit_active_new":
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard draft?"))
                send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #1?"))
            send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #2?"))
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-1000:]
            send(b"y")
            deadline = time.monotonic() + 5
            while child.poll() is None:
                assert time.monotonic() < deadline, "Approved exit stalled"
                settle()
            if scenario != "buffers_exit_deleted":
                assert cli("list") == initial_tasks
        elif scenario.startswith("buffers_navigation"):
            if scenario.endswith("compact"):
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 50, 0, 0))
                visible.resize(50, 24)
                os.kill(child.pid, signal.SIGWINCH)
                wait_visible(lambda: editor_row() == 12)
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second"))
            send(b"\x01Changed ")
            wait_visible(lambda: editor_line().startswith("Changed Second") and visible.x == 8)
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title() and editor_line().startswith("First"))
            assert "[*]" in visible.text().splitlines()[task_row("Second") - 1], visible.text()
            assert "Discard" not in visible.text(), visible.text()
            send(b"\x01First edit ")
            wait_visible(lambda: editor_line().startswith("First edit First") and visible.x == 11)
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Changed Second")
                         and (visible.x, visible.y) == (8, editor_row() + 1))
            assert cli("list") == initial_tasks
            for name in ("First", "Second"):
                assert "[*]" in visible.text().splitlines()[task_row(name) - 1], visible.text()
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #2")
                         and "[*]" not in visible.text().splitlines()[task_row("Second") - 1])
            assert cli("show", "2")["task"]["description"] == "Changed Second"
            assert cli("show", "1")["task"]["description"] == "First"
            assert "[*]" in visible.text().splitlines()[task_row("First") - 1], visible.text()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title() and editor_line().startswith("First edit First")
                         and (visible.x, visible.y) == (11, editor_row() + 1))
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #1")
                         and "[*]" not in visible.text())
            assert cli("show", "1")["task"]["description"] == "First edit First"
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-1000:]

        elif scenario == "buffers_mouse_revert":
            initial_tasks = cli("list")
            click(5, task_row("Second"))
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x01X")
            wait_visible(lambda: editor_line().startswith("XSecond") and "[*]" in visible.text())
            click(2, task_row("First"))
            wait_visible(lambda: "Task #1 (" in editor_title() and editor_line().startswith("First"))
            assert "[*]" in visible.text().splitlines()[task_row("Second") - 1], visible.text()
            click(2, task_row("Second"))
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("XSecond") and visible.x == 1)
            send(b"\x7f")
            wait_visible(lambda: editor_line().startswith("Second") and "[*]" not in visible.text())
            click(5, task_row("First"))
            wait_visible(lambda: "Task #1 (" in editor_title())
            click(5, task_row("Second"))
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second"))
            assert cli("list") == initial_tasks

        elif scenario == "buffers_scroll":
            original = cli("show", "2")["task"]["description"]
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x01X")
            wait_visible(lambda: "XLine20" in visible.text())
            wheel_editor(False, 20)
            wait_visible(lambda: editor_line().startswith("Line01"))
            wheel_editor(True, 2)
            wait_visible(lambda: editor_line().startswith("Line07") and not visible.cursor_visible)
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Line07")
                         and not visible.cursor_visible)
            assert cli("show", "2")["task"]["description"] == original
            send(b"\x05")
            wait_visible(lambda: "XLine20" in visible.text() and visible.cursor_visible)
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #2"))
            assert cli("show", "2")["task"]["description"] == original.removesuffix("Line20") + "XLine20"
        elif scenario == "buffers_atoms":
            payload = "p" * 1001
            image_path = Path(folder) / "retained.png"
            image_path.write_bytes(base64.b64decode(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="
            ))
            send(b"Draft " + b"\x1b[200~" + payload.encode() + b"\x1b[201~")
            send(b"\x1b[200~" + str(image_path).encode() + b"\x1b[201~")
            wait_visible(lambda: "[Pasted Content 1001 chars]" in visible.text() and "[Image #1: retained.png]" in visible.text())
            saved_cursor = (visible.x, visible.y)
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and "[Pasted Content 1001 chars]" in visible.text()
                         and "[Image #1: retained.png]" in visible.text() and (visible.x, visible.y) == saved_cursor)
            assert len(cli("list")) == 2
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x01X")
            wait_visible(lambda: editor_line().startswith("XSecond"))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and "[Image #1: retained.png]" in visible.text())
            send(b"\x03")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard draft?"))
            send(b"y")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard task #2?"))
            send(b"n")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                         and "[Pasted Content 1001 chars]" in visible.text() and "[Image #1: retained.png]" in visible.text()
                         and (visible.x, visible.y) == saved_cursor)
            assert cli("show", "2")["task"]["description"] == "Second"
            send(b"\x13")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #3"))
            saved = cli("show", "3")
            assert payload in saved["task"]["description"]
            assert [image["name"] for image in saved["images"]] == ["retained.png"]
            exported = Path(folder) / "exported.png"
            cli("show", "3", "--export-image", "1", "--output", str(exported))
            assert exported.read_bytes() == image_path.read_bytes()

        elif scenario == "buffers_new_child":
            initial_tasks = cli("list")
            send(b"General draft")
            wait_visible(lambda: editor_line().startswith("General draft"))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second"))
            send(CTRL_P)
            wait_visible(lambda: "New Task (parent #2)" in editor_title() and not editor_line().strip())
            send(b"Child draft")
            wait_visible(lambda: editor_line().startswith("Child draft"))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(CTRL_P)
            wait_visible(lambda: "New Task (parent #2)" in editor_title()
                         and editor_line().startswith("Child draft"))
            assert cli("list") == initial_tasks
            send(b"\x13")
            wait_visible(lambda: "Task #3 (" in editor_title())
            child_task = cli("show", "3")["task"]
            assert child_task["description"] == "Child draft" and child_task["parent_id"] == 2
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and "parent #" not in editor_title()
                         and editor_line().startswith("General draft"))
            send(b"\x13")
            wait_visible(lambda: "Task #4 (" in editor_title())
            general_task = cli("show", "4")["task"]
            assert general_task["description"] == "General draft" and general_task["parent_id"] is None
        elif scenario == "buffers_child_exit":
            initial_tasks = cli("list")
            send(b"\x1b[1;2A" + CTRL_P)
            wait_visible(lambda: "New Task (parent #2)" in editor_title())
            send(b"Child retained")
            wait_visible(lambda: editor_line().startswith("Child retained"))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x03")
            wait_visible(lambda: "New Task" in editor_title() and not editor_line().strip())
            send(b"\x03")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Discard child draft (parent #2)?"))
            assert "Child retained" in visible.text(), visible.text()
            send(b"n")
            wait_visible(lambda: "New Task" in editor_title() and not editor_line().strip())
            send(b"\x1b[1;2A" + CTRL_P)
            wait_visible(lambda: "New Task (parent #2)" in editor_title() and editor_line().startswith("Child retained"))
            assert cli("list") == initial_tasks

        elif scenario == "ctrl_c_filter_empty":
            initial_tasks = cli("list")
            send(CTRL_SLASH + b"first")
            wait_visible(lambda: filter_text() == "Filter: first")
            send(b"\x03")
            wait_visible(lambda: filter_text() == "Filter:"
                         and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1])
                         and (visible.x, visible.y) == (8, 0))
            assert "New Task" in editor_title(), visible.text()
            assert editor_line().strip() == "", visible.text()
            assert cli("list") == initial_tasks
            assert child.poll() is None
        elif scenario.startswith(("filter_escape_empty", "filter_ctrl_c_empty")):
            initial_tasks = cli("list")
            if "selected" in scenario or scenario.endswith("child"):
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #2 (New)" in editor_title()
                             and editor_line().startswith("Second")
                             and (visible.x, visible.y) == (6, editor_row() + 1))
            if scenario.endswith("child"):
                send(CTRL_P)
                wait_visible(lambda: "New Task (parent #2)" in editor_title()
                             and editor_line().strip() == ""
                             and (visible.x, visible.y) == (0, editor_row() + 1))
            elif scenario.endswith("dirty"):
                payload = b" changed" if "selected" in scenario else b"Draft"
                expected = "Second changed" if "selected" in scenario else "Draft"
                send(payload)
                wait_visible(lambda: editor_line().rstrip() == expected
                             and (visible.x, visible.y) == (len(expected), editor_row() + 1))
            original_title = editor_title()
            original_draft = editor_line().rstrip()
            original_cursor = (visible.x, visible.y)
            send(CTRL_SLASH)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to Filter")
                         and filter_text() == "Filter:"
                         and visible.cursor_visible and (visible.x, visible.y) == (8, 0))
            if scenario.endswith(("cleared", "backspace")):
                send(b"first")
                wait_visible(lambda: filter_text() == "Filter: first")
                send(b"\x7f" * 5 if scenario.endswith("backspace") else b"\x03")
                wait_visible(lambda: filter_text() == "Filter:"
                             and visible.text().splitlines()[-1].startswith("Type to Filter")
                             and visible.cursor_visible and (visible.x, visible.y) == (8, 0))
            send(b"\x03" if scenario.startswith("filter_ctrl_c_empty") else b"\x1b")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                         and not visible.text().splitlines()[0].startswith("Filter:")
                         and editor_title() == original_title
                         and editor_line().rstrip() == original_draft
                         and visible.cursor_visible
                         and (visible.x, visible.y) == original_cursor)
            assert child.poll() is None and cli("list") == initial_tasks
            send(b"/")
            wait_visible(lambda: editor_line().rstrip() == original_draft + "/"
                         and editor_title().rstrip().removesuffix(" [*]") == original_title.rstrip().removesuffix(" [*]")
                         and visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                         and (visible.x, visible.y) == (len(original_draft) + 1, editor_row() + 1))
            assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"38;" not in screen and b"48;" not in screen, screen[-2000:]
        elif scenario.startswith("handoff_hint"):
            initial_tasks = cli("list")

            def hint_visible(expected):
                return ("Ctrl-H Herdr" in visible.text().splitlines()[-1]) == expected

            settle()
            assert hint_visible(False), visible.text()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title()
                         and editor_line().startswith("Second") and hint_visible(True))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (" in editor_title()
                         and editor_line().startswith("First") and hint_visible(False))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title() and hint_visible(True))
            send(CTRL_SLASH)
            wait_visible(lambda: visible.text().splitlines()[0].startswith("Filter:")
                         and hint_visible(True) and (visible.x, visible.y) == (8, 0))
            send(b"\x1b")
            wait_visible(lambda: hint_visible(True) and visible.y == editor_row() + 1)
            send(b"\x1b")
            wait_visible(lambda: "New Task" in editor_title()
                         and not editor_line().strip() and hint_visible(False))
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title() and hint_visible(True))
            send(b"\x01Changed ")
            wait_visible(lambda: editor_line().startswith("Changed Second")
                         and (visible.x, visible.y) == (8, editor_row() + 1))
            cursor = (visible.x, visible.y)
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                saved_link = connection.execute("SELECT link_json FROM herdr_links WHERE task_id=2").fetchone()[0]
                connection.execute("DELETE FROM herdr_links WHERE task_id=2")
            wait_visible(lambda: hint_visible(False) and (visible.x, visible.y) == cursor)
            assert editor_line().startswith("Changed Second")
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                connection.execute("INSERT INTO herdr_links(task_id,link_json) VALUES(2,?)", (saved_link,))
            wait_visible(lambda: hint_visible(True) and (visible.x, visible.y) == cursor)
            assert editor_line().startswith("Changed Second")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 49, 0, 0))
            visible.resize(49, 24)
            wait_visible(lambda: "Task #2 (" in editor_title()
                         and editor_line().startswith("Changed Second") and hint_visible(True))
            assert not details_text(), visible.text()
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                connection.execute("DELETE FROM herdr_links WHERE task_id=2")
            wait_visible(lambda: hint_visible(False) and editor_line().startswith("Changed Second"))
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                connection.execute("INSERT INTO herdr_links(task_id,link_json) VALUES(2,?)", (saved_link,))
            wait_visible(lambda: hint_visible(True) and editor_line().startswith("Changed Second"))
            assert cli("list") == initial_tasks
            assert not (Path(folder) / "herdr-calls").exists(), "Hint discovery called Herdr"
            if scenario.endswith("no_color"):
                assert b"38;" not in screen and b"48;" not in screen, screen[-2000:]
        elif scenario.startswith("handoff_"):
            initial_tasks = cli("list")
            if scenario == "handoff_no_selection":
                send(b"Unsaved draft")
                wait_visible(lambda: editor_line().startswith("Unsaved draft")
                             and (visible.x, visible.y) == (13, editor_row() + 1))
            else:
                send(b"\x1b[1;2A" * (19 if scenario == "handoff_scroll" else 1))
                wait_visible(lambda: "Task #2 (" in editor_title())
                if scenario == "handoff_scroll":
                    send(b"\x1b[A" * 20)
                send(b"\x01Changed ")
                wait_visible(lambda: editor_line().startswith("Changed Second")
                             and (visible.x, visible.y) == (8, editor_row() + 1))
            if scenario == "handoff_filter":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: filter_text() == "Filter: first"
                             and (visible.x, visible.y) == (13, 0))
            cursor = (visible.x, visible.y)
            if scenario == "handoff_scroll":
                send(b"\x1b[<65;6;4M\x1b[<65;6;10M\x1b[<65;6;16M")
                wait_visible(lambda: "First" not in "\n".join(visible.text().splitlines()[1:list_bottom() + 1])
                             and not details_text().startswith("#2 · ")
                             and editor_line().startswith("Line 03"))
                settle()
                viewport = visible.text().splitlines()[:-1]
            clear_capture()
            send(b"\x08")
            errors = {
                "handoff_no_selection": "Select task to open its Herdr agent",
                "handoff_missing": "Task has no Herdr link",
                "handoff_stale": "Linked Herdr agent session is not live",
                "handoff_ambiguous": "Multiple Herdr panes match agent session",
                "handoff_focus_error": "Herdr failed: focus denied",
                "handoff_launch_error": "Cannot open Herdr client",
                "handoff_client_error": "Herdr client failed",
            }
            expected = errors.get(scenario, "Returned from Herdr")
            if scenario == "handoff_scroll":
                read_until(b"HERDR_CLIENT")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith(expected)
                         and (scenario == "handoff_scroll" or (visible.x, visible.y) == cursor))
            if scenario == "handoff_scroll":
                assert visible.text().splitlines()[:-1] == viewport, visible.text()
            else:
                assert (visible.x, visible.y) == cursor, visible.text()
                assert editor_line().startswith("Unsaved draft" if scenario == "handoff_no_selection" else "Changed Second")
            assert cli("list") == initial_tasks
            calls_path = Path(folder) / "herdr-calls"
            calls = [json.loads(line) for line in calls_path.read_text().splitlines()] if calls_path.exists() else []
            prefix = ["--session", "named"]
            expected_calls = [prefix + ["agent", "list"], prefix + ["agent", "focus", "live:p2"], prefix]
            count = (0 if scenario in ("handoff_missing", "handoff_no_selection") else 1
                     if scenario in ("handoff_stale", "handoff_ambiguous") else 2
                     if scenario in ("handoff_focus_error", "handoff_launch_error") else 3)
            assert calls == expected_calls[:count], calls
            opened = Path(folder) / "herdr-client-opened"
            assert opened.exists() == (count == 3)
            assert child.poll() is None
            if scenario not in errors and scenario != "handoff_scroll":
                assert opened.read_text() == "cooked"
                assert b"\x1b[?1049l" in screen, "TUI terminal not restored before client"
                if scenario == "handoff_filter":
                    assert filter_text() == "Filter: first"
                    send(b"\t")
                    wait_visible(lambda: (visible.x, visible.y) == (8, editor_row() + 1))
                clear_capture()
                send(b"\x1b[104;5u")
                read_until(b"HERDR_CLIENT")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Returned from Herdr")
                             and (visible.x, visible.y) == (8, editor_row() + 1))
                assert len(calls_path.read_text().splitlines()) == 6
                send(b"\x7f")
                wait_visible(lambda: editor_line().startswith("ChangedSecond"))
                assert len(calls_path.read_text().splitlines()) == 6
                send(b"\x13")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Saved #2"))
                assert cli("show", "2")["task"]["description"] == "ChangedSecond"
        elif scenario.startswith("ctrl_c_new_"):
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
                wait_visible(lambda: filter_text() == "Filter: first")
            if scenario == "ctrl_c_new_scroll":
                send(b"\x1b[<64;6;4M")
                wait_visible(lambda: "Task 12" in visible.text().splitlines()[2]
                             and "Task 17" in visible.text().splitlines()[7])
                scrolled_list = visible.text().splitlines()[:list_bottom() + 1]
            clear_capture()
            send(b"\x03")
            if scenario == "ctrl_c_new_filter":
                wait_visible(lambda: filter_text() == "Filter:"
                             and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1])
                             and editor_line().startswith("Unsaved draft"))
                settle()
                assert "Discard draft?" not in visible.text(), visible.text()
                assert "New Task" in editor_title(), visible.text()
                assert visible.cursor_visible and (visible.x, visible.y) == (8, 0), visible.text()
                send(b"\x03")
                wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                             and visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                             and editor_line().startswith("Unsaved draft")
                             and (visible.x, visible.y) == (len("Unsaved draft"), editor_row() + 1))
                assert child.poll() is None and cli("list") == initial_tasks
                send(b"\x03")
            read_until(b"Discard draft? (y/N)")
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
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                             and editor_line().startswith("Unsaved draft"))
                assert visible.text().splitlines()[:list_bottom() + 1] == scrolled_list, visible.text()
                assert cli("list") == initial_tasks
            if scenario in ("ctrl_c_new_keep", "ctrl_c_new_filter"):
                send(b"n")
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                             and editor_line().startswith("Unsaved draft"))
                send(b"\x13")
                wait_visible(lambda: "Task #3 (" in editor_title())
                assert cli("show", "3")["task"]["description"] == "Unsaved draft"
        elif scenario in ("live_title", "live_title_filtered"):
            cli("edit", "2", "--priority", "8")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            send(b"\x01Draft ")
            wait_visible(lambda: editor_line().startswith("Draft Second"))
            if scenario == "live_title_filtered":
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: filter_text() == "Filter: first"
                             and "Second" not in "\n".join(visible.text().splitlines()[2:7]))
            def title_status(status):
                wait_visible(lambda: f"Task #2 ({status})" in editor_title())
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
            wait_visible(lambda: "Second" not in "\n".join(visible.text().splitlines()[1:7]))
            cli("edit", "2", "--set-status", "new")
            title_status("New")
            assert cli("show", "2")["task"]["archived"]
            assert cli("show", "2")["task"]["description"] == "Second"
        elif scenario == "live_refresh":
            settle()
            clear_capture()
            cli("add", "External task")
            wait_visible(lambda: "External task" in visible.text())
            assert "New Task" in editor_title()
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
            wait_visible(lambda: "External task" in visible.text().splitlines()[1]
                         and "Changed externally" in visible.text().splitlines()[2])
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
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x01Unsaved " + CTRL_SLASH + b"second")
            wait_visible(lambda: editor_line().startswith("Unsaved Second")
                         and filter_text() == "Filter: second"
                         and (visible.x, visible.y) == (len("Filter: second"), 0))
            cursor_before = (visible.x, visible.y)
            cli("edit", "2", "--description", "Updated second externally")
            wait_visible(lambda: "Updated second externally" in visible.text().splitlines()[1])
            cli("add", "Second external task")
            wait_visible(lambda: "Second external task" in visible.text()
                         and (visible.x, visible.y) == cursor_before)
            assert "First" not in visible.text()
            assert editor_line().startswith("Unsaved Second")
            assert "Task #2 (" in editor_title()
            assert filter_text() == "Filter: second"
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
            wait_visible(lambda: "Task #20 (" in editor_title())
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 12" in visible.text().splitlines()[2])
            cli("edit", "13", "--description", "Refreshed row")
            wait_visible(lambda: "Refreshed row" in visible.text().splitlines()[3])
            assert "Task #20 (" in editor_title()
            assert editor_line().startswith("Task 20")
        elif scenario == "tree_navigation":
            expected = (2, 3, 1)
            for task_id in expected:
                clear_capture()
                send(b"\x1b[1;2A")
                wait_visible(lambda: f"Task #{task_id} (" in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #3 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "Task #2 (" in editor_title())
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title())
        elif scenario.startswith("escape_staged_"):
            initial_tasks = cli("list")
            child_draft = scenario == "escape_staged_child"
            empty = scenario in ("escape_staged_empty", "escape_staged_empty_filter") or child_draft
            if child_draft:
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second"))
                send(b"\x10")
                wait_visible(lambda: "New Task (parent #2)" in editor_title()
                             and editor_line().strip() == ""
                             and (visible.x, visible.y) == (0, editor_row() + 1))
            if scenario == "escape_staged_new_image":
                image_path = Path(folder) / "unsaved.png"
                image_path.write_bytes(base64.b64decode(
                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="
                ))
                send(b"\x1b[200~" + str(image_path).encode() + b"\x1b[201~")
                wait_visible(lambda: "[Image #1: unsaved.png]" in editor_line()
                             and (visible.x, visible.y) == (len("[Image #1: unsaved.png]"), editor_row() + 1))
            elif not empty:
                payload = b"   " if scenario.endswith("whitespace") else b"Unsaved draft"
                send(payload)
                wait_visible(lambda: editor_line().startswith(payload.decode())
                             and (visible.x, visible.y) == (len(payload), editor_row() + 1))
            original_draft = editor_line()
            empty_filter = scenario.endswith("empty_filter")
            send(CTRL_SLASH + (b"" if empty_filter else b"first"))
            wait_visible(lambda: filter_text() == ("Filter:" if empty_filter else "Filter: first")
                         and visible.cursor_visible)
            send(b"\x1b")
            wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                         and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1]))
            assert editor_line() == original_draft, visible.text()
            assert child.poll() is None and cli("list") == initial_tasks
            if child_draft:
                assert "parent #2" in editor_title(), visible.text()
                send(b"\x1b")
                wait_visible(lambda: "New Task" in editor_title() and "parent" not in editor_title()
                             and (visible.x, visible.y) == (0, editor_row() + 1))
                assert child.poll() is None and cli("list") == initial_tasks
            if not empty:
                clear_capture()
                send(b"\x1b")
                read_until(b"Discard changes and switch? (y/N)")
                send(b"n")
                wait_visible(lambda: editor_line() == original_draft
                             and visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
                assert child.poll() is None and cli("list") == initial_tasks
                clear_capture()
                send(b"\x1b")
                read_until(b"Discard changes and switch? (y/N)")
                send(b"y")
                wait_visible(lambda: "New Task" in editor_title() and editor_line().strip() == ""
                             and (visible.x, visible.y) == (0, editor_row() + 1))
                assert child.poll() is None and cli("list") == initial_tasks
            # Final Esc exit goes through common terminal-restoration assertions.
        elif scenario in (
            "escape_selected", "ctrl_c_selected", "escape_dirty_selected", "ctrl_c_dirty_selected",
            "ctrl_c_dirty_selected_filter_editor", "ctrl_c_dirty_selected_filter_focused",
            "ctrl_c_selected_filter_menu",
            "escape_selected_filter_editor", "escape_selected_filter_focused",
            "escape_dirty_selected_filter_editor", "escape_dirty_selected_filter_focused",
            "escape_selected_filter_menu",
            "escape_dirty_selected_filter_confirmation",
        ):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title())
            if "dirty_selected" in scenario:
                send(b"\x01Changed ")
                wait_visible(lambda: editor_line().startswith("Changed Second"))
            if "_filter_" in scenario:
                send(CTRL_SLASH + b"first")
                wait_visible(lambda: filter_text() == "Filter: first"
                             and "Second" not in "\n".join(visible.text().splitlines()[1:list_bottom() + 1]))
                if not scenario.endswith("focused"):
                    send(b"\t")
                    wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
                if scenario.endswith("menu"):
                    send(b"\x07")
                    wait_visible(lambda: "Task actions" in visible.text())
                if scenario.endswith("confirmation"):
                    send(b"\x07c")
                    wait_visible(lambda: visible.text().splitlines()[-1].startswith("Confirm action?"))
            key = b"\x03" if scenario.startswith("ctrl_c") else b"\x1b"
            clear_capture()
            send(key)
            if "_filter_" in scenario:
                if scenario.endswith("menu"):
                    # Wait for first cancellation to clear filter before sending
                    # another Escape. Fixed delays can merge inputs under load.
                    wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                                 and "Task actions" in visible.text()
                                 and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1]))
                    send(b"\x1b")
                expected_draft = "Changed Second" if "dirty_selected" in scenario else "Second"
                focused_ctrl_c = scenario.startswith("ctrl_c") and scenario.endswith("focused")
                wait_visible(lambda: (filter_text() == "Filter:" if focused_ctrl_c
                                      else not visible.text().splitlines()[0].startswith("Filter:"))
                             and (scenario.endswith("confirmation")
                                  or ("Task #2 (" in editor_title() and editor_line().startswith(expected_draft)))
                             and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1]))
                settle()
                if not scenario.endswith("confirmation"):
                    assert "Confirm action?" not in visible.text(), visible.text()
                assert child.poll() is None
                assert cli("list") == initial_tasks
                if scenario.endswith("confirmation"):
                    assert visible.text().splitlines()[-1].startswith("Confirm action?"), visible.text()
                    send(b"\x1b")
                    wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                                 and editor_line().startswith(expected_draft))
                clear_capture()
                send(key)
                if focused_ctrl_c:
                    wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                                 and visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                                 and "Task #2 (" in editor_title()
                                 and editor_line().startswith(expected_draft))
                    assert child.poll() is None and cli("list") == initial_tasks
                    send(key)
            if "dirty_selected" in scenario:
                read_until(b"Discard changes and switch? (y/N)")
                assert cli("list") == initial_tasks
                assert child.poll() is None
                if scenario.startswith("ctrl_c"):
                    clear_capture()
                    send(key)
                    wait_visible(lambda: bool(screen)
                                 and visible.text().splitlines()[-1].startswith("Discard changes and switch? (y/N)"))
                    assert child.poll() is None
                send(b"n")
                wait_visible(lambda: editor_line().startswith("Changed Second")
                             and "Task #2 (" in editor_title()
                             and visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
                clear_capture()
                send(key)
                read_until(b"Discard changes and switch? (y/N)")
                send(b"y")
            wait_visible(lambda: "New Task" in editor_title()
                         and editor_line().strip() == ""
                         and ("_filter_" not in scenario
                              or (not visible.text().splitlines()[0].startswith("Filter:")
                                  and "Second" in "\n".join(visible.text().splitlines()[1:list_bottom() + 1]))))
            assert cli("list") == initial_tasks
            assert child.poll() is None
        elif scenario in ("save_selected", "after_save_open_saved", "after_save_default_restored"):
            send(b"Created\x13")
            wait_visible(lambda: "Task #3 (New)" in editor_title()
                         and editor_line().startswith("Created"))
            assert cli("show", "3")["task"]["description"] == "Created"
            send(b"\x05 updated\x13")
            wait_visible(lambda: editor_line().startswith("Created updated")
                         and visible.text().splitlines()[-1].startswith("Saved #3")
                         and (visible.x, visible.y) == (len("Created updated"), editor_row() + 1))
            assert len(cli("list")) == 3
            assert cli("show", "3")["task"]["description"] == "Created updated"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title())
        elif scenario == "shift_enter_text":
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            send(SHIFT_ENTER + b"tail")
            wait_visible(lambda: "Task #2 (New)" in editor_title()
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
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            if scenario == "child_dirty":
                send(b"\x01Draft ")
                wait_visible(lambda: editor_line().startswith("Draft Second"))
                assert cli("show", "2")["task"]["description"] == "Second"
                send(CTRL_P)
            else:
                send(CTRL_P)
            wait_visible(lambda: "New Task (parent #2)" in editor_title()
                         and editor_line().strip() == ""
                         and (visible.x, visible.y) == (0, editor_row() + 1))
            assert len(cli("list")) == 2
            assert "Ctrl-P Create Child" in visible.text().splitlines()[-1], visible.text()
            if scenario == "child_no_selection":
                send(b"\x1b[1;2A")
                wait_visible(lambda: "Task #2 (New)" in editor_title())
                send(b"\x1b[1;2B")
                wait_visible(lambda: "New Task" in editor_title()
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
                wait_visible(lambda: "New Task" in editor_title()
                             and "parent" not in editor_title()
                             and editor_line().strip() == ""
                             and visible.text().splitlines()[-1].startswith("Saved #3. New task"))
            else:
                wait_visible(lambda: "Task #3 (New)" in editor_title()
                             and visible.text().splitlines()[-1].startswith("Saved #3"))
            task = cli("show", "3")["task"]
            assert task["description"] == "Child"
            assert task["parent_id"] == (None if scenario == "child_no_selection" else 2)
            assert cli("show", "2")["task"]["description"] == "Second"
            if scenario != "child_open_new":
                send(b"\x1b[1;2B")
                wait_visible(lambda: "New Task" in editor_title())
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
            wait_visible(lambda: "New Task" in editor_title()
                         and editor_line().strip() == ""
                         and visible.text().splitlines()[-1].startswith("Saved #3. New task")
                         and (visible.x, visible.y) == (0, editor_row() + 1))
            assert cli("show", "3")["task"]["description"] == "Created"
            send(b"Next\x13")
            wait_visible(lambda: "New Task" in editor_title()
                         and editor_line().strip() == ""
                         and visible.text().splitlines()[-1].startswith("Saved #4. New task"))
            assert cli("show", "4")["task"]["description"] == "Next"
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #4 (New)" in editor_title())
            send(b"\x05 updated\x13")
            wait_visible(lambda: "Task #4 (New)" in editor_title()
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
            wait_visible(lambda: "Target 12" in visible.text().splitlines()[2])
            settle()
            clear_capture()
            click(6, task_row("Target 14"))
            wait_visible(lambda: 'Task #14' in editor_title())
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
            wait_visible(lambda: "New Task" in editor_title()
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
            wait_visible(lambda: "No tasks yet." in visible.text().splitlines()[0])
            assert cli("list") == []
            assert editor_title().startswith("Task Editor - New Task")
        elif scenario == "workflow_status":
            wait_visible(lambda: "Completed" in visible.text()
                         and "Error" in visible.text()
                         and "Fresh item" in visible.text())
            assert "New          Fresh item" in visible.text(), visible.text()
            clear_capture()
            send(b"\x1b[1;2A" * 2)
            wait_visible(lambda: "Task #2 (Error)" in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #1 (Completed)" in editor_title())
        elif scenario in ("actions_popup", "actions_popup_no_color"):
            initial_tasks = cli("list")
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title()
                         and (visible.x, visible.y) == (6, editor_row() + 1))
            before_popup = visible.text().splitlines()
            send(b"\x07")
            wait_visible(lambda: visible.text().splitlines()[7][12] == "┌"
                         and "Task actions #2" in visible.text().splitlines()[8]
                         and visible.text().splitlines()[16][59] == "┘")
            popup_rows = visible.text().splitlines()
            for row in range(24):
                for column in range(72):
                    if not (7 <= row < 17 and 12 <= column < 60):
                        assert popup_rows[row][column] == before_popup[row][column], (row, column)
            send(b"\x1b[<0;6;4M\x1b[<0;6;4m\x10")
            settle()
            assert "Task actions #2" in visible.text(), visible.text()
            assert cli("list") == initial_tasks
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 90, 0, 0))
            visible.resize(90, 30)
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: visible.text().splitlines()[10][21] == "┌"
                         and "Task actions #2" in visible.text().splitlines()[11]
                         and visible.text().splitlines()[19][68] == "┘"
                         and (visible.x, visible.y) == (22, 12))
            settle()
            send(b"p")
            wait_visible(lambda: "Priority task #2" in visible.text()
                         and (visible.x, visible.y) == (24, 15))
            send(b"-9")
            wait_visible(lambda: "Priority task #2" in visible.text()
                         and "> -9" in visible.text()
                         and (visible.x, visible.y) == (26, 15))
            assert "c Complete" not in visible.text(), visible.text()
            send(b"\x7f\x7f\x1b[200~" + b" " * 50 + b"5\x1b[201~")
            wait_visible(lambda: visible.text().splitlines()[15][66] == "5"
                         and (visible.x, visible.y) == (67, 15))
            send(b"\x1b")
            wait_visible(lambda: "Task #2 (New)" in editor_title()
                         and editor_line().startswith("Second")
                         and (visible.x, visible.y) == (6, editor_row() + 1)
                         and "Priority task #2" not in visible.text())
            assert cli("list") == initial_tasks
            if scenario == "actions_popup_no_color":
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario.startswith("menu_error_"):
            task_id = 4 if scenario == "menu_error_new" else 1
            label = "Fresh item" if task_id == 4 else "Owned item"
            click(5, task_row(label))
            wait_frame(lambda: f"Task #{task_id} (" in editor_title() and editor_line().startswith(label))
            initial_detail = cli("show", str(task_id))
            dirty = scenario in ("menu_error_dirty", "menu_error_rejected", "menu_error_live")
            if dirty:
                send(b" local")
                wait_frame(lambda: editor_line().startswith(label + " local"))
            expected_editor = label + (" local" if dirty else "")

            def modal_ready(title, hint):
                wait_visible(lambda: title in visible.text() and hint in visible.text()
                             and not visible.pending
                             and screen.endswith(f"\x1b[{visible.y + 1};{visible.x + 1}H".encode()))

            def error_prompt(arrows=False):
                send(b"\x07")
                wait_visible(lambda: f"Task actions #{task_id}" in visible.text()
                             and "e Mark error" in visible.text())
                send(b"\x1b[A\r" if arrows else b"e")
                modal_ready(f"Error task #{task_id}", "Enter apply  Esc cancel")

            if scenario == "menu_error_narrow":
                clear_capture()
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 50, 0, 0))
                visible.resize(50, 12)
                os.kill(child.pid, signal.SIGWINCH)
                wait_frame(lambda: f"Task #{task_id} (" in editor_title()
                           and editor_line().startswith(label))
                reason = "Reason " + "X" * 100 + " TAIL"
            else:
                reason = "Worker failure"

            error_prompt(arrows=scenario == "menu_error_narrow")
            if scenario.startswith("menu_error_success"):
                send(b"\r")
                modal_ready("Error reason cannot be empty", "Enter apply  Esc cancel")
                assert cli("show", str(task_id)) == initial_detail
                send(b"   \r")
                modal_ready("Error reason cannot be empty", "Enter apply  Esc cancel")
                send(b"\x7f\x7f\x7f\x1b[200~Worker\nfailure!\x1b[201~\x7f")
                modal_ready("> Worker failure", "Enter apply  Esc cancel")
            else:
                send(reason.encode())
            send(b"\r")
            modal_ready(f"Mark error task #{task_id}?", "y confirm  n/Esc cancel")
            assert "Reason:" in visible.text(), visible.text()
            if dirty:
                assert "Lose draft?" in visible.text(), visible.text()
            assert cli("show", str(task_id)) == initial_detail
            send(b"n")
            wait_frame(lambda: "Mark error task" not in visible.text()
                       and editor_line().startswith(expected_editor))
            assert cli("show", str(task_id)) == initial_detail
            error_prompt()
            send(b"Cancelled")
            send(b"\x1b")
            wait_frame(lambda: f"Error task #{task_id}" not in visible.text()
                       and editor_line().startswith(expected_editor))
            assert cli("show", str(task_id)) == initial_detail
            error_prompt()
            send(reason.encode() + b"\r")
            modal_ready(f"Mark error task #{task_id}?", "y confirm  n/Esc cancel")
            if scenario == "menu_error_live":
                cli("edit", "1", "--set-status", "new", "--session", "worker")
                cli("next", "--local", "--session", "outside", "--filter", "id == 1")
                initial_detail = cli("show", "1")
            send(b"y")
            if scenario in ("menu_error_rejected", "menu_error_live", "menu_error_new"):
                modal_ready("Action error", "Up/Down Esc")
                assert cli("show", str(task_id)) == initial_detail
                send(b"\x1b")
                wait_frame(lambda: "Action error" not in visible.text()
                           and editor_line().startswith(expected_editor))
            else:
                wait_frame(lambda: f"Task #{task_id} (Error)" in editor_title()
                           and visible.text().splitlines()[-1].startswith(f"Marked error #{task_id}"))
                detail = cli("show", str(task_id))
                task = detail["task"]
                assert task["status"] == "error"
                for field in ("description", "priority", "parent_id", "content_revision"):
                    assert task[field] == initial_detail["task"][field]
                for field in ("harness_name", "harness_session", "orchestrator_name", "orchestrator_session"):
                    assert task[field] is None
                assert detail["images"] == initial_detail["images"]
                assert len(detail["events"]) == len(initial_detail["events"]) + 1
                assert detail["events"][-1]["action"] == "error"
                assert len(detail["messages"]) == len(initial_detail["messages"]) + 1
                assert detail["messages"][-1]["body"] == reason
                with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as connection:
                    assert connection.execute("SELECT claim_key FROM tasks WHERE id=?", (task_id,)).fetchone() == (None,)
                assert cli("next", "--local", "--session", "probe", "--filter", f"id == {task_id}") is None
                send(b"\x07")
                wait_visible(lambda: "r Retry error" in visible.text())
                send(b"\x1b")
                wait_frame(lambda: "Task actions" not in visible.text() and editor_line().startswith(label))
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen
        elif scenario.startswith("menu_retry_"):
            status = scenario.removeprefix("menu_retry_").removesuffix("_no_color")
            if status == "live":
                status = "error"
            label, task_id = {
                "new": ("Fresh item", 4),
                "in_progress": ("Owned item", 1),
                "completed": ("Finished item", 3),
                "error": ("Failed item", 2),
            }[status]
            initial_tasks = cli("list")
            click(5, task_row(label))
            wait_visible(lambda: f"Task #{task_id} (" in editor_title()
                         and editor_line().startswith(label))

            def selected_action(key):
                return any(part.strip().startswith(f"> {key} ")
                           for line in visible.text().splitlines() for part in line.split("│"))

            def open_menu():
                send(b"\x07")
                wait_visible(lambda: f"Task actions #{task_id}" in visible.text()
                             and selected_action("c")
                             and "Up/Down Select" in visible.text())

            open_menu()
            settle()
            assert ("r Retry error" in visible.text()) == (status == "error"), visible.text()
            if scenario == "menu_retry_live":
                send(b"\x1b[B")
                wait_visible(lambda: selected_action("r"))
                cli("edit", str(task_id), "--set-status", "new")
                wait_visible(lambda: "r Retry error" not in visible.text()
                             and selected_action("o"))
                send(b"r")
                settle()
                assert f"Task actions #{task_id}" in visible.text(), visible.text()
                assert selected_action("o"), visible.text()
                assert cli("next", "--local", "--session", "external")["id"] == task_id
                cli("edit", str(task_id), "--set-status", "error",
                    "--reason", "Failed again", "--session", "external")
                wait_visible(lambda: "r Retry error" in visible.text()
                             and selected_action("o"))
                send(b"\x1b[A")
            elif status != "error":
                send(b"r")
                settle()
                assert f"Task actions #{task_id}" in visible.text(), visible.text()
                assert "Retry task" not in visible.text(), visible.text()
                assert selected_action("c"), visible.text()
                send(b"\x1b[B")
            else:
                send(b"\x1b[B")
            wait_visible(lambda: selected_action("r" if status == "error" else "o"))
            if scenario != "menu_retry_live":
                assert cli("list") == initial_tasks
            send(b"\r")
            wait_visible(lambda: f"{'Retry' if status == 'error' else 'Reopen'} task #{task_id}?" in visible.text())
            send(b"y" if status == "error" else b"n")
            wait_visible(lambda: f"Task #{task_id} (" in editor_title()
                         and editor_line().startswith(label)
                         and visible.cursor_visible
                         and "Reopen task" not in visible.text()
                         and (status != "error" or f"Task #{task_id} (New)" in editor_title()))
            if status == "error":
                assert cli("show", str(task_id))["task"]["status"] == "new"
                open_menu()
                settle()
                assert "r Retry error" not in visible.text(), visible.text()
                send(b"\x1b[B")
                wait_visible(lambda: selected_action("o"))
                send(b"\x1b")
                wait_visible(lambda: "Task actions" not in visible.text()
                             and editor_line().startswith(label) and visible.cursor_visible)
            else:
                assert cli("list") == initial_tasks
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario.startswith("menu_arrows"):
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (New)" in editor_title())
            if scenario == "menu_arrows_narrow":
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 12, 0, 0))
                visible.resize(12, 8)
                os.kill(child.pid, signal.SIGWINCH)
                wait_visible(lambda: editor_row() is not None and editor_line().startswith("Second"))
            initial_tasks = cli("list")

            def selected_action(key):
                return any(part.strip().startswith(f"> {key} ")
                           for line in visible.text().splitlines() for part in line.split("│"))

            def open_menu():
                send(b"\x07")
                wait_visible(lambda: selected_action("c"))

            def close_menu():
                send(b"\x1b")
                wait_visible(lambda: editor_row() is not None and editor_line().startswith("Second")
                             and (visible.x, visible.y) == (len("Second"), editor_row() + 1))

            def prompt_visible(prompt):
                return prompt[:visible.width] in visible.text()

            open_menu()
            send(b"\x1b[A")
            wait_visible(lambda: selected_action("e"))
            send(b"\x1b[B")
            wait_visible(lambda: selected_action("c"))
            for key in "oapd":
                send(b"\x1b[B")
                wait_visible(lambda key=key: selected_action(key))
            assert cli("list") == initial_tasks
            send(b"\r")
            wait_visible(lambda: prompt_visible("Parent task"))
            close_menu()
            for index, (key, prompt) in enumerate([
                ("c", "Force complete task"), ("o", "Reopen task"), ("a", "Archive task"),
                ("p", "Priority task"),
            ]):
                open_menu()
                if index:
                    send(b"\x1b[B" * index)
                    wait_visible(lambda key=key: selected_action(key))
                send(b"\r")
                wait_visible(lambda prompt=prompt: prompt_visible(prompt))
                close_menu()
                assert cli("list") == initial_tasks
            open_menu()
            send(b"\x1b[A\x1b[A\x1b[A")
            wait_visible(lambda: selected_action("p"))
            send(b"\r")
            wait_visible(lambda: prompt_visible("Priority task"))
            send(b"9\r")
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Priority #2:"))
            assert cli("show", "2")["task"]["priority"] == 9
            assert cli("show", "2")["task"]["description"] == "Second"
            send(b"\x01X")
            wait_visible(lambda: editor_line().startswith("XSecond"))
            open_menu()
            send(b"\x1b[A\x1b[A\x1b[A\r")
            wait_visible(lambda: prompt_visible("Priority task"))
            send(b"8\r")
            wait_visible(lambda: "Lose draft?" in visible.text())
            send(b"n")
            wait_visible(lambda: editor_line().startswith("XSecond"))
            assert cli("show", "2")["task"]["priority"] == 9
            assert cli("show", "2")["task"]["description"] == "Second"
            if scenario == "menu_arrows_no_color":
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario == "force_complete_owner_db_error":
            click(5, task_row("Foreign item"))
            wait_visible(lambda: "Task #1 (In progress)" in editor_title())
            task_before = cli("show", "1")
            send(b"\x01X\x07c")
            wait_visible(lambda: "Action error" in visible.text() and "malformed JSON" in visible.text())
            assert "Force complete" not in visible.text(), visible.text()
            assert cli("show", "1") == task_before
            send(b"\x1b")
            wait_visible(lambda: "Action error" not in visible.text()
                         and editor_line().startswith("XForeign item"))
            send(b"\x7f")
            wait_visible(lambda: editor_line().startswith("Foreign item"))
        elif scenario.startswith("force_complete"):
            wait_visible(lambda: "Foreign item" in visible.text())
            click(5, task_row("Foreign item"))
            wait_visible(lambda: "Task #1 (In progress)" in editor_title())
            before_force = cli("show", "1")
            send(b"\x01X")
            wait_visible(lambda: editor_line().startswith("XForeign item"))
            send(b"\x07c")
            wait_visible(lambda: "Force complete task #1?" in visible.text()
                         and "Complete without matching task owner." in visible.text()
                         and "Lose draft?" in visible.text())
            assert cli("show", "1") == before_force
            send(b"n")
            wait_visible(lambda: "Force complete task" not in visible.text()
                         and editor_line().startswith("XForeign item"))
            assert cli("show", "1") == before_force
            send(b"\x07\r")
            wait_visible(lambda: "Force complete task #1?" in visible.text())
            send(b"y")
            wait_visible(lambda: "Task #1 (Completed)" in editor_title()
                         and editor_line().startswith("Foreign item"))
            completed = cli("show", "1")
            assert completed["task"]["status"] == "completed"
            assert completed["task"]["description"] == "Foreign item"
            assert completed["task"]["content_revision"] == before_force["task"]["content_revision"]
            assert completed["events"][-1]["action"] == "complete"
            actor = {"force_complete_sessionless": "manual", "force_complete_native": "native-display"}.get(scenario, "worker")
            assert completed["events"][-1]["session"] == actor

            for task_id, label in ((2, "Failed item"), (3, "Fresh item")):
                click(5, task_row(label))
                wait_visible(lambda: f"Task #{task_id}" in editor_title())
                task_before = cli("show", str(task_id))
                send(b"\x07c")
                wait_visible(lambda: f"Force complete task #{task_id}?" in visible.text())
                send(b"\x1b")
                wait_visible(lambda: "Force complete task" not in visible.text())
                assert cli("show", str(task_id)) == task_before
                send(b"\x07c")
                wait_visible(lambda: f"Force complete task #{task_id}?" in visible.text())
                send(b"y")
                wait_visible(lambda: f"Task #{task_id} (Completed)" in editor_title())
                after = cli("show", str(task_id))
                assert after["task"]["status"] == "completed"
                assert after["events"][-1]["action"] == "complete"
                assert after["events"][-1]["session"] == actor

            click(5, task_row("Owned item"))
            wait_visible(lambda: "Task #4 (In progress)" in editor_title())
            send(b"\x07c")
            if scenario == "force_complete_sessionless":
                wait_visible(lambda: "Force complete task #4?" in visible.text())
            else:
                wait_visible(lambda: "Complete task #4?" in visible.text())
                assert "Force complete" not in visible.text(), visible.text()
                assert "Complete without matching task owner." not in visible.text()
            if scenario == "force_complete_race":
                cli("edit", "4", "--set-status", "new", "--session", "worker")
                cli("next", "--local", "--session", "new-owner")
                transferred = cli("show", "4")
                send(b"y")
                wait_visible(lambda: "Action error" in visible.text()
                             and "Task 4 is not claimed by session worker" in visible.text())
                assert cli("show", "4") == transferred
                send(b"\x1b")
                wait_visible(lambda: "Action error" not in visible.text())
                send(b"\x07c")
                wait_visible(lambda: "Force complete task #4?" in visible.text())
            send(b"y")
            wait_visible(lambda: "Task #4 (Completed)" in editor_title())
            event = cli("show", "4")["events"][-1]
            assert event["action"] == "complete"

            click(5, task_row("Finished item"))
            wait_visible(lambda: "Task #5 (Completed)" in editor_title())
            task_before = cli("show", "5")
            send(b"\x07c")
            wait_visible(lambda: "Force complete task #5?" in visible.text())
            send(b"y")
            wait_visible(lambda: "Task 5 must be unfinished to force complete" in visible.text())
            assert cli("show", "5") == task_before
            send(b"\x1b")
            wait_visible(lambda: "Action error" not in visible.text())
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen, screen[-2000:]
        elif scenario == "actions_basic":
            wait_visible(lambda: "Owned item" in visible.text() and "Hidden item" in visible.text())
            def select_action_task(label, task_id):
                clear_capture()
                click(5, task_row(label))
                wait_visible(lambda: f"Task #{task_id}" in editor_title())

            def action(letter, prompt):
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "Task actions" in visible.text() and "c Complete" in visible.text())
                clear_capture()
                send(letter.encode())
                read_until(prompt.encode())

            click(5, task_row("Owned item"))
            wait_visible(lambda: 'Task #1 (In progress)' in editor_title())
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
            action("c", "Force complete task #4?")
            send(b"n")
            wait_visible(lambda: editor_line().startswith("XFresh item"))
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
            def rejected_action(label, task_id, letter, prompt, error, before_confirm=None):
                clear_capture()
                click(5, task_row(label))
                wait_visible(lambda: f"Task #{task_id}" in editor_title())
                clear_capture()
                send(b"\x07")
                wait_visible(lambda: "c Complete" in visible.text())
                send(letter.encode())
                read_until(prompt.encode())
                if before_confirm is not None:
                    before_confirm()
                clear_capture()
                send(b"y")
                read_until(error.encode())
                send(b"\x1b")
                wait_visible(lambda: f"Task #{task_id}" in editor_title())
                assert f"Task #{task_id}" in editor_title()

            wait_visible(lambda: "Owned item" in visible.text() and "Hidden item" in visible.text())
            rejected_action("Owned item", 1, "a", "Archive task #1?",
                            "Task 1 is in progress and cannot be archived")
            rejected_action("Owned item", 1, "o", "Reopen task #1?",
                            "Task 1 must be completed to reopen")
            rejected_action("Failed item", 2, "r", "Retry task #2?",
                            "Task 2 is no longer in error",
                            lambda: cli("edit", "2", "--set-status", "new"))
            assert cli("show", "1")["task"]["archived"] is False
            assert cli("show", "2")["task"]["status"] == "new"
            assert cli("show", "3")["task"]["status"] == "completed"

            clear_capture()
            click(5, task_row("Fresh item"))
            wait_visible(lambda: 'Task #4' in editor_title())
            send(b"\x07p101\r")
            wait_visible(lambda: "Priority must be -100..100" in visible.text())
            assert "101" in visible.text(), visible.text()
            send(b"\x1b")
            wait_visible(lambda: "Task #4" in editor_title())
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
            wait_visible(lambda: "Task #2" in editor_title())
            send(b"\x07a")
            read_until(b"Archive task #2?")
            send(b"y")
            wait_visible(lambda: "No matching tasks." in visible.text() and
                         "New Task" in editor_title() and
                         "Filter: Filtered" in visible.text())
            assert cli("show", "2")["task"]["archived"] is True
            assert not select.select([child.stdout], [], [], 0)[0], "TUI wrote stdout"
        elif scenario == "actions_narrow":
            click(5, task_row("First"))
            wait_visible(lambda: 'Task #1' in editor_title())
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 12, 0, 0))
            visible.resize(12, 8)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: visible.text().splitlines()[0].startswith(" 1"))
            wait_visible(lambda: editor_row() is not None
                         and editor_line().startswith("First")
                         and (visible.x, visible.y) == (len("First"), editor_row() + 1)
                         and screen.endswith(b"\x1b[6;6H"))
            settle()
            send(b"\x07")
            wait_visible(lambda: "c Complete" in visible.text() and
                         "d Parent" in visible.text() and "Up/Dn Enter" in visible.text())
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
            wait_visible(lambda: visible.text().splitlines()[0].startswith(" 1"))
            send(b"\x07d0\r")
            wait_visible(lambda: "Parent must be a positive task ID or none" in
                         " ".join(row.strip() for row in visible.text().splitlines()[3:8]))
            send(b"\x1b")
            wait_visible(lambda: visible.text().splitlines()[0].startswith(" 1"))
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
            wait_visible(lambda: 'Task Editor - Task #1' in editor_title())
            wait_visible(lambda: "Task 1 is in progress and cannot be archived" in visible.text())
            settle()
        elif scenario == "click":
            initial_tasks = cli("list")
            wait_visible(lambda: "Parent detail" in visible.text() and "Child" in visible.text())
            settle()
            clear_capture()
            click(6, 7)
            click(6, 8)
            click(6, 9)
            click(6, 12)
            click(6, 14)
            settle()
            assert not screen, f"Inactive click redrew TUI: {screen[-500:]!r}"
            assert cli("list") == initial_tasks

            clear_capture()
            click(5, 1)
            wait_visible(lambda: 'Task #1' in editor_title()
                         and editor_line().startswith("Parent"))
            clear_capture()
            click(5, task_row("Parent detail"))
            wait_visible(lambda: 'Task #1' in editor_title())
            wait_visible(lambda: editor_line().startswith("Parent"))
            clear_capture()
            send(b"\x1b[<65;6;4M")
            wait_visible(lambda: "word" in visible.text().splitlines()[4]
                         and "Child detail" not in visible.text().splitlines()[4])
            clear_capture()
            click(5, task_row("d word"))
            wait_visible(lambda: 'Task #2' in editor_title())
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
            wait_visible(lambda: "Workspace" in visible.text().splitlines()[1]
                         and "Needle child" in visible.text().splitlines()[2]
                         and "Other" not in visible.text())
            clear_capture()
            send(b"\x1b[<65;6;4M" * 4)
            wait_visible(lambda: "Needle 14" in visible.text().splitlines()[1])
            settle()
            clear_capture()
            click_editor(3)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
            clear_capture()
            send(CTRL_SLASH)
            wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to Filter"))
            clear_capture()
            click(6, task_row("Needle 14"))
            wait_visible(lambda: 'Task #14' in editor_title())
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
            wait_visible(lambda: 'Task #3' in editor_title())
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
            missing_row = task_row("Second")
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            clear_capture()
            click(5, missing_row)
            read_until(b"Task 2 not found")
            assert "New Task" in editor_title()
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
            wait_visible(lambda: 'Task #20' in editor_title())
            wheel_editor(False, 10)
            wait_visible(lambda: editor_line().startswith("Line01"))

            clear_capture()
            send(b"\x1b[<64;6;4M")
            wait_visible(lambda: "Task 14" in visible.text().splitlines()[2])
            assert editor_line().startswith("Line01"), visible.text()
            clear_capture()
            wheel_editor(True, 1)
            wait_visible(lambda: editor_line().startswith("Line04"))
            assert "Task 14" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x11")
            time.sleep(0.1)
            while select.select([master], [], [], 0)[0]:
                capture(os.read(master, 65536))
            assert editor_line().startswith("Line04"), visible.text()

            clear_capture()
            send(b"\x1b[<65;6;9M\x1b[<65;6;13M")
            time.sleep(0.1)
            assert "Task 14" in visible.text().splitlines()[2], visible.text()
            assert editor_line().startswith("Line04"), visible.text()
            clear_capture()
            wheel_editor(False, 10)
            wait_visible(lambda: editor_line().startswith("Line01"))
            clear_capture()
            wheel_editor(True, 10)
            wait_visible(lambda: editor_line().startswith("Line07"))
            assert "Task 14" in visible.text().splitlines()[2], visible.text()
            clear_capture()
            send(b"\x1b[<64;6;4M" * 10)
            wait_visible(lambda: "First" in visible.text().splitlines()[0])
            assert editor_line().startswith("Line07"), visible.text()
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Task 17" in visible.text().splitlines()[2]
                         and "Line03..." in visible.text().splitlines()[7])

            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 72, 0, 0))
            visible.resize(72, 36)
            clear_capture()
            os.kill(child.pid, signal.SIGWINCH)
            wait_visible(lambda: "Task 12" in visible.text().splitlines()[2]
                         and editor_line().startswith("Line02")
                         and (visible.x, visible.y) == (6, 34))
            settle()
            clear_capture()
            send(b"!")
            wait_visible(lambda: "Line15!" in "\n".join(visible.text().splitlines()[editor_row() + 1:-1]))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'Task #19' in editor_title())
            wait_visible(lambda: "Task 12" in visible.text().splitlines()[2]
                         and editor_line().startswith("Task 19"))
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b[1;2A" * 18)
            wait_visible(lambda: "Task #1 (" in editor_title())
            clear_capture()
            send(b"\x1b[<65;6;4M" * 10)
            wait_visible(lambda: "Task 12" in visible.text().splitlines()[2])
            clear_capture()
            send(b"\x1b[1;2A")
            read_until(b"No older task")
            wait_visible(lambda: "First" in visible.text().splitlines()[0])
        elif scenario.startswith("completed_toggle"):
            initial_tasks = cli("list")
            send(b"Unsaved")
            wait_frame(lambda: editor_line().startswith("Unsaved"))
            for task_id in (3, 2, 1):
                send(b"\x1b[1;2A")
                wait_frame(lambda: f"Task #{task_id} (" in editor_title())
            send(b" retained")
            wait_frame(lambda: editor_line().startswith("Finished parent retained") and "[*]" in list_text())
            send(CTRL_SLASH)
            wait_frame(lambda: completed_button_text() == "[✓ Completed]" and (visible.x, visible.y) == (8, 0))
            click(list_width() - 12, 1)
            wait_frame(lambda: completed_button_text() == "[× Completed]" and "Finished parent" not in list_text())
            assert "Needle child" in list_text() and "Needle other" in list_text(), visible.text()
            child_row = next(row for row in list_text().splitlines() if "Needle child" in row)
            assert "└──" not in child_row, visible.text()
            assert "Task #1 (Completed)" in editor_title(), visible.text()
            assert editor_line().startswith("Finished parent retained") and (visible.x, visible.y) == (8, 0), visible.text()
            send(b"needle")
            wait_frame(lambda: filter_text() == "Filter: needle" and "Finished parent" not in list_text())
            click(list_width() - 12, 1)
            wait_frame(lambda: completed_button_text() == "[✓ Completed]" and "Finished parent" in list_text())
            send(b"\x14")
            wait_frame(lambda: completed_button_text() == "[× Completed]" and "Finished parent" not in list_text())
            assert filter_text() == "Filter: needle", visible.text()
            send(b"\t\x1b")
            wait_frame(lambda: filter_text() == "Filter:" and completed_button_text() == "[× Completed]"
                         and (visible.x, visible.y) == (len("Finished parent retained"), editor_row() + 1))
            send(b"\x1b[1;2A")
            wait_frame(lambda: "Task #3 (New)" in editor_title())
            send(b"\x1b[1;2A")
            wait_frame(lambda: "Task #2 (New)" in editor_title())
            send(b"\x1b[1;2A")
            wait_frame(lambda: "No older task" in visible.text().splitlines()[-1])
            assert "Task #2 (New)" in editor_title(), visible.text()
            send(CTRL_SLASH + b"\x14\x1b[1;2A")
            wait_frame(lambda: "Task #1 (Completed)" in editor_title() and editor_line().startswith("Finished parent retained"))
            assert "[*]" in list_text(), visible.text()
            click(list_width() - 12, 1)
            wait_frame(lambda: completed_button_text() == "[× Completed]" and "Finished parent" not in list_text())
            assert cli("list") == initial_tasks
            cli("next", "--local", "--session", "worker")
            cli("complete", "2", "--session", "worker")
            wait_frame(lambda: "Needle child" not in list_text() and "Needle other" in list_text())
            assert completed_button_text() == "[× Completed]" and editor_line().startswith("Finished parent retained"), visible.text()
            for width in (50, 150, 72):
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
                visible.resize(width, 24)
                clear_capture()
                os.kill(child.pid, signal.SIGWINCH)
                wait_frame(lambda: completed_button_text() == "[× Completed]"
                             and "Needle other" in list_text() and "Needle child" not in list_text()
                             and visible.y == 0 and not visible.pending
                             and screen.endswith(f"\x1b[1;{visible.x + 1}H".encode()))
                click(list_width() - 12, 1)
                wait_frame(lambda: completed_button_text() == "[✓ Completed]" and "Needle child" in list_text())
                click(list_width() - 12, 1)
                wait_frame(lambda: completed_button_text() == "[× Completed]" and "Needle child" not in list_text())
            assert editor_line().startswith("Finished parent retained"), visible.text()
            send(b"\x1b")
            wait_frame(lambda: completed_button_text() == "[× Completed]"
                       and (visible.x, visible.y) == (len("Finished parent retained"), editor_row() + 1)
                       and visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
            click(list_width() - 12, 1)
            wait_frame(lambda: not visible.text().splitlines()[0].startswith("Filter:") and "Finished parent" in list_text())
            assert (visible.x, visible.y) == (len("Finished parent retained"), editor_row() + 1), visible.text()
            send(b"\x1b[1;2B" * 3)
            wait_frame(lambda: "New Task" in editor_title() and editor_line().startswith("Unsaved"))
            final_tasks = cli("list")
            assert [task["description"] for task in final_tasks] == [task["description"] for task in initial_tasks]
            assert [task["status"] for task in final_tasks] == ["completed", "completed", "new"]
            if scenario.endswith("no_color"):
                assert b"\x1b[38;" not in screen and b"\x1b[48;" not in screen
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
                unfiltered = visible.text().splitlines()[:list_bottom() + 1]
                send(shortcut)
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Type to Filter")
                             and filter_text() == "Filter:"
                             and visible.cursor_visible and (visible.x, visible.y) == (8, 0))
                assert visible.text().splitlines()[1:list_bottom() + 1] == unfiltered[:-1], visible.text()
                send(b"First")
                wait_visible(lambda: filter_text() == "Filter: First"
                             and visible.text().splitlines()[-1].startswith("Type to Filter")
                             and visible.cursor_visible)
                assert "Second" not in visible.text(), visible.text()
                assert editor_line().startswith("Draft/path"), visible.text()
                send(b"\x7f" * len("First"))
                wait_visible(lambda: filter_text() == "Filter:"
                             and visible.cursor_visible and (visible.x, visible.y) == (8, 0)
                             and "Second" in visible.text())
                assert visible.text().splitlines()[1:list_bottom() + 1] == unfiltered[:-1], visible.text()
                assert editor_line().startswith("Draft/path"), visible.text()
                send(b"First")
                wait_visible(lambda: filter_text() == "Filter: First"
                             and visible.cursor_visible)
                send(shortcut + b"/")
                wait_visible(lambda: filter_text() == "Filter: First/")
                assert cli("list") == initial_tasks
                send(b"\x1b")
                wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                             and (visible.x, visible.y) == (len("Draft/path"), editor_row() + 1)
                             and "Second" in visible.text())
                wait_visible(lambda: visible.text().splitlines()[-1].startswith("Ctrl-S Save")
                             and "Ctrl-L Go to Task" in visible.text().splitlines()[-1]
                             and "Ctrl-P Create Child" in visible.text().splitlines()[-1])
            send(b"\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft/path"
        elif scenario == "slash_edit":
            send(b"/")
            wait_visible(lambda: editor_line().startswith("/")
                         and visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
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
            wait_visible(lambda: filter_text() == "Filter: needl")
            clear_capture()
            send(b"e")
            wait_visible(lambda: filter_text() == "Filter: needle")
            assert "Parent" in visible.text(), visible.text()
            assert "Child" in visible.text(), visible.text()
            assert "Other" not in visible.text(), visible.text()
            assert "Unsaved" in visible.text(), visible.text()
            assert "New Task" in visible.text(), visible.text()
            assert cli("list") == initial_tasks
            if scenario == "filter_no_color":
                assert b"\x1b[38;" not in screen, screen[-2000:]

            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: "Task #2 (" in editor_title()
                         and visible.text().splitlines()[-1].startswith("Type to Filter"))
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and editor_line().startswith("Unsaved")
                         and visible.text().splitlines()[-1].startswith("Type to Filter"))
            clear_capture()
            send(b"/zzzz")
            wait_visible(lambda: filter_text() == "Filter: needle/zzzz"
                         and "No matching tasks." in visible.text())
            assert cli("list") == initial_tasks
            clear_capture()
            send(b"\x1b")
            wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                         and "Other" in visible.text())
            clear_capture()
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
            wait_visible(lambda: 'Task #2' in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'Task #1' in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: 'Task #2' in editor_title())
            clear_capture()
            send(b"\x1b[1;2B")
            read_until(b"New Task")
            assert cli("list")[0:3] == initial_tasks

            clear_capture()
            send(CTRL_SLASH)
            wait_visible(lambda: visible.y == 0 and filter_text() == "Filter: needle")
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
            read_until(b"\x1b[1;15H")
            time.sleep(0.1)
            clear_capture()
            send(b"\rx")
            wait_visible(lambda: editor_line().startswith("x"))
            send(b"\x7f")
            wait_visible(lambda: editor_line().strip() == "")
        elif scenario in ("save", "save_json"):
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'Task #2' in editor_title())
            assert list_text().splitlines()[task_row("Second") - 1].startswith(" 2"), visible.text()
            clear_capture()
            send(b"\x05 edited\x13")
            read_until(b"Saved #2")
            assert cli("show", "2")["task"]["description"] == "Second edited"
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title())
            clear_capture()
            send(b"Third\x13")
            read_until(b"Saved #3")
            assert "3      New" in visible.text(), visible.text()
            assert cli("show", "3")["task"]["description"] == "Third"
        elif scenario == "scroll":
            assert "Task 20" in visible.text(), visible.text()
            assert "Newest preview tail" in visible.text().splitlines()[list_bottom()], visible.text()
            assert "New Task" in editor_title() and editor_line().strip() == "", visible.text()
            for task_id in range(20, 0, -1):
                clear_capture()
                send(b"\x1b[1;2A")
                read_until(f"Task Editor - Task #{task_id}".encode())
            assert list_text().splitlines()[task_row("First") - 1].startswith(" 1"), visible.text()
            clear_capture()
            send(b"\x1b[1;2B")
            wait_visible(lambda: 'Task Editor - Task #2' in editor_title())
            assert list_text().splitlines()[task_row("Second") - 1].startswith(" 2"), visible.text()
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
            wait_visible(lambda: "New Task" in editor_title())
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'Task #3' in editor_title())
            wait_visible(lambda: "Task #3" in editor_title() and
                         "[Pasted Content 1001 chars]" in visible.text() and
                         visible.text().splitlines()[-1].startswith("Ctrl-S Save"))
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
            wait_visible(lambda: "Task #2 (" in editor_title() and editor_line().startswith("Second"))
            assert "Discard" not in visible.text(), visible.text()
            send(b"\x1b[1;2B")
            wait_visible(lambda: "New Task" in editor_title() and editor_line().startswith("Draft"))
            clear_capture()
            send(b"\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft"
        elif scenario == "save_error":
            clear_capture()
            send(b"\x1b[1;2A")
            wait_visible(lambda: 'Task Editor - Task #2' in editor_title())
            with sqlite3.connect(os.path.join(folder, ".qqq", "qqq.db")) as db:
                db.execute("DELETE FROM tasks WHERE id=2")
            clear_capture()
            send(b"\x05 edited\x13")
            wait_visible(lambda: "Content conflict #2" in visible.text() and "task removed" in visible.text()
                         and visible.text().splitlines()[-1].startswith("Content conflict for task 2"))
            assert b"38;5;1" in screen, screen[-2000:]
            assert child.poll() is None
            send(b"\x1b")
            wait_visible(lambda: "Content conflict #2" not in visible.text() and "Second edited" in visible.text())
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
            wait_visible(lambda: 'Task Editor - New Task' in editor_title())
            read_until(b"Draft")
            wait_visible(lambda: (visible.x, visible.y) == (5, editor_row() + 1))
            settle()
            clear_capture()
            assert not (termios.tcgetattr(slave)[0] & termios.IXON), termios.tcgetattr(slave)
            send(b"\x13")
            read_until(b"Saved #3")
            assert cli("show", "3")["task"]["description"] == "Draft"
        if scenario != "wheel_error" and child.poll() is None:
            if visible.text().splitlines()[0].startswith("Filter:") and filter_text() != "Filter:":
                send(b"\x03")
                wait_visible(lambda: filter_text() == "Filter:"
                             or not visible.text().splitlines()[0].startswith("Filter:"))
            if scenario == "ctrl_c_filter_empty":
                send(b"\x03")
                wait_visible(lambda: not visible.text().splitlines()[0].startswith("Filter:")
                             and "New Task" in editor_title()
                             and (visible.x, visible.y) == (0, editor_row() + 1))
                assert child.poll() is None and cli("list") == initial_tasks
                send(b"\x03")
            elif scenario in ("ctrl_c_new_discard", "ctrl_c_new_image", "ctrl_c_new_whitespace"):
                send(b"y")
                assert cli("list") == initial_tasks
            else:
                if scenario.startswith("escape_") or scenario in ("empty", "empty_json", "workflow_empty"):
                    send(b"\x1b")
                deadline = time.monotonic() + 5
                while child.poll() is None:
                    assert time.monotonic() < deadline, f"Cleanup stalled:\n{visible.text()}"
                    settle()
                    if child.poll() is None:
                        footer = visible.text().splitlines()[-1]
                        send(b"y" if footer.startswith(("Discard", "Drop", "Switch?")) else b"\x03")
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
