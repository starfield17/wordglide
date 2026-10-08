use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub text: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sense {
    pub glosses: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default)]
    pub targets: Vec<String>,
}

impl Sense {
    pub(crate) fn historical(&self) -> bool {
        self.tags.iter().any(|t| t == "obsolete" || t == "archaic")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub headword: String,
    pub pos: String,
    #[serde(default)]
    pub ipa: Vec<String>,
    pub senses: Vec<Sense>,
    #[serde(default)]
    pub etymology_number: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub headword: String,
    pub score: i32,
    pub groups: Vec<Group>,
    #[serde(default)]
    pub lemmas: Vec<String>,
    #[serde(default)]
    pub preview_lemmas: bool,
    pub source_url: String,
}

impl Entry {
    pub(crate) fn parts_of_speech(&self) -> Vec<String> {
        let mut parts: Vec<_> = self.groups.iter().map(|g| g.pos.clone()).collect();
        parts.sort();
        parts.dedup();
        parts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchKind {
    Exact,
    Inflection,
    Prefix,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub key: String,
    pub headword: String,
    pub score: i32,
    pub kind: MatchKind,
    /// Sorted, distinct source labels from all of the entry's groups.
    #[serde(default)]
    pub parts_of_speech: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub entry: Arc<Entry>,
    pub related: Vec<Arc<Entry>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub schema_version: u32,
    pub candidate_count: usize,
    pub ranking: String,
    pub source: serde_json::Value,
    pub files: std::collections::BTreeMap<String, String>,
    pub sizes: std::collections::BTreeMap<String, u64>,
}

pub(crate) const SCHEMA_VERSION: u32 = 3;
pub(crate) const RANKING: &str = "100*zipf-2*chars-100*extra_words-150*hyphens;hyphens=U+002D,U+2010;exact>inflection>prefix>fuzzy;key_tie";
