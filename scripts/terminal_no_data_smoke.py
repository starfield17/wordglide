#!/usr/bin/env python3
"""Check missing/invalid data startup and F2 recovery in a real POSIX terminal."""
import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile

from terminal_appearance_smoke import Session
from terminal_interaction_smoke import screen_text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/wordglide")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="wordglide-no-data-") as temporary:
        root = Path(temporary)
        binary = root / "wordglide"
        shutil.copy2(Path(args.binary).resolve(), binary)
        home = root / "home"
        home.mkdir()
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / "config"),
                   XDG_DATA_HOME=str(home / "data"), TERM="xterm-256color")
        env.pop("WORDGLIDE_DATA", None)
        missing = root / "absent"
        corrupt = root / "corrupt"
        corrupt.mkdir()
        (corrupt / "manifest.json").write_text("not json")
        old = root / "old"
        old.mkdir()
        (old / "manifest.json").write_text(json.dumps({
            "schema_version": 2, "candidate_count": 1, "ranking": "old",
            "source": {}, "files": {}, "sizes": {}}))
        cases = [([], {}, "No data pack"),
                 (["--data", str(missing)], {}, "No data pack"),
                 ([], {"WORDGLIDE_DATA": str(missing)}, "No data pack"),
                 (["--data", str(corrupt)], {}, "expected"),
                 (["--data", str(old)], {}, "Incompatible data pack version 2")]
        for extra, overrides, diagnostic in cases:
            session = Session([str(binary), *extra, "ho"], env | overrides)
            try:
                session.wait(lambda: "No usable dictionary" in screen_text(session.output), "welcome screen")
                visible = screen_text(session.output)
                assert "F2" in visible and "wordglide --download-data" in visible, visible
                assert diagnostic in visible, visible
                session.send(b"use")
                session.wait(lambda: "house" in screen_text(session.output), "editing without dictionary")
                assert "Looking up" not in screen_text(session.output)
                session.send(b"\x1bOQ")  # xterm F2
                session.wait(lambda: "Download / update dictionary" in screen_text(session.output), "F2 download entry")
                session.send(b"\x1b")
                session.drain(0.1)
                assert "house" in screen_text(session.output)
                assert "No usable dictionary" in screen_text(session.output)
            finally:
                session.close()
        # Discovery errors also enter the UI, rather than failing before raw mode.
        downloads = (home / "Library/Application Support/org.wordglide.dict/downloads"
                     if os.sys.platform == "darwin" else home / "data/dict/downloads")
        downloads.mkdir(parents=True, exist_ok=True)
        (downloads / "current.json").write_text("invalid receipt")
        session = Session([str(binary)], env)
        try:
            session.wait(lambda: "Invalid dictionary installation record" in screen_text(session.output), "invalid receipt diagnostic")
            assert "F2" in screen_text(session.output)
        finally:
            session.close()
    print("PASS: missing, overridden, corrupt, old-schema, and invalid-receipt data start the UI; editing/F2 and terminal restoration work")


if __name__ == "__main__":
    main()
