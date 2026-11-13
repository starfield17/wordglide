use crate::{Candidate, MatchKind};
use anyhow::{Context, Result, ensure};
use fst::{IntoStreamer, Streamer};
use levenshtein_automata::LevenshteinAutomatonBuilder;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    sync::OnceLock,
};

const MAGIC: &[u8; 8] = b"WGLIDX04";
const HEADER: usize = 32;
const RECORD: usize = 24;
const EMPTY: usize = u32::MAX as usize;

// All offsets and integers are decoded safely from a portable little-endian file.
// Only returned candidates allocate strings; the vocabulary remains one byte buffer.
pub(crate) struct Index {
    data: Vec<u8>,
    count: usize,
    leaves: usize,
    tree_start: usize,
    strings_start: usize,
    parts_start: usize,
    parts_count: usize,
    fst: fst::Map<Vec<u8>>,
    fuzzy: LevenshteinAutomatonBuilder,
    fuzzy2: OnceLock<LevenshteinAutomatonBuilder>,
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
        let mut parts_pool = HashMap::new();
        let mut parts_ranges = Vec::new();
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
            ensure!(
                !word.parts_of_speech.is_empty()
                    && word
                        .parts_of_speech
                        .iter()
                        .all(|p| !p.is_empty() && !p.contains('\0'))
                    && word.parts_of_speech.windows(2).all(|p| p[0] < p[1]),
                "Invalid candidate parts of speech"
            );
            let parts = word.parts_of_speech.join("\0");
            let id = if let Some(&id) = parts_pool.get(&parts) {
                id
            } else {
                let id = u32::try_from(parts_ranges.len())?;
                parts_ranges.push((u32::try_from(strings.len())?, u32::try_from(parts.len())?));
                strings.extend_from_slice(parts.as_bytes());
                parts_pool.insert(parts, id);
                id
            };
            records.extend_from_slice(&id.to_le_bytes());
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
        data.extend_from_slice(&u32::try_from(parts_ranges.len())?.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&records);
        for &i in &tree[..leaves] {
            data.extend_from_slice(&(i as u32).to_le_bytes());
        }
        for (offset, len) in parts_ranges {
            data.extend_from_slice(&offset.to_le_bytes());
            data.extend_from_slice(&len.to_le_bytes());
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
        let parts_count = u32_at(24);
        ensure!(
            parts_count > 0 && parts_count <= count && data[28..32] == [0; 4],
            "Unsupported index header"
        );
        let tree_start = count
            .checked_mul(RECORD)
            .and_then(|n| n.checked_add(HEADER))
            .context("Corrupt index size")?;
        let parts_start = leaves
            .checked_mul(4)
            .and_then(|n| n.checked_add(tree_start))
            .context("Corrupt index size")?;
        let strings_start = parts_count
            .checked_mul(8)
            .and_then(|n| n.checked_add(parts_start))
            .context("Corrupt parts table size")?;
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
            parts_start,
            parts_count,
            fst,
            fuzzy: LevenshteinAutomatonBuilder::new(1, true),
            fuzzy2: OnceLock::new(),
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
        self.string_range(record, field)
    }

