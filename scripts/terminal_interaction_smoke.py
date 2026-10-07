#!/usr/bin/env python3
"""Check reading navigation, menus, preferences, and mouse toggles in a POSIX PTY."""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile
import unicodedata

from terminal_appearance_smoke import Session


def screen_text(output, width=120, height=40):
    """Reconstruct the absolute-positioned ANSI frames emitted by this TUI."""
    rows = [[" "] * width for _ in range(height)]
    x = y = 0
    for token in re.findall(r"\x1b\[[0-?]*[ -/]*[@-~]|[^\x1b]",
                            output.decode("utf-8", errors="replace")):
        if token.startswith("\x1b["):
            if token[-1] in "Hf":
                coordinates = token[2:-1].split(";")
                y = int(coordinates[0] or "1") - 1
                x = int(coordinates[1] or "1") - 1 if len(coordinates) > 1 else 0
            elif token == "\x1b[2J":
                rows = [[" "] * width for _ in range(height)]
            continue
        if token == "\r":
            x = 0
        elif token == "\n":
            y += 1
        elif not unicodedata.category(token).startswith("C"):
            columns = 0 if unicodedata.combining(token) else (
                2 if unicodedata.east_asian_width(token) in "WF" else 1)
            if 0 <= y < height:
                if columns == 0 and 0 < x <= width:
                    rows[y][x - 1] += token
                elif 0 <= x < width:
                    rows[y][x] = token
                    if columns == 2 and x + 1 < width:
                        rows[y][x + 1] = ""
            x += columns
    return "\n".join("".join(row) for row in rows)


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
            # Separate Esc from Ctrl+U: one combined byte burst encodes Alt+Ctrl+U.
            session.send(b"\x1b")  # input focus
            session.drain(0.1)
            session.send(b"\x15")  # clear input
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
                session.wait(lambda: b"Actions" in session.output, "action menu")
                session.drain(0.1)
                paste(session, "mouse")
                # Differential rendering can reuse letters from the previous
                # row; verify the entered filter, then the actual terminal mode.
                session.wait(lambda: b"mouse" in session.output, "action filter input")
                session.send(b"\r")
                sequence = b"\x1b[?1006h" if enabled else b"\x1b[?1006l"
                session.wait(lambda: sequence in session.output, "mouse capture toggle")
        finally:
            session.close()

        saved = next(root.rglob("config.json")).read_bytes()
        session = launch("fist")
        try:
            session.send(b"\x1bOQ")  # F2
            session.wait(lambda: b"Settings" in session.output, "restored settings panel")
            session.wait(lambda: all(label in screen_text(session.output) for label in (
                "Reading layout: focus", "Examples / references: full",
                "Pronunciation (IPA): full")), "restored preferences")
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
