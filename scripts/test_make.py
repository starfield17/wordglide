"""Exercise build/clean sequencing in a disposable project, without deleting real outputs."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class BuildCleanupTests(unittest.TestCase):
    def test_clean_build_and_explicit_clean_all_preserve_dictionary_data(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copyfile(Path(__file__).resolve().parents[1] / "Makefile", root / "Makefile")
            tools = root / "tools"
            tools.mkdir()
            cargo = tools / "cargo"
            cargo.write_text('''#!/bin/sh
printf '%s\\n' "$*" >> calls
case "$1" in
  clean)
    if [ -f fail-clean ]; then exit 1; fi
    rm -rf -- "$CARGO_TARGET_DIR"
    ;;
  build)
    test ! -e "$CARGO_TARGET_DIR/stale" || exit 2
    mkdir -p "$CARGO_TARGET_DIR/release"
    for name in wordglide dict-build dict-bench; do
      printf '%s\\n' 'fixture binary' > "$CARGO_TARGET_DIR/release/$name"
    done
    ;;
esac
''')
            cargo.chmod(0o755)
            target = root / "custom-target"
            env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                       CARGO_TARGET_DIR=str(target))
            for name in ("artifacts", "dist", "data"):
                (root / name).mkdir()
                (root / name / "keep").write_text("fixture")
            for _ in range(2):
                target.mkdir(exist_ok=True)
                (target / "stale").write_text("obsolete compilation")
                subprocess.run(["make", "build"], cwd=root, env=env, check=True, capture_output=True)
                self.assertFalse((target / "stale").exists())
                for name in ("wordglide", "dict-build", "dict-bench"):
                    self.assertTrue((target / "release" / name).is_file())
            self.assertEqual((root / "calls").read_text().splitlines(), [
                "clean", "build --locked --release --bins", "clean", "build --locked --release --bins"])
            for name in ("artifacts", "dist", "data"):
                self.assertTrue((root / name / "keep").is_file())
            (root / "fail-clean").touch()
            before = (root / "calls").read_text()
            result = subprocess.run(["make", "build"], cwd=root, env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((root / "calls").read_text(), before + "clean\n")
            (root / "fail-clean").unlink()
            subprocess.run(["make", "clean-all"], cwd=root, env=env, check=True, capture_output=True)
            self.assertFalse(target.exists())
            self.assertFalse((root / "artifacts").exists())
            self.assertFalse((root / "dist").exists())
            self.assertEqual((root / "data/keep").read_text(), "fixture")
