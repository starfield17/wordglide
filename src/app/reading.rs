//! Source-preserving reading document and navigation addresses, independent of graphics.
use super::*;
use crate::{RelationKind, TermRelation};
use std::{ops::Range, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    Header,
    Gloss,
    Example(usize),
    Reference(usize),
    Blank,
    Form(usize),
    SupplementSummary,
    Etymology(usize),
    Relation(usize),
    RelationsSummary(RelationKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Node {
    entry: String,
    group: usize,
    historical: bool,
    sense: Option<usize>,
    part: Part,
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
    pub(crate) links: Vec<(Range<usize>, String)>,
    pub(crate) text: String,
    pub(crate) role: TextRole,
    pub(crate) secondary_start: Option<usize>,
    pub(crate) section: Option<usize>,
    node: Node,
}

fn sanitize(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c == '\t' {
                Some(' ')
            } else if c == '\n' || !c.is_control() {
                Some(c)
            } else {
                None
            }
        })
        .collect()
}

impl LogicalLine {
    fn new(text: String, role: TextRole, node: Node, section: Option<usize>) -> Self {
        Self {
            links: Vec::new(),
            text: sanitize(&text),
            role,
            node,
            section,
            secondary_start: None,
        }
    }

    pub(crate) fn is_link_list(&self) -> bool {
        matches!(self.node.part, Part::Form(_) | Part::Relation(_))
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
    pub(crate) spans: Option<Vec<(Range<usize>, String)>>,
}

#[derive(Clone)]
pub(crate) struct Match {
    pub(crate) logical: usize,
    pub(crate) range: Range<usize>,
}

#[derive(Default)]
pub(crate) struct ReadingState {
    pub(crate) content: ContentDisplay,
    pub(crate) lines: Vec<LogicalLine>,
    pub(crate) sections: Vec<Section>,
    pub(crate) rows: Vec<TextRow>,
    pub(crate) width: usize,
    pub(crate) preview: Option<Preview>,
    pub(crate) preferences: ReadingPreferences,
    pub(crate) restore_anchor: Option<Anchor>,
    pub(crate) restore_scroll: usize,
    pub(crate) find: String,
    pub(crate) matches: Vec<Match>,
    pub(crate) match_index: usize,
    pub(crate) find_original: Option<(usize, Option<Anchor>, String, usize)>,
}

impl ReadingState {
    /// Display and target derive from the same source node, including filtered rows.
    pub(crate) fn outline(&self, query: &str) -> Vec<(String, Anchor)> {
        let lower = query.to_lowercase();
        let trimmed = lower.trim();
        let mut sense_numbers = Vec::new();
        let mut hash_only = false;
        let mut invalid_hash = false;
        let mut text_terms = Vec::new();
        let mut has_hash_token = false;
        for token in trimmed.split_whitespace() {
            if let Some(rest) = token.strip_prefix('#') {
                has_hash_token = true;
                if rest.is_empty() {
                    hash_only = true;
                } else if let Ok(n) = rest.parse::<usize>()
                    && n > 0
                {
                    sense_numbers.push(n);
                } else {
                    invalid_hash = true;
                }
            } else {
                text_terms.push(token);
            }
        }
        if invalid_hash {
            return Vec::new();
        }
        let entries = self
            .lines
            .iter()
            .filter_map(|line| {
                let heading = line.node.part == Part::Header;
                if !heading && !(line.node.part == Part::Gloss && line.node.sense.is_some()) {
                    return None;
                }
                let context = line.section.and_then(|i| self.sections.get(i))?;
                let label = if heading {
                    context.title.clone()
                } else {
                    format!("  {} · {}", line.text, context.title)
                };
                Some((
                    heading,
                    context,
                    line,
                    label,
                    Anchor {
                        node: line.node.clone(),
                        offset: 0,
                    },
                ))
            })
            .collect::<Vec<_>>();

        if !has_hash_token {
            return entries
                .into_iter()
                .filter(|(_, context, line, _, _)| {
                    lower.is_empty()
                        || format!("{} {}", context.title, line.text)
                            .to_lowercase()
                            .contains(&lower)
                })
                .map(|(_, _, _, label, anchor)| (label, anchor))
                .collect();
        }

        let sense_candidates = entries
            .into_iter()
            .filter(|(heading, context, line, _, _)| {
                if *heading {
                    return false;
                }
                if text_terms.is_empty() {
                    return true;
                }
                let haystack = format!("{} {}", context.title, line.text).to_lowercase();
                text_terms.iter().all(|term| haystack.contains(term))
            })
            .collect::<Vec<_>>();

        if sense_numbers.is_empty() && hash_only {
            return sense_candidates
                .into_iter()
                .map(|(_, _, _, label, anchor)| (label, anchor))
                .collect();
        }

        let prefixed = sense_candidates
            .iter()
            .filter(|(_, _, line, _, _)| {
                sense_numbers.iter().any(|n| {
                    let prefix = format!("{n}. ");
                    line.text.starts_with(&prefix)
                })
            })
            .map(|(_, _, _, label, anchor)| (label.clone(), anchor.clone()))
            .collect::<Vec<_>>();
        if !prefixed.is_empty() {
            return prefixed;
        }
        if sense_numbers.len() == 1
            && let Some((_, _, _, label, anchor)) = sense_candidates.get(sense_numbers[0] - 1)
        {
            return vec![(label.clone(), anchor.clone())];
        }
        Vec::new()
    }
    pub(crate) fn outline_selection(&self, scroll: usize) -> usize {
        let Some(current) = self
            .rows
            .get(scroll)
            .and_then(|r| self.lines.get(r.logical))
        else {
            return 0;
        };
        self.outline("")
            .iter()
            .rposition(|(_, anchor)| {
                anchor.node.entry == current.node.entry
                    && anchor.node.group == current.node.group
                    && anchor.node.historical == current.node.historical
                    && (anchor.node.part == Part::Header || anchor.node.sense == current.node.sense)
            })
            .unwrap_or(0)
    }

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
                        && line.node.part == Part::Gloss
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
            let (folded, mapping) = fold_lowercase(&line.text);
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
    content: ContentDisplay,
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
            part: Part::Header,
        };
        lines.push(LogicalLine::new(
            format!("{} → {} (word form)", preview.entry.headword, names),
            TextRole::Accent,
            node.clone(),
            None,
        ));
        for entry in &preview.related {
            append_entry(&mut lines, &mut sections, entry, true, preferences, content);
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
        content,
    );
    lines.push(LogicalLine::new(
        format!("Source: {}", preview.entry.source_url),
        TextRole::Dim,
        Node {
            entry: preview.entry.key.clone(),
            group: usize::MAX,
            historical: false,
            sense: None,
            part: Part::Gloss,
        },
        None,
    ));
    (lines, sections)
}

