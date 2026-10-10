# Wordglide

## Soul
Is: craft — an independently implemented offline incremental reading tool for terminal readers.
Is not: a learner-dictionary platform; a multi-dictionary or plugin manager; a networked or personalized service.
S1 Source-faithful over smoothed — rejects: rewriting definitions or inventing examples to fill gaps.
S2 Prebuilt offline over rebuilt or fetched — rejects: network access or index rebuilding during startup and lookup.
S3 Fixed deterministic ranking over personalization — rejects: personal word frequency or persistent per-user lookup history.
Status: building

Done when: type `ho`, see ranked completions and automatic definition preview;
look up `hosue`, `went`, `better`, and `take off`; follow a word in a definition
and return to the original query, selection, focus, and scroll position.
Delivery: Rust `wordglide [QUERY] [--data PACK_DIRECTORY] [--no-color]
[--no-mouse]`, plus `--info`, `--verify-data`, and explicit `--download-data`; macOS and Linux
terminals only. Windows is out of scope for now: no Windows target is built and
only POSIX terminals are exercised, so do not add Windows-only paths.
Appearance delivery also accepts `--theme NAME`, `--theme-background=true|false`,
and `--truecolor=true|false`; these overrides are session-only.
Quality: readable source-grounded English definitions, deterministic ranking;
no Enter to search. Startup loads only compact, prebuilt indexes. Warm-session input-to-draw P95
target <=50 ms, resident-memory target <=512 MiB on the full data pack.
Decisions: Wiktionary via raw Wiktextract, prepared packs, fixed wordfreq baseline;
up to 20 candidates; exact > valid inflection > prefix > one-edit fuzzy fallback.
Base score: round(100*Zipf) - 2*character count - 100*extra whitespace-separated
words - 150*hyphen count. Hyphens are U+002D and U+2010 in the normalized key;
keys and query normalization are unchanged. Scores are fixed at preparation.
Fuzzy: 3–64 query characters, insertion/deletion/substitution/adjacent swap.
Completion: fish-style Tab common prefix then fixed top-20 cycling; Shift+Tab
reverses; Esc restores original input; gray prefix suffix accepted with Right at
input end or Ctrl+F. Prediction keeps the selected strict prefix extension,
otherwise takes the highest-ranked strict prefix extension, preferring a plain
single word over a hyphenated or multi-word compound and hiding the suffix when
it does not score above an exact match of the query. Exact matches keep their
selection and preview. Enter accepts candidate and focuses reading; Ctrl+L
switches focus. Completion does not add lookup history or affect fixed ranking.
Mouse: left click inside the definition pane focuses it; clicking a visible word
there follows it when the pack contains it. The first click only focuses, so a
single click never navigates. Clicks in the input box return focus and position the cursor by grapheme
display columns; prediction clicks clamp to the end of real input. A candidate click selects and previews that row without
moving focus; clicking the highlighted candidate again accepts it and focuses
the definition. The wheel scrolls the definition from any focus; motion and
drag events are ignored and do not redraw. Only press and wheel reporting is
enabled, not `?1003h` motion tracking. `--no-mouse` disables reporting entirely
so terminals keep native text selection.
Reading keys: PageUp/PageDown move by one visible page (one line of overlap),
Home/End jump to the start/end, and no plain letter scrolls, so `j` and `k` stay
ordinary input characters. In the definition, `e` toggles compact/full examples
and references, `p` toggles short/full pronunciation, and `?` opens the key
help; `F1` opens it from either focus. Hints do not freeze the page: while `f`
hint mode is active the same keys and the wheel still scroll, and the hints are
rebuilt for the newly visible lines. Editing shortcuts (Ctrl+U/A/E/F, plus
Ctrl+W delete-word, Ctrl+K kill-to-end, Ctrl+Left/Right word motion) act only
while the input has focus; in the definition they are ignored and Ctrl+U does
not clear the query. Alt+Backspace also deletes a word; Alt+Left/Right step
history back and forward.
History: Ctrl+Z steps back through followed words and Ctrl+Y steps forward
again. A new lookup or follow clears the forward steps so redo never restores a
replaced state. History is session-only and capped in both directions.
Distribution: public Wordglide repository; two download types (program and shared
data). Explicit --data wins, then a non-empty WORDGLIDE_DATA,
then a managed download, then adjacent english-pack auto-discovery, then the
existing user-data directory. Missing, invalid, or incompatible dictionary data,
including discovery/receipt errors, opens the terminal session with diagnostic
and F2/CLI download guidance. Query editing and Settings work without a worker;
no lookup is queued until a dictionary is active. Explicit path errors never
silently fall back. --info and --verify-data remain strict non-TUI commands.
Normal startup and lookups never use the network.
Explicit --download-data conflicts with query, --data, --info, and --verify-data.
It fetches metadata once from the fixed public GitHub latest release endpoint,
then retrieves the archive and SHA256SUMS from that pinned release. Streamed
SHA-256, six whitelisted regular archive members, file lengths/checksums,
schema/ranking, and Dictionary::open must pass before atomic activation.
Installation uses a user-data advisory lock, immutable version directories,
and an atomic downloads/current.json pointer. Failure/cancellation cleans its
staging directory and preserves the previous pointer; historical packs remain.
The same hash and an openable installed pack are reused without downloading.
F2 Settings includes a Download / update dictionary command, outside the action
palette. Its modal owns keys, paste, and mouse; Esc cancels and Ctrl+C exits.
It shows stage, target, byte progress, and success/error with return and retry.
The CLI owns SIGINT handling; library download APIs use caller-owned cooperative
cancellation without changing signal handlers. Existing RunOptions stays compatible.
A first installation in a session without data opens the verified installed pack
and starts lookup of the current query immediately, preserving appearance and
reading preferences. Updates to an active dictionary leave its worker, query,
history, and reading position untouched. Restart selects the new pack unless
--data or WORDGLIDE_DATA overrides it. Failed/cancelled downloads leave Settings
available for retry.
Validation: schema 4 only; schema 2/3 are rejected with dictionary-update and
--data/WORDGLIDE_DATA override guidance. Normal open checks structure and file lengths, plus
the loaded FST buffer CRC. No default SHA scan, vocabulary traversal, or definition-block decompression. `wordglide --verify-data [--data DIRECTORY]` performs full
verification including every compressed entry and entry/index score, headword,
and POS agreement, reports the entry count and elapsed time, and exits without TUI.
`wordglide --info` prints pack path, schema, entry count, source snapshot,
provenance, and licenses.
Reading: POS/IPA/senses, at most two source examples per sense, no generated
text; obsolete/archaic senses last. The compact default shows at most two IPA
variants and the first example of each sense without references; `e` and `p`
expand them. Wrapped lines hang under their sense marker. The pane title names
the entry and shows scroll progress. Candidate panes size to their content, and
a definition retained during loading is dimmed. An error is shown as a banner
above the last good definition. Existing phrases included. Navigation history
persists as reading preferences and does not affect ranking.
Long entries: Ctrl+J/K scrolls one line in reading focus; arrows continue selecting
candidates. [/] jumps between source groups, and o opens the source-group outline.
F4 switches split/focus reading, default split. Focus hides only the candidate
pane while the definition is focused; input restores it. Text columns cap at 96.
/ searches only currently displayed source content with case-insensitive literal
matching. Input previews matches; Enter keeps, Esc restores the initial position;
n/N repeats cyclically. Follow hints supersede highlighting. New words clear find.
Layout/display changes and history restoration use source group/sense/text anchors;
folded examples return to their parent sense. Ctrl+R selects existing session-only
back/forward snapshots without logging typed queries or changing ranking.
Reading preferences: config.json also stores reading_layout (split/focus),
expand_examples, and expand_ipa, defaulting to split/false/false. F2 includes
these controls; e/p/F4 and menu changes auto-save through the same atomic,
field-specific retry mechanism. Queries, positions, find text, and navigation
snapshots never enter configuration. Public ReadingPreferences and ReadingLayout
are additive; existing run, RunOptions, and appearance APIs remain compatible.
Input cursor movement, deletion, and horizontal rendering respect grapheme
boundaries. Idle terminal polling uses a longer interruptible timeout; pending
lookup results keep the short polling interval. Cached reading rows are independent
of terminal colors and only current-document text is retained.
Action discovery: Ctrl+G/F3 opens a searchable fixed local-action menu. Disabled
operations show a reason. Enter/click executes; Esc preserves the lookup. Only
one overlay owns input, paste, and mouse at a time; queued completion waits for
it to close. Help scrolls with arrows, page keys, Home/End, and the wheel.
Candidate metadata includes sorted distinct original POS labels from every
source group, stored as interned binary lists. Candidate rows show up to two
abbreviated labels, then an ellipsis. At least eight columns remain for the
headword where available; summaries shrink or disappear before match markers.
Public Candidate.parts_of_speech is a Vec<String>; adding it breaks Rust struct
literal construction in 0.4.0, while missing serialized fields default to empty.
Wide candidate panes use 24% of terminal width clamped to 20–36 columns,
independent of results. Candidate markers and the status line distinguish exact,
prefix, word-form, and fuzzy matches; footer shortcuts are clickable. The menu
can toggle mouse capture for the session; --no-mouse sets its initial state.
Appearance: five built-in palettes (`default`, `orange`, `gruvbox_light`,
`gruvbox_dark_v2`, `whiteout`), the latter four adapted from btop for reading.
Headword emphasis is separate from POS/IPA; examples and sources use readable
secondary colors. Panes use rounded borders without changing layout density.
F2 opens a modal Settings panel from either focus, exclusive with key help.
Up/Down or the wheel select a setting; Left/Right, Space, or a row click change
preferences immediately. Enter/Space on the download row starts installation;
Esc and F2 close without reverting, and Enter closes preference rows. The panel swallows
lookup keys and paste; pending completions wait until overlays close. Query,
selection, reading position, focus, hints, and session history are preserved.
`theme_background` and `truecolor` default to true. Background off restores the
terminal base background but retains local selection and hint backgrounds;
default always inherits the terminal base colors. Truecolor off converts RGB
to nearest xterm fixed 256-color cube/grayscale entries; default ANSI colors
are retained. `--no-color` or non-empty `NO_COLOR` suppresses all explicit
foreground/background colors while preserving stored appearance preferences.
Configuration: platform user config directory from ProjectDirs, `config.json`,
appearance fields `color_theme`, `theme_background`, `truecolor`, plus the three
reading preferences described above.
Precedence is CLI overrides > saved preferences > defaults. Loading never
creates a file. Panel changes atomically save only edited fields, preserving
unknown JSON keys and unrelated CLI overrides. Save failure retains session
changes and reports "Not saved"; later edits retry all pending changes. Invalid
config or an unknown stored theme fails before raw mode and is never overwritten.
`--info` and `--verify-data` bypass appearance configuration. Preference file I/O
belongs to the terminal session, never the dictionary worker or data pack.

