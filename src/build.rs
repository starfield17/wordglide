use crate::model::{DATA_FILES, Entry, Manifest, RANKING, SCHEMA_VERSION};
use crate::{Candidate, MatchKind, entry_codec, index::Index, normalize};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
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

// F1 ← S2 / F4 ← S2: only this builder produces pack bytes; the runtime and the
// installer never call it. Enforced by scripts/test_boundaries.py.
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
    let mut reader = BufReader::new(File::open(input)?);
    let mut records = Vec::new();
    let mut offset = 0u64;
    let mut line = String::new();
    let mut line_no = 0;
    loop {
        line.clear();
        let length = reader.read_line(&mut line)?;
        if length == 0 {
            break;
        }
        line_no += 1;
        let start = offset;
        offset += length as u64;
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry = serde_json::from_str(&line)
            .with_context(|| format!("Canonical input line {line_no}"))?;
        ensure!(
            !entry.key.is_empty() && normalize(&entry.key) == entry.key,
            "Invalid normalized key at line {line_no}"
        );
        ensure!(
            !entry.headword.is_empty() && !entry.groups.is_empty(),
            "Empty entry at line {line_no}"
        );
        ensure!(
            entry.groups.iter().all(|g| !g.pos.is_empty()
                && !g.pos.chars().any(char::is_control)
                && !g.senses.is_empty()
                && g.senses.iter().all(|s| !s.glosses.is_empty()
                    && s.glosses.iter().all(|text| !text.trim().is_empty())
                    && s.examples.len() <= 2
                    && s.examples.iter().all(|e| !e.text.trim().is_empty()))),
            "Invalid senses at line {line_no}"
        );
        entry_codec::encode(&entry)?;
        let parts_of_speech = entry.parts_of_speech();
        records.push((
            Candidate {
                key: entry.key,
                headword: entry.headword,
                score: entry.score,
                kind: MatchKind::Prefix,
                parts_of_speech,
            },
            start,
        ));
    }
    ensure!(
        !records.is_empty() && records.len() < u32::MAX as usize,
        "Invalid entry count"
    );
    records.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    ensure!(
        records.windows(2).all(|r| r[0].0.key < r[1].0.key),
        "Duplicate dictionary key"
    );
    fn read_entry(reader: &mut BufReader<File>, offset: u64) -> Result<Entry> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }
    let sample_count = records.len().min(4096);
    let mut samples = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        samples.push(entry_codec::encode(&read_entry(
            &mut reader,
            records[i * records.len() / sample_count].1,
        )?)?);
    }
    let dictionary =
        if sample_count >= 256 && samples.iter().map(Vec::len).sum::<usize>() >= 256 * 1024 {
            zstd::dict::from_samples(&samples, entry_codec::MAX_DICTIONARY)
                .context("Cannot train compression dictionary")?
        } else {
            Vec::new()
        };
    drop(samples);
    let mut compressor = zstd::bulk::Compressor::with_dictionary(19, &dictionary)?;
    compressor.include_checksum(true)?;
    compressor.window_log(20)?;
    let mut body = File::create(output.join("entries.bin"))?;
    let mut directory = Vec::new();
    let mut locations = Vec::new();
    let mut raw = Vec::new();
    let mut first = 0usize;
    let mut block_count = 0u32;
    let mut body_offset = 0u64;
    fn flush_block(
        body: &mut File,
        compressor: &mut zstd::bulk::Compressor<'_>,
        raw: &mut Vec<u8>,
        directory: &mut Vec<u8>,
        first: usize,
        count: usize,
        offset: &mut u64,
    ) -> Result<()> {
        let bytes = entry_codec::compress(compressor, raw)?;
        directory.extend_from_slice(&offset.to_le_bytes());
        for n in [bytes.len(), raw.len(), first, count] {
            directory.extend_from_slice(&u32::try_from(n)?.to_le_bytes());
        }
        body.write_all(&bytes)?;
        *offset += bytes.len() as u64;
        raw.clear();
        Ok(())
    }
    for (id, (candidate, offset)) in records.iter().enumerate() {
        let entry = read_entry(&mut reader, *offset)?;
        ensure!(
            entry.key == candidate.key
                && entry.score == candidate.score
                && entry.headword == candidate.headword
                && entry.parts_of_speech() == candidate.parts_of_speech,
            "Canonical input changed during build"
        );
        let bytes = entry_codec::encode(&entry)?;
        if !raw.is_empty() && raw.len() + bytes.len() > entry_codec::MAX_RAW {
            flush_block(
                &mut body,
                &mut compressor,
                &mut raw,
                &mut directory,
                first,
                id - first,
                &mut body_offset,
            )?;
            first = id;
            block_count += 1;
        }
        locations.extend_from_slice(&block_count.to_le_bytes());
        locations.extend_from_slice(&u32::try_from(raw.len())?.to_le_bytes());
        raw.extend_from_slice(&bytes);
    }
    let count = records.len();
    flush_block(
        &mut body,
        &mut compressor,
        &mut raw,
        &mut directory,
        first,
        count - first,
        &mut body_offset,
    )?;
    block_count += 1;
    drop(body);
    let mut locator = File::create(output.join("entries.idx"))?;
    locator.write_all(crate::entry_storage::MAGIC)?;
    for n in [
        u32::try_from(count)?,
        block_count,
        u32::try_from(dictionary.len())?,
        0,
    ] {
        locator.write_all(&n.to_le_bytes())?;
    }
    locator.write_all(&body_offset.to_le_bytes())?;
    locator.write_all(&dictionary)?;
    locator.write_all(&directory)?;
    locator.write_all(&locations)?;
    drop(locator);
    let words: Vec<_> = records.into_iter().map(|r| r.0).collect();
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
    for name in DATA_FILES {
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