/// Format a dictionary preview as plain or ANSI-styled lines for CLI output.
pub fn format_preview(preview: &Preview, color: bool) -> String {
    let (lines, _) = document(
        preview,
        ReadingPreferences::default(),
        ContentDisplay::default(),
    );
    let mut out = String::new();
    for line in lines {
        if !color {
            out.push_str(&line.text);
        } else {
            match line.role {
                TextRole::Heading => {
                    if let Some(sec) = line
                        .secondary_start
                        .filter(|&s| s <= line.text.len() && line.text.is_char_boundary(s))
                    {
                        out.push_str("\x1b[1m");
                        out.push_str(&line.text[..sec]);
                        out.push_str("\x1b[0;2m");
                        out.push_str(&line.text[sec..]);
                        out.push_str("\x1b[0m");
                    } else {
                        out.push_str("\x1b[1m");
                        out.push_str(&line.text);
                        out.push_str("\x1b[0m");
                    }
                }
                TextRole::Accent => {
                    out.push_str("\x1b[1m");
                    out.push_str(&line.text);
                    out.push_str("\x1b[0m");
                }
                TextRole::Dim => {
                    out.push_str("\x1b[2m");
                    out.push_str(&line.text);
                    out.push_str("\x1b[0m");
                }
                TextRole::Example => {
                    out.push_str("\x1b[3m");
                    out.push_str(&line.text);
                    out.push_str("\x1b[0m");
                }
                TextRole::Body => out.push_str(&line.text),
            }
        }
        out.push('\n');
    }
    out
}