## Do not build
- N1 No network during normal startup or lookup, and no LLM dependencies.
  Only an explicitly requested prebuilt-pack installation uses the network ← S2
  — check: scripts/test_boundaries.py (network calls stay in src/download.rs);
  review: no LLM dependency appears in Cargo.toml.
- N2 No personal ranking or persistent lookup history ← S3
  — check: cargo test config::tests::reading_preferences_save_retry_and_preserve_cli_overrides.
- N3 No audio, images, cloud services, plugins, or multi-dictionary management ← Is not
  — review: no such dependency, command, or pane appears in the diff.
- N4 No rewriting definitions or inventing examples to fill source gaps ← S1
  — check: cargo test entry_codec::tests::roundtrip_preserves_every_source_field;
  check: scripts/test_prepare.py.
- N5 Prefix top-k must not enumerate and sort all prefix matches ← F2
  — check: cargo test index::tests::prefix_matches_exhaustive_oracle_with_ties;
  review: src/index.rs prefix walks the prebuilt range-max tree, not every key.
- N6 When an out-of-scope issue appears, record it below; do not implement it
  — review: the finding appears under "Found · Not doing" instead of in code.
- N7 No runtime candidate JSON parsing or rebuilding the ranking tree ← F1
  — check: scripts/test_boundaries.py (pack encoders are referenced only by src/build.rs).

