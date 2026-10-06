# Full-data verification

Reference measurements use the full English data pack.
They establish one measured run; repeat the commands in deployment environments.

## Data

- Source: English Wiktionary via [raw Kaikki/Wiktextract](https://kaikki.org/dictionary/rawdata.html).
- Pack format: schema 2, compact prebuilt candidate/ranking index.
- Source dump: 2026-09-02; preparation date: 2026-10-06.
- Compressed input SHA-256: `3dac8a09e57827bef493e2e6b552fe917bf3b1fc55dee05c4e4a3702aff5dbdb`.
- 1,355,084 normalized entries; 1,491,592 part-of-speech groups; 1,785,990 senses.
- 382,357 senses have source examples; 297,854 have group pronunciation data.
- One malformed source record was quarantined with its original text and reason.

The manifest carries checksums, source provenance, quality counts, and licenses.
Definitions and examples retain source wording. Source labels and coverage are
uneven; these counts do not establish learner-dictionary editorial quality.

## Method

```sh
./target/release/dict-bench --data PACK_DIRECTORY --iterations 100
python3 scripts/terminal_smoke.py --data PACK_DIRECTORY
python3 scripts/terminal_smoke.py --data PACK_DIRECTORY \
  --query went --needle '(word form)'
```

Lookup measurements include candidate ranking and the top entry's definition
preview. Each category first opens a fresh dictionary cache, then measures 100
warm iterations. OS file caches are not flushed.

The asynchronous measurement alternates `ho` and `hosue`, waits for the worker,
and draws a 120×40 Ratatui TestBackend. It includes rendering work and excludes
terminal emulator display latency. The PTY checks type without Enter, verify
source preview text, exit with Ctrl+C, and check terminal restoration. Their
peak process resident memory includes startup; they do not exercise a completely
filled definition cache. That cache has a separate 32 MiB budget.

The CI matrix checks macOS and Linux. Release validation additionally builds
Intel/ARM64 macOS and x86_64/ARM64 Linux packages, checks runtime dependencies,
and exercises full-data bundles from an unrelated working directory without
a dictionary path argument.

## Startup comparison

Three successive release-program launches open the same full vocabulary and wait
for the terminal alternate screen to become active, with no initial query. OS
file caches are not flushed; these are warm-cache measurements, not cold-disk
claims. The PTY timing includes process launch and pack opening.

| Format | Startup to terminal UI, three runs (ms) | Median (ms) |
| --- | --- | ---: |
| Old JSON/runtime-built index | 2976.015, 2435.648, 2433.293 | 2435.648 |
| Compact prebuilt index | 11.754, 10.405, 9.784 | 10.405 |

The new benchmark reports dictionary-open time separately: **20.756 ms** in its
measured run. It excludes process launch and terminal setup. Index data occupies
about **65.7 MiB**; the SQLite definition database is read on demand. Normal open
performs no dictionary SHA scan or vocabulary traversal; the small FST buffer is
CRC-checked before use. Complete SHA/index/database verification passed separately.

## Results

| Lookup | Query | First cache read (ms) | Warm P50 (ms) | Warm P95 (ms) |
| --- | --- | ---: | ---: | ---: |
| Single letter | `h` | 0.212 | 0.005 | 0.007 |
| Prefix | `ho` | 0.115 | 0.003 | 0.004 |
| Exact | `house` | 0.077 | 0.003 | 0.003 |
| Fuzzy | `hosue` | 0.259 | 0.089 | 0.097 |
| Word form | `went` | 0.133 | 0.003 | 0.003 |
| Phrase | `take off` | 0.183 | 0.091 | 0.098 |

- Asynchronous input-to-render P95: **3.952 ms**; target: ≤50 ms.
- Peak resident memory: **71.0 MiB** (`fist`), **71.4 MiB** (`went`);
  target: ≤512 MiB. The previous format measured about 164 MiB.
- Both real-PTY checks passed: automatic preview without Enter, successful
  exit, restored terminal attributes, and restored alternate screen.
- Formatting, Clippy with warnings denied, 24 Rust tests, one compile-fail
  boundary doc test, and ten Python pipeline tests passed.

Full-pack checks confirm `went → go`, `better → good / well`, and
`chose → choose`. `house` and `fist` do not gain unrelated inverse aliases;
historical-only groups cannot supply modern inverse inflection links.
