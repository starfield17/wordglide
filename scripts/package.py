#!/usr/bin/env python3
"""Build program-only and shared dictionary Wordglide release archives."""
import argparse
from contextlib import contextmanager
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tomllib
from prepare import POLICY

DATA_FILES = ("entries.sqlite", "words.fst", "lexicon.bin")
DATA_MEMBERS = {"english-pack/" + name for name in
                ("manifest.json", *DATA_FILES, "THIRD_PARTY.md")}
DOCS = ("README.md", "LICENSE", "THIRD_PARTY.md", "PERFORMANCE.md")


def checksum(path):
    h = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def portable_metadata(info):
    """Exclude developer identity, filesystem dates, and special permissions."""
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mtime = 0
    info.pax_headers = {}
    info.mode = 0o755 if info.mode & 0o111 else 0o644
    return info


@contextmanager
def archive_writer(path):
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w|") as tar:
                yield tar


def validate_data_archive(path):
    manifest = None
    seen = set()
    hashes = {}
    sizes = {}
    with tarfile.open(path, "r|gz") as tar:
        for member in tar:
            if not member.isfile() or member.name not in DATA_MEMBERS or member.name in seen:
                raise ValueError("Unexpected, duplicate, or unsafe data archive member")
            if member.uid or member.gid or member.uname or member.gname or member.mtime or member.pax_headers:
                raise ValueError("Data archive includes nonportable filesystem metadata")
            if member.mode not in {0o644, 0o755}:
                raise ValueError("Data archive includes nonportable permissions")
            seen.add(member.name)
            source = tar.extractfile(member)
            name = Path(member.name).name
            sizes[name] = member.size
            if name == "manifest.json":
                manifest = json.load(source)
            else:
                h = hashlib.sha256()
                for block in iter(lambda: source.read(1024 * 1024), b""):
                    h.update(block)
                hashes[name] = h.hexdigest()
    if (seen != DATA_MEMBERS or not manifest or manifest.get("schema_version") != 3
            or manifest.get("ranking") != POLICY):
        raise ValueError("Incomplete data archive or unsupported schema")
    if manifest.get("candidate_count", 0) <= 0:
        raise ValueError("Empty data archive")
    for name in DATA_FILES:
        if hashes[name] != manifest.get("files", {}).get(name):
            raise ValueError(f"Corrupt data archive: {name}")
        if sizes[name] != manifest.get("sizes", {}).get(name):
            raise ValueError(f"Corrupt data archive length: {name}")
    return manifest


RELEASE_TARGETS = (
    "aarch64-apple-darwin", "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
)


def assemble_programs(artifacts, output, version):
    """Verify exactly four program receipts before collecting any archives."""
    expected = {f"wordglide-v{version}-{target}.tar.gz" for target in RELEASE_TARGETS}
    verified = {}
    for receipt in artifacts.rglob("SHA256SUMS.txt"):
        lines = receipt.read_text().splitlines()
        if len(lines) != 1:
            raise ValueError("Each platform must provide exactly one program checksum")
        parts = lines[0].split("  ")
        if len(parts) != 2:
            raise ValueError("Invalid program checksum receipt")
        digest, name = parts
        if name not in expected or name in verified or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError("Unexpected or duplicate program asset")
        archive = receipt.parent / name
        if checksum(archive) != digest:
            raise ValueError(f"Program checksum mismatch: {name}")
        with tarfile.open(archive, "r|gz") as tar:
            seen = set()
            members = {"wordglide/wordglide", *("wordglide/" + doc for doc in DOCS)}
            for member in tar:
                if not member.isfile() or member.name not in members or member.name in seen:
                    raise ValueError("Unexpected or unsafe program archive member")
                if member.uid or member.gid or member.uname or member.gname or member.mtime or member.pax_headers:
                    raise ValueError("Program archive includes nonportable filesystem metadata")
                required_mode = 0o755 if member.name == "wordglide/wordglide" else 0o644
                if member.mode != required_mode:
                    raise ValueError("Invalid program archive permissions")
                seen.add(member.name)
            if seen != members:
                raise ValueError("Incomplete program archive")
        verified[name] = archive
    if set(verified) != expected:
        raise ValueError("Expected all four platform programs")
    output.mkdir(parents=True, exist_ok=False)
    for name, archive in verified.items():
        shutil.copyfile(archive, output / name)