fn tags(values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!(" [{}]", values.join(", "))
    }
}

const GROUP_TOP_RELATIONS: &[(RelationKind, &str)] = &[
    (RelationKind::Synonym, "Synonyms"),
    (RelationKind::Antonym, "Antonyms"),
];
const GROUP_TAIL_RELATIONS: &[(RelationKind, &str)] = &[
    (RelationKind::Derived, "Derived terms"),
    (RelationKind::Related, "Related terms"),
];
const ALL_RELATIONS: &[(RelationKind, &str)] = &[
    (RelationKind::Synonym, "Synonyms"),
    (RelationKind::Antonym, "Antonyms"),
    (RelationKind::Derived, "Derived terms"),
    (RelationKind::Related, "Related terms"),
];

fn fold_lowercase(text: &str) -> (String, Vec<(usize, usize, usize, usize)>) {
    let mut folded = String::new();
    let mut mapping = Vec::new();
    for (byte, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            let start = folded.len();
            folded.push(lower);
            mapping.push((start, folded.len(), byte, byte + ch.len_utf8()));
        }
    }
    (folded, mapping)
}

fn link_term(line: &mut LogicalLine, term: &str, start: usize) {
    let visible = sanitize(term);
    if !visible.is_empty() {
        line.links
            .push((start..start + visible.len(), normalize(term)));
    }
}

