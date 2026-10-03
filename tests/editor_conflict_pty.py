import json
import os
from pathlib import Path
import pty
import select
import shlex
import subprocess
import sys
import tempfile
import time

binary, scenario = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    env = os.environ.copy()
    env["HOME"] = directory
    for key in ("QQQ_SESSION", "HERDR_ENV", "HERDR_PANE_ID", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
        env.pop(key, None)

    def cli(*args):
        result = subprocess.run([binary, "--json", *args], cwd=root, env=env, capture_output=True, check=True)
        return json.loads(result.stdout)

    cli("init")
    cli("add", "Original")
    image = root / "pending.png"
    image.write_bytes(b"\x89PNG\r\n\x1a\npending")
    editor = root / "edit.py"
    editor.write_text('''import os, pathlib, subprocess, sys
p = pathlib.Path(sys.argv[1])
count = pathlib.Path('calls')
n = int(count.read_text()) + 1 if count.exists() else 1
count.write_text(str(n))
pathlib.Path('draft-path').write_text(str(p))
if n == 1:
 subprocess.run([os.environ['TEST_BINARY'], '--json', 'edit', '1', '-d', 'Newer DB text'], check=True, stdout=subprocess.DEVNULL)
 p.write_text('Local complete text\\nDetails\\n')
else:
 assert p.read_text() == 'Newer DB text'
 p.write_text('Reloaded complete text\\nDetails\\n')
''')
    env["TEST_BINARY"] = binary
    env["EDITOR"] = shlex.quote(sys.executable) + " " + shlex.quote(str(editor))
    master, slave = pty.openpty()
    process = subprocess.Popen([binary, "--json", "edit", "1", "--edit", "--image", str(image)],
                               cwd=root, env=env, stdin=slave, stderr=slave, stdout=subprocess.PIPE,
                               start_new_session=True)
    os.close(slave)
    output = bytearray()

    def wait_for(text, start=0):
        deadline = time.monotonic() + 8
        while text.encode() not in output[start:]:
            if time.monotonic() > deadline:
                raise AssertionError(f"Missing {text!r}: {output.decode(errors='replace')}")
            if select.select([master], [], [], 0.1)[0]:
                try:
                    output.extend(os.read(master, 65536))
                except OSError:
                    raise AssertionError(output.decode(errors="replace"))

    def send(text):
        os.write(master, text.encode())

    try:
        wait_for("[k] Keep draft and exit:")
        assert cli("show", "1")["task"]["description"] == "Newer DB text"
        if scenario == "reload":
            send("r\n")
            wait_for("Discard local text and pending images")
            start = len(output)
            send("n\n")
            wait_for("[k] Keep draft and exit:", start)
            assert cli("show", "1")["task"]["description"] == "Newer DB text"
            start = len(output)
            send("r\n")
            wait_for("Discard local text and pending images", start)
            send("y\n")
            expected = "Reloaded complete text\nDetails\n"
        elif scenario == "overwrite":
            send("o\n")
            wait_for("Replace DB text at revision")
            start = len(output)
            send("n\n")
            wait_for("[k] Keep draft and exit:", start)
            start = len(output)
            send("o\n")
            wait_for("Replace DB text at revision", start)
            latest = cli("edit", "1", "-d", "Another newer DB text")
            start = len(output)
            send("y\n")
            wait_for("Content conflict", start)
            wait_for("[k] Keep draft and exit:", start)
            assert cli("show", "1")["task"] == latest
            start = len(output)
            send("o\n")
            wait_for("Replace DB text at revision", start)
            send("yes\n")
            expected = "Local complete text\nDetails\n"
        else:
            raise AssertionError(scenario)
        stdout, _ = process.communicate(timeout=8)
        assert process.returncode == 0, output.decode(errors="replace")
        saved = json.loads(stdout)
        assert saved["description"] == expected, saved
        detail = cli("show", "1")
        assert detail["task"] == saved
        assert len(detail["images"]) == (1 if scenario == "overwrite" else 0)
        if scenario == "overwrite":
            assert (root / ".qqq/images/1/1.png").read_bytes() == image.read_bytes()
        draft = Path((root / "draft-path").read_text())
        assert not draft.exists(), draft
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
