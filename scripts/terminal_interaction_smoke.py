#!/usr/bin/env python3
"""Check reading navigation, menus, preferences, and mouse toggles in a POSIX PTY."""
import argparse
import json
import os
from pathlib import Path
import tempfile

from terminal_appearance_smoke import Session


def paste(session, text):
    session.send(b"\x1b[200~" + text.encode() + b"\x1b[201~")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/wordglide")
    parser.add_argument("--data", required=True)
    args = parser.parse_args()
    command = [str(Path(args.binary).resolve()), "--data", str(Path(args.data).resolve())]
    with tempfile.TemporaryDirectory(prefix="wordglide-interaction-") as temporary:
        root = Path(temporary)
        env = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=temporary, TERM="xterm-256color")
        env.pop("NO_COLOR", None)

        def launch(query="take", dimensions=(40, 120)):
            session = Session(command + [query], env, dimensions)
            try:
                session.wait(lambda: b"Definition" in session.output, "initial preview")
            except BaseException:
                session.close()
                raise
            return session

        def preferences():
            paths = list(root.rglob("config.json"))
            return json.loads(paths[0].read_text()) if paths else {}

        session = launch()
        try:
            session.send(b"\r\x1bOS")  # Enter, xterm F4
            session.wait(lambda: preferences().get("reading_layout") == "focus", "focused layout saved")
            session.send(b"ep")
            session.wait(lambda: preferences().get("expand_examples") is True
                         and preferences().get("expand_ipa") is True, "reading preferences saved")
            session.output.clear()
            session.send(b"o")
            session.wait(lambda: b"Outline" in session.output, "source outline")
            session.send(b"\x1b[B\r")
            session.drain(0.1)
            session.output.clear()
            session.send(b"/")
            paste(session, "recording")
            session.wait(lambda: b"Find" in session.output and b"recording" in session.output, "definition find")
            session.send(b"\r")
            session.send(b"nN")
            session.drain(0.1)
            session.output.clear()
            session.send(b"/")
            paste(session, "zzzz-not-present")
            session.wait(lambda: b"No matches" in session.output, "no-match feedback")
            session.send(b"\x1b")
            session.drain(0.1)
            session.send(b"\x1b\x15")  # input focus, clear input
            paste(session, "fist")
            session.wait(lambda: b"clenched" in session.output, "new lookup")
            session.send(b"\r")
            session.drain(0.1)
            session.send(b"f")
            session.drain(0.1)
            session.send(b"ab")  # source-grounded sample: fist=aa, hand=ab
            session.drain(0.1)
            session.output.clear()
            session.send(b"\x12")  # Ctrl+R
            paste(session, "fist")
            session.wait(lambda: b"Session navigation" in session.output and b"fist" in session.output, "session history")
            session.output.clear()
            session.send(b"\r")
            session.wait(lambda: b"clenched" in session.output, "restored reading location")
            for enabled in [False, True]:
                session.output.clear()
                session.send(b"\x07")  # Ctrl+G
                paste(session, "mouse")
                session.wait(lambda: b"Mouse capture" in session.output, "filtered action")
                session.send(b"\r")
                sequence = b"\x1b[?1006h" if enabled else b"\x1b[?1006l"
                session.wait(lambda: sequence in session.output, "mouse capture toggle")
        finally:
            session.close()

        saved = next(root.rglob("config.json")).read_bytes()
        session = launch("fist")
        try:
            session.send(b"\x1bOQ")  # F2
            session.wait(lambda: b"Reading layout: focus" in session.output
                         and b"Examples / references: full" in session.output
                         and b"Pronunciation (IPA): full" in session.output, "restored preferences")
        finally:
            session.close()
        assert next(root.rglob("config.json")).read_bytes() == saved, "Restart rewrote preferences"
        assert "fist" not in saved.decode() and "take" not in saved.decode(), "Query leaked into preferences"
        path = next(root.rglob("config.json"))
        path.write_text('{"reading_layout":"missing"}')
        import subprocess
        result = subprocess.run(command + ["fist"], env=env, capture_output=True, timeout=5)
        assert result.returncode != 0 and str(path).encode() in result.stderr
        assert b"\x1b[?1049h" not in result.stdout, "Invalid reading configuration entered raw UI"
        print("PASS: find, outline, session navigation, persisted reading preferences, mouse toggles, and PTY restoration")


if __name__ == "__main__":
    main()
