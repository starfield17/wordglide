# Changelog

## 0.4.2

- Schema 4 replaces SQLite and per-entry zlib with flat entry storage,
  independent checksummed Zstd blocks, a trained compression dictionary, and
  lossless fixed-order MessagePack. Compact POS IDs and implicit ranking leaves
  reduce the search index while preserving the public Rust API and JSON shape.
- The same full dictionary now occupies about 184 MiB locally instead of
  608 MiB. The shared download switches to `english-pack.tar.xz`, about 127 MiB
  instead of 429 MiB; program archives remain gzip.
- Lookup decodes only the requested block, with bounded raw-block and entry
  caches. Full-data paired startup and uncached lookup P95 increases remained
  below 5 ms; all 1,355,084 entries were checked against canonical source data.
- Installation verifies XZ integrity, archive and member hashes, and pack
  structure before atomic activation. Corrupt frames, locators, dictionaries,
  malicious wire lengths, and damaged archive footers have regression coverage.
- `dict-bench --queries FILE` supports reproducible paired query measurements;
  PERFORMANCE.md records full-data sizes, latency, and validation limits.

Schema 2/3 packs are rejected. After updating the program, download the new
dictionary with F2 Settings or `wordglide --download-data`. Explicit `--data`
and `WORDGLIDE_DATA` overrides must point to a schema-4 pack. Older installed
packs remain on disk; old program/data pairs can still be used separately.

## 0.4.1

- The spec gains a Soul block and per-entry lineage: N1–N7 name a check or
  review, the Frame invariants are F1–F4, and `scripts/test_boundaries.py`
  enforces the boundary rules the compiler cannot express.
- `make check` runs `cargo deny check` with `deny.toml`, gating dependency
  license, advisory, and source policy.
- Normalization cases are shared between `scripts/prepare.py` and the runtime
  through `tests/fixtures/normalize.json`, so the two normalizers cannot drift.
- Startup pack errors reuse one hint, restoring rustfmt coverage of the open path.
- CI separates shared source checks from native program checks, adds MSRV
  compilation and Cargo caches, and verifies the complete shared dictionary once
  during release assembly. Platform artifacts contain only program archives.
- `make build` cleans Cargo outputs before building locked release binaries.
  `make clean-all` also removes disposable verification and packaging outputs.

No runtime or public API changes; the schema-3 dictionary and all CLI flags are
unchanged. New dependency-policy checks require `cargo-deny` for `make check`.

## 0.4.0

- Startup without a usable dictionary opens Settings and download guidance. The
  first F2 download enables lookup immediately; active-dictionary updates retain
  the current reading session until restart.
- Releases provide program archives and a shared dictionary archive. Combined
  with-data program bundles are no longer generated.

- Schema 3 stores independently compressed zlib definitions in SQLite and
  decompresses only requested entries, with bounded input/output and full-stream
  checks. Full verification also checks every entry against candidate metadata.
- Fixed ranking deducts 150 points per ASCII/Unicode hyphen (U+002D/U+2010),
  reducing inflated compound estimates while preserving exact-match priority.
- Candidate rows show compact source parts of speech directly from the prebuilt
  index, preserving single-row selection and mouse behavior in narrow layouts.
- Public `Candidate.parts_of_speech: Vec<String>` contains sorted distinct source
  labels. Rust callers constructing candidates must supply this new field;
  deserialization of missing fields defaults to an empty list.
- Schema 2 packs and old prepared ranking receipts are rejected. Install a new
  dictionary and check explicit data-path overrides. `scripts/prepare.py --prepared`
  can re-rank previous canonical data without altering source definitions/examples.

No new dependencies. Existing run, appearance, and reading interfaces remain
compatible. Old program/data pairs remain usable separately.

## 0.3.1

- First crates.io release; install with `cargo install wordglide --locked`.
- Explicit `wordglide --download-data` installs the latest prepared dictionary
  with progress, SHA-256 and file verification, safe extraction, and atomic
  activation. Ordinary startup and lookup remain offline.
- F2 Settings adds dictionary download/update with cancellation and retry.
  The current session retains its dictionary and reading state; restart to use
  the downloaded version. Existing `--data` and `WORDGLIDE_DATA` overrides win.
- Managed installations reuse an unchanged archive and retain older packs.
  Failed and cancelled installations preserve the previous active dictionary.
- Rust 1.88 compatibility verified; the crate excludes full dictionary assets
  and maintainer-only scripts while including the 89-word example.

Schema 2 packs remain compatible. Compression and selective runtime
unpacking remain a TODO; this release does not change dictionary storage.

## 0.3.0

- Searchable action menu (`Ctrl+G` / `F3`) with shortcuts, current settings,
  unavailable-action explanations, and mouse support.
- Scrollable contextual help (`F1`), stable candidate pane widths, match-kind
  markers, and a clickable shortcut footer with lookup and navigation status.
- Focused reading layout (`F4`) with a centered, bounded text width. Input focus
  restores the candidate pane.
- Literal, case-insensitive definition search (`/`), highlighted matches,
  cyclic next/previous navigation (`n` / `N`), and cancel-to-restore behavior.
- Definition outline (`o`), previous/next word-group navigation (`[` / `]`),
  and line scrolling (`Ctrl+J` / `Ctrl+K` while reading).
- Searchable session navigation (`Ctrl+R`) restores existing back/forward
  locations. Reading positions stay attached to source content through resizing,
  layout changes, expansion, and navigation.
- Reading layout, example/reference expansion, and pronunciation expansion now
  save automatically alongside appearance settings. `F2` controls all six
  preferences; failed writes remain visible and can be retried by another change.
- Grapheme-aware input movement, deletion, horizontal scrolling, and mouse caret
  placement, including combining marks and joined emoji.
- Mouse capture can be toggled from the action menu for native text selection.
- Cached reading layout and viewport rendering improve scrolling and reflow;
  the benchmark includes long-definition reading interactions.

The existing schema 2 dictionary pack remains compatible. Queries and navigation
history remain session-only. No new runtime dependencies or CLI flags are required.
