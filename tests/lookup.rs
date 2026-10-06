use local_english_dict::{Dictionary, MatchKind, build_pack};
use std::fs;

fn pack() -> (tempfile::TempDir, Dictionary) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("entries.jsonl");
    // Synthetic fixtures test ranking mechanics, never shipped as dictionary content.
    let words = [
        ("house", 600),
        ("home", 610),
        ("hope", 550),
        ("horn", 450),
        ("ho", 0),
        ("go", 700),
        ("went", 500),
        ("good", 600),
        ("well", 620),
        ("better", 650),
        ("take off", 400),
        ("palm", 410),
        ("fist", 400),
    ];
    let rows: Vec<_> = words
        .iter()
        .map(|(word, score)| {
            serde_json::json!({
                "key":word,"headword":word,"score":score,
                "groups":[{"headword":word,"pos":"noun","ipa":[],"senses":[{
                    "glosses":[format!("fixture {word} palm")],"tags":[],"examples":[],"targets":[]
                }]}],
                "lemmas": if *word == "went" {vec!["go"]} else {vec![]},
                "preview_lemmas": *word == "went",
                "source_url":format!("https://en.wiktionary.org/wiki/{word}")
            })
            .to_string()
        })
        .collect();
    fs::write(&input, rows.join("\n")).unwrap();
    let provenance =
        serde_json::json!({"source":"synthetic test fixture","snapshot":"test","licenses":[]});
    let meta = dir.path().join("source.json");
    fs::write(&meta, provenance.to_string()).unwrap();
    let out = dir.path().join("pack");
    build_pack(&input, &meta, &out).unwrap();
    let dict = Dictionary::open(&out).unwrap();
    (dir, dict)
}

#[test]
fn exact_prefix_fuzzy_and_morphology() {
    let (_dir, mut dict) = pack();
    let result = dict.search("HO").unwrap();
    assert_eq!(result[0].key, "ho");
    assert_eq!(result[0].kind, MatchKind::Exact);
    assert_eq!(result[1].key, "home");
    assert_eq!(dict.search("house").unwrap()[0].key, "house");
    let typo = dict.search("hosue").unwrap();
    assert_eq!(typo[0].key, "house");
    assert_eq!(typo[0].kind, MatchKind::Fuzzy);
    let went = dict.search("went").unwrap();
    assert_eq!(went[0].key, "went");
    assert_eq!(went[1].key, "go");
    assert_eq!(went[1].kind, MatchKind::Inflection);
    let preview = dict.preview(&went[0]).unwrap();
    assert_eq!(preview.related[0].key, "go");
    assert_eq!(dict.search("take off").unwrap()[0].key, "take off");
    assert!(dict.search("").unwrap().is_empty());
    assert!(dict.search("zzzzzzzzz").unwrap().is_empty());
    assert_eq!(dict.search("ho").unwrap(), result);
}

#[test]
fn corrupt_or_incompatible_packs_are_rejected() {
    let (dir, _dict) = pack();
    let path = dir.path().join("pack/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["schema_version"] = 999.into();
    fs::write(&path, manifest.to_string()).unwrap();
    assert!(Dictionary::open(&dir.path().join("pack")).is_err());
}

#[test]
fn checksum_damage_and_unreadable_canonical_entries_are_rejected() {
    let (dir, _dict) = pack();
    fs::write(dir.path().join("pack/candidates.json"), "[]").unwrap();
    assert!(
        Dictionary::open(&dir.path().join("pack"))
            .err()
            .expect("corrupted pack must fail")
            .to_string()
            .contains("Corrupt")
    );
    let input = dir.path().join("invalid.jsonl");
    fs::write(
        &input,
        serde_json::json!({"key":"test","headword":"test","score":0,
        "groups":[{"headword":"test","pos":"noun","senses":[]}],"source_url":"fixture"})
        .to_string(),
    )
    .unwrap();
    assert!(
        build_pack(
            &input,
            &dir.path().join("source.json"),
            &dir.path().join("invalid-pack")
        )
        .is_err()
    );
}
