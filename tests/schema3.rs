use flate2::{Compression, write::ZlibEncoder};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, sync::Arc};
use wordglide::{Dictionary, MatchKind, build_pack, verify_pack};

fn pack() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let words = [
        ("home", 573, vec!["noun", "verb", "noun", "unusual-label"]),
        ("how-to", 459, vec!["noun"]),
        ("house", 561, vec!["noun", "verb"]),
        ("household", 429, vec!["noun"]),
        ("house-like", 393, vec!["adj"]),
        ("house‐like", 393, vec!["adj"]),
        ("mother-in-law", 177, vec!["noun"]),
        ("take off", 350, vec!["verb"]),
    ];
    let rows = words.iter().map(|(key, score, parts)| serde_json::json!({
        "key":key,"headword":key,"score":score,"source_url":"source fixture",
        "groups":parts.iter().map(|pos| serde_json::json!({"headword":key,"pos":pos,
            "senses":[{"glosses":["source definition"],"examples":[{"text":"source example","reference":"source reference"}]}]
        })).collect::<Vec<_>>()
    }).to_string()).collect::<Vec<_>>();
    fs::write(dir.path().join("entries.jsonl"), rows.join("\n")).unwrap();
    fs::write(dir.path().join("source.json"), "{}").unwrap();
    build_pack(
        &dir.path().join("entries.jsonl"),
        &dir.path().join("source.json"),
        &dir.path().join("pack"),
    )
    .unwrap();
    dir
}

fn rehash_database(path: &Path) {
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let bytes = fs::read(path.join("entries.sqlite")).unwrap();
    manifest["files"]["entries.sqlite"] = format!("{:x}", Sha256::digest(&bytes)).into();
    manifest["sizes"]["entries.sqlite"] = bytes.len().into();
    fs::write(file, manifest.to_string()).unwrap();
}

#[test]
fn ranked_candidates_have_source_parts_without_database_reads() {
    let dir = pack();
    let path = dir.path().join("pack");
    let mut dict = Dictionary::open(&path).unwrap();
    assert_eq!(
        dict.search("ho")
            .unwrap()
            .iter()
            .map(|c| c.key.as_str())
            .collect::<Vec<_>>(),
        [
            "home",
            "house",
            "how-to",
            "household",
            "house-like",
            "house‐like"
        ]
    );
    let c = dict.search("home").unwrap().remove(0);
    assert_eq!(c.parts_of_speech, ["noun", "unusual-label", "verb"]);
    assert_eq!(
        dict.preview(&c).unwrap().entry.groups[0].senses[0].examples[0].reference,
        "source reference"
    );
    for key in [
        "how-to",
        "house-like",
        "house‐like",
        "mother-in-law",
        "take off",
    ] {
        let candidates = dict.search(key).unwrap();
        assert_eq!(candidates[0].key, key);
        assert_eq!(candidates[0].kind, MatchKind::Exact);
    }
    let conn = Connection::open(path.join("entries.sqlite")).unwrap();
    conn.execute_batch("ALTER TABLE entries RENAME TO inaccessible")
        .unwrap();
    let candidates = dict.search("hou").unwrap();
    assert_eq!(candidates[0].parts_of_speech, ["noun", "verb"]);
    assert!(dict.preview(&candidates[0]).is_err());
}

#[test]
fn cached_previews_do_not_decode_again() {
    let dir = pack();
    let path = dir.path().join("pack");
    let mut dict = Dictionary::open(&path).unwrap();
    let candidate = dict.search("hou").unwrap().remove(0);
    let first = dict.preview(&candidate).unwrap();
    let conn = Connection::open(path.join("entries.sqlite")).unwrap();
    conn.execute(
        "UPDATE entries SET payload=x'00' WHERE key=?1",
        [&candidate.key],
    )
    .unwrap();
    let again = dict.preview(&candidate).unwrap();
    assert!(Arc::ptr_eq(&first.entry, &again.entry));
    let mut fresh = Dictionary::open(&path).unwrap();
    assert!(fresh.preview(&candidate).is_err());
}

#[test]
fn full_verification_decodes_entries_and_compares_metadata_even_with_new_checksums() {
    for damage in [
        "score",
        "pos",
        "headword",
        "key",
        "json",
        "trailing",
        "raw_len",
        "oversized_blob",
        "text_blob",
        "text_len",
    ] {
        let dir = pack();
        let path = dir.path().join("pack");
        let conn = Connection::open(path.join("entries.sqlite")).unwrap();
        let (len, mut payload): (i64, Vec<u8>) = conn
            .query_row(
                "SELECT raw_len,payload FROM entries WHERE key='house'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        if damage == "trailing" {
            payload.push(0);
            conn.execute("UPDATE entries SET payload=?1 WHERE key='house'", [payload])
                .unwrap();
        } else if damage == "raw_len" {
            conn.execute("UPDATE entries SET raw_len=?1 WHERE key='house'", [len + 1])
                .unwrap();
        } else if damage == "oversized_blob" {
            conn.execute(
                "UPDATE entries SET payload=zeroblob(2097153) WHERE key='house'",
                [],
            )
            .unwrap();
        } else if damage == "text_blob" {
            conn.execute("UPDATE entries SET payload='text' WHERE key='house'", [])
                .unwrap();
        } else if damage == "text_len" {
            conn.execute("UPDATE entries SET raw_len='invalid' WHERE key='house'", [])
                .unwrap();
        } else {
            let mut decoder = flate2::read::ZlibDecoder::new(payload.as_slice());
            let mut entry: serde_json::Value = serde_json::from_reader(&mut decoder).unwrap();
            match damage {
                "score" => entry["score"] = 0.into(),
                "pos" => entry["groups"][0]["pos"] = "changed".into(),
                "headword" => entry["headword"] = "changed".into(),
                "key" => entry["key"] = "other".into(),
                _ => {}
            }
            let raw = if damage == "json" {
                b"not json".to_vec()
            } else {
                serde_json::to_vec(&entry).unwrap()
            };
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
            encoder.write_all(&raw).unwrap();
            conn.execute(
                "UPDATE entries SET raw_len=?1,payload=?2 WHERE key='house'",
                params![raw.len() as i64, encoder.finish().unwrap()],
            )
            .unwrap();
        }
        drop(conn);
        rehash_database(&path);
        assert!(
            Dictionary::open(&path).is_ok(),
            "ordinary open must stay lightweight"
        );
        assert!(verify_pack(&path).is_err(), "missed {damage}");
    }
}

#[test]
fn old_schema_and_old_prepared_ranking_are_rejected() {
    let dir = pack();
    let path = dir.path().join("pack");
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    manifest["schema_version"] = 2.into();
    fs::write(file, manifest.to_string()).unwrap();
    let error = Dictionary::open(&path).err().unwrap().to_string();
    assert!(error.contains("--download-data"));
    assert!(error.contains("WORDGLIDE_DATA"));
    fs::write(
        dir.path().join("source.json"),
        r#"{"ranking":"100*zipf-2*chars-100*extra_words;exact>inflection>prefix>fuzzy;key_tie"}"#,
    )
    .unwrap();
    assert!(
        build_pack(
            &dir.path().join("entries.jsonl"),
            &dir.path().join("source.json"),
            &dir.path().join("new-pack")
        )
        .is_err()
    );
    assert!(!dir.path().join("new-pack").exists());
}
