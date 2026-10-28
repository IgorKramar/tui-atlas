"""Настоящий PTY: каталог EN/RU, запуск fixture и восстановление терминала."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time


def smoke(binary, columns, rows, language):
    with tempfile.TemporaryDirectory(prefix="atlas-pty-") as directory:
        root = Path(directory)
        home = root / "home"
        fixture = home / ".local/bin/mdv"
        fixture.parent.mkdir(parents=True)
        fixture.write_text("#!/bin/sh\nif [ \"$1\" = --help ]; then printf 'fixture help\\n'; exit 0; fi\nprintf '%s\\n' \"$@\" > \"$HOME/launched\"\nprintf 'ATLAS_LAUNCH_OK\\n'\n")
        fixture.chmod(0o755)
        environment = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": str(fixture.parent), "TERM": "xterm-256color", "LANG": "C.UTF-8"}
        master, slave = pty.openpty()
        original = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))

        def session():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        output = bytearray()
        process = subprocess.Popen([str(binary), "--lang", language], cwd=root,
                                   env=environment, stdin=slave, stdout=slave, stderr=slave,
                                   preexec_fn=session)

        def drain(seconds):
            until = time.monotonic() + seconds
            while time.monotonic() < until:
                if select.select([master], [], [], min(0.03, max(0, until - time.monotonic())))[0]:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    output.extend(data)
                    if b"\x1b[6n" in data:
                        os.write(master, b"\x1b[1;1R")

        def wait_for(text, start=0):
            until = time.monotonic() + 5
            while text.encode() not in output[start:]:
                drain(0.05)
                assert process.poll() is None, output.decode(errors="replace")[-1000:]
                assert time.monotonic() < until, (text, output.decode(errors="replace")[-1000:])

        def send(keys):
            os.write(master, keys)
            drain(0.15)

        try:
            wait_for("TUI ATLAS")
            if columns < 36:
                wait_for("36")
                wait_for("12")
            else:
                wait_for("FIND AN APP" if language == "en" else "ПОИСК ПРИЛОЖЕНИЯ")
                send(b"/mdv\r")
                for key in [b"\t", b"2", b"3", b"4", b"1", b"\x06", b"mdv", b"\x1b", b"\x1b"]:
                    send(key)
                start = len(output)
                send(b"L")
                wait_for("ПОИСК ПРИЛОЖЕНИЯ" if language == "en" else "FIND AN APP", start)
                settings = home / "config/tui-atlas/config.json"
                assert json.loads(settings.read_text())["language"] == ("ru" if language == "en" else "en")
                send(b"\r")
                send(b"\r")
                wait_for("ATLAS_LAUNCH_OK")
                assert (home / "launched").read_text().splitlines() == ["--tui", "."]
                wait_for("вы снова в Atlas" if language == "en" else "back in Atlas")
                assert process.poll() is None, "Atlas не вернулся после запуска"
            send(b"q")
            assert process.wait(timeout=5) == 0
            drain(0.1)
            assert termios.tcgetattr(slave) == original, "Терминал не восстановлен"
            for sequence in [b"\x1b[?1049l", b"\x1b[?2004l", b"\x1b[?1000l"]:
                assert sequence in output, ("Не восстановлен режим терминала", sequence)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            os.close(master)
            os.close(slave)
    print(f"PTY Atlas {language} {columns}x{rows}: OK")


if __name__ == "__main__":
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/tui-atlas").resolve()
    for language in ["en", "ru"]:
        for size in [(160, 48), (72, 22), (30, 10)]:
            smoke(binary, *size, language)