fn link_gloss_targets(
    line: &mut LogicalLine,
    body_start: usize,
    body_end: usize,
    targets: &[String],
) {
    if targets.is_empty() || body_start >= body_end || body_end > line.text.len() {
        return;
    }
    let body = &line.text[body_start..body_end];
    let (folded, mapping) = fold_lowercase(body);
    let mut ordered: Vec<String> = targets
        .iter()
        .map(|t| sanitize(t.trim()))
        .filter(|t| !t.is_empty() && !normalize(t).is_empty())
        .collect();
    ordered.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    ordered.dedup();
    for target in ordered {
        let needle = target.to_lowercase();
        let key = normalize(&target);
        for (start, _) in folded.match_indices(&needle) {
            let end = start + needle.len();
            let Some(first) = mapping.iter().find(|m| m.0 <= start && start < m.1) else {
                continue;
            };
            let Some(last) = mapping.iter().rfind(|m| m.0 < end && end <= m.1) else {
                continue;
            };
            if first.0 != start || last.1 != end {
                continue;
            }
            let orig_start = body_start + first.2;
            let orig_end = body_start + last.3;
            let before_ok = line.text[body_start..orig_start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            let after_ok = line.text[orig_end..body_end]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            if !before_ok || !after_ok {
                continue;
            }
            if line
                .links
                .iter()
                .any(|(existing, _)| existing.start < orig_end && orig_start < existing.end)
            {
                continue;
            }
            line.links.push((orig_start..orig_end, key.clone()));
        }
    }
    line.links.sort_by_key(|(range, _)| range.start);
}

fn push_single_relation(
    lines: &mut Vec<LogicalLine>,
    title: &str,
    index: usize,
    relation: &TermRelation,
    sense: Option<usize>,
    section: usize,
    node: &impl Fn(Option<usize>, Part) -> Node,
) {
    let qualifier = if relation.sense.is_empty() {
        String::new()
    } else {
        format!(" — {}", relation.sense)
    };
    let source = if relation.source.is_empty() {
        String::new()
    } else {
        format!(" (source: {})", relation.source)
    };
    let mut line = LogicalLine::new(
        format!(
            "   {title}: {}{}{qualifier}{source}",
            relation.word,
            tags(&relation.tags)
        ),
        TextRole::Dim,
        node(sense, Part::Relation(index)),
        Some(section),
    );
    link_term(&mut line, &relation.word, format!("   {title}: ").len());
    lines.push(line);
}

fn append_relations(
    lines: &mut Vec<LogicalLine>,
    relations: &[TermRelation],
    expanded: bool,
    sense: Option<usize>,
    section: usize,
    node: &impl Fn(Option<usize>, Part) -> Node,
    kinds: &[(RelationKind, &str)],
) {
    for &(kind, title) in kinds {
        let items: Vec<_> = relations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == kind)
            .collect();
        if expanded {
            if sense.is_none() {
                let mut idx = 0;
                while idx < items.len() {
                    let unqualified = |r: &TermRelation| r.sense.is_empty() && r.source.is_empty();
                    if unqualified(items[idx].1) {
                        let start_idx = idx;
                        while idx < items.len() && unqualified(items[idx].1) {
                            idx += 1;
                        }
                        let run = &items[start_idx..idx];
                        if run.len() >= 2 {
                            let mut text = format!("   {title}: ");
                            let mut spans = Vec::new();
                            for (pos, &(_, relation)) in run.iter().enumerate() {
                                if pos > 0 {
                                    text.push_str(" · ");
                                }
                                let start = text.len();
                                let visible = sanitize(&relation.word);
                                text.push_str(&visible);
                                if !visible.is_empty() {
                                    spans.push((
                                        start..start + visible.len(),
                                        normalize(&relation.word),
                                    ));
                                }
                                text.push_str(&sanitize(&tags(&relation.tags)));
                            }
                            let mut line = LogicalLine::new(
                                text,
                                TextRole::Dim,
                                node(sense, Part::Relation(run[0].0)),
                                Some(section),
                            );
                            line.links = spans;
                            lines.push(line);
                        } else {
                            push_single_relation(
                                lines, title, run[0].0, run[0].1, sense, section, node,
                            );
                        }
                    } else {
                        push_single_relation(
                            lines,
                            title,
                            items[idx].0,
                            items[idx].1,
                            sense,
                            section,
                            node,
                        );
                        idx += 1;
                    }
                }
            } else {
                for &(i, relation) in &items {
                    push_single_relation(lines, title, i, relation, sense, section, node);
                }
            }
        } else if !items.is_empty() && sense.is_some() {
            lines.push(LogicalLine::new(
                format!("   {title}: {} · r expands", items.len()),
                TextRole::Dim,
                node(sense, Part::RelationsSummary(kind)),
                Some(section),
            ));
        }
    }
}

