use crate::model::{Entry, Manifest, RANKING, SCHEMA_VERSION};
use crate::{Candidate, MatchKind, entry_codec, index::Index, normalize};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

pub(crate) fn checksum(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Assemble a pack from canonical source-grounded JSONL and its provenance JSON.
/// Output must not exist. An interrupted build has no manifest and cannot be opened.
pub fn build_pack(input: &Path, provenance: &Path, output: &Path) -> Result<()> {
    ensure!(
        !output.exists(),
        "Output already exists: {} (choose a new directory)",
        output.display()
    );
    let source: serde_json::Value = serde_json::from_slice(&fs::read(provenance)?)?;
    ensure!(source.is_object(), "Provenance must be a JSON object");
    if let Some(ranking) = source.get("ranking") {
        ensure!(
            ranking.as_str() == Some(RANKING),
            "Incompatible prepared ranking policy; re-prepare the data"
        );
    }
    fs::create_dir_all(output)?;
    let mut conn = Connection::open(output.join("entries.sqlite"))?;
    conn.execute_batch("CREATE TABLE entries(key TEXT PRIMARY KEY, raw_len INTEGER NOT NULL, payload BLOB NOT NULL) WITHOUT ROWID; PRAGMA user_version=3;")?;
    let tx = conn.transaction()?;
    let mut count = 0;
    let mut words = Vec::new();
    {
        let mut insert = tx.prepare("INSERT INTO entries(key,raw_len,payload) VALUES(?1,?2,?3)")?;
        for (line_no, line) in BufReader::new(File::open(input)?).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: Entry = serde_json::from_str(&line)
                .with_context(|| format!("Canonical input line {}", line_no + 1))?;
            ensure!(
                !entry.key.is_empty() && normalize(&entry.key) == entry.key,
                "Invalid normalized key at line {}",
                line_no + 1
            );
            ensure!(
                !entry.headword.is_empty() && !entry.groups.is_empty(),
                "Empty entry at line {}",
                line_no + 1
            );
            ensure!(
                entry.groups.iter().all(|g| !g.pos.is_empty()
                    && !g.pos.chars().any(char::is_control)
                    && !g.senses.is_empty()
                    && g.senses.iter().all(|s| !s.glosses.is_empty()
                        && s.glosses.iter().all(|text| !text.trim().is_empty())
                        && s.examples.len() <= 2
                        && s.examples.iter().all(|e| !e.text.trim().is_empty()))),
                "Invalid senses at line {}",
                line_no + 1
            );
            let parts_of_speech = entry.parts_of_speech();
            let (raw_len, payload) = entry_codec::encode(&entry)?;
            insert
                .execute(params![entry.key, raw_len as i64, payload])
                .with_context(|| format!("Duplicate/invalid entry on line {}", line_no + 1))?;
            words.push(Candidate {
                key: entry.key,
                headword: entry.headword,
                score: entry.score,
                kind: MatchKind::Prefix,
                parts_of_speech,
            });
            count += 1;
        }
    }
    ensure!(count > 0, "No entries were supplied");
    tx.commit()?;
    drop(conn);
    words.sort_by(|a, b| a.key.cmp(&b.key));
    let mut builder = fst::MapBuilder::new(File::create(output.join("words.fst"))?)?;
    for (i, word) in words.iter().enumerate() {
        builder.insert(&word.key, i as u64)?;
    }
    builder.finish()?;
    let data = Index::encode(&words)?;
    fs::write(output.join("lexicon.bin"), &data)?;
    Index::open(data, fs::read(output.join("words.fst"))?)?.verify()?;
    drop(words);
    let mut files = BTreeMap::new();
    let mut sizes = BTreeMap::new();
    for name in ["entries.sqlite", "words.fst", "lexicon.bin"] {
        files.insert(name.to_string(), checksum(&output.join(name))?);
        sizes.insert(name.to_string(), fs::metadata(output.join(name))?.len());
    }
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        candidate_count: count,
        ranking: RANKING.into(),
        source,
        files,
        sizes,
    };
    let mut file = File::create(output.join("manifest.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
