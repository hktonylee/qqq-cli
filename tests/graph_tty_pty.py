"""Verify graph width uses stdout PTY while JSON stays complete/plain."""
import fcntl
import json
import os
import pty
import select
import time
import struct
import subprocess
import sys
import tempfile
import termios
import unicodedata

binary = sys.argv[1]
with tempfile.TemporaryDirectory(prefix="qqq-graph-tty-") as folder:
    env = dict(os.environ, HOME=folder)
    for key in ("QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(key, None)
    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=folder, env=env, capture_output=True, timeout=5)
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout)
    cli("init")
    title = "Unicode 雪 é " + "long title " * 15
    cli("add", title)
    cli("add", "Child", "--parent", "1")
    def width(line):
        return sum(0 if unicodedata.combining(ch) else 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1 for ch in line)
    for columns in (12, 32, 80):
        for extra in ({"TERM":"xterm-256color"}, {"TERM":"dumb"}, {"TERM":"xterm-256color","NO_COLOR":"1"}):
            for json_mode in (False, True):
                master, slave = pty.openpty()
                os.set_blocking(master, False)
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, columns, 0, 0))
                try:
                    result = subprocess.Popen([binary, "--json" if json_mode else "--human", "graph", "1"],
                        cwd=folder, env=dict(env, **extra), stdin=subprocess.DEVNULL, stdout=slave,
                        stderr=subprocess.PIPE)
                    chunks = bytearray()
                    deadline = time.monotonic() + 5
                    while result.poll() is None:
                        assert time.monotonic() < deadline, "Graph PTY output stalled"
                        if select.select([master], [], [], 0.05)[0]:
                            chunks.extend(os.read(master, 65536))
                    assert result.returncode == 0, result.stderr.read()
                    assert not result.stderr.read()
                    while True:
                        try:
                            chunk = os.read(master, 65536)
                        except BlockingIOError:
                            break
                        if not chunk:
                            break
                        chunks.extend(chunk)
                    text = chunks.decode().replace("\r\n", "\n")
                    assert "\x1b" not in text, text
                    if json_mode:
                        assert json.loads(text)["focus"]["task_name"] == title
                    else:
                        assert all(width(line) <= columns for line in text.splitlines()), (columns,text)
                        assert text.startswith("Dependency g"), text
                finally:
                    os.close(master)
                    os.close(slave)
