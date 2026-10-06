#!/usr/bin/env python3
"""Package a validated-format data directory and native binaries for offline copying."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import tarfile


def checksum(path):
    h = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def portable_metadata(info):
    """Keep developer identity and filesystem timestamps out of archives."""
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mtime = 0
    info.pax_headers = {}
    return info


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--pack", required=True, type=Path)
    p.add_argument("--output", default="dist", type=Path)
    p.add_argument("--binaries", default="target/release", type=Path)
    args = p.parse_args()
    root = Path(__file__).resolve().parent.parent
    manifest = json.loads((args.pack / "manifest.json").read_text())
    if manifest["schema_version"] != 1:
        raise ValueError("Unsupported pack schema")
    for name in ("entries.sqlite", "words.fst", "candidates.json"):
        if checksum(args.pack / name) != manifest["files"][name]:
            raise ValueError(f"Corrupt file: {name}")
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / "english-pack.tar.gz"
    binaries = args.output / f"dict-{platform.system().lower()}-{platform.machine().lower()}.tar.gz"
    if archive.exists() or binaries.exists():
        raise FileExistsError("Release archives already exist; use a new output directory")
    with tarfile.open(archive, "w:gz") as tar:
        for name in ("manifest.json", "entries.sqlite", "words.fst", "candidates.json"):
            tar.add(args.pack / name, arcname="english-pack/" + name, filter=portable_metadata)
        tar.add(root / "THIRD_PARTY.md", arcname="english-pack/THIRD_PARTY.md", filter=portable_metadata)
    with tarfile.open(binaries, "w:gz") as tar:
        for name in ("dict", "dict-build", "dict-bench"):
            tar.add(args.binaries / name, arcname="bin/" + name, filter=portable_metadata)
        for name in ("README.md", "LICENSE", "THIRD_PARTY.md", "PERFORMANCE.md"):
            tar.add(root / name, arcname=name, filter=portable_metadata)
    receipts = {path.name: checksum(path) for path in (archive, binaries)}
    (args.output / "SHA256SUMS.json").write_text(json.dumps(receipts, indent=2))
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
