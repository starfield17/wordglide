# Wordglide

Intent: craft — an independently implemented offline incremental reading tool.
Done when: type `ho`, see ranked completions and automatic definition preview;
look up `hosue`, `went`, `better`, and `take off`; follow a word in a definition
and return to the original query, selection, focus, and scroll position.
Delivery: Rust `wordglide [QUERY] [--data PACK_DIRECTORY]`; macOS and Linux.
Quality: readable source-grounded English definitions, deterministic ranking;
no Enter to search. Startup loads only compact, prebuilt indexes. Warm-session input-to-draw P95
target <=50 ms, resident-memory target <=512 MiB on the full data pack.
Decisions: Wiktionary via raw Wiktextract, prepared packs, fixed wordfreq baseline;
up to 20 candidates; exact > valid inflection > prefix > one-edit fuzzy fallback.
Base score: 100*Zipf - 2*character count - 100*extra whitespace-separated words.
Fuzzy: 3–64 query characters, insertion/deletion/substitution/adjacent swap.
Completion: fish-style Tab common prefix then fixed top-20 cycling; Shift+Tab
reverses; Esc restores original input; gray prefix suffix accepted with Right at
input end or Ctrl+F. Prediction prefers the selected strict prefix extension,
otherwise the first strict prefix extension in the ranked list; exact matches
keep their selection and preview. Enter accepts candidate and focuses reading; Ctrl+L switches
focus. Completion does not add lookup history or affect fixed ranking.
Distribution: public Wordglide repository; three download types (program, shared
data, combined bundle). Adjacent english-pack auto-discovery precedes existing
user-data directory; explicit --data wins. Runtime never downloads data.
Validation: schema 2 only; normal open checks structure and file lengths, plus
the loaded FST buffer CRC. No default SHA scan, vocabulary traversal, or SQLite
integrity scan. `wordglide --verify-data [--data DIRECTORY]` performs full
verification and exits without TUI.
Reading: POS/IPA/senses, at most two source examples per sense, no generated text;
obsolete/archaic senses last. Existing phrases included. Navigation history is
session-only and does not affect ranking.

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
- Open source definitions and example coverage do not equal Oxford editorial quality.
- Word frequency cannot infer the relative frequency of senses within a word.

