# Project map

Run `make check`: formatting, Clippy (all owned targets), Rust tests/doc tests,
and Python pipeline tests. `make build` builds the release binaries; `make clean`
removes the cargo target directory. Keep code, scripts, and Makefile portable: do
not embed developer-specific paths, environment activation, or machine settings.
The Makefile defaults to `python3`; select a Python environment at invocation.
Do not commit personal paths, usernames, environment names, or machine details,
including in this file. Keep environment selection outside the repository.

Targets are macOS and Linux only; Windows is unsupported for now. Keep terminal
and mouse code POSIX and ANSI, and do not add Windows-only paths.

- `src/lib.rs`: public contract; private modules enforce internal visibility.
- `src/index.rs`: compact lexicon codec, prebuilt ranking, prefix top-k, fuzzy queries.
- `src/store.rs`, `src/build.rs`: lightweight open, explicit full verification, pack assembly.
- `src/app/`: navigation state, asynchronous result ownership, keys, completion, history, worker, and view options.
- `src/ui/`: rendering, reading layout, panes, help, and mouse hit-testing.
- `src/theme.rs`: color roles and `NO_COLOR` handling.
- `scripts/prepare.py`: raw Wiktextract validation/normalization and wordfreq scoring.
- `scripts/terminal_smoke.py`: actual POSIX PTY input and terminal restoration check.
- `scripts/package.py`: validated program/data/bundle release archives.
- `.github/workflows/release.yml`: tag-triggered native builds and publication.
- `data-release.json`: pinned dictionary Release asset and checksum; CI input only.
- `tests/`: public API and real pack integration scenarios.
- `SPEC.md`: accepted behaviors and performance targets.
- `tuidict/`: separate reference project, excluded from our checks and build.

Runtime/build separation is explicit: `wordglide` opens prepared packs; only
`dict-build` and `scripts/prepare.py` write dictionary data. Review dependency
and public export changes at the root manifest and `src/lib.rs`.
