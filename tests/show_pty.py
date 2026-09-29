"""Capture real terminal stdout; verify color gates without controlling-terminal setup."""
import errno
import fcntl
import json
import os
import pty
import re
import select
import struct
import subprocess
import sys
import termios
import time

binary, folder, mode = sys.argv[1:]
env = dict(os.environ, HOME=folder, TERM="xterm-256color")
env.pop("NO_COLOR", None)
if mode == "no_color":
    env["NO_COLOR"] = "1"
elif mode == "dumb":
    env["TERM"] = "dumb"
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 32, 0, 0))
args = [binary, "show", "1"]
if mode == "json":
    args.append("--json")
child = subprocess.Popen(args, cwd=folder, env=env, stdin=subprocess.DEVNULL,
    stdout=slave, stderr=subprocess.PIPE)
os.close(slave)
screen = bytearray()
deadline = time.monotonic() + 5
try:
    while True:
        assert time.monotonic() < deadline, "Show terminal output timed out"
        if select.select([master], [], [], 0.05)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            screen.extend(chunk)
    _, error = child.communicate(timeout=5)
    assert child.returncode == 0, error.decode()
    assert not error, error.decode()
finally:
    os.close(master)
    if child.poll() is None:
        child.kill()
    child.wait(timeout=5)
text = screen.decode().replace("\r\n", "\n")
print(json.dumps({"screen": text, "plain": re.sub(r"\x1b\[[0-9;]*m", "", text)}))
