use crate::{
    Candidate, Entry, MatchKind, Preview,
    entry_storage::Entries,
    index::Index,
    model::{DATA_FILES, Manifest, RANKING, SCHEMA_VERSION},
    normalize,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::Arc,
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const LIMIT: usize = 20;

/// Startup hint shared by every command that needs a readable pack directory.
fn pack_open_hint(path: &Path) -> String {
    format!(
        "No data pack at {}. Start Wordglide and use F2 Settings to download, \
         run wordglide --download-data, or select a prepared pack with \
         --data / WORDGLIDE_DATA. See README.md.",
        path.display()
    )
}

pub struct Dictionary {
    index: Arc<Index>,
    entries: Entries,
    cache: HashMap<String, (Arc<Entry>, usize)>,
    order: VecDeque<String>,
    cache_bytes: usize,
}

/// Cheap, shareable handle to the read-only index. Lets the frontend answer
/// exact/prefix questions without naming the compact index internals.
#[derive(Clone)]
pub(crate) struct Lexicon {
    index: Arc<Index>,
}

impl Lexicon {
    pub(crate) fn contains(&self, word: &str) -> bool {
        self.index.exact(&normalize(word)).is_some()
    }

    pub(crate) fn common_prefix(&self, query: &str) -> Result<Option<String>> {
        self.index.common_prefix(query)
    }
}

impl Dictionary {
    /// Open with lightweight structural checks. Use `verify_pack` for full integrity verification.
    pub fn open(path: &Path) -> Result<Self> {
        let manifest: Manifest = serde_json::from_slice(
            &fs::read(path.join("manifest.json")).with_context(|| pack_open_hint(path))?,
        )?;
        ensure!(
            manifest.schema_version == SCHEMA_VERSION,
            "Incompatible data pack version {}; expected {}. Run wordglide --download-data with an updated program. Check --data / WORDGLIDE_DATA overrides for an old pack; old formats are not supported.",
            manifest.schema_version,
            SCHEMA_VERSION
        );
        ensure!(
            manifest.ranking == RANKING,
            "Incompatible ranking policy; rebuild or install a compatible data pack"
        );
        for name in DATA_FILES {
            let size = manifest
                .sizes
                .get(name)
                .with_context(|| format!("Missing size for {name}"))?;
            let hash = manifest
                .files
                .get(name)
                .with_context(|| format!("Missing checksum for {name}"))?;
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid checksum metadata"
            );
            ensure!(
                *size > 0 && fs::metadata(path.join(name))?.len() == *size,
                "Corrupt data pack file length: {name}"
            );
        }
        let index = Arc::new(Index::open(
            fs::read(path.join("lexicon.bin"))?,
            fs::read(path.join("words.fst"))?,
        )?);
        ensure!(
            index.len() == manifest.candidate_count,
            "Data pack candidate count mismatch"
        );
        let entries = Entries::open(path, index.len())?;
        Ok(Self {
            index,
            entries,
            cache: HashMap::new(),
            order: VecDeque::new(),
            cache_bytes: 0,
        })
    }

    pub fn candidate_count(&self) -> usize {
        self.index.len()
    }

    pub(crate) fn lexicon(&self) -> Lexicon {
        Lexicon {
            index: Arc::clone(&self.index),
        }
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
            results.push(self.index.candidate(i, MatchKind::Exact)?);
            let entry = self.entry(&query)?;
            let mut lemmas: Vec<_> = entry
                .lemmas
                .iter()
                .filter_map(|k| self.index.exact(k))
                .collect();
            let mut ranked = lemmas
                .iter()
                .map(|&i| Ok((std::cmp::Reverse(self.index.score(i)?), i)))
                .collect::<Result<Vec<_>>>()?;
            ranked.sort_unstable();
            lemmas = ranked.into_iter().map(|(_, i)| i).collect();
            lemmas.dedup();
            for j in lemmas {
                if !ids.contains(&j) {
                    ids.push(j);
                    results.push(self.index.candidate(j, MatchKind::Inflection)?);
                }
            }
        }
        for i in self.index.prefix(&query, LIMIT)? {
            if results.len() >= LIMIT {
                break;
            }
            if !ids.contains(&i) {
                ids.push(i);
                results.push(self.index.candidate(i, MatchKind::Prefix)?);
            }
        }
        if results.len() < LIMIT {
            for i in self.index.fuzzy(&query, LIMIT - results.len(), &ids)? {
                results.push(self.index.candidate(i, MatchKind::Fuzzy)?);
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
        let id = self.index.exact(key).context("Missing dictionary entry")?;
        let decoded = self.entries.read(id, key)?;
        let entry = Arc::new(decoded);
        // Count collection capacities even for empty strings; wire size is no
        // longer a safe proxy. Include the cache's two key copies and bookkeeping.
        let bytes = entry.cache_bytes() + key.len() * 2 + 512;
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

/// Human-readable metadata about a prepared data pack.
#[derive(Debug, Clone)]
pub struct PackInfo {
    pub schema_version: u32,
    pub candidate_count: usize,
    pub snapshot: String,
    pub source: String,
    pub source_url: String,
    pub input_sha256: String,
    pub licenses: Vec<String>,
}

/// Read pack metadata without opening the SQLite database or the indexes.
pub fn pack_info(path: &Path) -> Result<PackInfo> {
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(path.join("manifest.json")).with_context(|| pack_open_hint(path))?,
    )?;
    let field = |key: &str| {
        manifest
            .source
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string()
    };
    let licenses = manifest
        .source
        .get("licenses")
        .and_then(|value| value.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let license = entry.get("license")?.as_str()?;
                    let data = entry.get("data").and_then(|value| value.as_str());
                    Some(match data {
                        Some(data) if !data.is_empty() => format!("{license} ({data})"),
                        _ => license.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(PackInfo {
        schema_version: manifest.schema_version,
        candidate_count: manifest.candidate_count,
        snapshot: field("snapshot"),
        source: field("source"),
        source_url: field("source_url"),
        input_sha256: field("input_sha256"),
        licenses,
    })
}

/// Explicit full-pack verification. Never called by normal application startup.
/// Returns the number of verified entries.
pub fn verify_pack(path: &Path) -> Result<usize> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    let mut dict = Dictionary::open(path)?;
    for name in DATA_FILES {
        ensure!(
            manifest.files.get(name) == Some(&crate::build::checksum(&path.join(name))?),
            "Corrupt data pack checksum: {name}"
        );
    }
    dict.index.verify()?;
    dict.entries.verify_layout()?;
    for i in 0..dict.index.len() {
        let key = dict.index.key(i)?;
        let entry = dict.entries.read(i, key)?;
        let candidate = dict.index.candidate(i, MatchKind::Exact)?;
        ensure!(
            entry.score == candidate.score
                && entry.headword == candidate.headword
                && entry.parts_of_speech() == candidate.parts_of_speech,
            "Corrupt entry/index metadata: {key}"
        );
    }
    Ok(dict.index.len())
}
