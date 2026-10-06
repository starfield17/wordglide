# Local English dictionary

A small offline English–English dictionary for reading in a terminal. Type a
word or phrase and immediately see frequency-ranked candidates and the first
candidate's definition. Enter is never needed to search.

The implementation is independent. An optional `tuidict/` reference checkout
is excluded from this project's build and version control.
Definitions and examples come from Wiktionary, not an LLM. Open data has uneven
example coverage and is not equivalent to Oxford's learner-oriented editing.

## Try the real-data sample

Requires a Rust toolchain and a C toolchain for bundled SQLite. The project pins
Rust 1.99.0 for reproducible development and CI; package MSRV is 1.88, but that
older version has not been separately verified. Supported environments are
macOS and Linux terminals.

```sh
cargo build --release --bins
./target/release/dict-build \
  --input examples/sample/entries.jsonl \
  --source examples/sample/source.json \
  --output data/sample-pack
./target/release/dict --data data/sample-pack
```

The included 89-word sample demonstrates `ho`, `house`, `hosue`, `fist`, `bubble`,
`went`, `better`, `take`, `set`, and `take off`. It is intentionally small and
source-grounded; lookups outside it need the full pack. Existing output directories
are never overwritten: choose a new name when rebuilding.

You can start with a query: `dict "take off" --data data/sample-pack`.
`--data` points to an unpacked pack directory. Without it, the application looks
in the platform's user data directory under `dict/english` (the error message
prints the actual path). Copy an entire pack there or keep using `--data`.

## Keys

| Key | Action |
| --- | --- |
| Type / paste | Incremental lookup while input is focused |
| ↑ / ↓ or Ctrl+P / Ctrl+N | Select candidate; preview follows |
| PageUp / PageDown | Scroll definition |
| Tab | Switch input / definition focus |
| `j` / `k` in definition | Scroll one line |
| `f` in definition | Show two-letter hints on visible dictionary words |
| Hint letters | Follow that word, without Enter |
| Esc | Cancel hints or return input focus |
| Ctrl+O | Return to previous query, selection, focus, and scroll |
| Ctrl+U | Clear input for another lookup |
| Ctrl+C | Exit |
| ← / →, Home / End, Ctrl+A / Ctrl+E | Edit input position |

Hints highlight the first two letters of a visible word and do not shift the
text. Only words present in the local pack receive hints; one-letter tokens do
not. Navigation history lives only in the current session. Narrow terminals use
stacked panes; the minimum usable size is 30×10.

## Ranking and storage

Up to 20 candidates: exact match, valid word-form targets, prefix completions,
then one-edit fuzzy fallback. Fuzzy handles an inserted, missing, wrong, or
swapped adjacent character for queries of 3–64 characters. It compares complete
words/phrases; it does not provide fuzzy prefix completion.

The fixed base score is `100*Zipf - 2*characters - 100*extra_words`, ties broken
by normalized key. Wordfreq is queried only while building data. Runtime scores
never depend on your lookups. Short phrases receive a fixed penalty because
their wordfreq estimate is not a measured phrase count.

The sorted vocabulary and range-max tree return prefix top-k without sorting all
prefix matches. An FST handles fuzzy matching. A read-only SQLite database stores
structured words, parts of speech, pronunciation, senses, source examples, and
word forms. A bounded cache holds visited entries. A worker coalesces pending
queries; response IDs stop obsolete previews from overwriting newer input.

## Build the full data pack

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
./target/release/dict --data data/english-pack
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
make check                 # or make check PYTHON=/path/to/python
python3 scripts/terminal_smoke.py --data data/sample-pack
./target/release/dict-bench --data data/english-pack --iterations 100
python3 scripts/package.py --pack data/english-pack --output dist
```

Benchmarks distinguish first application-cache reads, warm lookup/preview time,
and asynchronous input-to-TestBackend render time at 120×40. OS file caches are
not flushed. The renderer measurement excludes terminal emulator display latency.
Full-pack target: P95 input-to-render ≤50 ms and resident memory about ≤512 MiB;
startup time is not a target. Record terminal size, data snapshot, and measurement
method with results. Small-sample results
do not establish full-pack performance. See [PERFORMANCE.md](PERFORMANCE.md)
for the full-data verification.

CI builds/tests on macOS and Linux and uploads each platform's executable tools.
The packaging command produces a separate offline data archive and host binary
archive, with SHA-256 receipts. No hosted release or public download URL is
configured yet. Update packs by unpacking to a new directory and restarting the
app with `--data`; the app does not download or mutate packs.

Code: MIT. Data: its original licenses and attribution; see [THIRD_PARTY.md](THIRD_PARTY.md).
