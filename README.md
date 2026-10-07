# Wordglide

See [CHANGELOG.md](CHANGELOG.md) for release changes.

A small offline English–English dictionary for reading in a terminal. Type a
word or phrase and immediately see frequency-ranked candidates and the first
candidate's definition. Enter is never needed to search.

Definitions and examples come from Wiktionary, not an LLM. Open data has uneven
example coverage and is not equivalent to Oxford's learner-oriented editing.

## Download and run

Download the **with-data** archive matching your platform from
[GitHub Releases](https://github.com/starfield17/wordglide/releases/latest).
It includes the program and the full 1,355,084-entry English dictionary.
Extract it and run:

```sh
./wordglide/wordglide
```

No Rust, Python, database installation, network connection, or dictionary-path
configuration is required to use a release bundle. Supported release targets:

| Platform | Target in archive name |
| --- | --- |
| macOS 11+ Apple Silicon | `aarch64-apple-darwin` |
| macOS 11+ Intel | `x86_64-apple-darwin` |
| Linux x86_64 | `x86_64-unknown-linux-musl` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |

Windows is not supported at this stage. No Windows target is built and only
POSIX terminals are exercised.

Each release provides three download types:

- **Program only:** `wordglide-vVERSION-TARGET.tar.gz`.
- **Dictionary only:** `english-pack.tar.gz`, shared by all platforms.
- **Ready-to-run bundle:** `wordglide-vVERSION-TARGET-with-data.tar.gz`.

For separate downloads, extract the program, then extract the dictionary into
the resulting `wordglide/` directory. `english-pack/` must sit beside the
`wordglide` executable. Verify downloads using the release's `SHA256SUMS.txt`.
Linux executables use static musl linking; macOS executables use system libraries.

## Install from crates.io

Requires Rust 1.88 or newer and a C compiler for bundled SQLite:

```sh
cargo install wordglide --locked
wordglide --download-data
wordglide
```

The crate contains the program and a small build example. The full dictionary is
installed separately. `--download-data` explicitly contacts GitHub Releases,
streams the archive, checks SHA-256 and the pack files, and atomically activates
the verified pack. Esc cancels an interactive download. Normal startup and
lookups stay offline; missing data prints the download command.

**F2 Settings → Download / update dictionary…** performs the same operation with
progress and cancellation. The current session keeps its open dictionary,
query, history, and reading position. Restart to use the newly installed pack.
Updates happen only when requested. A valid installation of the same archive is
reused. Failed or cancelled installations keep the previous dictionary.

Downloads live in the platform user-data directory under `downloads/packs`,
with `downloads/current.json` selecting an immutable pack. Linux uses
`$XDG_DATA_HOME/dict` (or `~/.local/share/dict`); macOS uses
`~/Library/Application Support/org.wordglide.dict`. Older installed versions are
retained; allow roughly 190 MiB for the download and 1.14 GiB for each unpacked
full dictionary. The data's attribution and licenses ship in its `THIRD_PARTY.md`.
The program is MIT licensed; the dictionary keeps its separate source licenses.

## Build and try the real-data sample

Requires a Rust toolchain and a C toolchain for bundled SQLite. The project pins
Rust 1.99.0 for reproducible development and CI; package MSRV is 1.88,
verified with the packaged source and tests. Supported environments are
macOS and Linux terminals; Windows is out of scope for now.

```sh
cargo build --release --bins
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

Add `--no-color` (or set `NO_COLOR`) for a color-free rendering, and `--no-mouse`
to keep native terminal text selection.

## Finding actions

Press **Ctrl+G** or **F3** to search available actions. Type or paste a filter,
use ↑/↓ to select, and press Enter or click a row to run it. Unavailable actions
show their reason; Esc cancels without changing your lookup. Existing shortcuts
still work. F1 help scrolls with ↑/↓, PageUp/PageDown, Home/End, and the wheel.
The bottom line's shortcuts are clickable.

The candidate column stays the same width as you type. `=` marks exact matches,
`→` word forms, and `≈` spelling suggestions. The status line shows focus,
match type, candidate position, and available back/forward locations.

The action menu also toggles mouse capture for the current session, allowing
native terminal selection without restarting. `--no-mouse` selects its initial
state. Idle sessions redraw only when input, a result, or the terminal size changes.

## Reading long entries

In reading focus, **/** finds text in the displayed definition. Type or paste a
literal, case-insensitive query, then Enter to keep it or Esc to restore your
previous reading position. ↑/↓ in the search prompt moves through matches;
**n/N** repeats after closing it. Hidden examples and IPA are not searched.
Changing words clears the search. Follow hints temporarily take precedence over
search highlighting.

Press **o** for the word / part-of-speech outline, or **[ / ]** for the previous /
next group. **Ctrl+J/K** scrolls one line down/up; ↑/↓ continues selecting
candidates. PageUp/PageDown and the wheel retain their existing behavior.
**F4** toggles split / focused reading. Focused reading hides candidates while
you read; returning to input shows them again. Lines are at most 96 columns.

Resizing, expanding examples/IPA, and switching layouts retain your current
source sense where possible. Collapsing an example you were reading returns to
its sense. **Ctrl+R** searches this session's existing back/forward locations;
Enter or a click restores the query, candidate, focus, and reading position.
It does not save your queries or add a log of everything you type.

## Settings and appearance

Press **F2** from either pane to open Settings. Use ↑/↓ to choose a
setting and ←/→ or Space to change it; clicking a setting changes it, and the
wheel selects a setting. Changes apply immediately and save automatically.
Layout, examples/references, and IPA preferences also save automatically, including
changes made with F4, e, and p. Esc or F2 closes the panel and keeps your choices. Enter closes a preference row;
on the download row, Enter or Space starts the download. Lookup, selection,
reading position, and navigation history are preserved.

Five built-in palettes are available: `default` (terminal colors with cyan
accents), `orange` (black and warm amber), `gruvbox_light` (cream),
`gruvbox_dark_v2` (warm dark gray), and `whiteout` (white with blue accents).
The latter four are reading-oriented adaptations of
[btop palettes](https://github.com/aristocratos/btop/tree/main/themes).
Headwords are emphasized separately from part of speech and IPA; examples and
source references use secondary colors. Panes have rounded borders.

**Theme background** uses the palette's background when on; off uses your
terminal's background, including any transparency configured in the terminal.
Selected rows and follow hints retain their local backgrounds. The `default`
palette always uses the terminal background. Dark text from a light
palette can be hard to read on a dark terminal background, and vice versa;
choose a palette that matches your terminal when theme background is off.

**Truecolor** uses RGB colors when on; off converts palette colors to xterm 256
colors. Both switches default to on. The default palette uses ANSI colors in
either mode. These per-session arguments override saved preferences:

```sh
wordglide --theme gruvbox_dark_v2
wordglide --theme orange --theme-background=false --truecolor=false
```

`--no-color` and non-empty `NO_COLOR` take priority over appearance preferences:
the screen uses terminal colors, bold, italics, and reverse video. F2 still lets
you save preferences for a future colored session.

Preferences live in `config.json` in the platform user configuration directory:
`$XDG_CONFIG_HOME/dict` (or `~/.config/dict`) on Linux, and
`~/Library/Application Support/org.wordglide.dict` on macOS. The file is created
only when you change a setting. Appearance and reading defaults are:

```json
{
  "color_theme": "default",
  "theme_background": true,
  "truecolor": true,
  "reading_layout": "split",
  "expand_examples": false,
  "expand_ipa": false
}
```

Launch arguments alone do not write this file. Changing a setting in F2 saves
only that field, preserving unrelated launch overrides as session-only choices.
If a save fails, the current session keeps the change and the panel reports
"Not saved"; a later change retries all unsaved edits. Invalid configuration
reports its path before the terminal UI opens and leaves the file untouched.
`--info` and `--verify-data` ignore appearance configuration. Custom themes and
btop `.theme` imports are not supported.

After building, exercise appearance settings and terminal restoration with a
prepared sample pack:

```sh
python3 scripts/terminal_appearance_smoke.py --data data/sample-pack
```

This check isolates preferences in a temporary directory.

After building, check the interaction flow with a prepared sample pack:

```sh
python3 scripts/terminal_interaction_smoke.py --data data/sample-pack
```

This exercises find, outline, session navigation, saved reading preferences,
mouse capture, and terminal restoration using isolated temporary preferences.
Input movement and deletion respect Unicode graphemes, including combining
marks and joined emoji. Clicking a prediction places the cursor at the end of
real input; it does not accept the prediction.

## Verify a data pack

Normal startup checks schema, file lengths, compact-index layout, and database
structure. It does not hash the dictionary or run a full SQLite integrity scan.
The small, already-loaded FST buffer receives its built-in CRC check to reject
accidentally damaged nodes before traversal.

For complete verification, run:

```sh
wordglide --verify-data --data PACK_DIRECTORY
```

This checks all SHA-256 receipts, vocabulary/index agreement, prebuilt ranking,
and SQLite integrity, prints the entry count and elapsed time, then exits without
opening the TUI. Omit `--data` to verify the automatically selected pack.
Successful verification exits with status 0; errors exit with a nonzero status.
Schema 2 packs are required; old packs must be replaced with a newly built or
downloaded pack.

To inspect a pack without verifying it:

```sh
wordglide --info --data PACK_DIRECTORY
```

This prints the pack path, schema, entry count, source snapshot, provenance, and
data and code licenses.

## Keys

| Key | Action |
| --- | --- |
| Type / paste | Incremental lookup while input is focused |
| ↑ / ↓ or Ctrl+P / Ctrl+N | Select candidate; preview follows |
| PageUp / PageDown, or the wheel | Scroll the definition by one visible page; works while hints show |
| Tab | Complete common prefix, then cycle a fixed candidate list |
| Shift+Tab | Cycle completions backward |
| → at input end / Ctrl+F | Accept the gray suggestion |
| Enter | Accept candidate and focus definition |
| Ctrl+L | Switch input / definition focus |
| Home / End in definition | Jump to the start / end of the definitions |
| `f` in definition | Show two-letter hints on visible dictionary words |
| Hint letters | Follow that word, without Enter |
| `e` in definition | Examples and references: compact / full |
| `p` in definition | Pronunciation (IPA): short / full |
| `?` in definition / `F1` | Show the key help; Esc closes it |
| Ctrl+G / F3 | Search and run available actions |
| / in definition | Find displayed text; Enter keeps, Esc restores position |
| n / N in definition | Next / previous match |
| o in definition | Jump through the definition outline |
| [ / ] in definition | Previous / next word or part-of-speech group |
| Ctrl+J / Ctrl+K in definition | Scroll one line down / up |
| F4 | Toggle split / focused reading |
| Ctrl+R | Search session back/forward locations |
| `F2` | Appearance and reading settings; changes save automatically |
| Click in definition | Focus the definition pane |
| Click a word in the focused definition | Follow that word |
| Click a candidate | Select it and preview it, keeping input focus |
| Click the highlighted candidate again | Accept it and focus the definition |
| Click in the input box | Focus input and place the cursor at the clicked grapheme |
| Esc | Undo active completion, cancel hints, or return input focus |
| Ctrl+Z | Return to the previous query, selection, focus, and scroll |
| Ctrl+Y | Go forward again after going back |
| Ctrl+U | Clear input for another lookup |
| Ctrl+W / Alt+Backspace | Delete the previous word |
| Ctrl+K | Delete from the cursor to the end |
| Ctrl+C | Exit |
| ← / →, Home / End, Ctrl+A / Ctrl+E | Edit input position |
| Ctrl+← / Ctrl+→ | Move the cursor by one word |
| Alt+← / Alt+→ | Step history back / forward |

Gray suffixes are displayed only for prefix suggestions at the end of the
input. The selected candidate supplies the prediction when it extends the query;
otherwise the first longer prefix candidate supplies it. The prediction prefers
a plain single word over a hyphenated or multi-word compound, and is hidden when
it does not score above an exact match of the query, so an exact match can stay
selected and previewed without a noisy suggestion. Right/Ctrl+F accepts the
prediction; Enter accepts the selected candidate. Typing still triggers immediate
previews without accepting a suggestion.
Tab completes the whole dictionary query, including phrases, without adding a
space. Multiple matches first extend to the common prefix of all prefix matches,
including words outside the visible top 20. Further Tab presses cycle the fixed
ranked list and fill the selected word. Esc restores the pre-completion query;
editing ends completion and starts a fresh lookup. Enter is optional for lookup
and only accepts the selected word to enter reading. Completion does not add
persistent history or personalize ranking.

Hints highlight the first two letters of a visible word and do not shift the
text. Only words present in the local pack receive hints; one-letter tokens do
not. Navigation history lives in the current session: `Ctrl+Z` steps
back through followed words and `Ctrl+Y` steps forward again; a new lookup or
follow clears the forward steps. Narrow terminals use stacked panes; the minimum
usable size is 30×10.

Mouse works in two steps: the first click inside the definition pane focuses it,
and clicking a visible word there follows it like a hint. Clicking the input box
returns focus without moving the cursor, and the wheel scrolls the definition
from any focus. A click on a candidate selects and previews it without moving
the focus; clicking the highlighted candidate again accepts it, like Enter.
Only words present in the local pack respond, and the jump
reuses the same session history as `Ctrl+Z`. Reporting is limited to presses and
the wheel, so pointer motion is never sent; terminals that still route
drag-selection to the application need their bypass key (usually Shift) for
native copy.

## Ranking and storage

Up to 20 candidates: exact match, valid word-form targets, prefix completions,
then one-edit fuzzy fallback. Fuzzy handles an inserted, missing, wrong, or
swapped adjacent character for queries of 3–64 characters. It compares complete
words/phrases; it does not provide fuzzy prefix completion.

The fixed base score is `100*Zipf - 2*characters - 100*extra_words`, ties broken
by normalized key. Wordfreq is queried only while building data. Runtime scores
never depend on your lookups. Short phrases receive a fixed penalty because
their wordfreq estimate is not a measured phrase count.

The compact binary vocabulary and prebuilt range-max tree return prefix top-k
without sorting all prefix matches. The builder writes `lexicon.bin`: a versioned
32-byte header, 20-byte candidate records, a little-endian u32 ranking tree, and
shared UTF-8 strings. Runtime loads this buffer and the FST once, without parsing
candidate JSON, creating millions of string objects, or rebuilding the tree.
Only returned candidates allocate strings. An FST handles exact and fuzzy matching. A read-only SQLite database stores
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
make build                 # release wordglide, dict-build, dict-bench
make check                 # or make check PYTHON=/path/to/python
make clean                 # remove the cargo target directory
python3 scripts/terminal_smoke.py --data data/sample-pack
./target/release/dict-bench --data data/english-pack --iterations 100
python3 scripts/package.py --pack data/english-pack --target RUST_TARGET --output dist/release
```

Benchmarks distinguish first application-cache reads, warm lookup/preview time,
and asynchronous input-to-TestBackend render time at 120×40. They also report
dictionary-open time; the real PTY check reports startup to terminal UI. OS file caches are
not flushed. The renderer measurement excludes terminal emulator display latency.
Full-pack target: P95 input-to-render ≤50 ms and resident memory about ≤512 MiB;
startup time is not a target. Record terminal size, data snapshot, and measurement
method with results. Small-sample results
do not establish full-pack performance. See [PERFORMANCE.md](PERFORMANCE.md)
for the full-data verification.

CI checks source on macOS and Linux. Pushing a `v*` tag whose version matches
Cargo.toml triggers four native release builds, full-data bundle smoke checks,
and automatic GitHub Release publication after every target passes. Manual
workflow dispatch builds and verifies the packages without publishing.

`data-release.json` pins the dictionary Release tag, asset, and SHA-256.
Release builds reuse that archive; they do not download and rebuild a moving
Wiktionary dump. Updating dictionary data requires a new data Release and an
explicit checksum update. All three package types retain license notices and
portable archive metadata. Python 3.11+ is required only for maintainer scripts.
Update packs by unpacking to a new directory and restarting with `--data`, or by
replacing the adjacent pack while the application is stopped.

Code: MIT. Data: its original licenses and attribution; see [THIRD_PARTY.md](THIRD_PARTY.md).

## Acknowledgements

Thanks to [404Simon/tuidict](https://github.com/404Simon/tuidict) for providing
an early reference for Wordglide's first version, and to its author for sharing
the project. Wordglide is independently implemented; no tuidict code is
incorporated.
