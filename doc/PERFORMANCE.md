# Schema-5 full-data verification

Wordglide 0.5 uses the 2026-09-02 English Wiktionary snapshot, re-prepared from
raw Wiktextract for the five source-content additions. Schema 4 is a measurement
baseline only; the new program rejects older packs. Measurements describe the
local native ARM64 macOS runs on 2026-10-11; repeat on deployment targets.
OS file caches were not flushed.

## Data and source-content verification

- 1,355,084 normalized entries; 1,491,592 POS groups; 1,785,990 senses.
- 382,357 senses have source examples; 297,854 have group pronunciation data.
- 535,412 groups have etymology text; 579,907 have source forms.
- 229,401 groups have term relations; 115,307 senses have synonyms/antonyms.
- 205,454 pronunciation records have source labels or notes.
- Compressed source SHA-256:
  `3dac8a09e57827bef493e2e6b552fe917bf3b1fc55dee05c4e4a3702aff5dbdb`.
- Preparation examined 10,913,997 records and rejected one malformed English
  record without a valid word, the same rejection as the previous preparation.
- An independent Python MessagePack/Zstd reader compared every field of all
  1,355,084 packed entries against canonical JSONL, including source ordering,
  new fields, pronunciation projection and scores: no differences, **9.573 s**.
- `wordglide --verify-data` checked file hashes, index/FST agreement, ranking,
  block/locator coverage and every decoded entry: **2.6 s** in the final idle run.
  Normal startup does not perform this full scan.

## Installed and download size

| Same source snapshot | Schema 4 baseline | Schema 5 |
| --- | ---: | ---: |
| Pack binaries and manifest, bytes | 192,989,452 | 222,047,750 |
| Pack binaries and manifest, MiB | 184.05 | 211.76 |
| Shared data archive, bytes | 133,568,716 | 162,696,252 |
| Shared data archive, MiB | 127.38 | 155.16 |

Source completeness takes priority over the former 190/140 MiB size goals.
The richer pack adds approximately 27.71 MiB installed and 27.78 MiB downloaded.
Archive attribution adds a few KiB when extracted; historical installations are
not included in these per-pack figures.

| Schema-5 component | Bytes |
| --- | ---: |
| Independent Zstd blocks, `entries.bin` | 145,230,008 |
| Dictionary, directory and locators, `entries.idx` | 10,918,984 |
| Compact candidate/POS/ranking index, `lexicon.bin` | 58,189,134 |
| Exact/fuzzy FST, `words.fst` | 7,704,412 |
| `manifest.json` | 5,212 |

Entry tuples use fixed-order MessagePack and independent checksummed Zstd-level-19
frames with at most 1 MiB of raw data. The entry header is `WGLENT05`; the
unchanged candidate index retains its independent `WGLIDX04` format. Download
archives use XZ preset 9; program archives use gzip. The validated local data
archive SHA-256 is
`f1b2a5dd3bd6d93dfb8986ac17245f49e86cf28ce624c75bf70512a4f377fa9a`.

## Lookup and reading latency

A sequential paired run, after build/packaging work finished, used 1,010 fixed
queries: 1,000 vocabulary keys sampled by reservoir sampling with seed 1729,
plus `h`, `ho`, `house`, `hosue`, `went`, `better`, `take off`, `set`, `take`, `run`.
Each query opened a fresh Dictionary before timing search plus preview, then
measured the cached lookup. OS caches remained warm.

| Metric | Schema 4 baseline | Schema 5 |
| --- | ---: | ---: |
| Open P50 / P95 (ms) | 5.750 / 6.262 | 5.651 / 6.187 |
| Uncached lookup + preview P50 / P95 (ms) | 0.858 / 1.156 | 0.806 / 0.866 |
| Cached lookup + preview P95 (ms) | 0.117 | 0.114 |
| Async input-to-120×40-TestBackend-render P95 (ms) | 4.031 | 3.755 |

Startup and uncached-preview P95 increments remain below 5 ms. The async result
is below the 50 ms target and includes worker and drawing time, not terminal
emulator display latency. A separate idle 100-iteration run measured async P95
at 4.200 ms. Runs concurrent with builds were noisier (7.021 ms async P95) and
are not used for paired deltas.

Schema-5 first draws of long `set`/`take`/`run` entries took 1.593/0.808/0.873 ms;
example-expansion plus resize P95 took 1.431/1.508/1.486 ms. Native PTY startup
to terminal UI took 56.073 ms; peak process resident memory was **81.0 MiB**,
including startup. This does not claim to measure fully filled caches. Owned
entry allocations are charged to the 32 MiB entry cache, while a separate 8 MiB
budget bounds raw block caching.

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
python3 scripts/terminal_content_smoke.py --data NEW_PACK_DIRECTORY
python3 scripts/terminal_interaction_smoke.py --data NEW_PACK_DIRECTORY
make check
```

Maintainers select their Python environment at invocation; the shipped program
needs neither Python nor an external compression executable.

`make check` passed formatting, all-target Clippy, 147 Rust unit/integration/doc
tests, cargo-deny advisory/license/source policy and 28 Python pipeline tests.
True Rust 1.88 all-target compilation passed. Schema-5 codec tests cover richer
roundtrips, bounded lengths, unknown relation kinds, pronunciation invariants,
corrupt frames/locators and rejection of schemas 2/3/4. Packaging validates the
full schema-5 archive and member hashes; installer regression tests cover verified
installation, reuse, atomic activation and invalid archives.

Actual full-data PTY usage covered `go`, `went`, `better`, `house`, `take`, `set`,
content expansion, labeled IPA and page navigation. The new content smoke also
checked sense synonyms, whole-phrase mouse follow/back, help, session-only toggles
and terminal restoration. Existing interaction, appearance and no-data recovery
PTY checks passed. Independent review found and corrected a source-link offset
collision; regression coverage includes labels, tabs, multiline phrases and
narrow wrapping. Linux and the four-platform release matrix remain CI checks
and were not executed locally.

## Publication status

The schema-5 archive above is validated locally and has not been published.
`data-release.json` still pins the previously published schema-4 asset. Publish
and verify the new data asset, then update that pin with its real tag/checksum
before releasing the 0.5 program. Until then, use a locally built schema-5 pack
with `--data`; the public downloader cannot supply the new pack yet.
