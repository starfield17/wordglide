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
    fs,
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
    /// Open with lightweight structural checks. Use `verify_pack` for full integrity verification.
    pub fn open(path: &Path) -> Result<Self> {
        let manifest:Manifest=serde_json::from_slice(&fs::read(path.join("manifest.json")).with_context(||format!("No data pack at {}. Download the Wordglide with-data release, set WORDGLIDE_DATA, pass --data DIRECTORY, or place english-pack beside the executable. See README.md.",path.display()))?)?;
        ensure!(
            manifest.schema_version == SCHEMA_VERSION,
            "Incompatible data pack version {}; expected {}. Download a new data pack; old formats are not supported.",
            manifest.schema_version,
            SCHEMA_VERSION
        );
        ensure!(
            manifest.ranking == RANKING,
            "Incompatible ranking policy; rebuild or install a compatible data pack"
        );
        for name in ["entries.sqlite", "words.fst", "lexicon.bin"] {
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
        let conn = Connection::open_with_flags(
            path.join("entries.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        conn.execute_batch("PRAGMA cache_size=-8192; PRAGMA query_only=ON;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(version == SCHEMA_VERSION, "Incompatible SQLite schema");
        let columns: Vec<(String, String, bool)> = conn
            .prepare("PRAGMA table_info(entries)")?
            .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get::<_, u32>(5)? == 1)))?
            .collect::<rusqlite::Result<_>>()?;
        ensure!(
            columns
                .iter()
                .any(|(n, t, p)| n == "key" && t.eq_ignore_ascii_case("TEXT") && *p)
                && columns
                    .iter()
                    .any(|(n, t, _)| n == "payload" && t.eq_ignore_ascii_case("TEXT")),
            "Incompatible entries table"
        );
        conn.prepare("SELECT payload FROM entries WHERE key=?1")?;
        Ok(Self {
            index,
            conn,
            cache: HashMap::new(),
            order: VecDeque::new(),
            cache_bytes: 0,
        })
    }

    pub fn candidate_count(&self) -> usize {
        self.index.len()
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
        &fs::read(path.join("manifest.json")).with_context(|| {
            format!(
                "No data pack at {}. Download the Wordglide with-data release, set WORDGLIDE_DATA, pass --data DIRECTORY, or place english-pack beside the executable. See README.md.",
                path.display()
            )
        })?,
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
    let dict = Dictionary::open(path)?;
    for name in ["entries.sqlite", "words.fst", "lexicon.bin"] {
        ensure!(
            manifest.files.get(name) == Some(&crate::build::checksum(&path.join(name))?),
            "Corrupt data pack checksum: {name}"
        );
    }
    dict.index.verify()?;
    let messages = dict
        .conn
        .prepare("PRAGMA integrity_check")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        messages == ["ok"],
        "Corrupt SQLite database: {}",
        messages.join("; ")
    );
    let mut stmt = dict
        .conn
        .prepare("SELECT key FROM entries ORDER BY key COLLATE BINARY")?;
    let mut rows = stmt.query([])?;
    for i in 0..dict.index.len() {
        let row = rows.next()?.context("Missing database entry")?;
        ensure!(
            row.get::<_, String>(0)? == dict.index.key(i)?,
            "Database and index vocabulary differ"
        );
    }
    ensure!(rows.next()?.is_none(), "Unexpected database entries");
    Ok(dict.index.len())
}
