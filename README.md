# Wordglide

An offline English–English terminal dictionary built for reading. Type a word
or phrase to see ranked candidates and its definition immediately; Enter is
not needed to search.

Entries come from English Wiktionary (via raw Wiktextract) with fixed
`wordfreq` ranking in a ~212 MiB Zstd+FST pack (~1.35M entries; ~155 MiB
download)—no lookup network calls, LLM rewriting, or invented examples.
Coverage varies and is not a professionally edited learner's dictionary.

## Design and performance

Startup loads prebuilt binary indexes directly: `words.fst` handles exact and
fuzzy lookup, a prebuilt range-max tree (`lexicon.bin`) returns top-20 prefixes
without sorting every match, and checksummed Zstd-19 blocks (`entries.bin` /
`entries.idx`) decompress only the entry block you open.

On the full 1,355,084-entry Schema-5 pack (ARM64 macOS, warm OS file caches):
**<1 ms** (`0.866 ms` P95) uncached lookup + preview (`0.114 ms` cached),
**81.0 MiB** peak RSS including startup, and **1.7–2.5 ms** uncached
leading-`*`/`?` wildcards (down from ~78–82 ms via pruned key scans). See
[performance measurements](doc/PERFORMANCE.md) for caveats and reproduction.

## Features

- **Search as you type**: exact, inflections, prefixes, `*`/`?` wildcards,
  1-edit fuzzy, plus zero-hit diacritic folding and `>=6`-char 2-edit fallback.
- **Source-faithful entries**: senses, examples, labeled IPA, word forms,
  etymology, and term relations, collapsed to summaries until expanded.
- **Keyboard-first reading**: follow hints (`f`) or mouse clicks on links,
  `Ctrl+Z`/`Ctrl+Y` history, `/` find, `#N` outline (`o`), `{`/`}` senses,
  archaic folding (`h`), OSC 52 copy (`y`), peek (`Space`), and 9 themes.
- **Scriptable CLI & tidy storage**: `--print` and `--json` stdout output,
  plus `--prune-data` to clean older managed downloads.

## Install and run

Download a release from [GitHub Releases](https://github.com/starfield17/wordglide/releases/latest),
extract it, and run `./wordglide/wordglide` (get data via
**F2 → Download / update dictionary…** or `--download-data`). With Rust 1.88+
and a C compiler, you can also install from crates.io:

```sh
./wordglide/wordglide --download-data
# or:
cargo install wordglide --locked && wordglide
```

Supported platforms: macOS 11+ and Linux, on Intel/x86_64 and ARM64. Windows is
not supported. Installed lookup needs neither Python nor a network connection.

```sh
wordglide --data PATH_TO_PACK 'take off'
wordglide --print house
wordglide --json house
wordglide --prune-data
```

To build from source or try the included sample, see
[building and trying the sample](doc/DEVELOPMENT.md#build-and-try-the-real-data-sample).

## Quick keys

| Key | Action |
| --- | --- |
| ↑ / ↓ | Select a candidate in input; scroll while reading |
| Enter / Esc | Read the definition / return to input |
| Tab | Complete and cycle matches |
| `f` | Show hints for following words and phrases |
| Ctrl+Z / Ctrl+Y | Go back / forward |
| `/` / `o` | Find within definition / open outline (`#N` jumps to sense N) |
| `{` / `}` | Step to previous / next sense |
| `y` | Copy clean definition text to terminal clipboard (OSC 52) |
| `e` / `p` / `h` | Expand examples / pronunciation / fold archaic senses |
| `v` / `r` / `t` | Expand forms / relations / etymology |
| F2 | Settings and dictionary download |
| Ctrl+G / F1 | Find actions / show help |
| Ctrl+C | Quit |

Reading controls apply while the definition is focused; click a linked word or
phrase to follow it. See [the usage guide](doc/USAGE.md) for all keys, settings,
navigation, and dictionary verification.

## Documentation

- [Usage](doc/USAGE.md)
- [Make your own dictionary pack](doc/PACK_AUTHORING.md)
- [Development, data preparation and releases](doc/DEVELOPMENT.md)
- [Performance measurements](doc/PERFORMANCE.md)
- [Changelog](CHANGELOG.md)

Code is [MIT licensed](LICENSE). Dictionary data retains its source licenses
and attribution; see [THIRD_PARTY.md](THIRD_PARTY.md).

Thanks to [404Simon/tuidict](https://github.com/404Simon/tuidict) and its author
for the early reference. Wordglide is independently implemented.
