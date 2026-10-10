//! Private schema-4 locator table and read-only, byte-bounded block cache.
use crate::{Entry, entry_codec};
use anyhow::{Context, Result, ensure};
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    ops::Range,
    path::Path,
};

pub(crate) const MAGIC: &[u8; 8] = b"WGLENT04";
pub(crate) const HEADER: usize = 32;
pub(crate) const BLOCK_RECORD: usize = 24;
const LOCATION: usize = 8;
const CACHE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct Block {
    pub offset: u64,
    pub compressed_len: usize,
    pub raw_len: usize,
    pub first: usize,
    pub count: usize,
}

pub(crate) struct Entries {
    data: Vec<u8>,
    count: usize,
    blocks: usize,
    directory: usize,
    locations: usize,
    file_len: u64,
    file: File,
    decoder: zstd::bulk::Decompressor<'static>,
    cache: VecDeque<(usize, Vec<u8>)>,
    cache_bytes: usize,
}

fn u32_at(data: &[u8], start: usize) -> usize {
    u32::from_le_bytes(data[start..start + 4].try_into().unwrap()) as usize
}

impl Entries {
    pub(crate) fn open(path: &Path, expected: usize) -> Result<Self> {
        let data = std::fs::read(path.join("entries.idx"))?;
        ensure!(
            data.len() >= HEADER && &data[..8] == MAGIC,
            "Corrupt entry index header"
        );
        let count = u32_at(&data, 8);
        let blocks = u32_at(&data, 12);
        let dictionary_len = u32_at(&data, 16);
        ensure!(
            count == expected
                && count > 0
                && blocks > 0
                && blocks <= count
                && dictionary_len <= entry_codec::MAX_DICTIONARY
                && data[20..24] == [0; 4],
            "Corrupt entry index counts"
        );
        let file_len = u64::from_le_bytes(data[24..32].try_into()?);
        let directory = HEADER + dictionary_len;
        let locations = blocks
            .checked_mul(BLOCK_RECORD)
            .and_then(|n| n.checked_add(directory))
            .context("Corrupt block directory size")?;
        ensure!(
            count
                .checked_mul(LOCATION)
                .and_then(|n| n.checked_add(locations))
                == Some(data.len()),
            "Corrupt entry index length"
        );
        let file = File::open(path.join("entries.bin"))?;
        ensure!(
            file_len > 0 && file.metadata()?.len() == file_len,
            "Corrupt entry data length"
        );
        let mut decoder = zstd::bulk::Decompressor::with_dictionary(&data[HEADER..directory])?;
        decoder.window_log_max(20)?;
        Ok(Self {
            data,
            count,
            blocks,
            directory,
            locations,
            file_len,
            file,
            decoder,
            cache: VecDeque::new(),
            cache_bytes: 0,
        })
    }

    fn block(&self, id: usize) -> Result<Block> {
        ensure!(id < self.blocks, "Corrupt block ID");
        let start = self.directory + id * BLOCK_RECORD;
        let b = &self.data[start..start + BLOCK_RECORD];
        let block = Block {
            offset: u64::from_le_bytes(b[..8].try_into()?),
            compressed_len: u32_at(b, 8),
            raw_len: u32_at(b, 12),
            first: u32_at(b, 16),
            count: u32_at(b, 20),
        };
        ensure!(
            (1..=entry_codec::MAX_COMPRESSED).contains(&block.compressed_len)
                && (1..=entry_codec::MAX_RAW).contains(&block.raw_len)
                && block.count > 0
                && block
                    .first
                    .checked_add(block.count)
                    .is_some_and(|n| n <= self.count)
                && block
                    .offset
                    .checked_add(block.compressed_len as u64)
                    .is_some_and(|n| n <= self.file_len),
            "Corrupt block range"
        );
        Ok(block)
    }

    fn locator(&self, id: usize) -> Result<(usize, usize)> {
        ensure!(id < self.count, "Corrupt entry ID");
        let start = self.locations + id * LOCATION;
        Ok((u32_at(&self.data, start), u32_at(&self.data, start + 4)))
    }

    fn location(&self, id: usize) -> Result<(usize, Block, Range<usize>)> {
        let (block_id, start) = self.locator(id)?;
        let block = self.block(block_id)?;
        ensure!(
            id >= block.first && id < block.first + block.count,
            "Corrupt entry/block agreement"
        );
        if id == block.first {
            ensure!(start == 0, "Corrupt first entry offset");
        } else {
            let (previous, offset) = self.locator(id - 1)?;
            ensure!(
                previous == block_id && offset < start,
                "Corrupt entry order"
            );
        }
        let end = if id + 1 < block.first + block.count {
            let (next, offset) = self.locator(id + 1)?;
            ensure!(next == block_id, "Corrupt next entry block");
            offset
        } else {
            block.raw_len
        };
        ensure!(start < end && end <= block.raw_len, "Corrupt entry offset");
        Ok((block_id, block, start..end))
    }

    pub(crate) fn read(&mut self, id: usize, key: &str) -> Result<Entry> {
        let (block_id, block, range) = self.location(id)?;
        if let Some(i) = self.cache.iter().position(|(b, _)| *b == block_id) {
            let cached = self.cache.remove(i).unwrap();
            self.cache.push_back(cached);
        } else {
            let mut compressed = vec![0; block.compressed_len];
            self.file.seek(SeekFrom::Start(block.offset))?;
            self.file.read_exact(&mut compressed)?;
            let raw = entry_codec::decompress(&mut self.decoder, block.raw_len, &compressed)?;
            while self.cache_bytes + raw.len() > CACHE_BYTES {
                let (_, old) = self
                    .cache
                    .pop_front()
                    .context("Invalid block cache budget")?;
                self.cache_bytes -= old.len();
            }
            self.cache_bytes += raw.len();
            self.cache.push_back((block_id, raw));
        }
        let raw = &self.cache.back().unwrap().1[range.clone()];
        entry_codec::decode(key, raw)
    }

    // Full traversal is exclusive to explicit verification, never startup.
    pub(crate) fn verify_layout(&self) -> Result<()> {
        let mut offset = 0;
        let mut first = 0;
        for id in 0..self.blocks {
            let block = self.block(id)?;
            ensure!(
                block.offset == offset && block.first == first,
                "Corrupt block directory order"
            );
            offset += block.compressed_len as u64;
            first += block.count;
        }
        ensure!(
            offset == self.file_len && first == self.count,
            "Corrupt block directory coverage"
        );
        Ok(())
    }
}
