# Making your own dictionary pack

This guide is for authors who can run shell commands and convert their dictionary
data to JSON. Wordglide 0.5 reads schema-5 packs only. The procedure below works
with the existing English runtime; it does not add multilingual support.

You do not need to modify Rust or use Wiktextract. Convert your source to the
public JSONL format, build a pack, verify it, and open it with `--data`. The
program reads prepared data offline; it never runs your converter or rebuilds
dictionary files during lookup.

## Tools and working directory

Run the commands from the repository root on macOS or Linux. Build the three
executables once:

```sh
cargo build --locked --release --bins
```

This requires Rust 1.88 or newer and a C compiler. The walkthrough also uses
POSIX shell tools, `tar`, and Python 3.11 or newer for archive checksums. Select your own
Python environment. Installing `scripts/requirements-build.txt` is needed for
the official wordfreq preparation workflow, not for building your own JSONL.
Program release archives contain the reader; obtain `dict-build` by building
the repository. Generated pack bytes can be shared across supported platforms.

## Input contract

Create a UTF-8 `entries.jsonl` containing one complete JSON object per line and
a UTF-8 `source.json` object describing your data. JSONL objects must not span
physical lines; use JSON escapes for newlines inside source text.

### Entries and content

| Object | Required fields | Optional fields and defaults |
| --- | --- | --- |
| Entry | `key`, `headword`, `score`, `groups`, `source_url` | `lemmas: []`, `preview_lemmas: false` |
| Group | `headword`, `pos`, `senses` | `ipa: []`, `pronunciations: []`, `forms: []`, `relations: []`, `etymology_number: null`, `etymology_text: ""` |
| Sense | `glosses` | `tags: []`, `examples: []`, `targets: []`, `relations: []` |
| Example | `text` | `reference: ""`, `kind: ""` |
| Word form | `form` | `tags: []` |
| Pronunciation | `ipa` | `tags: []`, `note: ""` |
| Term relation | `kind`, `word` | `tags: []`, `sense: ""`, `source: ""` |

Strings and lists retain source wording and order. Missing content can be omitted;
do not invent labels or use empty placeholder records. Keep display headwords
separate from lookup keys. Supply nonempty headwords, groups and senses, a
nonempty POS string without control characters, and nonblank glosses. Examples
must have nonblank text, with at most two per sense. Select them in your converter
and retain quotation references. `etymology_number`, when supplied, is a positive
integer up to 4,294,967,295. Keep etymology text at its source group.

Pronunciations must have nonblank IPA. `Group.ipa` must be the distinct IPA strings
from `Group.pronunciations`, in first-occurrence order. Two pronunciation records
with the same IPA and different dialect labels remain separate, while `ipa`
contains that IPA once. If there is no pronunciation data, omit both fields.

Forms require nonblank `form` values. Relations require nonblank `word` values;
`kind` is one of `synonym`, `antonym`, `derived`, or `related`. Put sense synonyms
and antonyms on their actual sense, and word-level relations on their group.
`sense` is the source's textual qualifier, not an inferred sense ID; `source` is
the source attribution. Forms and relation phrases can be followed as whole
terms when their normalized keys exist in the same pack.

`lemmas` contains normalized entry keys for form-to-lemma lookup. Set
`preview_lemmas: true` if a form should also preview the referenced entries.
`targets` preserves source sense references; it does not replace `lemmas`.
The builder does not infer inverse form links from `forms`: your converter must
create the form entry and its lemma keys. For example, `took` refers to `take`
in the walkthrough below. Unresolved lemma keys are accepted but ignored during
lookup and preview; include the referenced entry to make them useful. Leave
unresolved sense targets as source content and report unresolved references in
your conversion notes.

Each entry must fit within 1 MiB in both the codec's serialized JSON size and its
MessagePack encoding. A small input file alone does not guarantee this limit:
default fields are included during serialization. The builder rejects oversized
entries. Do not hand-author the private binary tuples or edit generated hashes.

### Lookup keys and scores

The current normalization applies Unicode NFKC, lowercases, changes left/right
curly apostrophes to ASCII apostrophes, trims whitespace, and collapses whitespace
runs to one space. It does not strip accents. Use the same rule as the reader;
from a Python converter running at the repository root, you can reuse:

```python
from scripts.prepare import normalize

key = normalize(source_headword)
```

Merge records that normalize to the same key into one entry with multiple groups.
Duplicate keys are rejected; unsorted JSONL is accepted and sorted by the builder.
Use normalized keys for `lemmas` too. Display text and relation words can retain
their original spelling.

`score` is a signed 32-bit integer supplied by your converter. Larger scores
rank earlier within prefix/fuzzy candidates; equal scores use key order. Exact
matches precede lemma candidates, then prefixes, then fuzzy matches. The builder
does not fetch frequencies or calculate scores. The minimal example uses zero
for every entry and therefore needs no frequency data.

