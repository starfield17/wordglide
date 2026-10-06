#!/usr/bin/env python3
"""Normalize a pinned raw Wiktextract JSONL(.gz) snapshot, English only.

This maintainer tool uses upstream wordfreq. Runtime never imports Python or
fetches data. Source definitions/examples are copied, not generated or rewritten.
"""
import argparse
import gzip
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sqlite3
import unicodedata
from urllib.parse import quote

POLICY = "100*zipf-2*chars-100*extra_words;exact>inflection>prefix>fuzzy;key_tie"
INFLECTION_TAGS = {"plural", "singular", "first-person", "second-person", "third-person",
                   "past", "present", "participle", "comparative", "superlative", "gerund"}


def normalize(text):
    return " ".join(unicodedata.normalize("NFKC", text).lower()
                    .replace("\u2018", "'").replace("\u2019", "'").split())


def historical(sense):
    return bool({"obsolete", "archaic"}.intersection(sense["tags"]))


def extract(raw):
    if raw.get("lang_code") != "en":
        return None
    word = raw.get("word")
    if not isinstance(word, str) or not normalize(word):
        raise ValueError("English record has no valid word")
    if not isinstance(raw.get("pos"), str) or not raw["pos"]:
        raise ValueError(f"English record has no valid part of speech: {word}")
    senses = []
    inherited = raw.get("tags", [])
    for s in raw.get("senses", []):
        glosses = s.get("glosses", [])
        if not glosses:
            continue
        if not all(isinstance(g, str) for g in glosses):
            raise ValueError(f"Invalid glosses for {word}")
        examples = [{"text": e["text"], "reference": e.get("ref", ""),
                     "kind": e.get("type", "example")}
                    for e in s.get("examples", []) if isinstance(e.get("text"), str)]
        examples.sort(key=lambda e: e["kind"] == "quotation")
        senses.append({"glosses": glosses,
                       "tags": list(dict.fromkeys(inherited + s.get("tags", []))),
                       "examples": examples[:2],
                       "targets": list(dict.fromkeys(normalize(t["word"])
                           for t in s.get("form_of", []) if isinstance(t.get("word"), str)))})
    if not senses:
        return None
    senses.sort(key=historical)  # Stable within each partition.
    etymology = raw.get("etymology_number")
    if etymology is not None:
        if isinstance(etymology, bool) or not str(etymology).isdigit():
            raise ValueError(f"Invalid etymology_number for {word}: {etymology!r}")
        etymology = int(etymology)
        if not 0 <= etymology <= 2**32 - 1:
            raise ValueError(f"etymology_number out of range for {word}")
    group = {"headword": word, "pos": raw.get("pos", "unknown"),
             "ipa": list(dict.fromkeys(s["ipa"] for s in raw.get("sounds", [])
                        if isinstance(s.get("ipa"), str))),
             "etymology_number": etymology, "senses": senses}
    forms = []
    # A historical-only source group cannot supply modern inverse links, even
    # when its inflection table omits the inherited historical label.
    for f in raw.get("forms", []) if any(not historical(s) for s in senses) else []:
        tags = set(f.get("tags", []))
        if isinstance(f.get("form"), str) and tags.intersection(INFLECTION_TAGS) and not tags.intersection(
                {"canonical", "table-tags", "romanization", "alternative", "archaic", "obsolete"}):
            key = normalize(f["form"])
            if key and key != normalize(word):
                forms.append(key)
    return normalize(word), group, list(dict.fromkeys(forms))


