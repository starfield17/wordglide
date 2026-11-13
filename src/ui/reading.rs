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
    pub(super) links: Vec<(Range<usize>, String)>,
    pub(super) text: String,
    pub(super) style: Style,
    pub(super) styles: Vec<(Range<usize>, Style)>,
}

impl ReadingLine {
    pub(super) fn new(text: String, style: Style) -> Self {
        Self {
            links: Vec::new(),
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
        let output_start = self.text.len();
        for (offset, grapheme) in source.text[range.clone()].grapheme_indices(true) {
            self.push(grapheme, source.style_at(range.start + offset));
        }
        for (link, target) in &source.links {
            let start = link.start.max(range.start);
            let end = link.end.min(range.end);
            if start < end {
                self.add_link(
                    output_start + start - range.start..output_start + end - range.start,
                    target,
                );
            }
        }
    }

    fn add_link(&mut self, range: Range<usize>, target: &str) {
        if let Some((previous, key)) = self.links.last_mut()
            && previous.end == range.start
            && key == target
        {
            previous.end = range.end;
        } else {
            self.links.push((range, target.into()));
        }
    }

    /// Explicit source terms override tokenization, including wrapped phrases.
    pub(super) fn lookup_spans(&self) -> Vec<(Range<usize>, String)> {
        compute_lookup_spans(&self.text, &self.links)
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
    line.links = source.links.clone();
    if let Some(start) = source.secondary_start {
        line.styles.push((start..line.text.len(), theme.dim()));
    }
    line
}

pub(super) fn compute_lookup_spans(
    text: &str,
    links: &[(Range<usize>, String)],
) -> Vec<(Range<usize>, String)> {
    let mut result = links.to_vec();
    let mut offset = 0;
    for token in text.split_word_bounds() {
        let range = offset..offset + token.len();
        offset = range.end;
        if token.chars().any(char::is_alphanumeric)
            && !links
                .iter()
                .any(|(link, _)| link.start < range.end && range.start < link.end)
        {
            result.push((range, crate::normalize(token)));
        }
    }
    result.sort_by_key(|(range, _)| range.start);
    result
}

fn slice_row_links(
    row: &TextRow,
    source_links: &[(Range<usize>, String)],
    dst: &mut Vec<(Range<usize>, String)>,
) {
    let row_end = row.start + row.text.len().saturating_sub(row.prefix);
    for (link, target) in source_links {
        let start = link.start.max(row.start);
        let end = link.end.min(row_end);
        if start < end {
            let mapped = row.prefix + start - row.start..row.prefix + end - row.start;
            if let Some((previous, key)) = dst.last_mut()
                && previous.end == mapped.start
                && key == target
            {
                previous.end = mapped.end;
            } else {
                dst.push((mapped, target.clone()));
            }
        }
    }
}

pub(super) fn ensure_visible_spans(app: &mut App, range: Range<usize>) {
    let end = range.end.min(app.reading.rows.len());
    for i in range.start..end {
        if app.reading.rows[i].spans.is_none() {
            let row = &app.reading.rows[i];
            let source = &app.reading.lines[row.logical];
            let mut links = Vec::new();
            slice_row_links(row, &source.links, &mut links);
            let spans = compute_lookup_spans(&row.text, &links);
            app.reading.rows[i].spans = Some(spans);
        }
    }
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
        || app.view.content != app.reading.content
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
        let (lines, sections) = document(preview, preferences, app.view.content);
        app.reading.lines = lines;
        app.reading.sections = sections;
        app.reading.preview = Some(preview.clone());
        app.reading.preferences = preferences;
        app.reading.content = app.view.content;
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
                    spans: None,
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
            slice_row_links(row, &source.links, &mut line.links);
            if app.theme.color && source.is_link_list() {
                let active_link = app.theme.body().add_modifier(Modifier::UNDERLINED);
                for (link, target) in &line.links {
                    if app.contains_normalized(target) {
                        line.styles.push((link.clone(), active_link));
                    }
                }
            }
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
            "No usable dictionary.\nPress Enter to download the dictionary, or F2 → Download / update dictionary… to install the latest dictionary, or run wordglide --download-data.\n\n{notice}"
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
            links: Vec::new(),
            text: error.clone(),
            style: app.theme.error(),
            styles: Vec::new(),
        });
    }
    let Some(preview) = &app.preview else {
        if lines.is_empty() {
            lines.push(ReadingLine {
                links: Vec::new(),
                text: empty_state(app),
                style: app.theme.dim(),
                styles: Vec::new(),
            });
        }
        return lines;
    };
    lines.extend(
        document(preview, app.reading_preferences(), app.view.content)
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
                let start = line.text.len();
                let mut buf = [0u8; 4];
                line.push(ch.encode_utf8(&mut buf), source.style_at(offset));
                if let Some((_, target)) = source.links.iter().find(|(r, _)| r.contains(&offset)) {
                    line.add_link(start..line.text.len(), target);
                }
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

        if line.text.contains(" · ") {
            let mut seg_start = 0;
            while seg_start < line.text.len() {
                let search_from = if line.text[seg_start..].starts_with(" · ") {
                    seg_start + " · ".len()
                } else {
                    seg_start
                };
                let seg_end = line.text[search_from..]
                    .find(" · ")
                    .map_or(line.text.len(), |rel| search_from + rel);
                let effective_start = if is_continuation
                    && columns == indent
                    && line.text[seg_start..seg_end].starts_with(' ')
                {
                    seg_start + 1
                } else {
                    seg_start
                };
                let seg_range = effective_start..seg_end;
                let seg_width = line.text[seg_range.clone()].width();
                let min_cols = if is_continuation { indent } else { 0 };
                if columns + seg_width <= width {
                    current.push_from(&line, seg_range);
                    columns += seg_width;
                } else {
                    let trimmed_start = if columns > min_cols {
                        result.push(std::mem::replace(
                            &mut current,
                            ReadingLine::new(indent_str.clone(), line.style),
                        ));
                        is_continuation = true;
                        columns = indent;
                        if line.text[seg_range.clone()].starts_with(' ') {
                            seg_range.start + 1
                        } else {
                            seg_range.start
                        }
                    } else {
                        seg_range.start
                    };
                    let trimmed_range = trimmed_start..seg_end;
                    let trimmed_width = line.text[trimmed_range.clone()].width();
                    if columns + trimmed_width <= width {
                        current.push_from(&line, trimmed_range);
                        columns += trimmed_width;
                    } else {
                        push_wrapped_tokens(
                            &line,
                            trimmed_range,
                            width,
                            indent,
                            &indent_str,
                            &mut result,
                            &mut current,
                            &mut columns,
                            &mut is_continuation,
                        );
                    }
                }
                seg_start = seg_end;
            }
        } else {
            push_wrapped_tokens(
                &line,
                0..line.text.len(),
                width,
                indent,
                &indent_str,
                &mut result,
                &mut current,
                &mut columns,
                &mut is_continuation,
            );
        }

        let min_cols = if is_continuation { indent } else { 0 };
        if !is_continuation || columns > min_cols {
            result.push(current);
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn push_wrapped_tokens(
    line: &ReadingLine,
    slice_range: Range<usize>,
    width: usize,
    indent: usize,
    indent_str: &str,
    result: &mut Vec<ReadingLine>,
    current: &mut ReadingLine,
    columns: &mut usize,
    is_continuation: &mut bool,
) {
    let mut offset = slice_range.start;
    for token in line.text[slice_range].split_word_bounds() {
        let start = offset;
        offset += token.len();
        let is_ws = token.chars().all(char::is_whitespace);
        if *is_continuation && *columns == indent && is_ws {
            continue;
        }
        let n = token.width();
        let min_cols = if *is_continuation { indent } else { 0 };
        if *columns + n > width && *columns > min_cols {
            result.push(std::mem::replace(
                current,
                ReadingLine::new(indent_str.to_string(), line.style),
            ));
            *is_continuation = true;
            *columns = indent;
            if is_ws {
                continue;
            }
        }
        if *columns + n <= width {
            current.push_from(line, start..offset);
            *columns += n;
        } else {
            for (byte, grapheme) in token.grapheme_indices(true) {
                let gn = grapheme.width();
                let min_cols = if *is_continuation { indent } else { 0 };
                if *columns + gn > width && *columns > min_cols {
                    result.push(std::mem::replace(
                        current,
                        ReadingLine::new(indent_str.to_string(), line.style),
                    ));
                    *is_continuation = true;
                    *columns = indent;
                }
                current.push_from(line, start + byte..start + byte + grapheme.len());
                *columns += gn;
            }
        }
    }
}

pub(super) fn label_line_with_spans(
    line: &ReadingLine,
    lookup_spans: &[(Range<usize>, String)],
    map: &HashMap<String, String>,
    hint_prefix: &str,
    theme: Theme,
) -> Line<'static> {
    let mut spans = vec![];
    let mut offset = 0;
    for (range, target) in lookup_spans {
        spans.extend(line.spans(offset..range.start));
        let token = &line.text[range.clone()];
        if let Some(label) = map.get(target) {
            // Replace the first two display columns with the hint (single-letter words use one).
            let graphemes: Vec<_> = token.graphemes(true).collect();
            let count = graphemes.len().min(2);
            let hint = label.chars().take(count).collect::<String>();
            let hint_style = if !hint_prefix.is_empty() && !label.starts_with(hint_prefix) {
                theme.dim()
            } else {
                theme.hint_label()
            };
            spans.push(Span::styled(hint, hint_style));
            let replaced_bytes: usize = graphemes[..count].iter().map(|g| g.len()).sum();
            spans.extend(line.spans(range.start + replaced_bytes..range.end));
        } else {
            spans.extend(line.spans(range.clone()));
        }
        offset = range.end;
    }
    spans.extend(line.spans(offset..line.text.len()));
    Line::from(spans)
}

#[cfg(test)]
pub(super) fn label_line(
    line: &ReadingLine,
    map: &HashMap<String, String>,
    theme: Theme,
) -> Line<'static> {
    label_line_with_spans(line, &line.lookup_spans(), map, "", theme)
}