def release_notes(tag, manifest):
    source = manifest["source"]
    snapshot = source.get("snapshot", "unknown")
    description = source.get("source", "English Wiktionary via Kaikki/Wiktextract")
    return (
        f"# Wordglide {tag}\n\n"
        "Download the program archive for your platform, extract it, and run `./wordglide/wordglide`. "
        "Use F2 Settings or `./wordglide/wordglide --download-data` to install the latest dictionary. "
        "Installed lookup stays offline.\n\n"
        "The shared `english-pack.tar.gz` is also provided for manual installation. "
        "Extract it into the program directory so `english-pack/` sits beside the executable. "
        "Verify downloads with `SHA256SUMS.txt`.\n\n"
        "Supports macOS 11+ (Intel/ARM64) and Linux (x86_64/ARM64, static musl executables).\n\n"
        f"Dictionary: {description}; snapshot {snapshot}; "
        f"{manifest['candidate_count']:,} entries; schema {manifest['schema_version']}. "
        "Old formats must be replaced. Run `wordglide --verify-data` for full verification.\n\n"
        "Code is MIT. Dictionary and frequency data retain their original licenses; "
        "see THIRD_PARTY.md in the program and dictionary archives.\n"
    )


def main():
    root = Path(__file__).resolve().parent.parent
    p = argparse.ArgumentParser(description=__doc__)
    data = p.add_mutually_exclusive_group(required=True)
    data.add_argument("--program-only", action="store_true", help="Package only the program, without dictionary inputs")
    data.add_argument("--pack", type=Path, help="Unpacked canonical data directory")
    data.add_argument("--data-archive", type=Path, help="Pinned, portable data archive")
    p.add_argument("--target", required=True, help="Rust target triple")
    p.add_argument("--version", default=tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"])
    p.add_argument("--output", default="dist", type=Path, help="New output directory")
    p.add_argument("--binaries", default="target/release", type=Path)
    args = p.parse_args()
    if not re.fullmatch(r"[a-z0-9_-]+", args.target) or not re.fullmatch(
            r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", args.version):
        raise ValueError("Invalid target or version")
    if not (args.binaries / "wordglide").is_file():
        raise ValueError("Build the Wordglide release executable first")
    if args.pack:
        manifest = json.loads((args.pack / "manifest.json").read_text())
        if manifest.get("schema_version") != 3 or manifest.get("ranking") != POLICY:
            raise ValueError("Unsupported pack schema or ranking")
        for name in DATA_FILES:
            if checksum(args.pack / name) != manifest["files"][name]:
                raise ValueError(f"Corrupt file: {name}")
            if (args.pack / name).stat().st_size != manifest.get("sizes", {}).get(name):
                raise ValueError(f"Corrupt file length: {name}")
    elif args.data_archive:
        validate_data_archive(args.data_archive)
    args.output.mkdir(parents=True, exist_ok=False)
    archive = args.output / "english-pack.tar.gz"
    if args.pack:
        with archive_writer(archive) as tar:
            for name in ("manifest.json", *DATA_FILES):
                tar.add(args.pack / name, arcname="english-pack/" + name, filter=portable_metadata)
            tar.add(root / "THIRD_PARTY.md", arcname="english-pack/THIRD_PARTY.md", filter=portable_metadata)
        validate_data_archive(archive)
    elif args.data_archive:
        shutil.copyfile(args.data_archive, archive)
    stem = f"wordglide-v{args.version}-{args.target}"
    with archive_writer(args.output / (stem + ".tar.gz")) as tar:
        tar.add(args.binaries / "wordglide", arcname="wordglide/wordglide", filter=portable_metadata)
        for doc in DOCS:
            tar.add(root / doc, arcname="wordglide/" + doc, filter=portable_metadata)
    receipts = {path.name: checksum(path) for path in sorted(args.output.glob("*.tar.gz"))}
    (args.output / "SHA256SUMS.txt").write_text("".join(f"{value}  {name}\n" for name, value in receipts.items()))
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
