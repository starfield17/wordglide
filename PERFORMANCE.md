# Full-data verification

Reference measurements use the full English data pack.
They establish one measured run; repeat the commands in deployment environments.

## Data

- Source: English Wiktionary via [raw Kaikki/Wiktextract](https://kaikki.org/dictionary/rawdata.html).
- Pack format: schema 3, prebuilt candidates/POS/ranking and independent zlib entries.
- Source dump: 2026-09-02; preparation date: 2026-10-06; re-ranking/build: 2026-10-08.
- Compressed input SHA-256: `3dac8a09e57827bef493e2e6b552fe917bf3b1fc55dee05c4e4a3702aff5dbdb`.
- 1,355,084 normalized entries; 1,491,592 part-of-speech groups; 1,785,990 senses.
- 382,357 senses have source examples; 297,854 have group pronunciation data.
- One malformed source record was quarantined with its original text and reason.

The manifest carries checksums, source provenance, quality counts, and licenses.
Definitions and examples retain source wording. Source labels and coverage are
uneven; these counts do not establish learner-dictionary editorial quality.

Every one of the 1,355,084 canonical entries was compared against the schema-2
preparation. All fields except score are identical; 70,730 scores changed by
exactly 150 per U+002D/U+2010 hyphen. The prior wordfreq baseline and source
snapshot were retained. Source receipts record the prior canonical SHA-256.

## Installed size and archive trade-off

| Same source snapshot | Schema 2 bytes | Schema 3 bytes |
| --- | ---: | ---: |
| Installed pack (manifest, database, index, FST) | 1,224,210,425 | 637,533,325 |

Schema 3 is **52.1%** of the old installed size, passing the <=70% target.
The portable data archive is **449,923,088 bytes** (about 429 MiB), versus the
previous roughly 190 MiB archive. Independent zlib streams reduce installed
storage but sacrifice cross-entry redundancy in the outer gzip archive, so this
release's download is larger. No claim is made that per-entry compression also
reduces download bytes.

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

The CI matrix is configured to check macOS and Linux. Release validation builds
Intel/ARM64 macOS and x86_64/ARM64 Linux packages, checks runtime dependencies,
and exercises packaged programs with the sample dictionary on each platform.
Release assembly verifies the separate full dictionary once with the Linux
x86_64 program and checks automatic discovery from an unrelated directory. This local schema-3 run validates the native macOS
target; the other three release targets require CI and were not executed locally.
Windows is not a target.

## Historical startup comparison (schema 2)

Three successive release-program launches open the same full vocabulary and wait
for the terminal alternate screen to become active, with no initial query. OS
file caches are not flushed; these are warm-cache measurements, not cold-disk
claims. The PTY timing includes process launch and pack opening.

| Format | Startup to terminal UI, three runs (ms) | Median (ms) |
| --- | --- | ---: |
| Old JSON/runtime-built index | 2976.015, 2435.648, 2433.293 | 2435.648 |
| Compact prebuilt index | 11.754, 10.405, 9.784 | 10.405 |

The schema-2 benchmark reported dictionary-open time separately: **20.756 ms**.
The current schema-3 run reports **15.388 ms**, excluding process launch and
terminal setup. Schema-3 index and FST data occupy about **76 MiB**. Normal open
performs no dictionary SHA scan or vocabulary traversal; the small FST buffer is
CRC-checked before use. Complete SHA/index/database verification passed separately.

## Schema-3 results

| Lookup | Query | First cache read (ms) | Warm P50 (ms) | Warm P95 (ms) |
| --- | --- | ---: | ---: | ---: |
| Single letter | `h` | 0.874 | 0.009 | 0.018 |
| Prefix | `ho` | 0.556 | 0.004 | 0.004 |
| Exact | `house` | 0.454 | 0.003 | 0.003 |
| Fuzzy | `hosue` | 0.280 | 0.084 | 0.089 |
| Word form | `went` | 1.180 | 0.003 | 0.003 |
| Phrase | `take off` | 0.856 | 0.095 | 0.121 |

- Asynchronous input-to-render P95: **3.695 ms**; target: ≤50 ms.
- Peak resident memory: about **82 MiB**; target: ≤512 MiB.
- Complete checksum/index/database/entry verification passed: **13.8 s**.
- Real PTY startup to UI: **55.942 ms** for the recorded `fist` run, including
  process launch and the smoke script's 50 ms polling interval; no cold-disk claim.
- Long-entry `set`/`take`/`run` scrolling P95: 0.170–0.320 ms;
  resize plus example expansion P95: 1.266–5.380 ms.
- Both real-PTY checks passed: automatic preview without Enter, successful
  exit, restored terminal attributes, and restored alternate screen.
- Reading-interaction PTY checks passed on the full pack. Appearance PTY checks
  passed on the 89-entry sample, including configuration bypass verification.
  Full-pack verification is checked separately: its 13.8 s duration exceeds the
  appearance harness's five-second subprocess limit, which remains unchanged.
- Missing/invalid-data PTY checks passed without a data argument and with explicit
  overrides: query editing, F2 download access, old-format/corrupt-pack diagnostics,
  invalid managed-receipt guidance, and terminal restoration. Local HTTP tests
  verify damaged receipts can be repaired while preserving them on failure/cancellation.
- Formatting, Clippy with warnings denied, cargo-deny (advisories, bans,
  licenses, sources), 126 Rust tests, two compile-fail boundary doc tests, and
  25 Python pipeline tests passed.

Full-pack terminal checks confirm `home` precedes `how-to`, `household` precedes
`house-like`, exact `house-like` remains selected, and source POS summaries appear.
They also confirm `went → go`, `better → good / well`, and `take off`.
`house` and `fist` do not gain unrelated inverse aliases;
historical-only groups cannot supply modern inverse inflection links.
