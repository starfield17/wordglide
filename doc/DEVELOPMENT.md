# Development and distribution

This document covers source builds, dictionary preparation, storage, verification
and releases. User-facing controls are in [Using Wordglide](USAGE.md); custom
source conversion is in [Making your own dictionary pack](PACK_AUTHORING.md).
Repository ownership and agent instructions live in [AGENTS.md](../AGENTS.md).

## Current data contract

The current program reads schema 5 only and never converts packs at startup.
Prepare from the raw source to retain all current source-content fields.
The public `Group` and `Sense` types expose forms, pronunciations, etymology and
relations; `WordForm`, `Pronunciation`, `TermRelation` and `RelationKind` are
public exports. `Group.ipa` must equal the distinct projection of
`Group.pronunciations` when building. See the authoring guide for JSON fields.

## Build and try the real-data sample

Requires a Rust toolchain and a C toolchain for bundled Zstd. The project pins
Rust 1.99.0 for reproducible development and CI; package MSRV is 1.88,
verified with the packaged source and tests. Supported environments are
macOS and Linux terminals; Windows is out of scope for now.

```sh
make build
./target/release/dict-build \
  --input examples/sample/entries.jsonl \
  --source examples/sample/source.json \
  --output data/sample-pack
./target/release/wordglide --data data/sample-pack
```

The included 89-word sample demonstrates `ho`, `house`, `hosue`, `fist`, `bubble`,
`went`, `better`, `take`, `set`, and `take off`. It is intentionally small and
source-grounded; lookups outside it need the full pack. Existing output directories
are never overwritten: choose a new name when rebuilding.

You can start with a query: `wordglide "take off" --data data/sample-pack`.
`--data` points to an unpacked pack directory. Without it, the application checks
`WORDGLIDE_DATA` (when set to a non-empty path), then the explicitly downloaded
pack, then `english-pack/` beside the actual executable (also through a symlink),
and finally the existing user-data directory under `dict/english`.
A selected damaged pack fails validation instead of silently switching
dictionaries. `--data` and `WORDGLIDE_DATA` continue to override downloaded packs.
Use `wordglide --download-data` to install data, or select an existing pack with
`--data`.


## Ranking and storage

Up to 20 candidates: exact match, valid word-form targets, prefix completions,
then one-edit fuzzy fallback. Fuzzy handles an inserted, missing, wrong, or
swapped adjacent character for queries of 3–64 characters. It compares complete
words/phrases; it does not provide fuzzy prefix completion.

The fixed base score is `round(100*Zipf) - 2*characters - 100*extra_words - 150*hyphens`, ties broken
by normalized key. Wordfreq is queried only while building data. Runtime scores
never depend on your lookups. Short phrases receive a fixed penalty because
their wordfreq estimate is not a measured phrase count. ASCII `-` and Unicode
`‐` (U+2010) each deduct 150 points; exact matches still come first, including
hyphenated words.

The compact binary vocabulary and prebuilt range-max tree return prefix top-k
without sorting all prefix matches. The builder writes `lexicon.bin`: a versioned
32-byte header, 24-byte candidate records, prebuilt internal ranking nodes,
a table of interned POS ranges, and shared UTF-8 strings. Ranking leaves are
implicit candidate IDs. Candidate labels are sorted and deduplicated across all
source groups. The UI shows up to two abbreviations and marks omitted labels;
narrow rows preserve word space and match markers before the POS summary.
Runtime loads this buffer and the FST once without parsing candidate JSON,
creating millions of string objects, or rebuilding the tree. Only returned
candidates allocate strings. An FST handles exact and fuzzy matching.

Entry storage uses `entries.bin` and `entries.idx`. The latter stores
an up-to-64-KiB trained dictionary, block directory, and eight-byte candidate
locators. Definition fields use a fixed-order, private MessagePack encoding.
The public JSON input is documented in [the authoring guide](PACK_AUTHORING.md).
Independently checksummed Zstd-level-19
blocks contain at most 1 MiB of uncompressed entries. A lookup decompresses
only its block and parses only the requested entry. A byte-bounded 8 MiB block
cache complements the 32 MiB parsed-entry cache, which counts owned vector and
string capacities. Startup never decompresses entries. Frame length, checksum,
window, tails, MessagePack lengths/depth, locator ranges, and entry keys are checked.
Raw entries are bounded to 1 MiB, compressed blocks to 2 MiB.
A worker coalesces pending
queries; response IDs stop obsolete previews from overwriting newer input.


## Build the full data pack

For your own English data, see [Making your own dictionary pack](PACK_AUTHORING.md).
The guide covers canonical JSONL, source attribution, building, verification,
manual community archives and local use. It also distinguishes current support
from the future multilingual design.

Python is needed only by maintainers preparing data, not by dictionary users.
Choose an environment of your own; no machine-specific paths are in the build.