## Frame
Compile source data to a local pack. Runtime indexes contain compact binary candidate metadata and prebuilt ranking;
definition text and necessary word-form targets are read on demand and cached within a byte budget.
Schema 4 stores fixed-order private MessagePack entries in independent,
checksummed Zstd-level-19 blocks of at most 1 MiB, with an up-to-64-KiB shared
compression dictionary trained deterministically at build time. Small packs
with fewer than 256 samples or 256 KiB of training input omit that dictionary.
Only build/prepare write data. Startup does not decompress entries. The private
codec bounds canonical JSON and binary entries to 1 MiB, compressed blocks to
2 MiB, Zstd window to 1 MiB, and MessagePack nesting to 16. It checks frame size,
completion/checksum, rejects tails, validates collection lengths before allocation,
and checks decoded keys. Parsed entries retain a 32 MiB cache budget charged
against owned collection/string capacities; uncompressed blocks have a separate
8 MiB cache. Full verification decompresses each block once and checks every entry.
The same-source installed pack target is <=190 MiB and download target <=140 MiB;
startup increment <=5 ms, uncached lookup+preview P95 increment <=5 ms,
input-to-draw P95 <=50 ms and resident-memory target <=512 MiB. Benchmark comparisons
use fixed queries and state OS-cache conditions rather than claiming cold disk.
The download envelope is a single XZ preset-9 tar archive, decoded only during
installation with a 128 MiB decoder-memory ceiling. Program archives remain gzip.

