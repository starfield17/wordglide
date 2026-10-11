#!/usr/bin/env python3
"""Exercise schema-5 source content and whole-phrase navigation in a POSIX PTY."""
import argparse
import os
from pathlib import Path
import tempfile

from terminal_appearance_smoke import Session
from terminal_interaction_smoke import paste, screen_text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/wordglide")
    parser.add_argument("--data", required=True)
    args = parser.parse_args()
    command = [str(Path(args.binary).resolve()), "--data", str(Path(args.data).resolve())]
    with tempfile.TemporaryDirectory(prefix="wordglide-content-") as temporary:
        env = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=temporary, TERM="xterm-256color")
        env.pop("NO_COLOR", None)
        session = Session(command + ["go"], env)
        last_find = ""

        def visible():
            return screen_text(session.output)

        def expect(text):
            session.wait(lambda: text in visible(), text)

        def find(text):
            nonlocal last_find
            session.send(b"/")
            session.drain(0.1)
            session.send(b"\x7f" * len(last_find))
            paste(session, text)
            session.wait(lambda: text in visible() and "No matches" not in visible(), "source match: " + text)
            session.send(b"\r")
            session.drain(0.1)
            last_find = text

        try:
            expect("Form: went [past]")
            expect("Received-Pronunciation")
            session.send(b"\rvt")
            session.drain(0.1)
            find("Etymology: From Middle English")
            expect("Etymology: From Middle English")
            # Return to input and replace the query, preserving session toggles.
            session.send(b"\x1b")
            session.drain(0.1)
            session.send(b"\x15")
            paste(session, "take")
            last_find = ""
            expect("Form: took [past]")
            session.send(b"\rr")
            session.drain(0.1)
            find("Synonyms: seize")
            expect("Synonyms: seize")
            find("Derived terms: take off")
            expect("Derived terms: take off")
            original = visible()
            rows = original.splitlines()
            row = next(i for i, value in enumerate(rows) if "Derived terms: take off" in value)
            column = rows[row].index("take off")
            # SGR mouse coordinates are one-based; click the linked source phrase.
            session.send(f"\x1b[<0;{column + 1};{row + 1}M".encode())
            session.wait(lambda: "Definition · take off" in visible(), "whole phrase lookup")
            session.send(b"\x1a")
            expect("Derived terms: take off")
            session.send(b"\x1bOP")  # F1 help
            expect("word forms: compact / full")
            session.send(b"\x1b")
            session.drain(0.1)
            assert not list(Path(temporary).rglob("config.json")), "Session content toggles wrote configuration"
        finally:
            session.close()
        print("PASS: source forms, etymology, labeled IPA, sense relations, whole-phrase mouse follow/back, help and terminal restoration")


if __name__ == "__main__":
    main()
