use crate::{Candidate, MatchKind};
use anyhow::{Context, Result, ensure};
use fst::{IntoStreamer, Streamer};
use levenshtein_automata::LevenshteinAutomatonBuilder;
use std::{cmp::Reverse, collections::BinaryHeap};

const MAGIC: &[u8; 8] = b"WGLIDX02";
const HEADER: usize = 32;
const RECORD: usize = 20;
const EMPTY: usize = u32::MAX as usize;

// All offsets and integers are decoded safely from a portable little-endian file.
// Only returned candidates allocate strings; the vocabulary remains one byte buffer.
pub(crate) struct Index {
    data: Vec<u8>,
    count: usize,
    leaves: usize,
    tree_start: usize,
    strings_start: usize,
    fst: fst::Map<Vec<u8>>,
    fuzzy: LevenshteinAutomatonBuilder,
}

impl Index {
    pub(crate) fn encode(words: &[Candidate]) -> Result<Vec<u8>> {
        ensure!(
            !words.is_empty() && words.len() < EMPTY,
            "Invalid candidate count"
        );
        ensure!(
            words.windows(2).all(|w| w[0].key < w[1].key),
            "Candidates are not sorted and unique"
        );
        let leaves = words
            .len()
            .checked_next_power_of_two()
            .context("Index too large")?;
        let mut strings = Vec::new();
        let mut records = Vec::new();
        for word in words {
            let key_offset = u32::try_from(strings.len())?;
            let key_len = u32::try_from(word.key.len())?;
            strings.extend_from_slice(word.key.as_bytes());
            let (head_offset, head_len) = if word.headword == word.key {
                (key_offset, key_len)
            } else {
                let offset = u32::try_from(strings.len())?;
                strings.extend_from_slice(word.headword.as_bytes());
                (offset, u32::try_from(word.headword.len())?)
            };
            for n in [key_offset, key_len, head_offset, head_len] {
                records.extend_from_slice(&n.to_le_bytes());
            }
            records.extend_from_slice(&word.score.to_le_bytes());
        }
        ensure!(strings.len() <= u32::MAX as usize, "String index too large");
        let best = |a: usize, b: usize| {
            if a == EMPTY {
                b
            } else if b == EMPTY || (words[a].score, Reverse(a)) >= (words[b].score, Reverse(b)) {
                a
            } else {
                b
            }
        };
        let mut tree = vec![EMPTY; 2 * leaves];
        for i in 0..words.len() {
            tree[leaves + i] = i;
        }
        for i in (1..leaves).rev() {
            tree[i] = best(tree[2 * i], tree[2 * i + 1]);
        }
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&(words.len() as u32).to_le_bytes());
        data.extend_from_slice(&u32::try_from(leaves)?.to_le_bytes());
        data.extend_from_slice(&(strings.len() as u64).to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&records);
        for i in tree {
            data.extend_from_slice(&(i as u32).to_le_bytes());
        }
        data.extend_from_slice(&strings);
        Ok(data)
    }

    pub(crate) fn open(data: Vec<u8>, bytes: Vec<u8>) -> Result<Self> {
        ensure!(
            data.len() >= HEADER && &data[..8] == MAGIC,
            "Corrupt compact index header"
        );
        let u32_at =
            |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let count = u32_at(8);
        let leaves = u32_at(12);
        let strings_len = usize::try_from(u64::from_le_bytes(data[16..24].try_into().unwrap()))?;
        ensure!(
            count > 0 && count < EMPTY && count.checked_next_power_of_two() == Some(leaves),
            "Corrupt index counts"
        );
        ensure!(data[24..32] == [0; 8], "Unsupported index header");
        let tree_start = count
            .checked_mul(RECORD)
            .and_then(|n| n.checked_add(HEADER))
            .context("Corrupt index size")?;
        let strings_start = leaves
            .checked_mul(8)
            .and_then(|n| n.checked_add(tree_start))
            .context("Corrupt index size")?;
        ensure!(
            strings_start.checked_add(strings_len) == Some(data.len()),
            "Corrupt index section lengths"
        );
        // The dependency does not validate nonzero root addresses before traversal.
        ensure!(bytes.len() >= 36, "Corrupt FST header");
        let version = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let footer = bytes.len() - 12;
        let root = usize::try_from(u64::from_le_bytes(
            bytes[footer..footer + 8].try_into().unwrap(),
        ))?;
        ensure!(
            version == 3
                && ((root == 0 && bytes.len() == 36) || root.checked_add(21) == Some(bytes.len())),
            "Corrupt FST root address"
        );
        let fst = fst::Map::new(bytes).context("Invalid fuzzy index")?;
        // CRC checks only the small, already-loaded FST buffer, not dictionary files
        // or vocabulary traversal. It prevents damaged nodes from panicking later.
        fst.as_fst().verify().context("Corrupt FST checksum")?;
        ensure!(
            fst.len() == count,
            "Fuzzy index count differs from vocabulary"
        );
        Ok(Self {
            data,
            count,
            leaves,
            tree_start,
            strings_start,
            fst,
            fuzzy: LevenshteinAutomatonBuilder::new(1, true),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.count
    }

    fn record(&self, i: usize) -> Result<&[u8]> {
        ensure!(i < self.count, "Corrupt candidate index");
        Ok(&self.data[HEADER + i * RECORD..HEADER + (i + 1) * RECORD])
    }

    fn text(&self, i: usize, field: usize) -> Result<&str> {
        let record = self.record(i)?;
        let offset = u32::from_le_bytes(record[field..field + 4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(record[field + 4..field + 8].try_into().unwrap()) as usize;
        let end = offset.checked_add(len).context("Corrupt string range")?;
        let bytes = self.data[self.strings_start..]
            .get(offset..end)
            .context("Corrupt string offset")?;
        std::str::from_utf8(bytes).context("Corrupt index UTF-8")
    }

    pub(crate) fn key(&self, i: usize) -> Result<&str> {
        self.text(i, 0)
    }
    pub(crate) fn score(&self, i: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(
            self.record(i)?[16..20].try_into().unwrap(),
        ))
    }

    fn tree(&self, i: usize) -> Result<usize> {
        ensure!(i < 2 * self.leaves, "Corrupt tree offset");
        let start = self.tree_start + 4 * i;
        let value = u32::from_le_bytes(self.data[start..start + 4].try_into().unwrap()) as usize;
        ensure!(
            value == EMPTY || value < self.count,
            "Corrupt tree candidate"
        );
        Ok(value)
    }

    fn best(&self, a: usize, b: usize) -> Result<usize> {
        Ok(if a == EMPTY {
            b
        } else if b == EMPTY || (self.score(a)?, Reverse(a)) >= (self.score(b)?, Reverse(b)) {
            a
        } else {
            b
        })
    }

    // Full traversal belongs to explicit verification and build time, never normal open.
    pub(crate) fn verify(&self) -> Result<()> {
        let mut stream = self.fst.stream();
        let mut previous = None;
        for i in 0..self.count {
            let key = self.key(i)?;
            ensure!(
                !key.is_empty() && previous.is_none_or(|p| p < key),
                "Corrupt vocabulary order"
            );
            ensure!(!self.text(i, 8)?.is_empty(), "Corrupt display word");
            let (fst_key, value) = stream.next().context("Incomplete fuzzy index")?;
            ensure!(
                fst_key == key.as_bytes() && value == i as u64,
                "Fuzzy index differs from vocabulary"
            );
            previous = Some(key);
        }
        ensure!(stream.next().is_none(), "Unexpected fuzzy index word");
        ensure!(self.tree(0)? == EMPTY, "Corrupt unused tree root");
        for i in 0..self.leaves {
            ensure!(
                self.tree(self.leaves + i)? == if i < self.count { i } else { EMPTY },
                "Corrupt tree leaf"
            );
        }
        for i in (1..self.leaves).rev() {
            ensure!(
                self.tree(i)? == self.best(self.tree(2 * i)?, self.tree(2 * i + 1)?)?,
                "Corrupt tree ranking"
            );
        }
        Ok(())
    }

    fn range_best(&self, mut left: usize, mut right: usize) -> Result<usize> {
        left += self.leaves;
        right += self.leaves;
        let mut winner = EMPTY;
        while left < right {
            if left % 2 == 1 {
                winner = self.best(winner, self.tree(left)?)?;
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                winner = self.best(winner, self.tree(right)?)?;
            }
            left /= 2;
            right /= 2;
        }
        Ok(winner)
    }

    pub(crate) fn exact(&self, key: &str) -> Option<usize> {
        self.fst
            .get(key)
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| i < self.count)
    }

    fn prefix_range(&self, query: &str) -> Result<(usize, usize)> {
        let mut left = 0;
        let mut right = self.count;
        while left < right {
            let middle = left + (right - left) / 2;
            if self.key(middle)? < query {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        let start = left;
        right = self.count;
        while left < right {
            let middle = left + (right - left) / 2;
            let key = self.key(middle)?;
            if key < query || key.starts_with(query) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        Ok((start, left))
    }

    pub(crate) fn common_prefix(&self, query: &str) -> Result<Option<String>> {
        if query.is_empty() {
            return Ok(None);
        }
        let (left, right) = self.prefix_range(query)?;
        if left == right {
            return Ok(None);
        }
        Ok(Some(
            self.key(left)?
                .chars()
                .zip(self.key(right - 1)?.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        ))
    }

    pub(crate) fn prefix(&self, query: &str, limit: usize) -> Result<Vec<usize>> {
        if query.is_empty() {
            return Ok(vec![]);
        }
        let (left, right) = self.prefix_range(query)?;
        let mut heap = BinaryHeap::new();
        self.push_range(&mut heap, left, right)?;
        let mut results = Vec::with_capacity(limit);
        while results.len() < limit {
            let Some((_, Reverse(i), left, right)) = heap.pop() else {
                break;
            };
            results.push(i);
            self.push_range(&mut heap, left, i)?;
            self.push_range(&mut heap, i + 1, right)?;
        }
        Ok(results)
    }

    fn push_range(
        &self,
        heap: &mut BinaryHeap<(i32, Reverse<usize>, usize, usize)>,
        left: usize,
        right: usize,
    ) -> Result<()> {
        if left < right {
            let i = self.range_best(left, right)?;
            ensure!(i >= left && i < right, "Corrupt tree range winner");
            heap.push((self.score(i)?, Reverse(i), left, right));
        }
        Ok(())
    }

    pub(crate) fn fuzzy(
        &self,
        query: &str,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        if !(3..=64).contains(&query.chars().count()) || limit == 0 {
            return Ok(vec![]);
        }
        let dfa = self.fuzzy.build_dfa(query);
        let mut stream = self.fst.search(&dfa).into_stream();
        let mut top: BinaryHeap<Reverse<(i32, Reverse<usize>)>> = BinaryHeap::new();
        while let Some((_, value)) = stream.next() {
            let i = usize::try_from(value)?;
            if excluded.contains(&i) {
                continue;
            }
            top.push(Reverse((self.score(i)?, Reverse(i))));
            if top.len() > limit {
                top.pop();
            }
        }
        let mut ranked: Vec<_> = top.into_iter().map(|Reverse(rank)| rank).collect();
        ranked.sort_unstable_by_key(|&(score, Reverse(i))| (Reverse(score), i));
        Ok(ranked.into_iter().map(|(_, Reverse(i))| i).collect())
    }

    pub(crate) fn candidate(&self, i: usize, kind: MatchKind) -> Result<Candidate> {
        Ok(Candidate {
            key: self.key(i)?.into(),
            headword: self.text(i, 8)?.into(),
            score: self.score(i)?,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_matches_exhaustive_oracle_with_ties() {
        let words: Vec<_> = (0..2000)
            .map(|i| Candidate {
                key: format!("h{i:04}"),
                headword: format!("h{i:04}"),
                score: (i * 37) % 101,
                kind: MatchKind::Prefix,
            })
            .collect();
        let mut builder = fst::MapBuilder::memory();
        for (i, w) in words.iter().enumerate() {
            builder.insert(&w.key, i as u64).unwrap();
        }
        let index = Index::open(
            Index::encode(&words).unwrap(),
            builder.into_inner().unwrap(),
        )
        .unwrap();
        index.verify().unwrap();
        for query in ["h", "h0", "h00", "h001", "h199", "not"] {
            let mut reference: Vec<_> = (0..words.len())
                .filter(|&i| words[i].key.starts_with(query))
                .collect();
            reference.sort_by_key(|&i| (Reverse(words[i].score), i));
            reference.truncate(20);
            assert_eq!(index.prefix(query, 20).unwrap(), reference);
        }
    }
}
