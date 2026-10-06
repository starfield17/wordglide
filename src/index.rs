use crate::{Candidate, MatchKind};
use anyhow::{Context, Result, ensure};
use fst::{IntoStreamer, Streamer};
use levenshtein_automata::LevenshteinAutomatonBuilder;
use std::{cmp::Reverse, collections::BinaryHeap};

pub(crate) struct Index {
    pub(crate) words: Vec<Candidate>,
    tree: Vec<usize>,
    leaves: usize,
    fst: fst::Map<Vec<u8>>,
    fuzzy: LevenshteinAutomatonBuilder,
}

impl Index {
    pub(crate) fn new(words: Vec<Candidate>, bytes: Vec<u8>) -> Result<Self> {
        ensure!(!words.is_empty(), "Data pack has no candidates");
        ensure!(
            words.windows(2).all(|w| w[0].key < w[1].key),
            "Candidates are not sorted and unique"
        );
        let fst = fst::Map::new(bytes).context("Invalid fuzzy index")?;
        ensure!(
            fst.len() == words.len(),
            "Fuzzy index count differs from vocabulary"
        );
        let mut stream = fst.stream();
        for (i, word) in words.iter().enumerate() {
            let (key, value) = stream.next().context("Incomplete fuzzy index")?;
            ensure!(
                key == word.key.as_bytes() && value == i as u64,
                "Fuzzy index differs from vocabulary"
            );
        }
        let leaves = words.len().next_power_of_two();
        let mut result = Self {
            words,
            tree: vec![usize::MAX; 2 * leaves],
            leaves,
            fst,
            fuzzy: LevenshteinAutomatonBuilder::new(1, true),
        };
        for i in 0..result.words.len() {
            result.tree[leaves + i] = i;
        }
        for i in (1..leaves).rev() {
            result.tree[i] = result.best(result.tree[2 * i], result.tree[2 * i + 1]);
        }
        Ok(result)
    }

    fn best(&self, a: usize, b: usize) -> usize {
        if a == usize::MAX {
            return b;
        }
        if b == usize::MAX {
            return a;
        }
        if (self.words[a].score, Reverse(a)) >= (self.words[b].score, Reverse(b)) {
            a
        } else {
            b
        }
    }

    fn range_best(&self, mut left: usize, mut right: usize) -> usize {
        left += self.leaves;
        right += self.leaves;
        let mut winner = usize::MAX;
        while left < right {
            if left % 2 == 1 {
                winner = self.best(winner, self.tree[left]);
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                winner = self.best(winner, self.tree[right]);
            }
            left /= 2;
            right /= 2;
        }
        winner
    }

    pub(crate) fn exact(&self, key: &str) -> Option<usize> {
        self.words
            .binary_search_by(|w| w.key.as_str().cmp(key))
            .ok()
    }

    fn prefix_range(&self, query: &str) -> (usize, usize) {
        let left = self.words.partition_point(|w| w.key.as_str() < query);
        let right = self
            .words
            .partition_point(|w| w.key.as_str() < query || w.key.starts_with(query));
        (left, right)
    }

    pub(crate) fn common_prefix(&self, query: &str) -> Option<String> {
        if query.is_empty() {
            return None;
        }
        let (left, right) = self.prefix_range(query);
        if left == right {
            return None;
        }
        Some(
            self.words[left]
                .key
                .chars()
                .zip(self.words[right - 1].key.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        )
    }

    pub(crate) fn prefix(&self, query: &str, limit: usize) -> Vec<usize> {
        if query.is_empty() {
            return vec![];
        }
        let (left, right) = self.prefix_range(query);
        let mut heap = BinaryHeap::new();
        self.push_range(&mut heap, left, right);
        let mut results = Vec::with_capacity(limit);
        while results.len() < limit {
            let Some((_, Reverse(i), left, right)) = heap.pop() else {
                break;
            };
            results.push(i);
            self.push_range(&mut heap, left, i);
            self.push_range(&mut heap, i + 1, right);
        }
        results
    }

    fn push_range(
        &self,
        heap: &mut BinaryHeap<(i32, Reverse<usize>, usize, usize)>,
        left: usize,
        right: usize,
    ) {
        if left < right {
            let i = self.range_best(left, right);
            heap.push((self.words[i].score, Reverse(i), left, right));
        }
    }

    pub(crate) fn fuzzy(&self, query: &str, limit: usize, excluded: &[usize]) -> Vec<usize> {
        if !(3..=64).contains(&query.chars().count()) || limit == 0 {
            return vec![];
        }
        let dfa = self.fuzzy.build_dfa(query);
        let mut stream = self.fst.search(&dfa).into_stream();
        let mut top: BinaryHeap<Reverse<(i32, Reverse<usize>)>> = BinaryHeap::new();
        while let Some((_, value)) = stream.next() {
            let i = value as usize;
            if excluded.contains(&i) {
                continue;
            }
            let rank = (self.words[i].score, Reverse(i));
            top.push(Reverse(rank));
            if top.len() > limit {
                top.pop();
            }
        }
        let mut result: Vec<_> = top.into_iter().map(|Reverse((_, Reverse(i)))| i).collect();
        result.sort_unstable_by_key(|&i| (Reverse(self.words[i].score), i));
        result
    }

    pub(crate) fn candidate(&self, i: usize, kind: MatchKind) -> Candidate {
        let mut result = self.words[i].clone();
        result.kind = kind;
        result
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
        let index = Index::new(words, builder.into_inner().unwrap()).unwrap();
        for query in ["h", "h0", "h00", "h001", "h199", "not"] {
            let mut reference: Vec<_> = (0..index.words.len())
                .filter(|&i| index.words[i].key.starts_with(query))
                .collect();
            reference.sort_by_key(|&i| (Reverse(index.words[i].score), i));
            reference.truncate(20);
            assert_eq!(index.prefix(query, 20), reference);
        }
    }
}
