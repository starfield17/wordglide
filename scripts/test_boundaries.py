"""Repository boundary checks that the Rust compiler cannot express.

Each check cites the SPEC.md entry it incarnates, using the lineage convention
documented at the top of SPEC.md. These are deliberately narrow source scans:
the language already enforces module privacy, so only the remaining
"which module may do this" rules are checked here.
"""
import re
import unittest
from pathlib import Path

SRC = Path(__file__).resolve().parents[1] / "src"
ROOT = SRC.parent

# Network-capable calls are confined to the explicit download module.
NETWORK = re.compile(r"\b(?:ureq|TcpListener|TcpStream|ToSocketAddrs|std::net)\b")
DOWNLOAD_MODULE = {"download.rs", "download/tests.rs"}

# Pack encoders live in their owner modules; only the builder may call them at
# runtime, so no lookup or startup path can rewrite or rebuild pack data.
PACK_ENCODERS = ("Index::encode", "entry_codec::encode", "entry_codec::compress", "zstd::dict::from_samples")
ENCODER_OWNERS = {"build.rs", "index.rs", "entry_codec.rs"}


def rust_sources():
    return sorted(SRC.rglob("*.rs"))


def relative(path):
    return path.relative_to(ROOT).as_posix()


def read(path):
    return path.read_text(encoding="utf-8")


class NetworkIsOptIn(unittest.TestCase):
    def test_network_calls_stay_in_download_module(self):
        # N1 ← S2: normal startup and lookup never touch the network.
        offenders = [
            relative(path)
            for path in rust_sources()
            if path.relative_to(SRC).as_posix() not in DOWNLOAD_MODULE
            and NETWORK.search(read(path))
        ]
        self.assertEqual(
            offenders,
            [],
            "network-capable calls appear outside src/download.rs",
        )


class PackWritesAreBuildOnly(unittest.TestCase):
    def test_only_build_encodes_pack_data(self):
        # F1 ← S2 and N7 ← F1: only build/prepare write pack data.
        offenders = []
        for path in rust_sources():
            if path.relative_to(SRC).as_posix() in ENCODER_OWNERS:
                continue
            text = read(path)
            offenders.extend(
                f"{relative(path)}: {needle}"
                for needle in PACK_ENCODERS
                if needle in text
            )
        self.assertEqual(
            offenders,
            [],
            "pack encoders are referenced outside src/build.rs and their owners",
        )


class InstallerCopiesBytes(unittest.TestCase):
    def test_installer_does_not_generate_data(self):
        # F4 ← S2: the installer copies verified prebuilt bytes.
        text = read(SRC / "download.rs")
        for needle in ("build_pack", *PACK_ENCODERS):
            self.assertNotIn(needle, text, "the installer must not generate data")


if __name__ == "__main__":
    unittest.main()
