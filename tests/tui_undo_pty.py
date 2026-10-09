"""Undo/redo through real terminal frames, with isolated DB and separate stdout."""
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
with tempfile.TemporaryDirectory(prefix="qqq-undo-test-") as directory:
    env = dict(os.environ, HOME=directory, TERM="xterm-256color")
    for key in ("EDITOR", "QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID", "NO_COLOR"):
        env.pop(key, None)
    if scenario.endswith("no_color"):
        env["NO_COLOR"] = "1"

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=directory, env=env,
                                capture_output=True, timeout=8)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    cli("init")
    cli("add", "Seed")
    cli("add", "Other")
    if scenario == "open_new":
        cli("config", "tui.after_save_new", "open_new")
    image = Path(directory) / "pending.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\nimage bytes")
    master, slave = pty.openpty()
    os.set_blocking(master, False)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
    original_terminal = termios.tcgetattr(slave)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    args = ["--json", "edit", "1"] if scenario == "edit" else ["--json", "add"] if scenario == "add" else ["tui"]
    child = subprocess.Popen([binary, *args], cwd=directory, env=env,
                             stdin=slave, stderr=slave, stdout=subprocess.PIPE)
    visible = TerminalScreen(72, 24)
    screen = bytearray()

    def capture():
        if select.select([master], [], [], 0.05)[0]:
            data = os.read(master, 65536)
            screen.extend(data)
            visible.feed(data)

    def wait(predicate):
        deadline = time.monotonic() + 8
        while not predicate():
            assert time.monotonic() < deadline, f"Frame stalled: {screen[-600:]!r}\n{visible.text()}"
            assert child.poll() is None, f"Editor exited: {screen[-1500:]!r}"
            capture()

    def send(data):
        screen.clear()
        remaining = memoryview(data)
        deadline = time.monotonic() + 8
        while remaining:
            assert time.monotonic() < deadline, "Input timed out"
            readable, writable, _ = select.select([master], [master], [], 0.05)
            if readable:
                capture()
            if writable:
                remaining = remaining[os.write(master, remaining):]

    def paste(value):
        send(b"\x1b[200~" + value.encode() + b"\x1b[201~")

    def editor_row():
        return next((index for index, line in enumerate(visible.text().splitlines())
                     if line.startswith("Task Editor")), None)

    def editor_line():
        row = editor_row()
        return visible.text().splitlines()[row + 1].rstrip() if row is not None else None

    def wait_editor(text, column):
        wait(lambda: editor_line() == text and visible.cursor_visible
             and not visible.pending and not visible.decoder.getstate()[0]
             and (visible.x, visible.y) == (column, editor_row() + 1)
             and screen.endswith(f"\x1b[{editor_row() + 2};{column + 1}H".encode()))

    def frame_ready():
        return (screen and visible.cursor_visible and not visible.pending
                and not visible.decoder.getstate()[0]
                and screen.endswith(f"\x1b[{visible.y + 1};{visible.x + 1}H".encode()))

    def jump(task_id, text):
        send(b"\x0b" + str(task_id).encode() + b"\r")
        wait_editor(text, len(text))

    def saved(task_id, text, column):
        send(b"\x13")
        wait_editor(text, column)
        wait(lambda: visible.text().splitlines()[-1].startswith(f"Saved #{task_id}"))
        return cli("show", str(task_id))

    try:
        wait(lambda: "Task Editor" in visible.text())
        if scenario in ("new", "open_new", "add"):
            wait_editor("", 0)
            paste("Seed")
            wait_editor("Seed", 4)
        elif scenario == "edit":
            wait_editor("Seed", 4)
        else:
            jump(1, "Seed")
        before = cli("show", "1")

        if scenario.startswith("text") or scenario in ("new", "open_new", "add", "edit"):
            paste(" changed")
            wait_editor("Seed changed", 12)
            send(b"\x1a")
            wait_editor("Seed", 4)
            assert cli("show", "1") == before
            if scenario.startswith("text"):
                assert "[*]" not in visible.text(), visible.text()
            send(b"\x19")
            wait_editor("Seed changed", 12)
            assert cli("show", "1") == before
            if scenario == "edit":
                send(b"\x13")
            elif scenario in ("add", "open_new"):
                state = saved(3, "", 0)
                assert state["task"]["description"] == "Seed changed"
                send(b"\x1a")
                wait_editor("", 0)
                wait(lambda: visible.text().splitlines()[-1].startswith("Nothing to undo"))
                assert cli("show", "3") == state
            else:
                task_id = 3 if scenario == "new" else 1
                state = saved(task_id, "Seed changed", 12)
                assert state["task"]["description"] == "Seed changed"
                send(b"\x1a")
                wait_editor("Seed", 4)
                assert cli("show", str(task_id)) == state
                assert "[*]" in visible.text(), visible.text()
                send(b"\x19")
                wait_editor("Seed changed", 12)
                assert cli("show", str(task_id)) == state
                assert "[*]" not in visible.text(), visible.text()

        elif scenario == "edits":
            paste(" alpha beta   ")
            wait_editor("Seed alpha beta", 18)
            send(b"\x17")
            wait_editor("Seed alpha", 11)
            send(b"\x1a")
            wait_editor("Seed alpha beta", 18)
            send(b"\x19")
            wait_editor("Seed alpha", 11)
            send(b"\x1b[H\x1b[3~")
            wait_editor("eed alpha", 0)
            send(b"\x1a")
            wait_editor("Seed alpha", 0)
            send(b"\x19")
            wait_editor("eed alpha", 0)
            send(b"\x1a\x1a\x1a")
            wait_editor("Seed", 4)
            send(b"\x19")
            wait_editor("Seed alpha beta", 18)
            paste("界e\u0301")
            wait_editor("Seed alpha beta   界e\u0301", 21)
            send(b"\x19")
            wait_editor("Seed alpha beta   界e\u0301", 21)
            wait(lambda: "Nothing to redo" in visible.text())
            send(b"\x1a\x1a")
            wait_editor("Seed", 4)
            assert cli("show", "1") == before

        elif scenario == "buffers":
            paste(" changed")
            wait_editor("Seed changed", 12)
            jump(2, "Other")
            send(b"\x1a")
            wait_editor("Other", 5)
            wait(lambda: "Nothing to undo" in visible.text())
            jump(1, "Seed changed")
            send(b"\x1a")
            wait_editor("Seed", 4)
            send(b"\x19")
            wait_editor("Seed changed", 12)
            send(b"\x1a")
            wait_editor("Seed", 4)
            jump(2, "Other")
            jump(1, "Seed")
            send(b"\x19")
            wait_editor("Seed", 4)
            wait(lambda: "Nothing to redo" in visible.text())
            assert cli("show", "1") == before

        elif scenario == "scopes":
            paste(" changed")
            wait_editor("Seed changed", 12)
            # All modal/filter guards run before task editor history dispatch.
            for shortcut, title in ((b"\x1f", "Filter"), (b"\x0c", "Tags"),
                                    (b"\x0b", "Go to task"), (b"\x07", "Task actions"),
                                    (b"\x02", "Choose view"), (b"\x0f", "Dependency graph")):
                send(shortcut)
                wait(lambda: title.lower() in visible.text().lower())
                send(b"\x1a\x19\x1b")
                wait_editor("Seed changed", 12)
            send(b"\x1b")
            wait(lambda: "discard" in visible.text().lower())
            send(b"\x1a\x19n")
            wait_editor("Seed changed", 12)
            assert cli("show", "1") == before
            send(b"\x1a")
            wait_editor("Seed", 4)

        elif scenario == "conflict":
            send(b"\x01\x17")  # no-op at line start must preserve history
            wait_editor("Seed", 0)
            send(b"\x1b[3~\x1b[3~\x1b[3~\x1b[3~\x13")
            wait_editor("", 0)
            wait(lambda: "Task description cannot be empty" in visible.text())
            send(b"\x1a\x1a\x1a\x1a")
            wait_editor("Seed", 0)
            send(b"\x05")
            wait_editor("Seed", 4)
            paste(" local")
            wait_editor("Seed local", 10)
            external = cli("edit", "1", "-d", "External")
            send(b"\x13")
            wait(lambda: "Content conflict #1" in visible.text())
            send(b"\x1a\x19\x1b")
            wait_editor("Seed local", 10)
            send(b"\x1a")
            wait_editor("Seed", 4)
            send(b"\x19")
            wait_editor("Seed local", 10)
            assert cli("show", "1")["task"] == external
            send(b"\x13")
            wait(lambda: "Content conflict #1" in visible.text())
            send(b"r")
            wait(lambda: "Reload current DB text?" in visible.text())
            send(b"y")
            wait_editor("External", 8)
            send(b"\x1a")
            wait_editor("External", 8)
            wait(lambda: "Nothing to undo" in visible.text())
            assert cli("show", "1")["task"] == external

        elif scenario == "tokens":
            paste(str(image))
            wait_editor("Seed[Image #1: pending.png]", 27)
            send(b"\x1a")
            wait_editor("Seed", 4)
            assert cli("show", "1") == before
            send(b"\x19")
            wait_editor("Seed[Image #1: pending.png]", 27)
            payload = "界\r\n```\n" + "x" * 1001 + "\n"
            paste(payload)
            label = f"[Pasted Content {len(payload)} chars]"
            wait(lambda: label in visible.text() and frame_ready())
            send(b"\x1a")
            wait_editor("Seed[Image #1: pending.png]", 27)
            send(b"\x19")
            wait(lambda: label in visible.text() and frame_ready())
            send(b"\x13")
            wait(lambda: visible.text().splitlines()[-1].startswith("Saved #1") and frame_ready())
            state = cli("show", "1")
            assert len(state["images"]) == 1, state
            expected = "Seed![pending.png](.qqq/images/1/1.png)\n````pasteboard\n" + payload + "\n````"
            assert state["task"]["description"] == expected, state
            assert (Path(directory) / ".qqq/images/1/1.png").read_bytes() == image.read_bytes()
            send(b"\x1a\x1a")
            wait_editor("Seed", 4)
            assert cli("show", "1") == state
            send(b"\x19\x19")
            wait(lambda: label in visible.text() and frame_ready())
            send(b"\x13")
            wait(lambda: visible.text().splitlines()[-1].startswith("Saved #1") and frame_ready())
            after = cli("show", "1")
            assert after["task"]["description"] == expected
            assert after["images"] == state["images"]
            # Deleting saved paste/image atoms remains reversible and cannot reupload.
            send(b"\x7f\x7f")
            wait_editor("Seed", 4)
            send(b"\x1a\x1a\x13")
            wait(lambda: visible.text().splitlines()[-1].startswith("Saved #1") and frame_ready())
            assert cli("show", "1")["images"] == state["images"]

        elif scenario == "resize":
            paste(" changed")
            wait_editor("Seed changed", 12)
            screen.clear()
            visible.resize(32, 12)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 32, 0, 0))
            os.kill(child.pid, signal.SIGWINCH)
            wait_editor("Seed changed", 12)
            send(b"\x1a")
            wait_editor("Seed", 4)
            send(b"\x19")
            wait_editor("Seed changed", 12)
            screen.clear()
            visible.resize(72, 24)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 72, 0, 0))
            os.kill(child.pid, signal.SIGWINCH)
            wait_editor("Seed changed", 12)
            send(b"\x1a")
            wait_editor("Seed", 4)
            assert cli("show", "1") == before
        else:
            raise AssertionError(f"Unknown scenario: {scenario}")

        if scenario != "edit":
            if scenario not in ("add", "open_new"):
                send(b"\x03")
                wait_editor("", 0)
            send(b"\x03")
        stdout, _ = child.communicate(timeout=8)
        assert child.returncode == 0, (child.returncode, stdout)
        if scenario == "add":
            assert json.loads(stdout)[0]["description"] == "Seed changed", stdout
        elif scenario == "edit":
            assert json.loads(stdout)["description"] == "Seed changed", stdout
        else:
            assert stdout == b"", stdout
        assert termios.tcgetattr(slave) == original_terminal
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
        os.close(slave)
