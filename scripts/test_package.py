import json
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from package import checksum, portable_metadata


class PackagingTests(unittest.TestCase):
    def test_three_archive_types_preserve_content_and_remove_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pack = root / "pack"
            pack.mkdir()
            for name in ("entries.sqlite", "words.fst", "candidates.json"):
                (pack / name).write_bytes(b"format fixture")
            manifest = {"schema_version": 1, "candidate_count": 1,
                        "files": {name: checksum(pack / name) for name in
                                  ("entries.sqlite", "words.fst", "candidates.json")}}
            (pack / "manifest.json").write_text(json.dumps(manifest))
            binaries = root / "binaries"
            binaries.mkdir()
            (binaries / "wordglide").write_bytes(b"executable fixture")
            (binaries / "wordglide").chmod(0o755)
            output = root / "output"
            subprocess.run([sys.executable, str(Path(__file__).with_name("package.py")),
                            "--pack", str(pack), "--binaries", str(binaries),
                            "--target", "x86_64-unknown-linux-musl", "--version", "0.1.0",
                            "--output", str(output)], check=True, capture_output=True)
            archives = list(output.glob("*.tar.gz"))
            self.assertEqual(len(archives), 3)
            for archive in archives:
                with tarfile.open(archive) as tar:
                    members = tar.getmembers()
                    for member in members:
                        self.assertEqual((member.uid, member.gid, member.uname, member.gname,
                                          member.mtime, member.pax_headers), (0, 0, "", "", 0, {}))
                    names = {m.name for m in members}
                    if archive.name == "english-pack.tar.gz":
                        self.assertIn("english-pack/manifest.json", names)
                        self.assertNotIn("wordglide/wordglide", names)
                    else:
                        self.assertEqual(tar.extractfile("wordglide/wordglide").read(), b"executable fixture")
                        self.assertEqual(bool("wordglide/english-pack/manifest.json" in names),
                                         "with-data" in archive.name)
            for line in (output / "SHA256SUMS.txt").read_text().splitlines():
                expected, name = line.split("  ")
                self.assertEqual(checksum(output / name), expected)

    def test_unsafe_and_corrupt_archives_are_rejected(self):
        from package import validate_data_archive
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = root / "payload"
            payload.write_text("fixture")
            for name in ("../outside", "english-pack/extra"):
                archive = root / "unsafe.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    tar.add(payload, arcname=name, filter=portable_metadata)
                with self.assertRaises(ValueError):
                    validate_data_archive(archive)
            with tarfile.open(archive, "w:gz") as tar:
                member = tarfile.TarInfo("english-pack/entries.sqlite")
                member.type = tarfile.SYMTYPE
                member.linkname = "outside"
                tar.addfile(member)
            with self.assertRaises(ValueError):
                validate_data_archive(archive)

    def test_data_archive_rejects_special_permissions_even_with_valid_hashes(self):
        from package import validate_data_archive
        payload = b"format fixture"
        files = {name: payload for name in ("entries.sqlite", "words.fst", "candidates.json")}
        files["manifest.json"] = json.dumps({"schema_version": 1, "candidate_count": 1,
            "files": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}).encode()
        files["THIRD_PARTY.md"] = b"attribution fixture"
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "data.tar.gz"
            for mode in (0o644, 0o4777):
                with tarfile.open(archive, "w:gz") as tar:
                    for name, data in files.items():
                        member = tarfile.TarInfo("english-pack/" + name)
                        member.mode, member.size = mode, len(data)
                        tar.addfile(member, io.BytesIO(data))
                if mode == 0o644:
                    self.assertEqual(validate_data_archive(archive)["candidate_count"], 1)
                else:
                    with self.assertRaises(ValueError):
                        validate_data_archive(archive)


if __name__ == "__main__":
    unittest.main()
