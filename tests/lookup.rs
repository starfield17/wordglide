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
    fs::write(dir.path().join("pack/lexicon.bin"), "[]").unwrap();
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

#[test]
fn lightweight_open_and_explicit_verification_are_separate() {
    let (dir, _dict) = pack();
    let path = dir.path().join("pack");
    local_english_dict::verify_pack(&path).unwrap();
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    manifest["files"]["entries.sqlite"] = "0".repeat(64).into();
    fs::write(file, manifest.to_string()).unwrap();
    assert!(
        Dictionary::open(&path).is_ok(),
        "normal open must not scan payload checksums"
    );
    assert!(local_english_dict::verify_pack(&path).is_err());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--verify-data", "--data"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!output.stdout.windows(8).any(|w| w == b"\x1b[?1049h"));
}

#[test]
fn verification_cli_exits_without_a_terminal_and_schema_is_checked() {
    let (dir, _dict) = pack();
    let path = dir.path().join("pack");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_wordglide"))
        .args(["--verify-data", "--data"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("verified"));
    let conn = rusqlite::Connection::open(path.join("entries.sqlite")).unwrap();
    conn.execute_batch("ALTER TABLE entries RENAME TO wrong_table")
        .unwrap();
    assert!(Dictionary::open(&path).is_err());
}

#[test]
fn invalid_record_offsets_return_errors_instead_of_panicking() {
    let (dir, _dict) = pack();
    let path = dir.path().join("pack");
    let file = path.join("lexicon.bin");
    let mut bytes = fs::read(&file).unwrap();
    // Fixed 32-byte header, then 20-byte candidate records: first key offset.
    bytes[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(file, bytes).unwrap();
    let mut dict = Dictionary::open(&path).unwrap();
    assert!(dict.search("better").is_err());
    assert!(local_english_dict::verify_pack(&path).is_err());
}

#[test]
fn full_verification_checks_tree_and_strings_even_with_updated_hashes() {
    use sha2::{Digest, Sha256};
    for damage in ["string", "tree"] {
        let (dir, _dict) = pack();
        let path = dir.path().join("pack");
        let file = path.join("lexicon.bin");
        let mut bytes = fs::read(&file).unwrap();
        if damage == "string" {
            bytes[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
        } else {
            let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
            let root = 32 + 20 * count + 4;
            bytes[root..root + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        }
        fs::write(file, &bytes).unwrap();
        let file = path.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        manifest["files"]["lexicon.bin"] = format!("{:x}", Sha256::digest(&bytes)).into();
        fs::write(file, manifest.to_string()).unwrap();
        assert!(Dictionary::open(&path).is_ok());
        assert!(
            local_english_dict::verify_pack(&path)
                .unwrap_err()
                .to_string()
                .contains("Corrupt")
        );
    }
}

#[test]
fn same_length_database_content_damage_is_detected_by_explicit_verification() {
    let (dir, _dict) = pack();
    let path = dir.path().join("pack");
    let file = path.join("entries.sqlite");
    let size = fs::metadata(&file).unwrap().len();
    let conn = rusqlite::Connection::open(&file).unwrap();
    let payload: String = conn
        .query_row("SELECT payload FROM entries WHERE key='house'", [], |r| {
            r.get(0)
        })
        .unwrap();
    conn.execute(
        "UPDATE entries SET payload=?1 WHERE key='house'",
        [payload.replace("fixture", "altered")],
    )
    .unwrap();
    drop(conn);
    assert_eq!(fs::metadata(file).unwrap().len(), size);
    assert!(Dictionary::open(&path).is_ok());
    assert!(local_english_dict::verify_pack(&path).is_err());
}

#[test]
fn damaged_fst_nodes_fail_open_without_a_query_panic() {
    let (dir, _dict) = pack();
    let file = dir.path().join("pack/words.fst");
    let original = fs::read(&file).unwrap();
    for damage in ["node", "footer"] {
        let mut bytes = original.clone();
        let footer = bytes.len() - 12;
        if damage == "node" {
            let root = u64::from_le_bytes(bytes[footer..footer + 8].try_into().unwrap()) as usize;
            bytes[root] = 63;
            bytes[root - 1] = 255;
        } else {
            bytes[footer..footer + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        }
        fs::write(&file, bytes).unwrap();
        assert!(Dictionary::open(&dir.path().join("pack")).is_err());
    }
}
