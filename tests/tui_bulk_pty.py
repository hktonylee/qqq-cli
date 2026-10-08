"""Bulk selection/preview through real terminal, with saved-state assertions."""
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
from terminal_screen import TerminalScreen


binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix="qqq-bulk-pty-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    for variable in ("QQQ_SESSION", "CODEX_THREAD_ID", "CODEX_SESSION_ID", "HERDR_ENV", "HERDR_PANE_ID", "EDITOR", "NO_COLOR"):
        env.pop(variable, None)
    if scenario.endswith("no_color"):
        env["NO_COLOR"] = "1"

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env,
                                capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    initial_second = "Second"
    if scenario == "editor_scroll":
        initial_second = "\n".join([f"Line{line:02}" for line in range(1, 21)] + ["Second"])
    for description in ("First", initial_second, "Third"):
        cli("add", description)
    if scenario == "archive_active":
        cli("next", "--local", "--session", "foreign", "--filter", "id == 3")
    master, slave = pty.openpty()
    os.set_blocking(master, False)
    width, height = (42, 16) if scenario == "basic_compact" else (90, 24)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
    before_terminal = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    tui_args = [binary, "tui"]
    if scenario.startswith("archive"):
        tui_args.append("--include-archived")
    child = subprocess.Popen(tui_args, cwd=folder, env=env,
                             stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    screen = TerminalScreen(width, height)
    captured = bytearray()

    def capture():
        if select.select([master], [], [], 0.03)[0]:
            data = os.read(master, 65536)
            captured.extend(data)
            screen.feed(data)

    def wait(predicate):
        deadline = time.monotonic() + 5
        def complete():
            final = (f"\x1b[?25h\x1b[{screen.y+1};{screen.x+1}H".encode()
                     if screen.cursor_visible else b"\x1b[?25l")
            return not screen.pending and not screen.decoder.getstate()[0] and captured.endswith(final)
        while not (predicate() and complete()):
            assert time.monotonic() < deadline, f"Visible state stalled:\n{screen.text()}\nBytes:{captured[-1000:]!r}"
            capture()
            assert child.poll() is None, f"Editor exited:\n{screen.text()}"

    def contains(text):
        wait(lambda: text in screen.text())

    def send(data):
        remaining = memoryview(data)
        deadline = time.monotonic() + 5
        while remaining:
            assert time.monotonic() < deadline
            readable, writable, _ = select.select([master], [master], [], 0.03)
            if readable:
                chunk = os.read(master, 65536)
                captured.extend(chunk)
                screen.feed(chunk)
            if writable:
                remaining = remaining[os.write(master, remaining):]

    def priorities():
        return [cli("show", str(task))["task"]["priority"] for task in (1, 2, 3)]

    def task(task_id):
        return cli("show", str(task_id))["task"]

    def open_action(key, heading):
        send(b"\x07")
        contains("Bulk actions (")
        send(key)
        contains(heading)

    def apply(count=2):
        send(b"y")
        contains(f"Bulk applied: {count} tasks")

    def remark():
        send(b"\x04")
        contains("Bulk selected: 1")
        send(b"\x1b[1;2A")
        contains("Task #1")
        send(b"\x1b[1;2B")
        contains("Task #2")
        send(b"\x1b[1;2B")
        contains("Task #3")
        send(b"\x04")
        contains("Bulk selected: 2")
        send(b"\x1b[1;2A")
        contains("Task #2")

    def jump(task_id):
        send(b"\x0b")
        contains("Go to task")
        send(str(task_id).encode() + b"\r")
        wait(lambda: "Go to task" not in screen.text() and f"Task #{task_id}" in screen.text())

    try:
        contains("New Task")
        send(b"\x1b[1;2A")
        contains("Task #3")
        contains("Ctrl-D Select")
        send(b"\x04")
        contains("Bulk selected: 1")
        assert any(row.startswith("+") and "Third" in row for row in screen.text().splitlines())
        send(b"\x1b[1;2A")
        contains("Task #2")
        send(b"\x01Dirty ")
        wait(lambda: "Dirty Second" in screen.text() and screen.x == 6
             and screen.y == (height-2 if scenario == "editor_scroll" else
                              next(index+1 for index,row in enumerate(screen.text().splitlines())
                                   if row.startswith("Task Editor"))))
        caret = (screen.x, screen.y)
        draft_label = "Dirty Second"
        if scenario == "basic_atoms":
            payload = "p" * 1001
            image_path = Path(folder) / "retained.png"
            image_path.write_bytes(base64.b64decode(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aGD8AAAAASUVORK5CYII="
            ))
            send(b"\x1b[200~" + payload.encode() + b"\x1b[201~")
            send(b"\x1b[200~" + str(image_path).encode() + b"\x1b[201~")
            draft_label = "[Image #1: retained.png]"
            contains(draft_label)
            assert "[Pasted Content 1001 chars]" in screen.text()
            caret = (screen.x, screen.y)
        send(b"\x04")
        contains("Bulk selected: 2")
        final_priority = 0
        if scenario.startswith("basic"):
            open_action(b"p", "Bulk priority")
            send(b"5\r")
            contains("Bulk preview (2)")
            assert priorities() == [0, 0, 0]
            assert task(2)["description"] == "Second"
            send(b"n")
            wait(lambda: "Bulk preview (2)" not in screen.text() and draft_label in screen.text()
                 and (screen.x, screen.y) == caret)
            open_action(b"p", "Bulk priority")
            send(b"5\r")
            contains("Bulk preview (2)")
            apply()
            assert priorities() == [0, 5, 5]
            final_priority = 5
        elif scenario == "tags":
            open_action(b"t", "Bulk add tags")
            send(b"\x1b[200~" + "review\nobsolete\n界面".encode() + b"\x1b[201~")
            contains("obsolete")
            send(b"\x1b[A\x15")
            wait(lambda: "obsolete" not in screen.text() and "界面" in screen.text())
            send(b"\x13")
            contains("Bulk preview (2)")
            assert task(2)["tags"] == []
            apply()
            assert task(2)["tags"] == task(3)["tags"] == ["review", "界面"]
            remark()
            open_action(b"r", "Bulk remove tags")
            send(b"review\r")
            contains("Enter newline")
            assert "Bulk preview (2)" not in screen.text()
            send(b"\x13")
            contains("Bulk preview (2)")
            apply()
            assert task(2)["tags"] == task(3)["tags"] == ["界面"]
            remark()
            open_action(b"s", "Bulk replace tags")
            send(b"\x13")
            contains("Bulk preview (2)")
            apply()
            assert task(2)["tags"] == task(3)["tags"] == []
        elif scenario == "conflict":
            open_action(b"p", "Bulk priority")
            send(b"5\r")
            contains("Bulk preview (2)")
            cli("edit", "3", "--priority", "9")
            send(b"y")
            contains("Bulk error")
            assert priorities() == [0, 0, 9]
            send(b"\x1b")
            wait(lambda: "Bulk error" not in screen.text() and (screen.x, screen.y) == caret)
            assert any(row.startswith("+") and "Second" in row for row in screen.text().splitlines())
            open_action(b"p", "Bulk priority")
            send(b"7\r")
            contains("Bulk preview (2)")
            apply()
            assert priorities() == [0, 7, 7]
            final_priority = 7
        elif scenario == "archive":
            open_action(b"a", "Bulk preview (2)")
            assert not task(2)["archived"] and not task(3)["archived"]
            apply()
            assert task(2)["archived"] and task(3)["archived"]
            remark()
            open_action(b"u", "Bulk preview (2)")
            apply()
            assert not task(2)["archived"] and not task(3)["archived"]
        elif scenario == "archive_active":
            open_action(b"a", "Bulk error")
            assert not task(2)["archived"] and not task(3)["archived"]
            assert task(3)["status"] == "in_progress"
            send(b"\x1b")
            wait(lambda: "Bulk error" not in screen.text() and (screen.x, screen.y) == caret)
            send(b"\x07")
            contains("Bulk actions (2)")
            send(b"x")
            contains("Bulk selection cleared")
        elif scenario == "filter":
            send(b"\x1fthird")
            wait(lambda: screen.text().splitlines()[0].startswith("Filter: third")
                 and screen.y == 0)
            filter_caret = (screen.x, screen.y)
            assert not any("Second" in row for row in screen.text().splitlines()[1:8])
            open_action(b"p", "Bulk priority")
            send(b"4\r")
            contains("Bulk preview (2)")
            contains("IDs: #2, #3")
            send(b"n")
            wait(lambda: "Bulk preview (2)" not in screen.text()
                 and (screen.x, screen.y) == filter_caret)
            open_action(b"p", "Bulk priority")
            send(b"4\r")
            contains("Bulk preview (2)")
            apply()
            wait(lambda: (screen.x, screen.y) == filter_caret)
            assert priorities() == [0, 4, 4]
            assert screen.text().splitlines()[0].startswith("Filter: third")
            send(b"\x1b")
            wait(lambda: not screen.text().splitlines()[0].startswith("Filter:")
                 and (screen.x, screen.y) == caret)
            final_priority = 4
        elif scenario == "editor_scroll":
            body_start = next(index+1 for index,row in enumerate(screen.text().splitlines())
                              if row.startswith("Task Editor"))
            wheel = f";2;{body_start+1}M".encode()
            send((b"\x1b[<64" + wheel) * 20)
            wait(lambda: screen.text().splitlines()[body_start].startswith("Line01"))
            send((b"\x1b[<65" + wheel) * 2)
            wait(lambda: screen.text().splitlines()[body_start].startswith("Line07")
                 and not screen.cursor_visible)
            body_before = screen.text().splitlines()[body_start:-1]
            open_action(b"p", "Bulk priority")
            send(b"5\r")
            contains("Bulk preview (2)")
            send(b"n")
            wait(lambda: "Bulk preview (2)" not in screen.text() and not screen.cursor_visible)
            assert screen.text().splitlines()[body_start:-1] == body_before
            assert task(2)["description"] == initial_second
            open_action(b"p", "Bulk priority")
            send(b"5\r")
            contains("Bulk preview (2)")
            apply()
            wait(lambda: not screen.cursor_visible)
            assert screen.text().splitlines()[body_start:-1] == body_before
            assert priorities() == [0, 5, 5]
            send(b"\x05")
            wait(lambda: "Dirty Second" in screen.text() and screen.cursor_visible)
            caret = (screen.x, screen.y)
            final_priority = 5
        elif scenario == "scroll":
            for task_id in range(4, 14):
                cli("add", f"Extra {task_id}")
                jump(task_id)
                send(b"\x04")
                contains(f"Bulk selected: {task_id-1}")
            send(b"\x1b[<64;2;3M")
            wait(lambda: not screen.text().splitlines()[0].startswith("+6"))
            list_before = screen.text().splitlines()[:8]
            send(b"\x07")
            contains("Bulk actions (12)")
            contains("#12, #13")
            send(b"p")
            contains("Bulk priority")
            send(b"8\r")
            contains("Bulk preview (12)")
            send(b"\x1b[F")
            contains("│#13")
            contains("Archived: false -> false")
            assert "y apply" in screen.text(), screen.text()
            send(b"n")
            contains("Bulk preview cancelled")
            assert screen.text().splitlines()[:8] == list_before
            assert priorities() == [0, 0, 0]
            open_action(b"p", "Bulk priority")
            send(b"8\r")
            contains("Bulk preview (12)")
            send(b"\x1b[F")
            contains("│#13")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 42, 0, 0))
            screen.resize(42, 12)
            os.kill(child.pid, signal.SIGWINCH)
            contains("Bulk preview (12)")
            contains("y apply")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 2, 42, 0, 0))
            screen.resize(42, 2)
            os.kill(child.pid, signal.SIGWINCH)
            contains("Resize terminal (min 12x8)")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 42, 0, 0))
            screen.resize(42, 12)
            os.kill(child.pid, signal.SIGWINCH)
            contains("Bulk preview (12)")
            send(b"y")
            contains("Bulk applied: 12 tasks")
            assert all(task(task_id)["priority"] == 8 for task_id in range(2, 14))
            jump(2)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
            screen.resize(width, height)
            os.kill(child.pid, signal.SIGWINCH)
            final_priority = 8
        else:
            raise AssertionError(f"Unknown bulk scenario {scenario}")
        wait(lambda: draft_label in screen.text() and (screen.x, screen.y) == caret)
        assert task(2)["description"] == initial_second
        if scenario == "basic_atoms":
            assert cli("show", "2")["images"] == []
            assert "[Pasted Content 1001 chars]" in screen.text()
        assert not any(row.startswith("+") for row in screen.text().splitlines())
        send(b"\x13")
        contains("Saved #2")
        if scenario == "basic_atoms":
            saved = cli("show", "2")
            assert payload in saved["task"]["description"]
            assert saved["task"]["description"].startswith("Dirty ")
            assert [item["name"] for item in saved["images"]] == ["retained.png"]
            exported = Path(folder) / "exported.png"
            cli("show", "2", "--export-image", "1", "--output", str(exported))
            assert exported.read_bytes() == image_path.read_bytes()
        else:
            assert task(2)["description"] == initial_second.removesuffix("Second") + "Dirty Second"
        assert task(2)["priority"] == final_priority
        send(b"\x1b")
        contains("New Task")
        send(b"\x03")
        assert child.wait(timeout=5) == 0
        assert child.stdout.read() == b""
        assert termios.tcgetattr(slave) == before_terminal
        if scenario.endswith("no_color"):
            assert b"\x1b[38;" not in captured and b"\x1b[48;" not in captured
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
        os.close(slave)
