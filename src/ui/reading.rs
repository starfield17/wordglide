use crate::{App, Entry, app::ViewOptions, theme::Theme};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use std::{collections::HashMap, ops::Range};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub(super) struct ReadingLine {
    pub(super) text: String,
    pub(super) style: Style,
    pub(super) styles: Vec<(Range<usize>, Style)>,
}

impl ReadingLine {
    pub(super) fn new(text: String, style: Style) -> Self {
        Self {
            text,
            style,
            styles: Vec::new(),
        }
    }

    fn style_at(&self, byte: usize) -> Style {
        self.styles
            .iter()
            .find(|(range, _)| range.contains(&byte))
            .map_or(self.style, |(_, style)| *style)
    }

    fn push(&mut self, text: &str, style: Style) {
        let start = self.text.len();
        self.text.push_str(text);
        if style != self.style && !text.is_empty() {
            if let Some((range, previous)) = self.styles.last_mut()
                && *previous == style
                && range.end == start
            {
                range.end = self.text.len();
            } else {
                self.styles.push((start..self.text.len(), style));
            }
        }
    }

    fn push_from(&mut self, source: &Self, range: Range<usize>) {
        for (offset, grapheme) in source.text[range.clone()].grapheme_indices(true) {
            self.push(grapheme, source.style_at(range.start + offset));
        }
    }

    fn spans(&self, range: Range<usize>) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut start = range.start;
        while start < range.end {
            let style = self.style_at(start);
            let mut end = range.end;
            for (section, _) in &self.styles {
                if section.start > start {
                    end = end.min(section.start);
                }
                if section.end > start {
                    end = end.min(section.end);
                }
            }
            spans.push(Span::styled(self.text[start..end].to_string(), style));
            start = end;
        }
        spans
    }

    pub(super) fn rendered(&self) -> Line<'static> {
        Line::from(self.spans(0..self.text.len()))
    }
}

// Screen regions and word positions from the most recent frame, filled during
// rendering because ratatui performs no hit testing. Kept out of `App` so the
// application state stays independent of the terminal backend.

fn append_entry(
    lines: &mut Vec<ReadingLine>,
    entry: &Entry,
    related: bool,
    theme: Theme,
    view: ViewOptions,
) {
    if related {
        lines.push(ReadingLine {
            text: format!("→ {}", entry.headword),
            style: theme.heading(),
            styles: Vec::new(),
        });
    }
    // Partition across groups as well as within each group: historical-only groups come last.
    for historical in [false, true] {
        for group in &entry.groups {
            let senses: Vec<_> = group
                .senses
                .iter()
                .filter(|s| s.historical() == historical)
                .collect();
            if senses.is_empty() {
                continue;
            }
            let ipa = if !view.expand_ipa && group.ipa.len() > 2 {
                format!("{} …", group.ipa[..2].join(" · "))
            } else {
                group.ipa.join(" · ")
            };
            lines.push(ReadingLine {
                text: format!(
                    "{}  {}  {}{}",
                    group.headword,
                    group.pos,
                    ipa,
                    if historical {
                        "  [archaic / obsolete]"
                    } else {
                        ""
                    }
                ),
                style: theme.heading(),
                styles: Vec::new(),
            });
            if let Some(line) = lines.last_mut() {
                line.styles
                    .push((group.headword.len()..line.text.len(), theme.dim()));
            }
            for (i, sense) in senses.iter().enumerate() {
                let tags = if sense.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", sense.tags.join(", "))
                };
                lines.push(ReadingLine {
                    text: format!("{}. {}{}", i + 1, sense.glosses.join(" › "), tags),
                    style: theme.body(),
                    styles: Vec::new(),
                });
                if view.expand_examples {
                    for example in &sense.examples {
                        lines.push(ReadingLine {
                            text: format!("   • {}", example.text),
                            style: theme.example(),
                            styles: Vec::new(),
                        });
                        if !example.reference.is_empty() {
                            lines.push(ReadingLine {
                                text: format!("     — {}", example.reference),
                                style: theme.dim(),
                                styles: Vec::new(),
                            });
                        }
                    }
                } else if let Some(example) = sense.examples.first() {
                    lines.push(ReadingLine {
                        text: format!("   • {}", example.text),
                        style: theme.example(),
                        styles: Vec::new(),
                    });
                }
                lines.push(ReadingLine {
                    text: String::new(),
                    style: theme.body(),
                    styles: Vec::new(),
                });
            }
        }
    }
}

fn empty_state(app: &App) -> String {
    if app.loading {
        return "Looking up…".into();
    }
    let query = crate::normalize(&app.input);
    if query.is_empty() {
        "Start typing an English word or phrase.".into()
    } else if query.chars().count() < 3 {
        format!("No word starts with \"{query}\". Spelling suggestions need at least 3 letters.")
    } else {
        format!("No matching words for \"{query}\". Check the spelling.")
    }
}

/// Dim every reading line so a retained definition reads as stale while a new
/// query is in flight.
pub(super) fn stale_style(lines: Vec<ReadingLine>) -> Vec<ReadingLine> {
    lines
        .into_iter()
        .map(|mut line| {
            line.style = line.style.add_modifier(Modifier::DIM);
            for (_, style) in &mut line.styles {
                *style = style.add_modifier(Modifier::DIM);
            }
            line
        })
        .collect()
}