def sha256(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def prepare(args):
    from wordfreq import zipf_frequency
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=False)
    staging = output / "staging.sqlite"
    db = sqlite3.connect(staging)
    db.executescript("""PRAGMA journal_mode=OFF;
        CREATE TABLE groups(key TEXT, payload TEXT);
        CREATE TABLE forms(form TEXT, lemma TEXT, pos TEXT);
        CREATE INDEX group_key ON groups(key);
        CREATE INDEX form_key ON forms(form);""")
    opener = gzip.open if str(args.input).endswith(".gz") else open
    processed = retained = rejected = 0
    rejection_samples = []
    with opener(args.input, "rt", encoding="utf-8") as f, (output / "rejected.jsonl").open("w", encoding="utf-8") as rejects:
        for line_number, line in enumerate(f, 1):
            if not line.strip():
                continue
            try:
                raw = json.loads(line)
            except (ValueError, TypeError, AttributeError) as e:
                raise ValueError(f"Input line {line_number}: {e}") from e
            # Malformed source records are auditable exclusions, not silently invented entries.
            # JSON/gzip corruption still aborts the build rather than publishing truncated data.
            try:
                result = extract(raw)
            except (ValueError, TypeError, AttributeError) as e:
                rejected += 1
                receipt = {"line": line_number, "reason": str(e), "record": raw}
                rejects.write(json.dumps(receipt, ensure_ascii=False) + "\n")
                if len(rejection_samples) < 10:
                    rejection_samples.append({"line": line_number, "reason": str(e)})
                result = None
            processed += 1
            if result:
                key, group, forms = result
                db.execute("INSERT INTO groups VALUES (?,?)", (key, json.dumps(group, ensure_ascii=False)))
                db.executemany("INSERT INTO forms VALUES (?,?,?)", ((form, key, group["pos"]) for form in forms))
                retained += 1
            if processed % 100000 == 0:
                db.commit()
                print(f"Read {processed:,} records; kept {retained:,} English groups", flush=True)
    db.commit()
    report = {"entries": 0, "groups": retained, "raw_records": processed,
              "rejected_records": rejected, "rejection_samples": rejection_samples,
              "senses": 0, "senses_with_examples": 0,
              "senses_with_ipa": 0, "unresolved_form_targets": 0, "samples": {}}
    pending_key = None
    pending_groups = []
    sample_words = {"fist", "bubble", "take", "set", "went", "better", "house", "take off"}

    def emit(key, groups, out):
        groups.sort(key=lambda g: g["headword"] != key)
        headword = groups[0]["headword"]
        primary = [g for g in groups if g["headword"] == headword]
        active = [s for g in primary for s in g["senses"] if not historical(s)]
        form_targets = [t for s in active for t in s["targets"]]
        valid = []
        for group in primary:
            for sense in group["senses"]:
                if historical(sense) or not sense["targets"]:
                    continue
                direct = [t for t in sense["targets"] if t != key and db.execute(
                    "SELECT 1 FROM groups WHERE key=? LIMIT 1", (t,)).fetchone()]
                if direct:
                    valid.extend(direct)
                else:
                    # Recover malformed source targets only from explicit, modern inflection
                    # tables of the same POS; never split 'good and well' heuristically.
                    valid.extend(r[0] for r in db.execute(
                        "SELECT DISTINCT lemma FROM forms WHERE form=? AND pos=? AND lemma!=? ORDER BY lemma",
                        (key, group["pos"], key)))
        valid = list(dict.fromkeys(valid))
        report["unresolved_form_targets"] += len(set(form_targets).difference(valid, {key}))
        z = zipf_frequency(key, "en", wordlist="large")
        score = round(100 * z) - 2 * len(key) - 100 * max(0, len(key.split()) - 1)
        entry = {"key": key, "headword": groups[0]["headword"], "score": score,
                 "groups": groups, "lemmas": valid,
                 "preview_lemmas": bool(valid) and bool(active) and bool(active[0]["targets"]),
                 "source_url": "https://en.wiktionary.org/wiki/" + quote(groups[0]["headword"], safe="")}
        out.write(json.dumps(entry, ensure_ascii=False, separators=(",", ":")) + "\n")
        senses = [s for g in groups for s in g["senses"]]
        report["entries"] += 1
        report["senses"] += len(senses)
        report["senses_with_examples"] += sum(bool(s["examples"]) for s in senses)
        report["senses_with_ipa"] += sum(len(g["senses"]) for g in groups if g["ipa"])
        if key in sample_words:
            report["samples"][key] = {"senses": len(senses), "lemmas": valid,
                "first_gloss": senses[0]["glosses"],
                "senses_with_examples": sum(bool(s["examples"]) for s in senses)}

    with (output / "entries.jsonl").open("w", encoding="utf-8") as out:
        for key, payload in db.execute("SELECT key,payload FROM groups ORDER BY key COLLATE BINARY,rowid"):
            if pending_key is not None and key != pending_key:
                emit(pending_key, pending_groups, out)
                pending_groups = []
            pending_key = key
            pending_groups.append(json.loads(payload))
        if pending_key is not None:
            emit(pending_key, pending_groups, out)
    if not report["entries"]:
        raise ValueError("No usable English entries in input")
    # Keep upstream notices embedded in the provenance, not a separable CSV dump.
    distribution = importlib.metadata.distribution("wordfreq")
    notices = {}
    for file in distribution.files or []:
        if Path(str(file)).name in {"LICENSE.txt", "NOTICE.md"}:
            notices[str(file)] = distribution.locate_file(file).read_text()
    if not notices:
        raise ValueError("Installed wordfreq distribution is missing its license/notice files")
    provenance = {"source": "English Wiktionary via raw Wiktextract/Kaikki",
        "source_url": args.source_url, "snapshot": args.snapshot,
        "input_sha256": sha256(Path(args.input)), "ranking": POLICY, "quality_report": report,
        "wordfreq_version": distribution.version, "wordfreq_notices": notices,
        "licenses": [
            {"data": "Wiktionary definitions and usage examples", "license": "CC BY-SA 4.0",
             "url": "https://creativecommons.org/licenses/by-sa/4.0/",
             "attribution": "Wiktionary contributors; per-entry source URL links to attribution history",
             "terms": "https://en.wiktionary.org/wiki/Wiktionary:Copyrights"},
            {"data": "wordfreq frequency data", "license": "CC BY-SA 4.0",
             "url": "https://creativecommons.org/licenses/by-sa/4.0/",
             "attribution": "Robyn Speer; Marc Brysbaert et al. (SUBTLEX), OPUS OpenSubtitles and other sources; see embedded upstream notices"}],
        "limitations": ["Example and pronunciation coverage is incomplete.",
             "Definitions are source text, not rewritten learner definitions.",
             "Quotations retain source references; original authors' rights may differ.",
             "Multiword frequency is estimated; wordfreq is a roughly pre-2022 usage snapshot."]}
    (output / "source.json").write_text(json.dumps(provenance, ensure_ascii=False, indent=2))
    (output / "quality.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    db.close()
    staging.unlink()
    print(json.dumps(report, ensure_ascii=False, indent=2))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--input", required=True, type=Path)
    p.add_argument("--output", required=True)
    p.add_argument("--snapshot", required=True, help="Source snapshot date or reproducible identifier")
    p.add_argument("--source-url", default="https://kaikki.org/dictionary/rawdata.html")
    prepare(p.parse_args())


if __name__ == "__main__":
    main()