For official English frequency scoring, reuse `ranking_score` from
`scripts.prepare` with your source Zipf value and preserve frequency provenance.
The generated manifest still identifies the current fixed ranking policy; this
version has no configurable language/ranking profile. Omit `source.ranking` or
use the exact current policy string; an incompatible string is rejected. Do not
claim a new language policy by changing this descriptive field.

### Provenance and attribution

`source.json` must be an object. The reader exposes `source`, `snapshot`,
`source_url`, and license descriptions in pack information. Record the source
version, input checksum, conversion choices, and attribution needed for your
data. `language_code: "en"` currently records provenance only: changing it does
not select another normalization rule or change the English interface.

Include your own `THIRD_PARTY.md` with the pack. Preserve original source links,
quotation references and applicable notices, including those for frequency data
if used. Wordglide's code license and the repository's English-data attribution
file do not describe every custom dictionary. Use your source's actual terms;
the manifest checksum is an integrity receipt, not a quality endorsement.

## Walkthrough: a small English pack

The following excerpts come from the repository's Wiktionary sample, snapshot
2026-09-02. They deliberately omit most content and use uniform scores instead
of wordfreq. Start in a new workspace directory; build output must not already
exist. The sample attribution below is for these excerpts only.
The demo lives under disposable `artifacts/`, which `make clean-all` removes.
Keep real source data and packs under `data/` or outside these disposable outputs.

```sh
mkdir -p artifacts/authoring-demo
cat > artifacts/authoring-demo/entries.jsonl <<'JSONL'
{"key":"take","headword":"take","score":0,"groups":[{"headword":"take","pos":"verb","ipa":["/ˈteɪ̯k/"],"pronunciations":[{"ipa":"/ˈteɪ̯k/","tags":[],"note":""}],"forms":[{"form":"took","tags":["past"]}],"etymology_text":"From Middle English taken (“to take, lay hold of, grasp, strike”), from Old English tacan (“to grasp, touch”), probably of North Germanic origin, from Old Norse taka (“to touch, take”).","relations":[{"kind":"derived","word":"take off"}],"senses":[{"glosses":["To get into one's hands, possession, or control, with or without force."],"examples":[{"text":"They took Charlton's gun from his cold, dead hands.","kind":"example"}],"relations":[{"kind":"synonym","word":"seize"}]}]}],"source_url":"https://en.wiktionary.org/wiki/take"}
{"key":"take off","headword":"take off","score":0,"groups":[{"headword":"take off","pos":"verb","senses":[{"glosses":["To remove."]}]}],"source_url":"https://en.wiktionary.org/wiki/take%20off"}
{"key":"took","headword":"took","score":0,"lemmas":["take"],"preview_lemmas":true,"groups":[{"headword":"took","pos":"verb","ipa":["/tʊk/"],"pronunciations":[{"ipa":"/tʊk/","tags":["Australia","General-American","New-Zealand","Received-Pronunciation"]}],"senses":[{"glosses":["simple past of take"],"targets":["take"]}]}],"source_url":"https://en.wiktionary.org/wiki/took"}
JSONL
cat > artifacts/authoring-demo/source.json <<'JSON'
{
  "source": "English Wiktionary contributors; excerpts from Wordglide's sample",
  "source_url": "https://en.wiktionary.org/",
  "snapshot": "2026-09-02-authoring-demo",
  "language_code": "en",
  "licenses": [{
    "data": "Wiktionary excerpts",
    "license": "CC BY-SA 4.0",
    "url": "https://creativecommons.org/licenses/by-sa/4.0/",
    "attribution": "Wiktionary contributors; per-entry URLs identify source pages and contributor histories",
    "terms": "https://en.wiktionary.org/wiki/Wiktionary:Copyrights"
  }],
  "notes": ["Selected excerpts, shortened etymology, uniform zero scores; no frequency data used."]
}
JSON
cat > artifacts/authoring-demo/THIRD_PARTY.md <<'NOTICE'
# Dictionary attribution

English Wiktionary contributors, source snapshot 2026-09-02.
These selected excerpts retain per-entry source URLs and contributor histories.
License: CC BY-SA 4.0, https://creativecommons.org/licenses/by-sa/4.0/
Source terms: https://en.wiktionary.org/wiki/Wiktionary:Copyrights
Modifications: selected senses/forms/relations, shortened etymology, uniform
zero scores, and conversion to Wordglide schema 5. No frequency data is included.
NOTICE
./target/release/dict-build \
  --input artifacts/authoring-demo/entries.jsonl \
  --source artifacts/authoring-demo/source.json \
  --output artifacts/authoring-demo/my-pack
cp artifacts/authoring-demo/THIRD_PARTY.md artifacts/authoring-demo/my-pack/
./target/release/wordglide --data artifacts/authoring-demo/my-pack --verify-data
./target/release/wordglide --data artifacts/authoring-demo/my-pack --info
./target/release/wordglide --data artifacts/authoring-demo/my-pack take
```

