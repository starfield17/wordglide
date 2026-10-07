# Data and dependencies

Our implementation is independent; no code is copied from `tuidict/`.

The built-in orange, gruvbox_light, gruvbox_dark_v2, and whiteout palettes are
inspired by the [btop theme collection](https://github.com/aristocratos/btop/tree/main/themes).
The orange palette credits neocerambyx; gruvbox_light credits kk9uk;
gruvbox_dark_v2 credits BachoSeven and Pietryszak and the
[Gruvbox palette](https://github.com/morhetz/gruvbox); whiteout credits
aristocratos. Wordglide implements its own reading-specific color roles and
secondary colors; no btop implementation or theme parser is incorporated.

Dictionary text is authored by English Wiktionary contributors. Raw structured
records come from [Kaikki/Wiktextract](https://kaikki.org/dictionary/rawdata.html).
Definitions retain source wording. Per-entry Wiktionary URLs identify source
pages and their contributor histories. Usage quotations retain source references;
the quoted authors' original rights may differ from the surrounding wiki content.
See [Wiktionary copyrights](https://en.wiktionary.org/wiki/Wiktionary:Copyrights)
and [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/).

Frequency scoring uses Robyn Speer's [wordfreq](https://github.com/rspeer/wordfreq)
3.1.1. Its code is Apache-2.0; frequency data is CC BY-SA 4.0 with additional
attribution notices. Each pack embeds the installed distribution's LICENSE.txt
and NOTICE.md, including credit for Marc Brysbaert et al. (SUBTLEX) and the
OpenSubtitles/OPUS project. SUBTLEX is freely available data. Frequencies are a
snapshot of usage through roughly 2021; multiword frequencies are estimates.
Scores, policy, upstream versions, and attribution travel together in the pack.

`examples/sample/` contains 89 real English word forms selected from the raw
Wiktextract records embedded in Kaikki pages, downloaded on 2026-10-06. The pages
reported the 2026-09-02 source dump. `receipts.json` records page URLs and SHA-256
receipts; `source.json` records the raw JSONL checksum and full wordfreq notices.
Selection, normalization, source-field extraction, example ordering/truncation,
and ranking metadata are our modifications. No definitions or examples were
generated. The sample is not a complete dictionary.

For redistribution, keep the manifest, source links, embedded notices, and these
attributions with the data. The repository's MIT code license does not replace
data licensing. Rust dependency versions are recorded in Cargo.lock; crates
retain their own licenses.
