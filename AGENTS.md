# Project map

Run `make check`: formatting, Clippy (all owned targets), Rust tests/doc tests,
and Python pipeline tests. Keep code, scripts, and Makefile portable: do not
embed developer-specific paths, environment activation, or machine settings.
The Makefile defaults to `python3`; select a Python environment at invocation.
Do not commit personal paths, usernames, environment names, or machine details,
including in this file. Keep environment selection outside the repository.

- `src/lib.rs`: public contract; private modules enforce internal visibility.
- `src/index.rs`: normalization-independent fixed ranking, prefix top-k, fuzzy queries.
- `src/store.rs`, `src/build.rs`: data pack validation, read-only reads, pack assembly.
- `src/app.rs`, `src/ui.rs`: navigation, asynchronous result ownership, rendering.
- `scripts/prepare.py`: raw Wiktextract validation/normalization and wordfreq scoring.
- `scripts/terminal_smoke.py`: actual POSIX PTY input and terminal restoration check.
- `tests/`: public API and real pack integration scenarios.
- `SPEC.md`: accepted behaviors and performance targets.
- `tuidict/`: separate reference project, excluded from our checks and build.

Runtime/build separation is explicit: `dict` opens prepared packs; only
`dict-build` and `scripts/prepare.py` write dictionary data. Review dependency
and public export changes at the root manifest and `src/lib.rs`.