fn append_entry(
    lines: &mut Vec<LogicalLine>,
    sections: &mut Vec<Section>,
    entry: &crate::Entry,
    related: bool,
    preferences: ReadingPreferences,
    content: ContentDisplay,
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
                part: Part::Blank,
            },
            None,
        ));
    }
    let has_modern_senses = entry
        .groups
        .iter()
        .any(|group| group.senses.iter().any(|s| !s.historical()));
    let mut last_etymology: Option<(Option<u32>, &str, &str)> = None;
    for historical in [false, true] {
        if historical && content.hide_historical && has_modern_senses {
            let hidden_count: usize = entry
                .groups
                .iter()
                .map(|group| group.senses.iter().filter(|s| s.historical()).count())
                .sum();
            if hidden_count > 0 {
                lines.push(LogicalLine::new(
                    format!(
                        "   {} archaic / obsolete {} hidden · h shows",
                        hidden_count,
                        if hidden_count == 1 { "sense" } else { "senses" }
                    ),
                    TextRole::Dim,
                    Node {
                        entry: entry.key.clone(),
                        group: usize::MAX,
                        historical: true,
                        sense: None,
                        part: Part::SupplementSummary,
                    },
                    None,
                ));
            }
            continue;
        }
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
            let etym_outline = group
                .etymology_number
                .map(|n| format!("[Etym {n}] "))
                .unwrap_or_default();
            let sense_word = if senses.len() == 1 { "sense" } else { "senses" };
            let title = format!(
                "{etym_outline}{} · {} · {} {sense_word}{}",
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
            let ipa = group
                .pronunciations
                .iter()
                .take(if preferences.expand_ipa {
                    usize::MAX
                } else {
                    2
                })
                .map(|p| {
                    format!(
                        "{}{}{}",
                        p.ipa,
                        tags(&p.tags),
                        if p.note.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", p.note)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(" · ");
            let ipa = if !preferences.expand_ipa && group.pronunciations.len() > 2 {
                format!("{ipa} …")
            } else {
                ipa
            };
            let node = |sense, part| Node {
                entry: entry.key.clone(),
                group: group_index,
                historical,
                sense,
                part,
            };
            let etym_header = group
                .etymology_number
                .map(|n| format!("  [Etym {n}]"))
                .unwrap_or_default();
            let mut header = LogicalLine::new(
                format!(
                    "{}  {}{etym_header}  ({} {sense_word})  {}{}",
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
                node(None, Part::Header),
                Some(section),
            );
            header.secondary_start = Some(sanitize(&group.headword).len());
            lines.push(header);
            // Group-level source content occurs once, even when its senses span
            // modern and historical reading sections.
            let first_partition = !historical || group.senses.iter().all(|s| s.historical());
            if first_partition {
                let mut supplements = Vec::new();
                if !group.forms.is_empty() {
                    supplements.push(format!("v forms {}", group.forms.len()));
                }
                if !group.relations.is_empty() {
                    supplements.push(format!("r relations {}", group.relations.len()));
                }
                if !group.etymology_text.is_empty() {
                    supplements.push("t etymology".into());
                }
                if !supplements.is_empty() {
                    lines.push(LogicalLine::new(
                        format!("   {}", supplements.join(" · ")),
                        TextRole::Dim,
                        node(None, Part::SupplementSummary),
                        Some(section),
                    ));
                }
                if content.forms && !group.forms.is_empty() {
                    if group.forms.len() == 1 {
                        let form = &group.forms[0];
                        let mut line = LogicalLine::new(
                            format!("   Form: {}{}", form.form, tags(&form.tags)),
                            TextRole::Dim,
                            node(None, Part::Form(0)),
                            Some(section),
                        );
                        link_term(&mut line, &form.form, "   Form: ".len());
                        lines.push(line);
                    } else {
                        let mut text = String::from("   Forms: ");
                        let mut spans = Vec::new();
                        for (i, form) in group.forms.iter().enumerate() {
                            if i > 0 {
                                text.push_str(" · ");
                            }
                            let start = text.len();
                            let visible = sanitize(&form.form);
                            text.push_str(&visible);
                            if !visible.is_empty() {
                                spans.push((start..start + visible.len(), normalize(&form.form)));
                            }
                            text.push_str(&sanitize(&tags(&form.tags)));
                        }
                        let mut line = LogicalLine::new(
                            text,
                            TextRole::Dim,
                            node(None, Part::Form(0)),
                            Some(section),
                        );
                        line.links = spans;
                        lines.push(line);
                    }
                }
                if !group.etymology_text.is_empty() && content.etymology {
                    if let Some((prev_num, prev_text, prev_pos)) = last_etymology
                        && prev_num == group.etymology_number
                        && prev_text == group.etymology_text
                    {
                        let prev_label = if prev_pos.is_empty() {
                            "section"
                        } else {
                            prev_pos
                        };
                        lines.push(LogicalLine::new(
                            format!("   Etymology: (same as {prev_label} above)"),
                            TextRole::Dim,
                            node(None, Part::Etymology(0)),
                            Some(section),
                        ));
                    } else {
                        for (i, text) in group.etymology_text.lines().enumerate() {
                            lines.push(LogicalLine::new(
                                format!("   Etymology: {text}"),
                                TextRole::Dim,
                                node(None, Part::Etymology(i)),
                                Some(section),
                            ));
                        }
                        last_etymology = Some((
                            group.etymology_number,
                            group.etymology_text.as_str(),
                            group.pos.as_str(),
                        ));
                    }
                }
                append_relations(
                    lines,
                    &group.relations,
                    content.relations,
                    None,
                    section,
                    &node,
                    GROUP_TOP_RELATIONS,
                );
            }
            for (number, (index, sense)) in senses.iter().enumerate() {
                let prefix = format!("{}. ", number + 1);
                let gloss_body = sense.glosses.join(" › ");
                let tags = if sense.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", sense.tags.join(", "))
                };
                let mut gloss_line = LogicalLine::new(
                    format!("{prefix}{gloss_body}{tags}"),
                    TextRole::Body,
                    node(Some(*index), Part::Gloss),
                    Some(section),
                );
                let body_start = prefix.len();
                let body_end = body_start + sanitize(&gloss_body).len();
                link_gloss_targets(&mut gloss_line, body_start, body_end, &sense.targets);
                lines.push(gloss_line);
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
                        node(Some(*index), Part::Example(ei)),
                        Some(section),
                    ));
                    if preferences.expand_examples && !example.reference.is_empty() {
                        lines.push(LogicalLine::new(
                            format!("     — {}", example.reference),
                            TextRole::Dim,
                            node(Some(*index), Part::Reference(ei)),
                            Some(section),
                        ));
                    }
                }
                append_relations(
                    lines,
                    &sense.relations,
                    content.relations,
                    Some(*index),
                    section,
                    &node,
                    ALL_RELATIONS,
                );
                lines.push(LogicalLine::new(
                    String::new(),
                    TextRole::Body,
                    node(Some(*index), Part::Blank),
                    Some(section),
                ));
            }
            if first_partition {
                append_relations(
                    lines,
                    &group.relations,
                    content.relations,
                    None,
                    section,
                    &node,
                    GROUP_TAIL_RELATIONS,
                );
            }
        }
    }
}

