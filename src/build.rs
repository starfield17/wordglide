use crate::model::{DATA_FILES, Entry, Manifest, RANKING, SCHEMA_VERSION};
use crate::{Candidate, MatchKind, entry_codec, index::Index, normalize};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
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

struct PendingBlock {
    first: usize,
    count: usize,
    raw: Vec<u8>,
}

fn read_encoded(file: &mut File, offset: u64, len: u32) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0u8; len as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn new_compressor(dictionary: &[u8]) -> Result<zstd::bulk::Compressor<'_>> {
    let mut compressor = zstd::bulk::Compressor::with_dictionary(19, dictionary)?;
    compressor.include_checksum(true)?;
    compressor.window_log(20)?;
    Ok(compressor)
}

fn write_compressed_batch(
    dictionary: &[u8],
    workers: usize,
    batch: &[PendingBlock],
    body: &mut File,
    directory: &mut Vec<u8>,
    body_offset: &mut u64,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let compressed: Vec<Vec<u8>> = if batch.len() == 1 || workers <= 1 {
        let mut compressor = new_compressor(dictionary)?;
        batch
            .iter()
            .map(|block| entry_codec::compress(&mut compressor, &block.raw))
            .collect::<Result<Vec<_>>>()?
    } else {
        let slots: Vec<Mutex<Option<Result<Vec<u8>>>>> =
            (0..batch.len()).map(|_| Mutex::new(None)).collect();
        let next = AtomicUsize::new(0);
        let worker_count = workers.min(batch.len());
        thread::scope(|scope| {
            for _ in 0..worker_count {
                scope.spawn(|| {
                    let mut compressor = match new_compressor(dictionary) {
                        Ok(c) => c,
                        Err(err) => {
                            let idx = next.fetch_add(1, Ordering::Relaxed);
                            if idx < batch.len() {
                                *slots[idx].lock().unwrap() = Some(Err(err));
                            }
                            return;
                        }
                    };
                    loop {
                        let idx = next.fetch_add(1, Ordering::Relaxed);
                        if idx >= batch.len() {
                            break;
                        }
                        let res = entry_codec::compress(&mut compressor, &batch[idx].raw);
                        *slots[idx].lock().unwrap() = Some(res);
                    }
                });
            }
        });
        slots
            .into_iter()
            .map(|slot| {
                slot.into_inner()
                    .unwrap()
                    .unwrap_or_else(|| Err(anyhow::anyhow!("Missing compressed block result")))
            })
            .collect::<Result<Vec<_>>>()?
    };

    for (block, bytes) in batch.iter().zip(compressed) {
        directory.extend_from_slice(&body_offset.to_le_bytes());
        for n in [bytes.len(), block.raw.len(), block.first, block.count] {
            directory.extend_from_slice(&u32::try_from(n)?.to_le_bytes());
        }
        body.write_all(&bytes)?;
        *body_offset += bytes.len() as u64;
    }
    Ok(())
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
    let mut encoded_writer = BufWriter::new(tempfile::tempfile()?);
    let mut encoded_offset = 0u64;
    let mut records = Vec::new();
    let mut line = String::new();
    let mut line_no = 0;
    loop {
        line.clear();
        let length = reader.read_line(&mut line)?;
        if length == 0 {
            break;
        }
        line_no += 1;
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
        let encoded = entry_codec::encode(&entry)?;
        let enc_len = u32::try_from(encoded.len())?;
        encoded_writer.write_all(&encoded)?;
        let parts_of_speech = entry.parts_of_speech();
        records.push((
            Candidate {
                key: entry.key,
                headword: entry.headword,
                score: entry.score,
                kind: MatchKind::Prefix,
                parts_of_speech,
            },
            encoded_offset,
            enc_len,
        ));
        encoded_offset += enc_len as u64;
    }
    drop(reader);
    let mut encoded_file = encoded_writer.into_inner()?;
    ensure!(
        !records.is_empty() && records.len() < u32::MAX as usize,
        "Invalid entry count"
    );
    records.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    ensure!(
        records.windows(2).all(|r| r[0].0.key < r[1].0.key),
        "Duplicate dictionary key"
    );
    let sample_count = records.len().min(4096);
    let mut samples = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        let (_, enc_offset, enc_len) = records[i * records.len() / sample_count];
        samples.push(read_encoded(&mut encoded_file, enc_offset, enc_len)?);
    }
    let dictionary =
        if sample_count >= 256 && samples.iter().map(Vec::len).sum::<usize>() >= 256 * 1024 {
            zstd::dict::from_samples(&samples, entry_codec::MAX_DICTIONARY)
                .context("Cannot train compression dictionary")?
        } else {
            Vec::new()
        };
    drop(samples);

    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 16);
    let batch_cap = (workers * 2).max(1);
    let mut body = File::create(output.join("entries.bin"))?;
    let mut directory = Vec::new();
    let mut locations = Vec::with_capacity(records.len() * 8);
    let mut raw = Vec::new();
    let mut first = 0usize;
    let mut block_count = 0u32;
    let mut body_offset = 0u64;
    let mut pending_blocks = Vec::with_capacity(batch_cap);

    for (id, (_, enc_offset, enc_len)) in records.iter().enumerate() {
        let bytes = read_encoded(&mut encoded_file, *enc_offset, *enc_len)?;
        if !raw.is_empty() && raw.len() + bytes.len() > entry_codec::MAX_RAW {
            pending_blocks.push(PendingBlock {
                first,
                count: id - first,
                raw: std::mem::take(&mut raw),
            });
            first = id;
            if pending_blocks.len() >= batch_cap {
                write_compressed_batch(
                    &dictionary,
                    workers,
                    &pending_blocks,
                    &mut body,
                    &mut directory,
                    &mut body_offset,
                )?;
                block_count += u32::try_from(pending_blocks.len())?;
                pending_blocks.clear();
            }
        }
        let current_block_id = block_count + u32::try_from(pending_blocks.len())?;
        locations.extend_from_slice(&current_block_id.to_le_bytes());
        locations.extend_from_slice(&u32::try_from(raw.len())?.to_le_bytes());
        raw.extend_from_slice(&bytes);
    }
    drop(encoded_file);

    let count = records.len();
    pending_blocks.push(PendingBlock {
        first,
        count: count - first,
        raw,
    });
    write_compressed_batch(
        &dictionary,
        workers,
        &pending_blocks,
        &mut body,
        &mut directory,
        &mut body_offset,
    )?;
    block_count += u32::try_from(pending_blocks.len())?;
    drop(pending_blocks);
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
