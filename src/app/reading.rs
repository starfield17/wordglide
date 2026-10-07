//! Source-preserving reading document and navigation addresses, independent of graphics.
use super::*;
use std::{ops::Range, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Node {
    entry: String,
    group: usize,
    historical: bool,
    sense: Option<usize>,
    part: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Anchor {
    node: Node,
    offset: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum TextRole {
    Heading,
    Body,
    Dim,
    Example,
    Accent,
}

#[derive(Clone)]
pub(crate) struct LogicalLine {
    pub(crate) text: String,
    pub(crate) role: TextRole,
    pub(crate) secondary_start: Option<usize>,
    pub(crate) section: Option<usize>,
    node: Node,
}

impl LogicalLine {
    fn new(text: String, role: TextRole, node: Node, section: Option<usize>) -> Self {
        Self {
            text: text
                .chars()
                .filter_map(|c| {
                    if c == '\t' {
                        Some(' ')
                    } else if c == '\n' || !c.is_control() {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect(),
            role,
            node,
            section,
            secondary_start: None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Section {
    pub(crate) title: String,
    pub(crate) line: usize,
}

#[derive(Clone)]
pub(crate) struct TextRow {
    pub(crate) text: String,
    pub(crate) logical: usize,
    pub(crate) start: usize,
    pub(crate) prefix: usize,
}

#[derive(Clone)]
pub(crate) struct Match {
    pub(crate) logical: usize,
    pub(crate) range: Range<usize>,
}

#[derive(Default)]
pub(crate) struct ReadingState {
    pub(crate) lines: Vec<LogicalLine>,
    pub(crate) sections: Vec<Section>,
    pub(crate) rows: Vec<TextRow>,
    pub(crate) width: usize,
    pub(crate) preview: Option<Preview>,
    pub(crate) preferences: ReadingPreferences,
    pub(crate) restore_anchor: Option<Anchor>,
    pub(crate) find: String,
    pub(crate) matches: Vec<Match>,
    pub(crate) match_index: usize,
    pub(crate) find_original: Option<(usize, Option<Anchor>, String, usize)>,
}

impl ReadingState {
    pub(crate) fn same_preview(&self, preview: &Preview) -> bool {
        self.preview.as_ref().is_some_and(|old| {
            Arc::ptr_eq(&old.entry, &preview.entry)
                && old.related.len() == preview.related.len()
                && old
                    .related
                    .iter()
                    .zip(&preview.related)
                    .all(|(a, b)| Arc::ptr_eq(a, b))
        })
    }
    pub(crate) fn anchor(&self, scroll: usize) -> Option<Anchor> {
        let row = self.rows.get(scroll)?;
        Some(Anchor {
            node: self.lines.get(row.logical)?.node.clone(),
            offset: row.start,
        })
    }
    pub(crate) fn row_for_anchor(&self, anchor: &Anchor) -> Option<usize> {
        let line = self
            .lines
            .iter()
            .position(|line| line.node == anchor.node)
            .or_else(|| {
                self.lines.iter().position(|line| {
                    line.node.entry == anchor.node.entry
                        && line.node.group == anchor.node.group
                        && line.node.historical == anchor.node.historical
                        && line.node.sense == anchor.node.sense
                        && line.node.part == 1
                })
            })
            .or_else(|| {
                self.lines.iter().position(|line| {
                    line.node.entry == anchor.node.entry
                        && line.node.group == anchor.node.group
                        && line.node.historical == anchor.node.historical
                })
            })?;
        let exact = self.lines[line].node == anchor.node;
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.logical == line && (!exact || row.start <= anchor.offset))
            .map(|(i, _)| i)
            .next_back()
            .or_else(|| self.rows.iter().position(|row| row.logical == line))
    }
    pub(crate) fn section_row(&self, index: usize) -> Option<usize> {
        let logical = self.sections.get(index)?.line;
        self.rows.iter().position(|row| row.logical == logical)
    }
    pub(crate) fn current_section(&self, scroll: usize) -> Option<usize> {
        self.rows
            .get(scroll)
            .and_then(|row| self.lines.get(row.logical))
            .and_then(|line| line.section)
    }
    pub(crate) fn clear_find(&mut self) {
        self.find.clear();
        self.matches.clear();
        self.match_index = 0;
        self.find_original = None;
    }
    pub(crate) fn update_matches(&mut self) {
        self.matches.clear();
        if self.find.is_empty() {
            return;
        }
        let needle = self.find.to_lowercase();
        for (logical, line) in self.lines.iter().enumerate() {
            let mut folded = String::new();
            let mut mapping = Vec::new();
            for (byte, ch) in line.text.char_indices() {
                for lower in ch.to_lowercase() {
                    let start = folded.len();
                    folded.push(lower);
                    mapping.push((start, folded.len(), byte, byte + ch.len_utf8()));
                }
            }
            for (start, _) in folded.match_indices(&needle) {
                let end = start + needle.len();
                if let (Some(first), Some(last)) = (
                    mapping.iter().find(|m| m.0 <= start && start < m.1),
                    mapping.iter().rfind(|m| m.0 < end && end <= m.1),
                ) {
                    self.matches.push(Match {
                        logical,
                        range: first.2..last.3,
                    });
                }
            }
        }
        self.match_index = self.match_index.min(self.matches.len().saturating_sub(1));
    }
    pub(crate) fn match_row(&self, index: usize) -> Option<usize> {
        let found = self.matches.get(index)?;
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.logical == found.logical && row.start <= found.range.start)
            .map(|(i, _)| i)
            .next_back()
    }
}

pub(crate) fn document(
    preview: &Preview,
    preferences: ReadingPreferences,
) -> (Vec<LogicalLine>, Vec<Section>) {
    let mut lines = Vec::new();
    let mut sections = Vec::new();
    if !preview.related.is_empty() {
        let names = preview
            .related
            .iter()
            .map(|e| e.headword.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let node = Node {
            entry: preview.entry.key.clone(),
            group: usize::MAX,
            historical: false,
            sense: None,
            part: 0,
        };
        lines.push(LogicalLine::new(
            format!("{} → {} (word form)", preview.entry.headword, names),
            TextRole::Accent,
            node.clone(),
            None,
        ));
        for entry in &preview.related {
            append_entry(&mut lines, &mut sections, entry, true, preferences);
        }
        lines.push(LogicalLine::new(
            format!("Original form: {}", preview.entry.headword),
            TextRole::Dim,
            node,
            None,
        ));
    }
    append_entry(
        &mut lines,
        &mut sections,
        &preview.entry,
        false,
        preferences,
    );
    lines.push(LogicalLine::new(
        format!("Source: {}", preview.entry.source_url),
        TextRole::Dim,
        Node {
            entry: preview.entry.key.clone(),
            group: usize::MAX,
            historical: false,
            sense: None,
            part: 1,
        },
        None,
    ));
    (lines, sections)
}

fn append_entry(
    lines: &mut Vec<LogicalLine>,
    sections: &mut Vec<Section>,
    entry: &crate::Entry,
    related: bool,
    preferences: ReadingPreferences,
) {
    if related {
        lines.push(LogicalLine::new(
            format!("→ {}", entry.headword),
            TextRole::Heading,
            Node {
                entry: entry.key.clone(),
                group: usize::MAX,
                historical: false,
                sense: None,
                part: 2,
            },
            None,
        ));
    }
    for historical in [false, true] {
        for (group_index, group) in entry.groups.iter().enumerate() {
            let senses = group
                .senses
                .iter()
                .enumerate()
                .filter(|(_, sense)| sense.historical() == historical)
                .collect::<Vec<_>>();
            if senses.is_empty() {
                continue;
            }
            let section = sections.len();
            let title = format!(
                "{} · {} · {} senses{}",
                group.headword,
                group.pos,
                senses.len(),
                if historical {
                    " · archaic / obsolete"
                } else {
                    ""
                }
            );
            sections.push(Section {
                title,
                line: lines.len(),
            });
            let ipa = if !preferences.expand_ipa && group.ipa.len() > 2 {
                format!("{} …", group.ipa[..2].join(" · "))
            } else {
                group.ipa.join(" · ")
            };
            let node = |sense, part| Node {
                entry: entry.key.clone(),
                group: group_index,
                historical,
                sense,
                part,
            };
            let mut header = LogicalLine::new(
                format!(
                    "{}  {}  ({} senses)  {}{}",
                    group.headword,
                    group.pos,
                    senses.len(),
                    ipa,
                    if historical {
                        "  [archaic / obsolete]"
                    } else {
                        ""
                    }
                ),
                TextRole::Heading,
                node(None, 0),
                Some(section),
            );
            header.secondary_start = Some(group.headword.len());
            lines.push(header);
            for (number, (index, sense)) in senses.iter().enumerate() {
                let tags = if sense.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", sense.tags.join(", "))
                };
                lines.push(LogicalLine::new(
                    format!("{}. {}{}", number + 1, sense.glosses.join(" › "), tags),
                    TextRole::Body,
                    node(Some(*index), 1),
                    Some(section),
                ));
                for (ei, example) in sense
                    .examples
                    .iter()
                    .take(if preferences.expand_examples {
                        usize::MAX
                    } else {
                        1
                    })
                    .enumerate()
                {
                    lines.push(LogicalLine::new(
                        format!("   • {}", example.text),
                        TextRole::Example,
                        node(Some(*index), 2 + 2 * ei),
                        Some(section),
                    ));
                    if preferences.expand_examples && !example.reference.is_empty() {
                        lines.push(LogicalLine::new(
                            format!("     — {}", example.reference),
                            TextRole::Dim,
                            node(Some(*index), 3 + 2 * ei),
                            Some(section),
                        ));
                    }
                }
                lines.push(LogicalLine::new(
                    String::new(),
                    TextRole::Body,
                    node(Some(*index), usize::MAX),
                    Some(section),
                ));
            }
        }
    }
}

impl App {
    pub(crate) fn find_start(&mut self) {
        self.reading.find_original = Some((
            self.scroll,
            self.reading.anchor(self.scroll),
            self.reading.find.clone(),
            self.reading.match_index,
        ));
        self.panel_query = self.reading.find.clone();
        self.view.overlay = Overlay::Find;
        self.focus = Focus::Definition;
    }
    pub(crate) fn find_update(&mut self) {
        self.reading.find = self.panel_query.clone();
        self.reading.match_index = 0;
        self.reading.update_matches();
        if let Some(row) = self.reading.match_row(0) {
            self.scroll = row.min(self.max_scroll);
        }
    }
    pub(crate) fn next_match(&mut self, reverse: bool) {
        let count = self.reading.matches.len();
        if count == 0 {
            return;
        }
        self.reading.match_index =
            (self.reading.match_index + if reverse { count - 1 } else { 1 }) % count;
        if let Some(row) = self.reading.match_row(self.reading.match_index) {
            self.scroll = row.min(self.max_scroll);
        }
    }
    pub(crate) fn move_section(&mut self, reverse: bool) {
        let current = self.reading.current_section(self.scroll).unwrap_or(0);
        let target = if reverse {
            current.saturating_sub(1)
        } else {
            (current + 1).min(self.reading.sections.len().saturating_sub(1))
        };
        if let Some(row) = self.reading.section_row(target) {
            self.scroll = row.min(self.max_scroll);
        }
    }
    pub(crate) fn navigation_locations(&self) -> Vec<(isize, String)> {
        let mut result = self
            .history
            .iter()
            .rev()
            .enumerate()
            .map(|(i, l)| {
                (
                    -((i + 1) as isize),
                    format!(
                        "← {}",
                        l.preview
                            .as_ref()
                            .map_or(l.input.as_str(), |p| p.entry.headword.as_str())
                    ),
                )
            })
            .collect::<Vec<_>>();
        result.extend(self.forward.iter().rev().enumerate().map(|(i, l)| {
            (
                (i + 1) as isize,
                format!(
                    "→ {}",
                    l.preview
                        .as_ref()
                        .map_or(l.input.as_str(), |p| p.entry.headword.as_str())
                ),
            )
        }));
        let query = self.panel_query.to_lowercase();
        result.retain(|(_, title)| {
            query
                .split_whitespace()
                .all(|word| title.to_lowercase().contains(word))
        });
        result
    }
    pub(crate) fn navigate_distance(&mut self, distance: isize) {
        for _ in 0..distance.unsigned_abs() {
            if distance < 0 {
                self.back();
            } else {
                self.forward();
            }
        }
    }
}
