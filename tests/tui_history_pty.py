"""Exercise add-editor task navigation through a real PTY."""

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
from pathlib import Path


binary, scenario = sys.argv[1:]
UP = b"\x1b[1;2A"
DOWN = b"\x1b[1;2B"

with tempfile.TemporaryDirectory(prefix="qqq-history-test-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    for key in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(key, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env,
                                capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    if scenario == "history_save":
        cli("add", "First")
        cli("add", "Second")
        cli("add", "Third", "--parent", "1")
        for task_id in (1, 2):
            assert cli("next", "--local", "--session", "worker")["id"] == task_id
            cli("complete", str(task_id), "--session", "worker")
        assert cli("next", "--local", "--session", "worker")["id"] == 3
    elif scenario == "status_header":
        for description in ("Done", "Active", "Failed", "Waiting"):
            cli("add", description)
        assert cli("next", "--local", "--session", "worker-1")["id"] == 1
        cli("complete", "1", "--session", "worker-1")
        assert cli("next", "--local", "--session", "worker-1")["id"] == 2
        assert cli("next", "--local", "--session", "worker-2")["id"] == 3
        cli("edit", "3", "--set-status", "error", "--reason", "Failed", "--session", "worker-2")
    elif scenario == "skip_deleted":
        for description in ("First", "Removed", "Third"):
            cli("add", description)
        with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as db:
            db.execute("DELETE FROM tasks WHERE id=2")
    elif scenario == "long_task_starts_at_top":
        cli("add", "Top marker\n" + "\n".join(f"Line {index}" for index in range(25)) + "\nBottom marker")
    elif scenario == "existing_and_flagged_images":
        existing = Path(folder) / "existing.png"
        existing.write_bytes(b"\x89PNG\r\n\x1a\nexisting")
        flagged = Path(folder) / "flagged.png"
        flagged.write_bytes(b"\x89PNG\r\n\x1a\nflagged")
        cli("add", "First", "--image", str(existing))
    elif scenario != "empty_boundary":
        cli("add", "First")
        if scenario == "dirty_loaded_keep":
            cli("add", "Second")

    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 80, 0, 0))
    before = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    args = [binary, "--json", "add"]
    if scenario == "existing_and_flagged_images":
        args.extend(["--image", str(flagged)])
    if scenario == "history_save":
        image = Path(folder) / "selected.png"
        image.write_bytes(b"\x89PNG\r\n\x1a\nbytes")
        args.extend(["--image", str(image)])
    child = subprocess.Popen(args, cwd=folder, env=env,
                             stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    screen = bytearray()

    def read_until(needle):
        deadline = time.monotonic() + 5
        while needle not in screen:
            assert time.monotonic() < deadline, f"Missing {needle!r}: {screen[-2000:]!r}"
            if select.select([master], [], [], 0.05)[0]:
                try:
                    screen.extend(os.read(master, 65536))
                except OSError as error:
                    raise AssertionError(f"Editor exited before {needle!r}: {screen!r}") from error
            assert child.poll() is None, f"Editor exited before {needle!r}: {screen!r}"

    def send(data):
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

    def press(data, needle):
        screen.clear()
        send(data)
        read_until(needle)

    def finish(expected_id=None, next_task=None, expect_error=False):
        screen.clear()
        send(b"\x13")
        if expect_error:
            read_until(b"Task 1 not found")
            assert child.poll() is None
            send(b"\x03")
        else:
            read_until(f"Saved #{expected_id}. New task".encode())
            assert child.poll() is None
            assert b"qqq task editor - new task" in screen
            if next_task is not None:
                screen.clear()
                send(next_task + b"\x13")
                read_until(f"Saved #{expected_id + 1}. New task".encode())
            send(b"\x1b")
        deadline = time.monotonic() + 5
        pending_stdout = bytearray()
        while child.poll() is None:
            assert time.monotonic() < deadline, f"Save timed out: {screen[-2000:]!r}"
            readable, _, _ = select.select([master, child.stdout], [], [], 0.05)
            for stream in readable:
                if stream == master:
                    try:
                        screen.extend(os.read(master, 65536))
                    except OSError:
                        pass
                else:
                    pending_stdout.extend(os.read(child.stdout.fileno(), 65536))
        stdout, _ = child.communicate(timeout=5)
        output = bytes(pending_stdout) + stdout
        assert before[3] == termios.tcgetattr(slave)[3], "Terminal flags not restored"
        assert b"\x1b[?1049l" in screen, "Alternate screen not restored"
        if expect_error:
            assert child.returncode == 1 and output == b"", screen[-2000:]
            return None
        assert child.returncode == 0, screen[-2000:]
        saved = json.loads(output)
        assert isinstance(saved, list)
        return saved

    try:
        read_until(b"Ctrl-S")
        if scenario == "history_save":
            press(UP, b"task #3")
            press(UP, b"task #2")
            press(DOWN, b"task #3")
            send(b"\x05 updated")
            batch = finish(3, b"Fresh")
            saved = batch[0]
            assert saved["id"] == 3 and saved["description"] == "Third updated"
            assert saved["parent_id"] == 1
            assert saved["status"] == "in_progress"
            assert saved["harness_session"] == "worker"
            assert batch[1]["id"] == 4 and batch[1]["description"] == "Fresh"
            assert [task["description"] for task in cli("list")] == ["First", "Second", "Third updated", "Fresh"]
            assert [image["name"] for image in cli("show", "3")["images"]] == ["selected.png"]
            assert cli("show", "4")["images"] == []
        elif scenario == "status_header":
            for task_id, status in ((4, "New"), (3, "Error"), (2, "In progress"), (1, "Completed")):
                press(UP, f"task #{task_id} ({status})".encode())
            for task_id, status in ((2, "In progress"), (3, "Error"), (4, "New")):
                press(DOWN, f"task #{task_id} ({status})".encode())
            press(DOWN, b"new task")
            send(b"Fresh")
            assert finish(5)[0]["description"] == "Fresh"
        elif scenario == "skip_deleted":
            press(UP, b"task #3")
            press(UP, b"task #1")
            press(DOWN, b"task #3")
            send(b"\x05!")
            saved = finish(3)[0]
            assert saved["id"] == 3 and saved["description"] == "Third!"
            assert [task["id"] for task in cli("list")] == [1, 3]
        elif scenario == "long_task_starts_at_top":
            press(UP, b"task #1")
            read_until(b"Shift-Up/Down")
            assert b"Top marker" in screen, screen[-2000:]
            assert b"Bottom marker" not in screen, screen[-2000:]
            saved = finish(1)[0]
            assert saved["id"] == 1
        elif scenario == "existing_and_flagged_images":
            press(UP, b"task #1")
            send(b"\x05 edited")
            saved = finish(1)[0]
            assert saved["id"] == 1 and saved["description"] == "First edited"
            assert [image["name"] for image in cli("show", "1")["images"]] == ["existing.png", "flagged.png"]
        elif scenario == "selected_deleted":
            press(UP, b"task #1")
            with sqlite3.connect(Path(folder) / ".qqq/qqq.db") as db:
                db.execute("DELETE FROM tasks WHERE id=1")
            send(b"\x05!")
            finish(expect_error=True)
            assert b"Task 1 not found" in screen
            assert cli("list") == []
        elif scenario == "return_new":
            press(UP, b"task #1")
            press(DOWN, b"new task")
            send(b"Fresh")
            saved = finish(2)[0]
            assert saved["id"] == 2 and saved["description"] == "Fresh"
            assert cli("show", "1")["task"]["description"] == "First"
        elif scenario == "dirty_new_keep":
            send(b"Draft")
            press(UP, b"Discard changes and switch? (y/N)")
            send(b"zzz")
            press(b"n", b"new task")
            saved = finish(2)[0]
            assert saved["id"] == 2 and saved["description"] == "Draft"
        elif scenario == "dirty_new_discard":
            send(b"Draft")
            press(UP, b"Discard changes and switch? (y/N)")
            press(b"y", b"task #1")
            send(b"\x05 edited")
            saved = finish(1)[0]
            assert saved["id"] == 1 and saved["description"] == "First edited"
            assert len(cli("list")) == 1
        elif scenario == "dirty_image_discard":
            image = Path(folder) / "new.png"
            image.write_bytes(b"\x89PNG\r\n\x1a\nbytes")
            send(b"\x1b[200~" + str(image).encode() + b"\x1b[201~")
            read_until(b"[Image #1:")
            press(UP, b"Discard changes and switch? (y/N)")
            press(b"y", b"task #1")
            send(b"\x05 edited")
            saved = finish(1)[0]
            assert saved["id"] == 1 and saved["description"] == "First edited"
            assert cli("show", "1")["images"] == []
        elif scenario == "dirty_loaded_keep":
            press(UP, b"task #2")
            send(b"\x05!")
            press(UP, b"Discard changes and switch? (y/N)")
            press(b"\x1b", b"task #2")
            saved = finish(2)[0]
            assert saved["id"] == 2 and saved["description"] == "Second!"
            assert cli("show", "1")["task"]["description"] == "First"
        elif scenario == "oldest_boundary":
            press(UP, b"task #1")
            send(b"\x05!")
            press(UP, b"No older task")
            assert b"Discard changes and switch?" not in screen
            saved = finish(1)[0]
            assert saved["id"] == 1 and saved["description"] == "First!"
        elif scenario == "empty_boundary":
            press(UP, b"No older task")
            send(b"Fresh")
            saved = finish(1)[0]
            assert saved["id"] == 1 and saved["description"] == "Fresh"
        else:
            raise AssertionError(f"Unknown scenario: {scenario}")
    finally:
        if child.poll() is None:
            child.kill()
        os.close(master)
        os.close(slave)
        child.wait(timeout=2)