impl App {
    pub(crate) fn cancel_find(&mut self) {
        if let Some((scroll, anchor, find, index)) = self.reading.find_original.take() {
            self.scroll = anchor
                .as_ref()
                .and_then(|a| self.reading.row_for_anchor(a))
                .unwrap_or(scroll)
                .min(self.max_scroll);
            self.reading.find = find;
            self.reading.match_index = index;
            self.reading.update_matches();
        }
    }
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
            let before = self.scroll;
            self.scroll = row.min(self.max_scroll);
            if self.scroll != before && self.preview.is_some() && !self.loading {
                let key = self.preview.as_ref().map(|p| p.entry.key.clone());
                let input = self.input.clone();
                self.confirm_studied(key.as_deref(), &input);
            }
        }
    }
    pub(crate) fn move_sense(&mut self, reverse: bool) {
        let mut last_logical = usize::MAX;
        let mut sense_rows = self.reading.rows.iter().enumerate().filter_map(|(i, row)| {
            if row.logical == last_logical {
                return None;
            }
            last_logical = row.logical;
            let line = self.reading.lines.get(row.logical)?;
            if line.node.part == Part::Gloss && line.node.sense.is_some() {
                Some(i)
            } else {
                None
            }
        });
        let target = if reverse {
            sense_rows.rfind(|&row| row < self.scroll)
        } else {
            sense_rows.find(|&row| row > self.scroll)
        };
        if let Some(row) = target {
            let before = self.scroll;
            self.scroll = row.min(self.max_scroll);
            if self.scroll != before && self.preview.is_some() && !self.loading {
                let key = self.preview.as_ref().map(|p| p.entry.key.clone());
                let input = self.input.clone();
                self.confirm_studied(key.as_deref(), &input);
            }
        }
    }
    pub(crate) fn yank(&mut self) {
        let Some(preview) = self.preview.as_ref() else {
            return;
        };
        let text = self
            .reading
            .rows
            .get(self.scroll)
            .and_then(|row| self.reading.lines.get(row.logical))
            .and_then(|line| match line.node.part {
                Part::Header => {
                    let target = std::iter::once(&preview.entry)
                        .chain(&preview.related)
                        .find(|entry| entry.key == line.node.entry);
                    target.and_then(|entry| {
                        entry
                            .groups
                            .get(line.node.group)
                            .map(|g| g.headword.clone())
                            .or_else(|| Some(entry.headword.clone()))
                    })
                }
                Part::Blank | Part::SupplementSummary => None,
                _ => {
                    let trimmed = line.text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
            })
            .unwrap_or_else(|| preview.entry.headword.clone());
        let label = if unicode_width::UnicodeWidthStr::width(text.as_str()) > 32 {
            format!("Copied ({} chars)", text.chars().count())
        } else {
            format!("Copied: {text}")
        };
        self.pending_yank = Some(text);
        self.set_status_notice(label);
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
                    if l.preview
                        .as_ref()
                        .is_some_and(|p| p.entry.headword != l.input)
                    {
                        format!(
                            "{} · {}",
                            l.input,
                            l.preview.as_ref().unwrap().entry.headword
                        )
                    } else {
                        l.input.clone()
                    }
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

#[cfg(test)]
mod content_tests {
    use super::*;

    #[test]
    fn source_term_links_skip_category_labels_and_keep_whitespace_addresses() {
        let entry: crate::Entry = serde_json::from_value(serde_json::json!({
            "key":"test", "headword":"test", "score":0, "source_url":"source",
            "groups":[{"headword":"test","pos":"noun",
                "forms":[{"form":"Form"}],
                "relations":[{"kind":"synonym","word":"Synonyms"},
                    {"kind":"related","word":"take\toff\nagain"}],
                "senses":[{"glosses":["source gloss"]}]}]
        }))
        .unwrap();
        let preview = Preview {
            entry: Arc::new(entry),
            related: vec![],
        };
        let (lines, _) = document(
            &preview,
            ReadingPreferences::default(),
            ContentDisplay {
                forms: true,
                relations: true,
                etymology: false,
                hide_historical: false,
            },
        );
        for (prefix, visible, target) in [
            ("   Form: ", "Form", "form"),
            ("   Synonyms: ", "Synonyms", "synonyms"),
            ("   Related terms: ", "take off\nagain", "take off again"),
        ] {
            let line = lines
                .iter()
                .find(|line| line.text.starts_with(prefix))
                .unwrap();
            assert_eq!(
                line.links,
                vec![(prefix.len()..prefix.len() + visible.len(), target.into())]
            );
            assert_eq!(&line.text[line.links[0].0.clone()], visible);
        }
    }

    #[test]
    fn compact_supplements_etymology_dedup_targets_historical_fold_and_outline_hash() {
        let entry: crate::Entry = serde_json::from_value(serde_json::json!({
            "key":"alpha", "headword":"alpha", "score":100, "source_url":"source",
            "groups":[
                {
                    "headword":"alpha",
                    "pos":"noun",
                    "etymology_number":1,
                    "etymology_text":"From Greek.",
                    "forms":[{"form":"alphas","tags":["plural"]},{"form":"alphae"}],
                    "relations":[
                        {"kind":"synonym","word":"first"},
                        {"kind":"synonym","word":"second"},
                        {"kind":"derived","word":"alphabet"},
                        {"kind":"related","word":"beta"}
                    ],
                    "senses":[
                        {"glosses":["The beginning with beta."],"targets":["beta"]},
                        {"glosses":["Second meaning"]},
                        {"glosses":["Obsolete sense"],"tags":["archaic"]}
                    ]
                },
                {
                    "headword":"alpha",
                    "pos":"verb",
                    "etymology_number":1,
                    "etymology_text":"From Greek.",
                    "senses":[
                        {"glosses":["Verb meaning"]}
                    ]
                }
            ]
        }))
        .unwrap();
        let preview = Preview {
            entry: Arc::new(entry),
            related: vec![],
        };
        let (lines, sections) = document(
            &preview,
            ReadingPreferences::default(),
            ContentDisplay {
                forms: true,
                relations: true,
                etymology: true,
                hide_historical: true,
            },
        );
        // Forms >= 2 compact onto one line with links for both targets.
        let forms_line = lines
            .iter()
            .find(|l| l.text.starts_with("   Forms: "))
            .unwrap();
        assert!(
            forms_line.text.contains("alphas [plural] · alphae"),
            "{}",
            forms_line.text
        );
        assert_eq!(forms_line.links.len(), 2);

        // Synonyms appear before senses and Derived/Related appear after senses, compacted cleanly.
        let syn_pos = lines
            .iter()
            .position(|l| l.text.contains("Synonyms: first · second"))
            .unwrap();
        let gloss_pos = lines
            .iter()
            .position(|l| l.text.contains("1. The beginning with beta."))
            .unwrap();
        let der_pos = lines
            .iter()
            .position(|l| l.text.contains("Derived terms: alphabet"))
            .unwrap();
        assert!(syn_pos < gloss_pos && gloss_pos < der_pos);

        // Word-boundary Sense::targets link injected into gloss text.
        assert_eq!(lines[gloss_pos].links, vec![(22..26, "beta".into())]);

        // Consecutive identical etymology deduped with same Etym number.
        assert!(
            lines
                .iter()
                .any(|l| l.text == "   Etymology: (same as noun above)")
        );

        // Archaic senses folded cleanly while visible senses remain.
        assert!(
            lines
                .iter()
                .any(|l| l.text == "   1 archaic / obsolete sense hidden · h shows")
        );
        assert!(!lines.iter().any(|l| l.text.contains("Obsolete sense")));

        // #N outline matching picks displayed sense prefix and falls back cleanly.
        let state = ReadingState {
            lines,
            sections,
            ..ReadingState::default()
        };
        let s2 = state.outline("#2");
        assert_eq!(s2.len(), 1);
        assert!(s2[0].0.contains("2. Second meaning"), "{:?}", s2);
        let s1_verb = state.outline("#1 verb");
        assert_eq!(s1_verb.len(), 1);
        assert!(s1_verb[0].0.contains("Verb meaning"), "{:?}", s1_verb);
        // Global fallback when section numbering resets (3 visible senses total: 2 noun + 1 verb).
        let s3 = state.outline("#3");
        assert_eq!(s3.len(), 1);
        assert!(s3[0].0.contains("Verb meaning"), "{:?}", s3);
    }

    #[test]
    fn hide_historical_keeps_purely_archaic_entries_readable() {
        let entry: crate::Entry = serde_json::from_value(serde_json::json!({
            "key": "forsooth",
            "headword": "forsooth",
            "score": 100,
            "source_url": "source",
            "groups": [{
                "headword": "forsooth",
                "pos": "adv",
                "senses": [
                    {"glosses": ["In truth; indeed."], "tags": ["archaic"]}
                ]
            }]
        }))
        .unwrap();
        let preview = Preview {
            entry: Arc::new(entry),
            related: vec![],
        };
        let (lines, _) = document(
            &preview,
            ReadingPreferences::default(),
            ContentDisplay {
                hide_historical: true,
                ..ContentDisplay::default()
            },
        );
        assert!(
            lines
                .iter()
                .any(|l| l.text.contains("1. In truth; indeed.")),
            "{:?}",
            lines.iter().map(|l| &l.text).collect::<Vec<_>>()
        );
        assert!(lines.iter().any(|l| l.text.contains("(1 sense)")));
        assert!(!lines.iter().any(|l| l.text.contains("hidden · h shows")));
    }
}