pub(super) fn reading_lines(app: &App) -> Vec<ReadingLine> {
    let mut lines = vec![];
    if let Some(error) = &app.error {
        lines.push(ReadingLine {
            text: error.clone(),
            style: app.theme.error(),
            styles: Vec::new(),
        });
    }
    let Some(preview) = &app.preview else {
        if lines.is_empty() {
            lines.push(ReadingLine {
                text: empty_state(app),
                style: app.theme.dim(),
                styles: Vec::new(),
            });
        }
        return lines;
    };
    if !preview.related.is_empty() {
        let relations = preview
            .related
            .iter()
            .map(|e| e.headword.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(ReadingLine {
            text: format!("{} → {} (word form)", preview.entry.headword, relations),
            style: app.theme.accent(),
            styles: Vec::new(),
        });
        for related in &preview.related {
            append_entry(&mut lines, related, true, app.theme, app.view);
        }
        lines.push(ReadingLine {
            text: format!("Original form: {}", preview.entry.headword),
            style: app.theme.dim(),
            styles: Vec::new(),
        });
    }
    append_entry(&mut lines, &preview.entry, false, app.theme, app.view);
    lines.push(ReadingLine {
        text: format!("Source: {}", preview.entry.source_url),
        style: app.theme.dim(),
        styles: Vec::new(),
    });
    lines
}

pub(super) fn hanging_indent(text: &str) -> usize {
    let digits_len = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits_len > 0 && text[digits_len..].starts_with(". ") {
        return text[..digits_len + 2].width();
    }
    let ws_len: usize = text
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    let ws = &text[..ws_len];
    let rest = &text[ws_len..];
    if rest.starts_with("• ") {
        return text[..ws_len + "• ".len()].width();
    }
    if rest.starts_with("— ") {
        return text[..ws_len + "— ".len()].width();
    }
    if ws_len > 0 {
        return ws.width();
    }
    0
}

// Wrap first, then label; annotations never change line lengths or scroll coordinates.

pub(super) fn wrap(lines: Vec<ReadingLine>, width: usize) -> Vec<ReadingLine> {
    let width = width.max(1);
    let mut result = Vec::new();
    let mut normalized = Vec::new();
    for source in lines {
        let mut line = ReadingLine::new(String::new(), source.style);
        for (offset, ch) in source.text.char_indices() {
            if ch == '\n' {
                normalized.push(line);
                line = ReadingLine::new(String::new(), source.style);
            } else if ch == '\t' {
                line.push(" ", source.style_at(offset));
            } else if !ch.is_control() {
                line.push(&ch.to_string(), source.style_at(offset));
            }
        }
        normalized.push(line);
    }
    for line in normalized {
        if line.text.width() <= width {
            result.push(line);
            continue;
        }
        let indent = hanging_indent(&line.text).min(width.saturating_sub(1));
        let indent_str = " ".repeat(indent);
        let mut current = ReadingLine::new(String::new(), line.style);
        let mut columns = 0;
        let mut is_continuation = false;
        let mut offset = 0;
        for token in line.text.split_word_bounds() {
            let start = offset;
            offset += token.len();
            let is_ws = token.chars().all(char::is_whitespace);
            if is_continuation && columns == indent && is_ws {
                continue;
            }
            let n = token.width();
            let min_cols = if is_continuation { indent } else { 0 };
            if columns + n > width && columns > min_cols {
                result.push(current);
                is_continuation = true;
                current = ReadingLine::new(indent_str.clone(), line.style);
                columns = indent;
                if is_ws {
                    continue;
                }
            }
            if columns + n <= width {
                current.push_from(&line, start..offset);
                columns += n;
            } else {
                for (byte, grapheme) in token.grapheme_indices(true) {
                    let gn = grapheme.width();
                    let min_cols = if is_continuation { indent } else { 0 };
                    if columns + gn > width && columns > min_cols {
                        result.push(current);
                        is_continuation = true;
                        current = ReadingLine::new(indent_str.clone(), line.style);
                        columns = indent;
                    }
                    current.push(grapheme, line.style_at(start + byte));
                    columns += gn;
                }
            }
        }
        let min_cols = if is_continuation { indent } else { 0 };
        if !is_continuation || columns > min_cols {
            result.push(current);
        }
    }
    result
}

pub(super) fn label_line(
    line: &ReadingLine,
    map: &HashMap<String, String>,
    theme: Theme,
) -> Line<'static> {
    let mut spans = vec![];
    let mut offset = 0;
    for token in line.text.split_word_bounds() {
        let start = offset;
        offset += token.len();
        if let Some(label) = map.get(&crate::normalize(token)) {
            // Replace the first two display columns with the hint (single-letter words use one).
            let graphemes: Vec<_> = token.graphemes(true).collect();
            let count = graphemes.len().min(2);
            let hint = label.chars().take(count).collect::<String>();
            spans.push(Span::styled(hint, theme.hint_label()));
            let replaced_bytes: usize = graphemes[..count].iter().map(|g| g.len()).sum();
            spans.extend(line.spans(start + replaced_bytes..offset));
        } else {
            spans.extend(line.spans(start..offset));
        }
    }
    Line::from(spans)
}