Pack members: manifest.json, entries.bin, entries.idx, lexicon.bin, words.fst,
and THIRD_PARTY.md (archive attribution). Manifest sizes/SHA-256 cover the four
binary files; source provenance, ranking and public JSON shapes are unchanged.
`entries.idx` uses a 32-byte little-endian header: magic WGLENT04, u32 entry
count, u32 block count, u32 dictionary length, zero u32 reserved, u64 body length.
Then dictionary bytes, 24-byte block records (u64 body offset; u32 compressed
length, raw length, first entry ID, entry count), and eight-byte entry locators
(u32 block ID, u32 raw offset). Entries are in sorted candidate-ID order; the next
offset or block length supplies each entry's end. Full verification requires
contiguous body coverage, unique contiguous entry coverage, and ordered locators.
The WGLIDX04 index retains its 32-byte header, with POS-combination count at byte
24 and zero reserved at 28. Each 24-byte candidate stores four u32 key/headword
range fields, i32 score, and u32 POS ID. Only internal ranking nodes are stored;
leaf winners are their candidate IDs or EMPTY. The POS table stores eight-byte
UTF-8 ranges before the shared string pool.
MessagePack tuple fields are frozen independently of public structs: entry
(key, headword, score, groups, lemmas, preview_lemmas, source_url); group
(headword, pos, ipa, senses, etymology_number); sense (glosses, tags, examples,
targets); example (text, reference, kind). No source field is omitted or rewritten.
F1 Only build and prepare write pack data; startup never decompresses entries or
rebuilds the ranking tree ← S2 — check: scripts/test_boundaries.py;
check: cargo test --doc.
F2 Indexed top-k equals exhaustive fixed-score ranking, and prefix search never
enumerates and sorts all matches ← S3
— check: cargo test index::tests::prefix_matches_exhaustive_oracle_with_ties;
review: src/index.rs prefix walks the prebuilt range-max tree.
F3 The private entry codec bounds raw entries/blocks to 1 MiB, compressed blocks to
2 MiB, and rejects truncated, tailed, checksum-failing, or malformed MessagePack streams ← S2
— check: cargo test entry_codec::tests; cargo test --test schema4.
F4 The installer copies verified prebuilt bytes and never calls the data
generator ← S2 — check: scripts/test_boundaries.py.
Oracle: compare indexed top-k with exhaustive fixed-score ranking in tests;
compare every source field after pack roundtrip with the canonical input.
Boundary check: compiler privacy/doc test; the installer copies verified prebuilt
bytes and never calls the data generator.

## Found · Not doing
- Windows support: no target and no WinAPI terminal paths, until POSIX shells are fully settled.
- Open source definitions and example coverage do not equal Oxford editorial quality.
- Word frequency cannot infer the relative frequency of senses within a word.
- Custom theme files and btop `.theme` imports are deferred; only built-in
  palettes are supported. No automatic light/dark detection or theme downloads.
- The download lifecycle is a set of correlated flags (`download_enabled`,
  `download_requested`, `download_cancelled`, `DownloadView.running`); collapse
  it into one `DownloadState` when it next needs another branch.

## Maintainer builds and release checks

`make build` cleans the selected Cargo target directory, then builds all release
binaries with locked dependencies. `make clean-all` additionally removes only the
repository's disposable artifacts/ and dist/ directories; data/ and user-installed
packs remain. Incremental Cargo commands remain available separately.
Ordinary CI checks main pushes and PRs with format, Python, and cargo-deny
license/source checks once, native macOS/Linux Rust and sample PTY checks, and
Rust 1.88 compilation. Tag releases
retain all four native targets, validating programs against the sample pack on
each platform. The shared complete dictionary is downloaded and verified once
at assembly, including full entry verification using the Linux x86_64 program.
Only program archives enter platform artifacts. Publication keeps four programs,
one shared dictionary and combined checksums; manual dispatch never publishes.

## Amendments
- 2026-10-09 Soul/Spirit: added the first `## Soul` block, N1–N7 lineage with
  checks, and F1–F4 Frame invariants; N1–N7 meaning is unchanged.
