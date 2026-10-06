# Full-data verification

Reference measurements use the full English data pack.
They establish one measured run; repeat the commands in deployment environments.

## Data

- Source: English Wiktionary via [raw Kaikki/Wiktextract](https://kaikki.org/dictionary/rawdata.html).
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

## Results

| Lookup | Query | First cache read (ms) | Warm P50 (ms) | Warm P95 (ms) |
| --- | --- | ---: | ---: | ---: |
| Single letter | `h` | 0.098 | 0.002 | 0.003 |
| Prefix | `ho` | 0.072 | 0.002 | 0.002 |
| Exact | `house` | 0.081 | 0.001 | 0.002 |
| Fuzzy | `hosue` | 0.246 | 0.075 | 0.082 |
| Word form | `went` | 0.128 | 0.002 | 0.002 |
| Phrase | `take off` | 0.275 | 0.089 | 0.099 |

- Asynchronous input-to-render P95: **4.091 ms**; target: ≤50 ms.
- Peak resident memory: **163.7 MiB** (`fist`), **164.1 MiB** (`went`);
  target: ≤512 MiB.
- Both real-PTY checks passed: automatic preview without Enter, successful
  exit, restored terminal attributes, and restored alternate screen.
- Formatting, Clippy with warnings denied, 18 Rust tests, one compile-fail
  boundary doc test, and ten Python pipeline tests passed.

Full-pack checks confirm `went → go`, `better → good / well`, and
`chose → choose`. `house` and `fist` do not gain unrelated inverse aliases;
historical-only groups cannot supply modern inverse inflection links.