The full verification should report **3 entries**. In the terminal, `take`
shows the source gloss and `Form: took [past]`. Press Enter for reading focus,
then `t` to expand etymology. The `Derived terms: take off` link is a complete
phrase: click it, or press `f` and type its displayed hint. Use Ctrl+Z to return
to `take`. The synonym `seize` is intentionally absent from this tiny pack, so
it displays as source content without a dictionary link. Return to input with
Esc, replace the query with `took`, and check the labeled pronunciation and
lemma preview. Ctrl+C exits and restores terminal state.

Use your own input converter instead of these heredocs for a real dictionary.
The official raw Wiktextract/wordfreq route is documented in
[Build the full data pack](DEVELOPMENT.md#build-the-full-data-pack).

## Archive and reopen

The pack contains `manifest.json`, `entries.bin`, `entries.idx`, `lexicon.bin`,
and `words.fst`, plus your attribution document. Keep them together. A custom
directory name works with `--data`; it need not be called `english-pack`.

`scripts/package.py` currently assembles official releases, uses the fixed
`english-pack` name, includes repository attribution, and requires a program
binary. It is not yet a generic community dictionary packager. For a manual
community bundle, use a simple gzip archive and checksum:

```sh
tar -czf artifacts/authoring-demo/my-pack.tar.gz \
  -C artifacts/authoring-demo my-pack
python3 - <<'PY'
import hashlib
from pathlib import Path
archive = Path("artifacts/authoring-demo/my-pack.tar.gz")
with archive.open("rb") as stream:
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
(archive.parent / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n")
PY
python3 - <<'PY'
import hashlib
from pathlib import Path
root = Path("artifacts/authoring-demo")
digest, name = (root / "SHA256SUMS.txt").read_text().strip().split("  ", 1)
with (root / name).open("rb") as stream:
    assert hashlib.file_digest(stream, "sha256").hexdigest() == digest
print("Archive checksum matches")
PY
mkdir artifacts/authoring-demo/reopened
tar -xzf artifacts/authoring-demo/my-pack.tar.gz \
  -C artifacts/authoring-demo/reopened
./target/release/wordglide \
  --data artifacts/authoring-demo/reopened/my-pack --verify-data
./target/release/wordglide --data artifacts/authoring-demo/reopened/my-pack 'take off'
```

The checksum code requires Python 3.11 or newer. These commands create a manual
bundle, not the official installer's XZ format or a reproducible release archive.
The extraction command above is for the archive you just created. For received
community archives, inspect members before extraction and reject absolute paths,
parent traversal and links outside the destination. Run `--verify-data` before
using the extracted pack. This command verifies pack structure and content
receipts, not the archive's extraction paths or its editorial quality.

Recipients need only the matching reader and extracted pack, not Rust or Python.
Tell them the required Wordglide/schema version, language, data source/version,
archive checksum and `--data` command. `WORDGLIDE_DATA` can also select the
directory. F2 and `--download-data` install the official dictionary only;
community packs do not register with that downloader or replace its managed pack.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Output already exists | Choose a new output directory. A failed build may leave partial files; preserve the inputs and rebuild into a fresh directory. |
| Invalid normalized key / duplicate key | Apply the shared normalization and merge collisions before building. |
| JSON/type or sense validation error | Check the reported JSONL line, required fields, nonblank glosses and the two-example limit. |
| IPA must match structured pronunciations | Populate `ipa` from distinct structured IPA records in source order, or omit both when unavailable. |
| Entry exceeds 1 MiB | Reduce that entry's source selection explicitly and document it; do not modify the binary limit. |
| Incompatible ranking or pack schema | Use the current policy and schema-5 builder. Rebuild from canonical inputs; do not patch the manifest to disguise an old format. |
| A displayed form/phrase has no link | Include its normalized key in the pack. Forms alone do not create extra entries or inverse lemma links. |
| Custom pack is not selected | Pass its extracted directory through `--data`; the archive itself is not a data directory. |

## Future languages: design only

The current runtime is English-oriented. Unicode storage alone does not establish
support for another language: normalization can merge distinct spellings,
word boundaries affect definition following, and English frequency/inflection
rules can rank or interpret other languages incorrectly.

The [multilingual TODO in SPEC](../SPEC.md#found--not-doing) describes the intended
extension points: pack identity and separate headword/definition languages,
shared build/query normalization, language-specific follow and ranking policies,
independent dictionary packaging, and community authoring. Chinese–Chinese and
Japanese–Japanese are the first validation targets. None of those runtime
capabilities or new CLI flags are implemented by this guide.

Future community packs should use the same supported format as official packs,
without requiring us to maintain or understand every language. Language-strategy
contributions must include lookup, normalization, follow and terminal-display
cases. Report technical validation separately from review by a speaker of the
language; a successful build does not certify dictionary accuracy.
