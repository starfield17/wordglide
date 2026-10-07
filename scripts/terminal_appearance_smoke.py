#!/usr/bin/env python3
"""Exercise appearance settings, persistence, ANSI colors, and restoration in a POSIX PTY."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import tempfile
import termios
import time


SGR = re.compile(rb"\x1b\[([0-9;:]*)m")


class Session:
    def __init__(self, command, env, dimensions=(40, 120)):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ,
                    struct.pack("HHHH", *dimensions, 0, 0))
        self.before = termios.tcgetattr(self.slave)
        self.output = bytearray()
        self.process = subprocess.Popen(command, env=env, stdin=self.slave,
                                        stdout=self.slave, stderr=self.slave)

    def wait(self, predicate, description):
        deadline = time.monotonic() + 5
        while not predicate():
            if self.process.poll() is not None:
                raise AssertionError(f"Process exited while waiting for {description}: {self.output[-600:]!r}")
            if time.monotonic() > deadline:
                raise AssertionError(f"Timed out waiting for {description}: {self.output[-600:]!r}")
            self.drain(0.05)

    def drain(self, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], min(0.05, max(0, deadline - time.monotonic())))[0]:
                self.output.extend(os.read(self.master, 65536))

    def send(self, data):
        os.write(self.master, data)

    def close(self):
        try:
            self.send(b"\x03")
            self.process.wait(timeout=5)
            self.drain(0.1)
            assert self.process.returncode == 0, self.output[-600:]
            assert termios.tcgetattr(self.slave) == self.before, "Terminal attributes not restored"
            assert b"\x1b[?1049l" in self.output, "Alternate screen not restored"
            assert b"\x1b[?1006l" in self.output, "Mouse capture not restored"
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait(timeout=5)
            os.close(self.master)
            os.close(self.slave)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/wordglide")
    parser.add_argument("--data", required=True)
    args = parser.parse_args()
    command = [str(Path(args.binary).resolve()), "--data", str(Path(args.data).resolve()), "fist"]
    with tempfile.TemporaryDirectory(prefix="wordglide-appearance-") as temporary:
        root = Path(temporary)
        env = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=temporary, TERM="xterm-256color")
        env.pop("NO_COLOR", None)

        def launch(extra=(), overrides=None, dimensions=(40, 120)):
            session = Session(command + list(extra), env | (overrides or {}), dimensions)
            try:
                needle = b"clenched" if dimensions[0] >= 12 and dimensions[1] >= 80 else b"Wordglide"
                session.wait(lambda: needle in session.output, "initial screen")
            except BaseException:
                session.close()
                raise
            return session

        def config_path():
            paths = list(root.rglob("config.json"))
            assert len(paths) == 1, paths
            return paths[0]

        def preferences():
            try:
                return json.loads(config_path().read_text())
            except (FileNotFoundError, AssertionError):
                return {}

        session = launch()
        try:
            assert not list(root.rglob("config.json")), "Startup must not create configuration"
            session.send(b"\x1bOQ")  # xterm F2
            session.wait(lambda: b"Appearance" in session.output, "appearance panel")
            for theme in ["orange", "gruvbox_light", "gruvbox_dark_v2", "whiteout", "default"]:
                session.send(b"\x1b[C")
                session.wait(lambda: preferences().get("color_theme") == theme, f"saved {theme}")
            session.send(b"\x1b[C")  # orange
            session.wait(lambda: preferences().get("color_theme") == "orange", "orange selection")
            session.send(b"\x1b[B\x1b[C")  # background off
            session.wait(lambda: preferences().get("theme_background") is False, "background off")
            session.send(b"\x1b[B\x1b[C")  # truecolor off
            session.wait(lambda: preferences().get("truecolor") is False, "256-color mode")
            assert any(b"38;2;" in sgr or b"48;2;" in sgr for sgr in SGR.findall(session.output)), "RGB colors never emitted"
            path = config_path()
            path.unlink()
            path.mkdir()  # deterministic write failure, even when running as root
            session.send(b"\x1b[A\x1b[C")  # background on, save fails
            session.wait(lambda: b"Not saved" in session.output, "save failure status")
            path.rmdir()
            session.send(b"\x1b[A\x1b[C")  # light theme, retry pending background edit
            session.wait(lambda: preferences().get("color_theme") == "gruvbox_light"
                         and preferences().get("theme_background") is True, "retried pending changes")
            session.send(b"\x1b\r")  # close panel, enter reading
            session.drain(0.1)
        finally:
            session.close()

        saved = config_path().read_bytes()
        session = launch()
        try:
            assert any(b"38;5;" in sgr or b"48;5;" in sgr for sgr in SGR.findall(session.output)), "Saved 256-color mode not restored"
            assert not any(b"38;2;" in sgr or b"48;2;" in sgr for sgr in SGR.findall(session.output)), "RGB emitted in 256-color mode"
            session.send(b"\x1bOQ")
            session.wait(lambda: b"gruvbox_light" in session.output, "restored theme")
        finally:
            session.close()
        assert config_path().read_bytes() == saved, "Restart rewrote configuration"

        session = launch(["--theme", "whiteout", "--truecolor=true"], dimensions=(10, 30))
        try:
            session.send(b"\x1bOQ")
            session.wait(lambda: b"whiteout" in session.output, "narrow appearance panel")
            assert any(b"38;2;" in sgr or b"48;2;" in sgr for sgr in SGR.findall(session.output)), "Truecolor override not applied"
        finally:
            session.close()
        assert config_path().read_bytes() == saved, "CLI overrides were persisted"

        for extra, overrides in [(["--no-color"], {}), ([], {"NO_COLOR": "1"})]:
            session = launch(extra, overrides)
            try:
                session.send(b"\x1bOQ")
                session.wait(lambda: b"Appearance" in session.output, "color-free settings")
                session.send(b"\x1b[C")
                session.wait(lambda: preferences().get("color_theme") != "gruvbox_light", "color-free preference update")
                for sgr in SGR.findall(session.output):
                    numbers = [int(value) for value in re.split(rb"[;:]", sgr) if value]
                    assert not any(number in {38, 48} or 30 <= number <= 37 or 40 <= number <= 47
                                   or 90 <= number <= 97 or 100 <= number <= 107 for number in numbers), sgr
            finally:
                session.close()
            # Restore the same starting preferences for both suppression mechanisms.
            config_path().write_bytes(saved)

        config_path().write_text('{"color_theme":"missing"}')
        invalid = subprocess.run(command, env=env, capture_output=True, timeout=5)
        assert invalid.returncode != 0
        assert str(config_path()).encode() in invalid.stderr
        assert b"\x1b[?1049h" not in invalid.stdout
        for operation in ["--info", "--verify-data"]:
            result = subprocess.run(command[:-1] + [operation], env=env, capture_output=True, timeout=5)
            assert result.returncode == 0, result.stderr
            assert b"\x1b[?1049h" not in result.stdout
        print("PASS: five themes, atomic auto-save/retry, restart, overrides, 256 colors, NO_COLOR, narrow panel, and PTY restoration")


if __name__ == "__main__":
    main()
