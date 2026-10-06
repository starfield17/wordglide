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

The configured CI matrix covers macOS and Linux. Remote CI execution and public
release hosting remain outside this verification run.

## Results

| Lookup | Query | First cache read (ms) | Warm P50 (ms) | Warm P95 (ms) |
| --- | --- | ---: | ---: | ---: |
| Single letter | `h` | 0.137 | 0.002 | 0.003 |
| Prefix | `ho` | 0.076 | 0.002 | 0.002 |
| Exact | `house` | 0.077 | 0.001 | 0.002 |
| Fuzzy | `hosue` | 0.238 | 0.076 | 0.083 |
| Word form | `went` | 0.151 | 0.002 | 0.002 |
| Phrase | `take off` | 0.253 | 0.086 | 0.095 |

- Asynchronous input-to-render P95: **4.367 ms**; target: ≤50 ms.
- Peak resident memory: **163.6 MiB** (`fist`), **164.1 MiB** (`went`);
  target: ≤512 MiB.
- Both real-PTY checks passed: automatic preview without Enter, successful
  exit, restored terminal attributes, and restored alternate screen.
- Formatting, Clippy with warnings denied, 11 Rust tests, one compile-fail
  boundary doc test, and seven Python pipeline tests passed.

Full-pack checks confirm `went → go`, `better → good / well`, and
`chose → choose`. `house` and `fist` do not gain unrelated inverse aliases;
historical-only groups cannot supply modern inverse inflection links.
