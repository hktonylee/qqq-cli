"""Check list column detection and multiline output through a real PTY."""

import fcntl
import json
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time


binary = sys.argv[1]
with tempfile.TemporaryDirectory(prefix="qqq-list-tty-") as folder:
    env = dict(os.environ, HOME=folder, TERM="xterm-256color")
    for key in ("QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(key, None)

    def cli(*args):
        result = subprocess.run([binary, *args], cwd=folder, env=env, capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr.decode()
        return result.stdout.decode()

    def terminal_list(columns, term="xterm-256color", controlling_columns=None):
        master, slave = pty.openpty()
        os.set_blocking(master, False)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, columns, 0, 0))
        control_master = control_slave = None
        if controlling_columns is not None:
            control_master, control_slave = pty.openpty()
            fcntl.ioctl(control_slave, termios.TIOCSWINSZ,
                        struct.pack("HHHH", 12, controlling_columns, 0, 0))

        def establish_control():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        try:
            result = subprocess.run([binary, "list"], cwd=folder, env=dict(env, TERM=term),
                                    stdin=control_slave if control_slave is not None else subprocess.DEVNULL,
                                    stdout=slave, stderr=subprocess.PIPE,
                                    preexec_fn=establish_control if control_slave is not None else None,
                                    timeout=5)
            assert result.returncode == 0, result.stderr.decode()
            chunks = bytearray()
            while True:
                try:
                    chunk = os.read(master, 4096)
                except BlockingIOError:
                    break
                if not chunk:
                    break
                chunks.extend(chunk)
            return chunks.decode().replace("\r\n", "\n")
        finally:
            if slave is not None:
                os.close(slave)
            os.close(master)
            if control_slave is not None:
                os.close(control_slave)
                os.close(control_master)

    def terminal_watch_snapshot(columns):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, columns, 0, 0))
        child = subprocess.Popen([binary, "list", "--watch"], cwd=folder, env=env,
                                 stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.PIPE)
        try:
            chunks = bytearray()
            deadline = time.monotonic() + 5
            while b"last detail" not in chunks:
                assert time.monotonic() < deadline, f"Watch output timed out: {chunks!r}"
                if select.select([master], [], [], 0.05)[0]:
                    chunks.extend(os.read(master, 4096))
                assert child.poll() is None, child.stderr.read().decode()
            return chunks.decode().replace("\r\n", "\n")
        finally:
            child.kill()
            child.wait(timeout=2)
            os.close(master)
            os.close(slave)

    cli("init")
    cli("add", "Root line\nmore root")
    cli("add", "First child line\nmore child", "--parent", "1")
    cli("add", "Last child\nlast detail", "--parent", "1")

    piped = cli("list")
    assert len(piped.splitlines()) == 4 and "more root" not in piped
    tasks = json.loads(cli("--json", "list"))
    assert tasks[0]["description"] == "Root line\nmore root"

    for term in ("xterm-256color", "dumb"):
        rows = terminal_list(40, term).splitlines()
        pad = " " * 20
        assert len(rows) == 7, rows
        assert rows[2] == pad + "more root", rows
        assert rows[4] == pad + "│   more child", rows
        assert rows[6] == pad + "    last detail", rows
        assert all(len(row) <= 40 for row in rows), rows

    watch = terminal_watch_snapshot(40)
    assert "\x1b[H\x1b[2J" in watch
    assert " " * 20 + "│   more child" in watch

    cli("add", "ABCDEFGHIJKLMN\n界界界界界界界")
    rows = terminal_list(32).splitlines()
    assert " " * 20 + "MN" in rows, rows
    assert " " * 20 + "界界界界界界" in rows, rows
    assert " " * 20 + "界" in rows, rows

    cli("add", "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
    rows = terminal_list(40, term="dumb", controlling_columns=80).splitlines()
    assert "5      New          ABCDEFGHIJKLMNOPQRST" in rows, rows
    assert " " * 20 + "UVWXYZ0123456789" in rows, rows