```sh
python3 -m pip install -r scripts/requirements-build.txt
mkdir -p data
curl --fail --location --retry 2 \
  --output data/raw-wiktextract-data.jsonl.gz \
  https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz
python3 scripts/prepare.py \
  --input data/raw-wiktextract-data.jsonl.gz \
  --output data/prepared \
  --snapshot SOURCE_DUMP_DATE
./target/release/dict-build \
  --input data/prepared/entries.jsonl \
  --source data/prepared/source.json \
  --output data/english-pack
./target/release/wordglide --data data/english-pack
```

Get `SOURCE_DUMP_DATE` from the [raw download page](https://kaikki.org/dictionary/rawdata.html)
when downloading. The moving URL is not a version: retain the downloaded file,
snapshot identifier, and the SHA-256 recorded in provenance. The source covers
all languages; preparation streams it and retains English only. Reserve several
GB of disk space for compressed input, staging database, and generated pack.
Definitions/examples remain source text; empty fields stay empty. Explicitly
archaic/obsolete senses move to the end. `quality.json` reports coverage and
sample words, including malformed word-form targets that could not be resolved.
The target count tracks unmatched literal source links, even when a modern
inflection table supplies useful fallback lemmas.
Malformed source records are excluded with their original records and reasons
written to `rejected.jsonl` and counted in the report. Invalid JSON or damaged
gzip input aborts preparation instead of publishing an incomplete pack.

To refresh the small sample from current raw page records:

```sh
python3 scripts/fetch_sample.py --output data/sample-source
python3 scripts/prepare.py --input data/sample-source/raw.jsonl \
  --output data/sample-prepared --snapshot SOURCE_DUMP_DATE-sampled-TODAY
```

## Checks, benchmarks, and distribution

```sh
make build                 # clean Cargo outputs, then build all three release programs
make check                 # or make check PYTHON=/path/to/python
make clean                 # remove the Cargo target directory
make clean-all             # also delete artifacts/ and dist/; keep data/
python3 scripts/terminal_smoke.py --data data/sample-pack
./target/release/dict-bench --data data/english-pack --iterations 100
python3 scripts/package.py --program-only --target RUST_TARGET --output dist/program
python3 scripts/package.py --pack data/english-pack --target RUST_TARGET --output dist/release
```

`make build` removes the selected Cargo build directory before every build,
including old versions and cached dependencies. It rebuilds all three release
programs with locked dependencies, so it takes longer than incremental builds.
For an incremental build, run `cargo build --locked --release --bins` directly.
`make clean` respects `CARGO_TARGET_DIR`; `make clean-all` additionally deletes
local verification logs, extracted packages, and release archives in `artifacts/`
and `dist/`. Neither command removes source/prepared data or installed dictionaries.
CI uses Cargo directly so it can reuse compilation caches.

Benchmarks distinguish first application-cache reads, warm lookup/preview time,
and asynchronous input-to-TestBackend render time at 120×40. They also report
dictionary-open time; the real PTY check reports startup to terminal UI. OS file caches are
not flushed. The renderer measurement excludes terminal emulator display latency.
Full-pack target: P95 input-to-render ≤50 ms and resident memory about ≤512 MiB;
startup time is not a target. Record terminal size, data snapshot, and measurement
method with results. Small-sample results
do not establish full-pack performance. See [performance measurements](PERFORMANCE.md)
for the full-data verification.

CI runs on main-branch pushes and pull requests, with superseded runs cancelled.
Formatting and Python tests run once; Rust lint/tests and sample-dictionary
terminal checks run on macOS and Linux. A separate job compiles all targets with
Rust 1.88. Ordinary checks use debug programs and do not upload release binaries.

Version tags run an early metadata/source/MSRV gate before four native release
builds. Each platform checks its Rust targets, runtime dependencies, and packaged
terminal behavior with the sample dictionary, then uploads only the program
archive and checksum. The assembly job downloads the pinned full dictionary
once, verifies its archive and every entry using the Linux x86_64 release program,
and checks full-data previews from an unrelated directory. It publishes four
program archives, the shared dictionary, and combined checksums only after all
gates pass. Same-ref release runs are serialized; published versions are left
unchanged. Manual dispatch performs the same verification without publishing.

`data-release.json` pins the dictionary Release tag, asset, and SHA-256.
Release builds reuse that archive; they do not download and rebuild a moving
Wiktionary dump. Updating dictionary data requires a new data Release and an
explicit checksum update. Both package types retain license notices and
portable archive metadata. Python 3.11+ is required only for maintainer scripts.
Update packs by unpacking to a new directory and restarting with `--data`, or by
replacing the adjacent pack while the application is stopped.

Code: MIT. Data: its original licenses and attribution; see [THIRD_PARTY.md](../THIRD_PARTY.md).

## Data publication prerequisite

Publish and verify a matching schema-5 archive before releasing the current
program. `data-release.json` currently pins an asset incompatible with the source
build; release preflight blocks until the pin contains the new asset's real tag
and checksum. Until publication, use the sample or a locally built pack via
`--data`. The public downloader cannot supply the current source format yet.

Future language identity, normalization, ranking, following and community
packaging boundaries are recorded in [SPEC.md](../SPEC.md#found--not-doing).
