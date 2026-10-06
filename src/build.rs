use crate::model::{Entry, Manifest, RANKING, SCHEMA_VERSION};
use crate::{Candidate, MatchKind, normalize};
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
    fs::create_dir_all(output)?;
    let mut conn = Connection::open(output.join("entries.sqlite"))?;
    conn.execute_batch("CREATE TABLE entries(key TEXT PRIMARY KEY, payload TEXT NOT NULL) WITHOUT ROWID; PRAGMA user_version=1;")?;
    let tx = conn.transaction()?;
    let mut count = 0;
    {
        let mut insert = tx.prepare("INSERT INTO entries(key,payload) VALUES(?1,?2)")?;
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
                    && !g.senses.is_empty()
                    && g.senses.iter().all(|s| !s.glosses.is_empty()
                        && s.glosses.iter().all(|text| !text.trim().is_empty())
                        && s.examples.len() <= 2
                        && s.examples.iter().all(|e| !e.text.trim().is_empty()))),
                "Invalid senses at line {}",
                line_no + 1
            );
            insert
                .execute(params![entry.key, serde_json::to_string(&entry)?])
                .with_context(|| format!("Duplicate/invalid entry on line {}", line_no + 1))?;
            count += 1;
        }
    }
    ensure!(count > 0, "No entries were supplied");
    tx.commit()?;
    let mut stmt = conn.prepare("SELECT payload FROM entries ORDER BY key COLLATE BINARY")?;
    let mut rows = stmt.query([])?;
    let mut words = Vec::with_capacity(count);
    let mut builder = fst::MapBuilder::new(File::create(output.join("words.fst"))?)?;
    while let Some(row) = rows.next()? {
        let entry: Entry = serde_json::from_str(&row.get::<_, String>(0)?)?;
        builder.insert(&entry.key, words.len() as u64)?;
        words.push(Candidate {
            key: entry.key,
            headword: entry.headword,
            score: entry.score,
            kind: MatchKind::Prefix,
        });
    }
    builder.finish()?;
    serde_json::to_writer(File::create(output.join("candidates.json"))?, &words)?;
    let mut files = BTreeMap::new();
    for name in ["entries.sqlite", "words.fst", "candidates.json"] {
        files.insert(name.to_string(), checksum(&output.join(name))?);
    }
    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        candidate_count: count,
        ranking: RANKING.into(),
        source,
        files,
    };
    let mut file = File::create(output.join("manifest.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
