# Wordglide

Intent: craft — an independently implemented offline incremental reading tool.
Done when: type `ho`, see ranked completions and automatic definition preview;
look up `hosue`, `went`, `better`, and `take off`; follow a word in a definition
and return to the original query, selection, focus, and scroll position.
Delivery: Rust `wordglide [QUERY] [--data PACK_DIRECTORY] [--no-color]
[--no-mouse]`, plus `--info` and `--verify-data`; macOS and Linux
terminals only. Windows is out of scope for now: no Windows target is built and
only POSIX terminals are exercised, so do not add Windows-only paths.
Quality: readable source-grounded English definitions, deterministic ranking;
no Enter to search. Startup loads only compact, prebuilt indexes. Warm-session input-to-draw P95
target <=50 ms, resident-memory target <=512 MiB on the full data pack.
Decisions: Wiktionary via raw Wiktextract, prepared packs, fixed wordfreq baseline;
up to 20 candidates; exact > valid inflection > prefix > one-edit fuzzy fallback.
Base score: 100*Zipf - 2*character count - 100*extra whitespace-separated words.
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
single click never navigates. Clicks in the input box return focus without
moving the cursor. A candidate click selects and previews that row without
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
Distribution: public Wordglide repository; three download types (program, shared
data, combined bundle). Explicit --data wins, then a non-empty WORDGLIDE_DATA,
then adjacent english-pack auto-discovery, then the existing user-data
directory. Runtime never downloads data.
Validation: schema 2 only; normal open checks structure and file lengths, plus
the loaded FST buffer CRC. No default SHA scan, vocabulary traversal, or SQLite
integrity scan. `wordglide --verify-data [--data DIRECTORY]` performs full
verification, reports the entry count and elapsed time, and exits without TUI.
`wordglide --info` prints pack path, schema, entry count, source snapshot,
provenance, and licenses.
Reading: POS/IPA/senses, at most two source examples per sense, no generated
text; obsolete/archaic senses last. The compact default shows at most two IPA
variants and the first example of each sense without references; `e` and `p`
expand them. Wrapped lines hang under their sense marker. The pane title names
the entry and shows scroll progress. Candidate panes size to their content, and
a definition retained during loading is dimmed. An error is shown as a banner
above the last good definition. Existing phrases included. Navigation history
is session-only and does not affect ranking.

## Do not build
- N1 No network or LLM dependencies in the runtime.
- N2 No personal ranking or persistent lookup history.
- N3 No audio, images, cloud services, plugins, or multi-dictionary management.
- N4 No rewriting definitions or inventing examples to fill source gaps.
- N5 Prefix top-k must not enumerate and sort all prefix matches.
- N6 Do not modify or fork the reference implementation in `tuidict/`.
- N7 When an out-of-scope issue appears, record it below; do not implement it.
- N8 No runtime candidate JSON parsing or rebuilding the ranking tree.

## Frame
Compile source data to a local pack. Runtime indexes contain compact binary candidate metadata and prebuilt ranking;
definition text is read only for the selected word and cached within a byte budget.
Oracle: compare indexed top-k with exhaustive fixed-score ranking in tests.
Boundary check: compiler privacy/doc test; runtime owns no downloader.

## Found · Not doing
- Windows support: no target and no WinAPI terminal paths, until POSIX shells are fully settled.
- Open source definitions and example coverage do not equal Oxford editorial quality.
- Word frequency cannot infer the relative frequency of senses within a word.
- Hyphenated compounds can outrank common single words in prefix completion
  because the score uses raw wordfreq Zipf; changing that needs a ranking policy
  revision and a rebuilt pack.
- Candidate rows carry no part of speech; adding one needs a schema-3 index
  record or a per-candidate database read, both deferred.

