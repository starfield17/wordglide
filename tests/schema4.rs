use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::Arc};
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

fn rehash(path: &Path, name: &str) {
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let bytes = fs::read(path.join(name)).unwrap();
    manifest["files"][name] = format!("{:x}", Sha256::digest(&bytes)).into();
    manifest["sizes"][name] = bytes.len().into();
    fs::write(file, manifest.to_string()).unwrap();
}
fn damage_body(path: &Path) {
    let file = path.join("entries.bin");
    let mut bytes = fs::read(&file).unwrap();
    bytes[0] ^= 1;
    fs::write(file, bytes).unwrap();
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
    let mut dict = Dictionary::open(&path).unwrap();
    damage_body(&path);
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
    damage_body(&path);
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
        "msgpack",
        "trailing",
        "concatenated",
        "raw_len",
        "locator",
        "block_count",
        "dictionary",
    ] {
        let dir = pack();
        let path = dir.path().join("pack");
        let mut index = fs::read(path.join("entries.idx")).unwrap();
        let mut body = fs::read(path.join("entries.bin")).unwrap();
        let raw = zstd::bulk::decompress(&body, 1048576).unwrap();
        // This small fixture has no training dictionary and exactly one block.
        assert_eq!(&index[12..20], &[1, 0, 0, 0, 0, 0, 0, 0]);
        let keys = [
            "home",
            "house",
            "house-like",
            "household",
            "house‐like",
            "how-to",
            "mother-in-law",
            "take off",
        ];
        let id = keys.iter().position(|k| *k == "house").unwrap();
        let at = 32 + 24 + id * 8;
        let start = u32::from_le_bytes(index[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = u32::from_le_bytes(index[at + 12..at + 16].try_into().unwrap()) as usize;
        if matches!(damage, "score" | "pos" | "headword" | "key" | "msgpack") {
            let mut entry: serde_json::Value = rmp_serde::from_slice(&raw[start..end]).unwrap();
            match damage {
                "score" => entry[2] = 0.into(),
                "pos" => entry[3][0][1] = "changed".into(),
                "headword" => entry[1] = "changed".into(),
                "key" => entry[0] = "other".into(),
                _ => {}
            }
            let bytes = if damage == "msgpack" {
                vec![0xdd, 255, 255, 255, 255]
            } else {
                rmp_serde::to_vec(&entry).unwrap()
            };
            let delta = bytes.len() as i64 - (end - start) as i64;
            let mut replacement = raw[..start].to_vec();
            replacement.extend_from_slice(&bytes);
            replacement.extend_from_slice(&raw[end..]);
            for j in id + 1..keys.len() {
                let off = 32 + 24 + j * 8 + 4;
                let old = u32::from_le_bytes(index[off..off + 4].try_into().unwrap());
                index[off..off + 4].copy_from_slice(&((old as i64 + delta) as u32).to_le_bytes());
            }
            let mut encoder = zstd::bulk::Compressor::new(19).unwrap();
            encoder.include_checksum(true).unwrap();
            body = encoder.compress(&replacement).unwrap();
            index[40..44].copy_from_slice(&(body.len() as u32).to_le_bytes());
            index[44..48].copy_from_slice(&(replacement.len() as u32).to_le_bytes());
        } else {
            match damage {
                "trailing" => {
                    body.push(0);
                    index[40..44].copy_from_slice(&(body.len() as u32).to_le_bytes());
                }
                "concatenated" => {
                    body.extend_from_within(..);
                    index[40..44].copy_from_slice(&(body.len() as u32).to_le_bytes());
                }
                "raw_len" => index[44..48].copy_from_slice(&(raw.len() as u32 + 1).to_le_bytes()),
                "locator" => index[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes()),
                "block_count" => index[52..56].copy_from_slice(&1u32.to_le_bytes()),
                "dictionary" => index[16..20].copy_from_slice(&65537u32.to_le_bytes()),
                _ => unreachable!(),
            }
        }
        index[24..32].copy_from_slice(&(body.len() as u64).to_le_bytes());
        fs::write(path.join("entries.bin"), body).unwrap();
        fs::write(path.join("entries.idx"), index).unwrap();
        rehash(&path, "entries.bin");
        rehash(&path, "entries.idx");
        if damage != "dictionary" {
            assert!(
                Dictionary::open(&path).is_ok(),
                "ordinary open must stay lightweight: {damage}"
            );
        }
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
    manifest["schema_version"] = 3.into();
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

#[test]
fn blocks_cover_unsorted_input_and_training_is_reproducible() {
    // Enough varied text for a trained dictionary and multiple 1 MiB blocks.
    let dir = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    for i in (0..1024).rev() {
        let key = format!("word{i:04}");
        let text = (0..2048)
            .map(|j| char::from(b'a' + ((i * 17 + j * 13 + j / 7) % 26) as u8))
            .collect::<String>();
        rows.push(serde_json::json!({"key":key,"headword":key,"score":i,
            "source_url":"source", "groups":[{"headword":key,"pos":"noun","senses":[{"glosses":[text]}]}]}).to_string());
    }
    let input = dir.path().join("entries.jsonl");
    let source = dir.path().join("source.json");
    fs::write(&input, rows.join("\r\n") + "\r\n\n").unwrap();
    fs::write(&source, "{}").unwrap();
    for name in ["a", "b"] {
        build_pack(&input, &source, &dir.path().join(name)).unwrap();
    }
    let path = dir.path().join("a");
    for name in [
        "entries.bin",
        "entries.idx",
        "lexicon.bin",
        "words.fst",
        "manifest.json",
    ] {
        assert_eq!(
            fs::read(path.join(name)).unwrap(),
            fs::read(dir.path().join("b").join(name)).unwrap()
        );
    }
    let table = fs::read(path.join("entries.idx")).unwrap();
    let blocks = u32::from_le_bytes(table[12..16].try_into().unwrap());
    let dictionary = u32::from_le_bytes(table[16..20].try_into().unwrap());
    assert!(blocks > 1);
    assert!(dictionary > 0 && dictionary <= 65536);
    assert_eq!(verify_pack(&path).unwrap(), 1024);
    let mut dict = Dictionary::open(&path).unwrap();
    for i in [0, 1023, 500, 0] {
        let key = format!("word{i:04}");
        let candidate = dict.search(&key).unwrap().remove(0);
        assert_eq!(dict.preview(&candidate).unwrap().entry.key, key);
    }
    // Same-sized wrong training dictionary: open stays cheap, reads/verify reject it.
    let mut table = table;
    table[32..32 + dictionary as usize].fill(0);
    fs::write(path.join("entries.idx"), table).unwrap();
    rehash(&path, "entries.idx");
    let mut fresh = Dictionary::open(&path).unwrap();
    let candidate = wordglide::Candidate {
        key: "word0000".into(),
        headword: "word0000".into(),
        score: 0,
        kind: MatchKind::Exact,
        parts_of_speech: vec!["noun".into()],
    };
    assert!(fresh.preview(&candidate).is_err());
    assert!(verify_pack(&path).is_err());
}

#[test]
fn block_directory_gaps_and_overlaps_and_bad_ids_are_rejected() {
    for field in [
        "offset",
        "entry_id",
        "block_id",
        "first_offset",
        "compressed_len",
    ] {
        let dir = pack();
        let path = dir.path().join("pack");
        let mut table = fs::read(path.join("entries.idx")).unwrap();
        match field {
            "offset" => table[32..40].copy_from_slice(&1u64.to_le_bytes()),
            "entry_id" => table[48..52].copy_from_slice(&1u32.to_le_bytes()),
            "block_id" => table[56..60].copy_from_slice(&u32::MAX.to_le_bytes()),
            "first_offset" => table[60..64].copy_from_slice(&1u32.to_le_bytes()),
            "compressed_len" => table[40..44].copy_from_slice(&2097153u32.to_le_bytes()),
            _ => unreachable!(),
        }
        fs::write(path.join("entries.idx"), table).unwrap();
        rehash(&path, "entries.idx");
        assert!(verify_pack(&path).is_err(), "missed {field}");
    }
}
