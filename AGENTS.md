# Project map

Run `make check`: formatting, Clippy (all owned targets), Rust tests/doc tests,
cargo-deny license/source policy, and Python pipeline tests. `make build` cleans Cargo outputs before building release binaries; `make clean`
removes the Cargo target directory. `make clean-all` also removes disposable
`artifacts/` and `dist/` outputs, while retaining `data/` and installed packs. Keep code, scripts, and Makefile portable: do
not embed developer-specific paths, environment activation, or machine settings.
The Makefile defaults to `python3`; select a Python environment at invocation.
Do not commit personal paths, usernames, environment names, or machine details,
including in this file. Keep environment selection outside the repository.

Targets are macOS and Linux only; Windows is unsupported for now. Keep terminal
and mouse code POSIX and ANSI, and do not add Windows-only paths.

Keep README.md concise and user-facing: introduction, installation, quick keys,
and documentation links. Put developer workflows and technical detail in `doc/`
or this file; version history belongs in CHANGELOG.md. Keep documentation paths
included in Cargo packages and program release archives when moving them.

- `src/lib.rs`: public contract; private modules enforce internal visibility.
- `src/index.rs`: compact lexicon codec, prebuilt ranking, prefix top-k, fuzzy queries.
- `src/store.rs`, `src/build.rs`: lightweight open, explicit full verification, pack assembly.
- `src/entry_storage.rs`: read-only block/locator validation and byte-bounded block cache.
- `src/entry_codec.rs`: bounded schema-5 MessagePack entry and Zstd block encoding/decoding, private to pack storage.
- `src/app/`: navigation state, asynchronous result ownership, keys, completion, history, worker, fixed actions, source-addressed reading documents, and reading preferences.
- `src/ui/`: rendering, reading layout, panes, help, action panels, and mouse hit-testing.
- `src/theme.rs`: built-in palettes, appearance types, color roles, RGB-to-256 conversion.
- `src/download.rs`: explicit prebuilt-pack installation, checksums, safe extraction, lock, and atomic managed-pack receipt.
- `src/config.rs`: appearance-only configuration loading and atomic saving.
- `src/ui/appearance.rs`: F2 Settings and dictionary download panels; terminal entry owns persistence.
- `scripts/prepare.py`: raw Wiktextract validation/normalization and wordfreq scoring.
- `scripts/terminal_no_data_smoke.py`: POSIX PTY missing/invalid-data startup and F2 recovery checks.
- `scripts/terminal_smoke.py`: actual POSIX PTY input and terminal restoration check.
- `scripts/terminal_interaction_smoke.py`: POSIX PTY reading navigation, menus, preferences, and mouse toggles.
- `scripts/terminal_content_smoke.py`: POSIX PTY source-content toggles, labeled IPA, relations and whole-phrase navigation.
- `scripts/terminal_appearance_smoke.py`: PTY theme, save/retry, color-mode and restart checks.
- `scripts/package.py`: validated program and shared-data release archives.
- `scripts/test_boundaries.py`: source scans for the SPEC lineage checks the compiler
  cannot express (N1 network locality, N7/F1 build-only pack writes, F4 installer).
- `deny.toml`: `cargo deny check` dependency license/source policy; a `make check` input.
- `.github/workflows/check.yml`: source checks, native sample PTY checks, and MSRV compilation.
- `.github/workflows/release.yml`: tag-triggered native builds and publication.
- `data-release.json`: pinned dictionary Release asset and checksum; CI input only.
- `tests/`: public API and real pack integration scenarios.
- `tests/fixtures/normalize.json`: normalization cases shared by the Rust and Python tests.
- `SPEC.md`: accepted behaviors, performance targets, and future monolingual-pack TODO.
- `doc/`: detailed usage, community pack authoring, development/release workflows,
  and performance measurements.

New source-content types preserve arbitrary Unicode forms and source tags. Future
Chinese–Chinese/Japanese–Japanese pack support is deferred; consult SPEC.md before
adding language policies. Keep English-specific preparation, normalization and
ranking out of the source-content model.

Runtime/build separation is explicit: `wordglide` opens prepared packs; explicit downloads install verified prebuilt bytes. Only
`dict-build` and `scripts/prepare.py` write dictionary data. Review dependency
and public export changes at the root manifest, `deny.toml`, and `src/lib.rs`.
