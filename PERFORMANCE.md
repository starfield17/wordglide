# Full-data storage and latency verification

The schema-4 reference pack uses the same 2026-09-02 English Wiktionary snapshot
and ranking as schema 3. Measurements describe these runs; repeat the commands
on deployment targets. OS file caches were not flushed.

## Data and lossless verification

- 1,355,084 normalized entries; 1,491,592 POS groups; 1,785,990 senses.
- 382,357 senses have source examples; 297,854 have group pronunciation data.
- Compressed source SHA-256: `3dac8a09e57827bef493e2e6b552fe917bf3b1fc55dee05c4e4a3702aff5dbdb`.
- The existing canonical preparation and wordfreq scores were retained.
- An independent MessagePack/Zstd reader compared every field of all 1,355,084
  entries against canonical JSONL: no content, order, score, or source-field
  differences. This comparison took 18.245 s.
- `wordglide --verify-data` checked all file SHA-256 receipts, index/FST agreement,
  ranking, block/locator coverage, and every decoded entry: **1.8 s**. Each block
  was decompressed once; normal startup does none of this full scan.

## Installed and download size

| Same source snapshot | Schema 3 | Schema 4 |
| --- | ---: | ---: |
| Pack binaries and manifest, bytes | 637,533,325 | 192,989,452 |
| Pack binaries and manifest, MiB | 608.00 | 184.05 |
| Shared data archive, bytes | 449,923,088 | 133,568,716 |
| Shared data archive, MiB | 429.08 | 127.38 |

The local pack is **69.7% smaller**; the download is **70.3% smaller**. Archive
attribution adds only a few KiB to the installed total. Historical installed
versions remain separate and are not included in the per-pack comparison.

| Schema-4 binary component | Bytes |
| --- | ---: |
| Independent Zstd blocks, `entries.bin` | 116,174,817 |
| Dictionary, directory and locators, `entries.idx` | 10,915,888 |
| Compact candidate/POS/ranking index, `lexicon.bin` | 58,189,134 |
| Existing exact/fuzzy FST, `words.fst` | 7,704,412 |

Entry tuples use lossless fixed-order MessagePack. The builder sorts entries,
trains an up-to-64-KiB dictionary on deterministic samples, and writes independent
checksummed Zstd-level-19 frames with at most 1 MiB of raw data. SQLite page/key
storage is removed. POS IDs and implicit ranking leaves save about 13 MiB more.
The download is XZ preset 9; program archives remain gzip. The data archive's
SHA-256 is `5b2a03f626dbe06673ea010e156efba76828a5a40f6fba3c48ed19fd273d54b8`.

A full-size localhost HTTP test invoked the production installer, including
streamed archive SHA-256, bounded XZ decoding, per-member hashing, structural open
and atomic activation. Installation took **3.003 s**, excluding internet latency.
Full verification and same-archive reuse also passed. This is a test-source
measurement, not a public-release download claim.

## Lookup latency

Three paired runs used the same benchmark source against old and new libraries,
with 1,010 fixed queries: 1,000 distinct vocabulary keys sampled with seed 1729,
plus `h`, `ho`, `house`, `hosue`, `went`, `better`, `take off`, `set`, `take`, `run`.
Each query opened a fresh Dictionary before timing search plus preview, then
measured the cached lookup. Opening time is reported separately. Some runs
coincided with verification work; all runs and their variation are shown.

| Run | Open P95, schema 3 → 4 (ms) | Uncached lookup+preview P95, schema 3 → 4 (ms) | Cached P95, schema 3 → 4 (ms) |
| --- | --- | --- | --- |
| 1 | 7.141 → 6.573 | 0.695 → 1.204 | 0.116 → 0.122 |
| 2 | 6.967 → 10.030 | 0.223 → 2.140 | 0.119 → 0.320 |
| 3 | 9.421 → 7.690 | 0.412 → 1.241 | 0.229 → 0.156 |

All paired startup and uncached P95 increments stay below 5 ms. Median open
P50 is about 6 ms in both formats. First access pays one block decode; cached
queries retain their existing path. The native PTY process peak was about
**80–82 MiB**, including startup. The 32 MiB entry-cache charge now counts owned
string/vector capacities; a separate 8 MiB budget bounds raw block caching.
PTY RSS does not claim to measure fully filled caches.

Asynchronous input-to-120×40-TestBackend-render P95 in schema 4 was **3.392,
7.470, 4.004 ms**, below the 50 ms target. This includes worker and drawing time,
not terminal-emulator display latency. Long `set`/`take`/`run` entries, scrolling,
find and example-expansion/resize remained responsive.

## Reproduction and checks

```sh
cargo build --locked --release --bins
target/release/dict-build --input CANONICAL_ENTRIES_JSONL \
  --source PROVENANCE_JSON --output NEW_PACK_DIRECTORY
target/release/wordglide --data NEW_PACK_DIRECTORY --verify-data
target/release/dict-bench --data NEW_PACK_DIRECTORY --iterations 100 \
  --queries FIXED_NEWLINE_QUERY_FILE
python3 scripts/package.py --pack NEW_PACK_DIRECTORY --target TARGET \
  --output NEW_OUTPUT_DIRECTORY
python3 scripts/terminal_smoke.py --data NEW_PACK_DIRECTORY
python3 scripts/terminal_interaction_smoke.py --data NEW_PACK_DIRECTORY
make check
```

Maintainers select their Python environment at invocation; the shipped program
needs neither Python nor an external compression executable.

Formatting, all-target Clippy, Rust tests/doc tests, cargo-deny policy, and Python
pipeline tests passed. Corruption checks include bad/truncated/concatenated
frames, malicious MessagePack lengths, invalid locators/directories, dictionary
mismatches, oversized entries, and damaged XZ footers/tails. Unsorted CRLF input,
multiple blocks, and repeated deterministic builds are exercised. A forbidden
runtime compression call was rejected by the boundary check; valid owners passed.
Independent review confirmed the storage checks and corrected cache accounting.

Full-data PTY lookup, word-form preview, reading interaction and terminal
restoration passed. Theme/preferences and no-data recovery checks also passed.
True Rust 1.88 all-target compilation and Intel macOS cross-target checking
passed; runtime testing was performed on native ARM64 macOS. Native Linux and
all four release builds remain CI checks and were not executed locally.

The new data archive is a local validated artifact. Public publication is
separate: publish schema 4 and its checksum first, then update `data-release.json`
with the real published tag before releasing the program. The existing pin still
accurately names the published schema-3 asset; schema 2/3 is not converted at runtime.