    fn string_range(&self, record: &[u8], field: usize) -> Result<&str> {
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

    pub(crate) fn parts_of_speech(&self, i: usize) -> Result<Vec<String>> {
        let record = self.record(i)?;
        let id = u32::from_le_bytes(record[20..24].try_into()?) as usize;
        ensure!(id < self.parts_count, "Corrupt parts ID");
        let start = self.parts_start + 8 * id;
        let text = self.string_range(&self.data[start..start + 8], 0)?;
        let parts: Vec<_> = text.split('\0').map(str::to_owned).collect();
        ensure!(
            parts
                .iter()
                .all(|p| !p.is_empty() && !p.chars().any(char::is_control))
                && parts.windows(2).all(|p| p[0] < p[1]),
            "Corrupt candidate parts of speech"
        );
        Ok(parts)
    }

    fn tree(&self, i: usize) -> Result<usize> {
        ensure!(i < 2 * self.leaves, "Corrupt tree offset");
        if i >= self.leaves {
            let id = i - self.leaves;
            return Ok(if id < self.count { id } else { EMPTY });
        }
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
            self.parts_of_speech(i)?;
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
        self.search_automaton(&dfa, limit, excluded)
    }

    pub(crate) fn fuzzy_two(
        &self,
        query: &str,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        if !(6..=64).contains(&query.chars().count()) || limit == 0 {
            return Ok(vec![]);
        }
        let builder = self
            .fuzzy2
            .get_or_init(|| LevenshteinAutomatonBuilder::new(2, true));
        let dfa = builder.build_dfa(query);
        self.search_automaton(&dfa, limit, excluded)
    }

    pub(crate) fn wildcard(
        &self,
        pattern: &str,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        if pattern.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        if !pattern.chars().any(|c| c != '*') {
            return self.top_in_range(0, self.count, limit, excluded);
        }
        if let Some(lit) = pattern.strip_suffix('*')
            && !lit.contains(['*', '?'])
        {
            let (left, right) = self.prefix_range(lit)?;
            return self.top_in_range(left, right, limit, excluded);
        }
        let Some(automaton) = WildcardAutomaton::new(pattern) else {
            return Ok(vec![]);
        };
        if pattern.starts_with(['*', '?']) {
            return self.scan_wildcard(&automaton, pattern, limit, excluded);
        }
        self.search_automaton(&automaton, limit, excluded)
    }

    pub(crate) fn folded_fallback(
        &self,
        query: &str,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        let folded_query = crate::normalize::fold_diacritics(query);
        if folded_query.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let query_ascii = query.is_ascii() && query == folded_query;
        let mut top: BinaryHeap<Reverse<(u8, i32, Reverse<usize>)>> = BinaryHeap::new();
        for i in 0..self.count {
            if excluded.contains(&i) {
                continue;
            }
            let key = self.key(i)?;
            if query_ascii && key.is_ascii() {
                continue;
            }
            let folded_key = if key.is_ascii() {
                std::borrow::Cow::Borrowed(key)
            } else {
                std::borrow::Cow::Owned(crate::normalize::fold_diacritics(key))
            };
            let kind = if folded_key.as_ref() == folded_query {
                2u8
            } else if folded_key.starts_with(&folded_query) {
                1u8
            } else {
                continue;
            };
            top.push(Reverse((kind, self.score(i)?, Reverse(i))));
            if top.len() > limit {
                top.pop();
            }
        }
        let mut ranked: Vec<_> = top.into_iter().map(|Reverse(rank)| rank).collect();
        ranked
            .sort_unstable_by_key(|&(kind, score, Reverse(i))| (Reverse(kind), Reverse(score), i));
        Ok(ranked.into_iter().map(|(_, _, Reverse(i))| i).collect())
    }

    fn top_in_range(
        &self,
        left: usize,
        right: usize,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        let mut heap = BinaryHeap::new();
        self.push_range(&mut heap, left, right)?;
        let mut results = Vec::with_capacity(limit);
        while results.len() < limit {
            let Some((_, Reverse(i), left, right)) = heap.pop() else {
                break;
            };
            if !excluded.contains(&i) {
                results.push(i);
            }
            self.push_range(&mut heap, left, i)?;
            self.push_range(&mut heap, i + 1, right)?;
        }
        Ok(results)
    }

    fn search_automaton<A: fst::Automaton>(
        &self,
        automaton: A,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        let mut stream = self.fst.search(automaton).into_stream();
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

    fn scan_wildcard(
        &self,
        automaton: &WildcardAutomaton,
        pattern: &str,
        limit: usize,
        excluded: &[usize],
    ) -> Result<Vec<usize>> {
        let records = &self.data[HEADER..self.tree_start];
        let strings = &self.data[self.strings_start..];
        let mut top: BinaryHeap<Reverse<(i32, Reverse<usize>)>> = BinaryHeap::new();
        let push = |top: &mut BinaryHeap<Reverse<(i32, Reverse<usize>)>>, score: i32, i: usize| {
            top.push(Reverse((score, Reverse(i))));
            if top.len() > limit {
                top.pop();
            }
        };
        let below_cutoff =
            |top: &BinaryHeap<Reverse<(i32, Reverse<usize>)>>, score: i32, i: usize| {
                top.len() == limit
                    && top
                        .peek()
                        .is_some_and(|Reverse(min)| (score, Reverse(i)) <= *min)
            };

        let suffix = pattern.trim_start_matches('*');
        if !suffix.is_empty() && !suffix.contains(['*', '?']) {
            let needle = suffix.as_bytes();
            let last = needle[needle.len() - 1];
            for (i, rec) in records.as_chunks::<RECORD>().0.iter().enumerate() {
                let len = u32::from_le_bytes(rec[4..8].try_into().unwrap()) as usize;
                if len < needle.len() {
                    continue;
                }
                let score = i32::from_le_bytes(rec[16..20].try_into().unwrap());
                if below_cutoff(&top, score, i) {
                    continue;
                }
                let offset = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                let end = offset.checked_add(len).context("Corrupt string range")?;
                let bytes = strings.get(offset..end).context("Corrupt string offset")?;
                if bytes[len - 1] != last || !bytes.ends_with(needle) {
                    continue;
                }
                std::str::from_utf8(bytes).context("Corrupt index UTF-8")?;
                if excluded.contains(&i) {
                    continue;
                }
                push(&mut top, score, i);
            }
        } else {
            let trimmed = pattern.trim_matches('*');
            if !trimmed.is_empty() && !trimmed.contains(['*', '?']) {
                let needle = trimmed.as_bytes();
                for (i, rec) in records.as_chunks::<RECORD>().0.iter().enumerate() {
                    let len = u32::from_le_bytes(rec[4..8].try_into().unwrap()) as usize;
                    if len < needle.len() {
                        continue;
                    }
                    let score = i32::from_le_bytes(rec[16..20].try_into().unwrap());
                    if below_cutoff(&top, score, i) {
                        continue;
                    }
                    let offset = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                    let end = offset.checked_add(len).context("Corrupt string range")?;
                    let bytes = strings.get(offset..end).context("Corrupt string offset")?;
                    if !contains_bytes(bytes, needle) {
                        continue;
                    }
                    std::str::from_utf8(bytes).context("Corrupt index UTF-8")?;
                    if excluded.contains(&i) {
                        continue;
                    }
                    push(&mut top, score, i);
                }
            } else {
                let min_bytes: usize = pattern
                    .chars()
                    .map(|c| match c {
                        '*' => 0,
                        '?' => 1,
                        _ => c.len_utf8(),
                    })
                    .sum();
                let tail = pattern.rsplit(['*', '?']).next().unwrap_or("").as_bytes();
                let longest = pattern
                    .split(['*', '?'])
                    .max_by_key(|seg| seg.len())
                    .unwrap_or("")
                    .as_bytes();
                let anchor = if longest == tail {
                    b"".as_slice()
                } else {
                    longest
                };
                for (i, rec) in records.as_chunks::<RECORD>().0.iter().enumerate() {
                    let len = u32::from_le_bytes(rec[4..8].try_into().unwrap()) as usize;
                    if len < min_bytes {
                        continue;
                    }
                    let score = i32::from_le_bytes(rec[16..20].try_into().unwrap());
                    if below_cutoff(&top, score, i) {
                        continue;
                    }
                    let offset = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                    let end = offset.checked_add(len).context("Corrupt string range")?;
                    let bytes = strings.get(offset..end).context("Corrupt string offset")?;
                    if !tail.is_empty() && !bytes.ends_with(tail) {
                        continue;
                    }
                    if !anchor.is_empty() && !contains_bytes(bytes, anchor) {
                        continue;
                    }
                    if !automaton.matches_bytes(bytes) {
                        continue;
                    }
                    std::str::from_utf8(bytes).context("Corrupt index UTF-8")?;
                    if excluded.contains(&i) {
                        continue;
                    }
                    push(&mut top, score, i);
                }
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
            parts_of_speech: self.parts_of_speech(i)?,
        })
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    let first = needle[0];
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let Some(pos) = haystack[i..=limit].iter().position(|&b| b == first) else {
            return false;
        };
        let start = i + pos;
        if haystack[start..start + needle.len()] == *needle {
            return true;
        }
        i = start + 1;
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BitState([u64; 4]);

impl BitState {
    fn insert(&mut self, index: usize) {
        self.0[index >> 6] |= 1u64 << (index & 63);
    }

    fn contains(&self, index: usize) -> bool {
        (self.0[index >> 6] & (1u64 << (index & 63))) != 0
    }
}

enum WildStep {
    Byte(u8),
    Utf8Lead {
        cont1: u8,
        cont2: u8,
        cont3: u8,
        done: u8,
    },
    Utf8Cont(u8),
}

struct WildcardAutomaton {
    steps: Vec<WildStep>,
    star_eps: Vec<(usize, usize)>,
    accept: usize,
}

impl WildcardAutomaton {
    fn new(pattern: &str) -> Option<Self> {
        let mut steps = Vec::new();
        let mut star_eps = Vec::new();
        let mut prev_star = false;
        let mut tokens = 0usize;
        for ch in pattern.chars() {
            match ch {
                '*' => {
                    if prev_star {
                        continue;
                    }
                    prev_star = true;
                    tokens += 1;
                    if tokens > 63 {
                        return None;
                    }
                    let base = steps.len() as u8;
                    steps.push(WildStep::Utf8Lead {
                        cont1: base + 3,
                        cont2: base + 2,
                        cont3: base + 1,
                        done: base,
                    });
                    steps.push(WildStep::Utf8Cont(base + 2));
                    steps.push(WildStep::Utf8Cont(base + 3));
                    steps.push(WildStep::Utf8Cont(base));
                    star_eps.push((base as usize, (base + 4) as usize));
                }
                '?' => {
                    prev_star = false;
                    tokens += 1;
                    if tokens > 63 {
                        return None;
                    }
                    let base = steps.len() as u8;
                    steps.push(WildStep::Utf8Lead {
                        cont1: base + 3,
                        cont2: base + 2,
                        cont3: base + 1,
                        done: base + 4,
                    });
                    steps.push(WildStep::Utf8Cont(base + 2));
                    steps.push(WildStep::Utf8Cont(base + 3));
                    steps.push(WildStep::Utf8Cont(base + 4));
                }
                _ => {
                    prev_star = false;
                    tokens += 1;
                    if tokens > 63 {
                        return None;
                    }
                    let mut buf = [0u8; 4];
                    for &b in ch.encode_utf8(&mut buf).as_bytes() {
                        steps.push(WildStep::Byte(b));
                    }
                }
            }
        }
        let accept = steps.len();
        if accept >= 256 {
            return None;
        }
        Some(Self {
            steps,
            star_eps,
            accept,
        })
    }

    fn close(&self, mut state: BitState) -> BitState {
        for &(base, after) in &self.star_eps {
            if state.contains(base) {
                state.insert(after);
            }
        }
        state
    }

    fn matches_bytes(&self, bytes: &[u8]) -> bool {
        use fst::Automaton as _;
        let mut state = self.start();
        for &b in bytes {
            state = self.accept(&state, b);
            if !self.can_match(&state) {
                return false;
            }
        }
        self.is_match(&state)
    }
}

impl fst::Automaton for WildcardAutomaton {
    type State = BitState;

    fn start(&self) -> Self::State {
        let mut state = BitState([0; 4]);
        state.insert(0);
        self.close(state)
    }

    fn is_match(&self, state: &Self::State) -> bool {
        state.contains(self.accept)
    }

    fn can_match(&self, state: &Self::State) -> bool {
        state.0 != [0; 4]
    }

    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        let mut next = BitState([0; 4]);
        for (word_idx, mut word) in state.0.into_iter().enumerate() {
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let idx = word_idx * 64 + bit;
                let Some(step) = self.steps.get(idx) else {
                    continue;
                };
                match *step {
                    WildStep::Byte(b) => {
                        if byte == b {
                            next.insert(idx + 1);
                        }
                    }
                    WildStep::Utf8Lead {
                        cont1,
                        cont2,
                        cont3,
                        done,
                    } => match byte {
                        0x00..=0x7F => next.insert(done as usize),
                        0xC2..=0xDF => next.insert(cont1 as usize),
                        0xE0..=0xEF => next.insert(cont2 as usize),
                        0xF0..=0xF4 => next.insert(cont3 as usize),
                        _ => {}
                    },
                    WildStep::Utf8Cont(target) => {
                        if matches!(byte, 0x80..=0xBF) {
                            next.insert(target as usize);
                        }
                    }
                }
            }
        }
        self.close(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // F2 ← S3: indexed top-k must equal exhaustive fixed-score ranking.
    #[test]
    fn prefix_matches_exhaustive_oracle_with_ties() {
        let words: Vec<_> = (0..2000)
            .map(|i| Candidate {
                key: format!("h{i:04}"),
                headword: format!("h{i:04}"),
                score: (i * 37) % 101,
                kind: MatchKind::Prefix,
                parts_of_speech: vec!["noun".into()],
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

    #[test]
    fn parts_lists_are_interned_and_corrupt_metadata_is_rejected() {
        let words: Vec<_> = ["alpha", "beta"]
            .into_iter()
            .map(|key| Candidate {
                key: key.into(),
                headword: key.into(),
                score: 0,
                kind: MatchKind::Prefix,
                parts_of_speech: vec!["noun".into(), "unusual-label".into(), "verb".into()],
            })
            .collect();
        let mut builder = fst::MapBuilder::memory();
        for (i, word) in words.iter().enumerate() {
            builder.insert(&word.key, i as u64).unwrap();
        }
        let fst = builder.into_inner().unwrap();
        let mut bytes = Index::encode(&words).unwrap();
        assert_eq!(
            &bytes[HEADER + 20..HEADER + 24],
            &bytes[HEADER + RECORD + 20..HEADER + RECORD + 24]
        );
        let index = Index::open(bytes.clone(), fst.clone()).unwrap();
        index.verify().unwrap();
        assert_eq!(
            index
                .candidate(0, MatchKind::Exact)
                .unwrap()
                .parts_of_speech,
            words[0].parts_of_speech
        );
        bytes[HEADER + 20..HEADER + 24].copy_from_slice(&u32::MAX.to_le_bytes());
        let index = Index::open(bytes, fst).unwrap();
        assert!(index.candidate(0, MatchKind::Exact).is_err());
        assert!(index.verify().is_err());
    }

    fn glob_match(pattern: &[char], text: &[char]) -> bool {
        match (pattern.first(), text.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                glob_match(&pattern[1..], text)
                    || (!text.is_empty() && glob_match(pattern, &text[1..]))
            }
            (Some('?'), Some(_)) => glob_match(&pattern[1..], &text[1..]),
            (Some(&p), Some(&t)) if p == t => glob_match(&pattern[1..], &text[1..]),
            _ => false,
        }
    }

    fn test_index(items: Vec<(&str, i32)>) -> (Vec<Candidate>, Index) {
        let mut words: Vec<_> = items
            .into_iter()
            .map(|(k, score)| Candidate {
                key: k.into(),
                headword: k.into(),
                score,
                kind: MatchKind::Prefix,
                parts_of_speech: vec!["noun".into()],
            })
            .collect();
        words.sort_by(|a, b| a.key.cmp(&b.key));
        let mut builder = fst::MapBuilder::memory();
        for (i, w) in words.iter().enumerate() {
            builder.insert(&w.key, i as u64).unwrap();
        }
        let index = Index::open(
            Index::encode(&words).unwrap(),
            builder.into_inner().unwrap(),
        )
        .unwrap();
        (words, index)
    }

    #[test]
    fn wildcard_matches_exhaustive_oracle_and_utf8_scalars() {
        let (words, index) = test_index(vec![
            ("accommodate", 50),
            ("baked", 40),
            ("baking", 45),
            ("café", 60),
            ("cat", 90),
            ("catnip", 90),
            ("cot", 70),
            ("cut", 85),
            ("déjà vu", 65),
            ("naïve", 75),
            ("résumé", 80),
            ("taking", 95),
            ("über", 55),
        ]);
        for pattern in [
            "*", "***", "cat*", "*ing", "**ing", "**ak**", "*a?e", "*?*é", "*café", "c?t", "?aïve",
            "caf?", "caf??", "*é*", "??*g", "absent*",
        ] {
            let pat_chars: Vec<_> = pattern.chars().collect();
            let mut oracle: Vec<_> = (0..words.len())
                .filter(|&i| {
                    let text: Vec<_> = words[i].key.chars().collect();
                    glob_match(&pat_chars, &text)
                })
                .collect();
            oracle.sort_by_key(|&i| (Reverse(words[i].score), i));
            oracle.truncate(20);
            assert_eq!(
                index.wildcard(pattern, 20, &[]).unwrap(),
                oracle,
                "pattern {pattern:?}"
            );
        }
        // Bounded limit + exclusions + score ties must match exhaustive oracle on leading '?' and '*'.
        for pattern in ["*é*", "??*g", "????", "*a*", "c?t", "*"] {
            let pat_chars: Vec<_> = pattern.chars().collect();
            let excluded = [1usize, 5usize];
            let mut oracle: Vec<_> = (0..words.len())
                .filter(|i| !excluded.contains(i))
                .filter(|&i| {
                    let text: Vec<_> = words[i].key.chars().collect();
                    glob_match(&pat_chars, &text)
                })
                .collect();
            oracle.sort_by_key(|&i| (Reverse(words[i].score), i));
            oracle.truncate(2);
            assert_eq!(
                index.wildcard(pattern, 2, &excluded).unwrap(),
                oracle,
                "bounded pattern {pattern:?}"
            );
        }
        // Overly long wildcard patterns (>63 collapsed tokens) never panic.
        assert!(
            index
                .wildcard(&"a?".repeat(40), 20, &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn folded_fallback_and_fuzzy_two_respect_thresholds() {
        let (_words, index) = test_index(vec![
            ("accommodate", 80),
            ("café", 70),
            ("cat", 90),
            ("déjà vu", 60),
            ("résumé", 75),
        ]);
        let folded = index.folded_fallback("resume", 20, &[]).unwrap();
        assert_eq!(folded.len(), 1);
        assert_eq!(index.key(folded[0]).unwrap(), "résumé");

        let deja = index.folded_fallback("deja", 20, &[]).unwrap();
        assert_eq!(deja.len(), 1);
        assert_eq!(index.key(deja[0]).unwrap(), "déjà vu");

        // Fewer than 6 chars (< 6) never runs 2-edit fuzzy fallback.
        assert!(index.fuzzy_two("cxy", 20, &[]).unwrap().is_empty());
        assert!(index.fuzzy_two("kattz", 20, &[]).unwrap().is_empty());

        // >= 6 chars finds 2-edit match cleanly.
        let two = index.fuzzy_two("acommodtae", 20, &[]).unwrap();
        assert_eq!(two.len(), 1);
        assert_eq!(index.key(two[0]).unwrap(), "accommodate");
    }
}
