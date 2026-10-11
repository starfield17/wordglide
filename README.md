# Wordglide

A small offline English–English dictionary for reading in a terminal. Type a
word or phrase to see candidates and definitions immediately; Enter is not
needed to search.

Definitions and examples come from Wiktionary. Coverage varies by entry and is
not equivalent to a professionally edited learner's dictionary.

## Features

- About 1.35 million entries, with word forms, examples, IPA, etymology and
  synonyms/antonyms where the source provides them.
- Search as you type, with completion, form lookup and spelling suggestions.
- Follow words and phrases with keyboard hints or the mouse, then return to
  your reading position.
- Find within long entries, jump through an outline, and expand source content.
- Nine themes, saved reading preferences and adjustable terminal colors.
- Offline lookup; dictionary downloads happen only when requested.

## Install and run

Download the program for your platform from [GitHub Releases](https://github.com/starfield17/wordglide/releases/latest),
extract it, and run:

```sh
./wordglide/wordglide
```

Press **F2 → Download / update dictionary…** to install the dictionary. You can
also install it from the command line:

```sh
./wordglide/wordglide --download-data
```

With Rust 1.88 or newer and a C compiler, install the published program with:

```sh
cargo install wordglide --locked
wordglide
```

Supported platforms: macOS 11+ and Linux, on Intel/x86_64 and ARM64. Windows is
not supported. Installed lookup needs neither Python nor a network connection.

For an unpacked dictionary you already have:

```sh
wordglide --data PATH_TO_PACK 'take off'
```

For the current unreleased source build, use the included sample or a locally
built dictionary; see [building and trying the sample](doc/DEVELOPMENT.md#build-and-try-the-real-data-sample).

## Quick keys

| Key | Action |
| --- | --- |
| ↑ / ↓ | Select a candidate |
| Enter / Esc | Read the definition / return to input |
| Tab | Complete and cycle matches |
| `f` | Show hints for following words and phrases |
| Ctrl+Z / Ctrl+Y | Go back / forward |
| `/` | Find within the displayed definition |
| `e` / `p` | Expand examples / pronunciation |
| `v` / `r` / `t` | Expand forms / relations / etymology |
| F2 | Settings and dictionary download |
| Ctrl+G / F1 | Find actions / show help |
| Ctrl+C | Quit |

Reading controls apply while the definition is focused. Click a linked word or
phrase to follow it. See [the usage guide](doc/USAGE.md) for all keys, settings,
navigation and dictionary verification.

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
