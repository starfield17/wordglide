use crate::{
    App,
    app::{LogicalLine, TextRole, TextRow, document},
    theme::Theme,
};
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

fn logical_line(source: &LogicalLine, theme: Theme) -> ReadingLine {
    let style = match source.role {
        TextRole::Heading => theme.heading(),
        TextRole::Body => theme.body(),
        TextRole::Dim => theme.dim(),
        TextRole::Example => theme.example(),
        TextRole::Accent => theme.accent(),
    };
    let mut line = ReadingLine::new(source.text.clone(), style);
    if let Some(start) = source.secondary_start {
        line.styles.push((start..line.text.len(), theme.dim()));
    }
    line
}

/// Rebuild source text only when the preview or display preferences change;
/// reflow only when the document or column width changes. Cached rows contain
/// no terminal styles, so themes can change without rebuilding text.
pub(super) fn prepare_reading(app: &mut App, width: usize) {
    let Some(preview) = app.preview.as_ref() else {
        app.reading.lines.clear();
        app.reading.sections.clear();
        app.reading.rows.clear();
        app.reading.preview = None;
        app.reading.width = 0;
        return;
    };
    let preferences = app.reading_preferences();
    let same = app.reading.same_preview(preview);
    let rebuild = !same
        || preferences.expand_examples != app.reading.preferences.expand_examples
        || preferences.expand_ipa != app.reading.preferences.expand_ipa;
    let reflow = rebuild || app.reading.width != width;
    if !reflow && app.reading.restore_anchor.is_none() {
        return;
    }
    let anchor = app
        .reading
        .restore_anchor
        .take()
        .filter(|_| app.scroll == app.reading.restore_scroll)
        .or_else(|| {
            if same {
                app.reading.anchor(app.scroll)
            } else {
                None
            }
        });
    if rebuild {
        let (lines, sections) = document(preview, preferences);
        app.reading.lines = lines;
        app.reading.sections = sections;
        app.reading.preview = Some(preview.clone());
        app.reading.preferences = preferences;
        app.reading.update_matches();
    }
    if reflow {
        if app.picking {
            app.label_input.clear();
        }
        let mut rows = Vec::new();
        for (logical, source) in app.reading.lines.iter().enumerate() {
            let mut from = 0;
            for line in wrap(
                vec![ReadingLine::new(source.text.clone(), Style::default())],
                width,
            ) {
                let visible = line.text.trim_start();
                let prefix = line.text.len() - visible.len();
                let start = if visible.is_empty() {
                    from
                } else {
                    source.text[from..]
                        .find(visible)
                        .map_or(from, |offset| from + offset)
                };
                from = (start + visible.len()).min(source.text.len());
                rows.push(TextRow {
                    text: line.text,
                    logical,
                    start,
                    prefix,
                });
            }
        }
        app.reading.rows = rows;
        app.reading.width = width;
    }
    if let Some(anchor) = anchor
        && let Some(row) = app.reading.row_for_anchor(&anchor)
    {
        app.scroll = row;
    }
}

pub(super) fn styled_rows(app: &App, range: Range<usize>) -> Vec<ReadingLine> {
    app.reading
        .rows
        .iter()
        .skip(range.start)
        .take(range.end.saturating_sub(range.start))
        .map(|row| {
            let source = &app.reading.lines[row.logical];
            let style = match source.role {
                TextRole::Heading => app.theme.heading(),
                TextRole::Body => app.theme.body(),
                TextRole::Dim => app.theme.dim(),
                TextRole::Example => app.theme.example(),
                TextRole::Accent => app.theme.accent(),
            };
            let mut line = ReadingLine::new(row.text.clone(), style);
            if let Some(start) = source.secondary_start {
                let end = row.start + row.text.len().saturating_sub(row.prefix);
                if start < end {
                    let mapped = row.prefix + start.saturating_sub(row.start);
                    line.styles.push((
                        mapped.min(line.text.len())..line.text.len(),
                        app.theme.dim(),
                    ));
                }
            }
            if !app.picking {
                for found in app
                    .reading
                    .matches
                    .iter()
                    .filter(|m| m.logical == row.logical)
                {
                    let start = found.range.start.max(row.start);
                    let end = found
                        .range
                        .end
                        .min(row.start + row.text.len().saturating_sub(row.prefix));
                    if start < end {
                        line.styles.insert(
                            0,
                            (
                                row.prefix + start - row.start..row.prefix + end - row.start,
                                app.theme.hint_label(),
                            ),
                        );
                    }
                }
            }
            line.style = style;
            line
        })
        .collect()
}

fn empty_state(app: &App) -> String {
    if let Some(notice) = &app.dictionary_notice {
        return format!(
            "No usable dictionary.\nPress F2 → Download / update dictionary… to install the latest dictionary, or run wordglide --download-data.\n\n{notice}"
        );
    }
    if app.loading {
        return "Looking up…".into();
    }
    let query = crate::normalize(&app.input);
    if query.is_empty() {
        "Start typing an English word or phrase. Enter reads the selected word; Ctrl+G opens actions; F1 shows keys.".into()
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
    lines.extend(
        document(preview, app.reading_preferences())
            .0
            .iter()
            .map(|source| logical_line(source, app.theme)),
    );
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
