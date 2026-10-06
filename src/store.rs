use crate::{
    Candidate, Entry, MatchKind, Preview,
    index::Index,
    model::{Manifest, RANKING, SCHEMA_VERSION},
    normalize,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::BufReader,
    path::Path,
    sync::Arc,
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const LIMIT: usize = 20;

pub struct Dictionary {
    index: Arc<Index>,
    conn: Connection,
    cache: HashMap<String, (Arc<Entry>, usize)>,
    order: VecDeque<String>,
    cache_bytes: usize,
}

impl Dictionary {
    /// Open and validate a prepared pack. Missing/incompatible packs fail before TUI setup.
    pub fn open(path: &Path) -> Result<Self> {
        let manifest:Manifest=serde_json::from_slice(&fs::read(path.join("manifest.json")).with_context(||format!("No data pack at {}. Download and unpack a Wordglide with-data release, or use --data DIRECTORY. See README.md.",path.display()))?)?;
        ensure!(
            manifest.schema_version == SCHEMA_VERSION,
            "Incompatible data pack version {}; expected {}. Install a compatible pack.",
            manifest.schema_version,
            SCHEMA_VERSION
        );
        ensure!(
            manifest.ranking == RANKING,
            "Incompatible ranking policy; rebuild or install a compatible data pack"
        );
        for name in ["entries.sqlite", "words.fst", "candidates.json"] {
            let expected = manifest
                .files
                .get(name)
                .with_context(|| format!("Missing checksum for {name}"))?;
            ensure!(
                &crate::build::checksum(&path.join(name))? == expected,
                "Corrupt data pack file: {name}"
            );
        }
        let words: Vec<Candidate> =
            serde_json::from_reader(BufReader::new(File::open(path.join("candidates.json"))?))?;
        ensure!(
            words.len() == manifest.candidate_count,
            "Data pack candidate count mismatch"
        );
        let index = Arc::new(Index::new(words, fs::read(path.join("words.fst"))?)?);
        let conn = Connection::open_with_flags(
            path.join("entries.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        conn.execute_batch("PRAGMA cache_size=-8192; PRAGMA query_only=ON;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(version == SCHEMA_VERSION, "Incompatible SQLite schema");
        Ok(Self {
            index,
            conn,
            cache: HashMap::new(),
            order: VecDeque::new(),
            cache_bytes: 0,
        })
    }

    pub fn candidate_count(&self) -> usize {
        self.index.words.len()
    }

    pub(crate) fn lexicon(&self) -> Arc<Index> {
        Arc::clone(&self.index)
    }

    pub fn contains(&self, word: &str) -> bool {
        self.index.exact(&normalize(word)).is_some()
    }

    pub fn search(&mut self, input: &str) -> Result<Vec<Candidate>> {
        let query = normalize(input);
        if query.is_empty() {
            return Ok(vec![]);
        }
        let mut results = vec![];
        let mut ids = vec![];
        if let Some(i) = self.index.exact(&query) {
            ids.push(i);
            results.push(self.index.candidate(i, MatchKind::Exact));
            let entry = self.entry(&query)?;
            let mut lemmas: Vec<_> = entry
                .lemmas
                .iter()
                .filter_map(|k| self.index.exact(k))
                .collect();
            lemmas.sort_by_key(|&i| (std::cmp::Reverse(self.index.words[i].score), i));
            lemmas.dedup();
            for j in lemmas {
                if !ids.contains(&j) {
                    ids.push(j);
                    results.push(self.index.candidate(j, MatchKind::Inflection));
                }
            }
        }
        for i in self.index.prefix(&query, LIMIT) {
            if results.len() >= LIMIT {
                break;
            }
            if !ids.contains(&i) {
                ids.push(i);
                results.push(self.index.candidate(i, MatchKind::Prefix));
            }
        }
        if results.len() < LIMIT {
            for i in self.index.fuzzy(&query, LIMIT - results.len(), &ids) {
                results.push(self.index.candidate(i, MatchKind::Fuzzy));
            }
        }
        results.truncate(LIMIT);
        Ok(results)
    }

    fn entry(&mut self, key: &str) -> Result<Arc<Entry>> {
        if let Some((entry, _)) = self.cache.get(key) {
            let entry = Arc::clone(entry);
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            return Ok(entry);
        }
        let payload: String = self
            .conn
            .query_row("SELECT payload FROM entries WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .with_context(|| format!("Missing dictionary entry: {key}"))?;
        let entry: Arc<Entry> = Arc::new(serde_json::from_str(&payload)?);
        // Approximate heap footprint conservatively; cache is bounded by payload+overhead.
        let bytes = payload.len() * 4 + 512;
        if bytes <= CACHE_BYTES {
            while self.cache_bytes + bytes > CACHE_BYTES {
                if let Some(old) = self.order.pop_front()
                    && let Some((_, size)) = self.cache.remove(&old)
                {
                    self.cache_bytes -= size;
                }
            }
            self.cache_bytes += bytes;
            self.order.push_back(key.to_string());
            self.cache
                .insert(key.to_string(), (Arc::clone(&entry), bytes));
        }
        Ok(entry)
    }

    pub fn preview(&mut self, candidate: &Candidate) -> Result<Preview> {
        let entry = self.entry(&candidate.key)?;
        let mut related = vec![];
        if entry.preview_lemmas {
            for key in &entry.lemmas {
                if key != &entry.key && self.index.exact(key).is_some() {
                    related.push(self.entry(key)?);
                }
            }
        }
        Ok(Preview { entry, related })
    }
}
