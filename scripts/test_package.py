import json
import lzma
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from package import checksum, portable_metadata, assemble_programs, release_notes, RELEASE_TARGETS, DOCS
from prepare import POLICY, OLD_POLICY


class PackagingTests(unittest.TestCase):
    def test_xz_stream_rejects_damaged_footer_truncation_and_tails(self):
        from package import XzStream
        valid = lzma.compress(b"fixture" * 100, preset=0)
        bad_footer = bytearray(valid)
        bad_footer[-6] ^= 1
        for archive in (valid[:-1], bytes(bad_footer), valid + b"tail", valid + valid):
            with self.assertRaises(ValueError):
                stream = XzStream(io.BytesIO(archive))
                while stream.read(17):
                    pass
        stream = XzStream(io.BytesIO(valid))
        self.assertEqual(stream.read(1000), b"fixture" * 100)
        self.assertEqual(stream.read(1000), b"")

    def test_program_only_needs_no_dictionary_and_rejects_conflicting_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = root / "binaries"
            binaries.mkdir()
            (binaries / "wordglide").write_bytes(b"executable fixture")
            (binaries / "wordglide").chmod(0o755)
            command = [sys.executable, str(Path(__file__).with_name("package.py")),
                       "--program-only", "--binaries", str(binaries),
                       "--target", "aarch64-apple-darwin", "--version", "0.4.0"]
            output = root / "program"
            subprocess.run(command + ["--output", str(output)], check=True, capture_output=True)
            self.assertEqual({p.name for p in output.iterdir()}, {
                "wordglide-v0.4.0-aarch64-apple-darwin.tar.gz", "SHA256SUMS.txt"})
            archive = output / "wordglide-v0.4.0-aarch64-apple-darwin.tar.gz"
            with tarfile.open(archive) as tar:
                self.assertEqual(set(tar.getnames()), {
                    "wordglide/wordglide", *("wordglide/" + doc for doc in DOCS)})
                self.assertEqual(tar.getmember("wordglide/wordglide").mode, 0o755)
                self.assertEqual(tar.extractfile("wordglide/wordglide").read(), b"executable fixture")
                for member in tar.getmembers():
                    self.assertEqual((member.uid, member.gid, member.uname, member.gname,
                                      member.mtime, member.pax_headers), (0, 0, "", "", 0, {}))
            digest, name = (output / "SHA256SUMS.txt").read_text().strip().split("  ")
            self.assertEqual(name, archive.name)
            self.assertEqual(digest, checksum(archive))
            for option in ("--pack", "--data-archive"):
                result = subprocess.run(command + [option, str(root / "absent"),
                                         "--output", str(root / "conflict")], capture_output=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn(b"not allowed with argument", result.stderr)
                self.assertFalse((root / "conflict").exists())

    def test_release_collection_requires_four_valid_programs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "artifacts"
            binaries = root / "binaries"
            binaries.mkdir()
            (binaries / "wordglide").write_bytes(b"executable fixture")
            (binaries / "wordglide").chmod(0o755)
            for target in RELEASE_TARGETS:
                subprocess.run([sys.executable, str(Path(__file__).with_name("package.py")),
                    "--program-only", "--binaries", str(binaries), "--target", target,
                    "--version", "0.4.0", "--output", str(artifacts / target)],
                    check=True, capture_output=True)
            output = root / "release"
            assemble_programs(artifacts, output, "0.4.0")
            self.assertEqual(len(list(output.iterdir())), 4)
            receipt = artifacts / RELEASE_TARGETS[0] / "SHA256SUMS.txt"
            original = receipt.read_text()
            for contents in ("", original + original, original.replace("wordglide-v", "../wordglide-v"),
                             "0" * 64 + original[64:]):
                receipt.write_text(contents)
                with self.assertRaises(ValueError):
                    assemble_programs(artifacts, root / "bad", "0.4.0")
                self.assertFalse((root / "bad").exists())
            receipt.write_text(original)
            receipt.unlink()
            with self.assertRaisesRegex(ValueError, "all four"):
                assemble_programs(artifacts, root / "missing", "0.4.0")

    def test_release_collection_rejects_unsafe_programs_with_correct_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "artifacts"
            binaries = root / "binaries"
            binaries.mkdir()
            (binaries / "wordglide").write_bytes(b"executable fixture")
            (binaries / "wordglide").chmod(0o755)
            for target in RELEASE_TARGETS:
                subprocess.run([sys.executable, str(Path(__file__).with_name("package.py")),
                    "--program-only", "--binaries", str(binaries), "--target", target,
                    "--version", "0.4.0", "--output", str(artifacts / target)],
                    check=True, capture_output=True)
            platform = artifacts / RELEASE_TARGETS[0]
            archive = next(platform.glob("*.tar.gz"))
            for kind in ("traversal", "symlink", "permissions"):
                with tarfile.open(archive, "w:gz") as tar:
                    for name in ["wordglide/wordglide", *("wordglide/" + doc for doc in DOCS)]:
                        member = tarfile.TarInfo(name)
                        member.mode = 0o755 if name == "wordglide/wordglide" else 0o644
                        if name == "wordglide/wordglide":
                            if kind == "traversal":
                                member.name = "../outside"
                            elif kind == "symlink":
                                member.type = tarfile.SYMTYPE
                                member.linkname = "../../outside"
                            else:
                                member.mode = 0o4755
                        tar.addfile(member, io.BytesIO(b""))
                (platform / "SHA256SUMS.txt").write_text(checksum(archive) + "  " + archive.name + "\n")
                with self.assertRaises(ValueError):
                    assemble_programs(artifacts, root / "unsafe", "0.4.0")
                self.assertFalse((root / "unsafe").exists())

    def test_release_notes_follow_dictionary_metadata(self):
        notes = release_notes("v1.2.3", {"schema_version": 9, "candidate_count": 42,
            "source": {"snapshot": "future-snapshot", "source": "source fixture"}})
        for expected in ("v1.2.3", "future-snapshot", "source fixture", "42 entries", "schema 9"):
            self.assertIn(expected, notes)
        self.assertNotIn("2026-09-02", notes)
        self.assertNotIn("1,355,084", notes)

    def test_schema_four_archives_reject_old_or_missing_ranking(self):
        from package import validate_data_archive
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "data.tar.xz"
            for ranking in (OLD_POLICY, None):
                files = {name: b"format fixture" for name in ("entries.bin", "entries.idx", "words.fst", "lexicon.bin")}
                manifest = {"schema_version":4, "candidate_count":1,
                            "files":{name:hashlib.sha256(data).hexdigest() for name,data in files.items()},
                            "sizes":{name:len(data) for name,data in files.items()}}
                if ranking is not None:
                    manifest["ranking"] = ranking
                files["manifest.json"] = json.dumps(manifest).encode()
                files["THIRD_PARTY.md"] = b"attribution fixture"
                with tarfile.open(archive, "w:xz") as tar:
                    for name,data in files.items():
                        member = tarfile.TarInfo("english-pack/" + name)
                        member.size = len(data)
                        member.mode = 0o644
                        tar.addfile(member, io.BytesIO(data))
                with self.assertRaises(ValueError):
                    validate_data_archive(archive)

    def test_two_archive_types_preserve_content_and_remove_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pack = root / "pack"
            pack.mkdir()
            for name in ("entries.bin", "entries.idx", "words.fst", "lexicon.bin"):
                (pack / name).write_bytes(b"format fixture")
            manifest = {"schema_version": 4, "ranking": POLICY, "candidate_count": 1,
                        "sizes": {name: (pack / name).stat().st_size for name in
                                  ("entries.bin", "entries.idx", "words.fst", "lexicon.bin")},
                        "files": {name: checksum(pack / name) for name in
                                  ("entries.bin", "entries.idx", "words.fst", "lexicon.bin")}}
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
            archives = list(output.glob("*.tar.*"))
            self.assertEqual(len(archives), 2)
            for archive in archives:
                with tarfile.open(archive) as tar:
                    members = tar.getmembers()
                    for member in members:
                        self.assertEqual((member.uid, member.gid, member.uname, member.gname,
                                          member.mtime, member.pax_headers), (0, 0, "", "", 0, {}))
                    names = {m.name for m in members}
                    if archive.name == "english-pack.tar.xz":
                        self.assertIn("english-pack/manifest.json", names)
                        self.assertNotIn("wordglide/wordglide", names)
                    else:
                        self.assertEqual(tar.extractfile("wordglide/wordglide").read(), b"executable fixture")
                        self.assertNotIn("wordglide/english-pack/manifest.json", names)
                        self.assertNotIn("with-data", archive.name)
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
                archive = root / "unsafe.tar.xz"
                with tarfile.open(archive, "w:xz") as tar:
                    tar.add(payload, arcname=name, filter=portable_metadata)
                with self.assertRaises(ValueError):
                    validate_data_archive(archive)
            with tarfile.open(archive, "w:xz") as tar:
                member = tarfile.TarInfo("english-pack/entries.bin")
                member.type = tarfile.SYMTYPE
                member.linkname = "outside"
                tar.addfile(member)
            with self.assertRaises(ValueError):
                validate_data_archive(archive)

    def test_data_archive_rejects_special_permissions_even_with_valid_hashes(self):
        from package import validate_data_archive
        payload = b"format fixture"
        files = {name: payload for name in ("entries.bin", "entries.idx", "words.fst", "lexicon.bin")}
        files["manifest.json"] = json.dumps({"schema_version": 4, "ranking": POLICY, "candidate_count": 1,
            "files": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()},
            "sizes": {name: len(data) for name, data in files.items()}}).encode()
        files["THIRD_PARTY.md"] = b"attribution fixture"
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "data.tar.xz"
            for mode in (0o644, 0o4777):
                with tarfile.open(archive, "w:xz") as tar:
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
