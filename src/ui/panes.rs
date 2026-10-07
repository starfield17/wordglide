use crate::{App, Focus, MatchKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::pointer::{Pointer, Region};
use super::reading::{label_line, prepare_reading, reading_lines, stale_style, styled_rows, wrap};

pub(super) fn render_candidates(frame: &mut Frame, app: &App, area: Rect, pointer: &mut Pointer) {
    let items: Vec<_> = app
        .results
        .iter()
        .map(|c| {
            let marker = match c.kind {
                MatchKind::Exact => " =",
                MatchKind::Fuzzy => " ≈",
                MatchKind::Inflection => " →",
                MatchKind::Prefix => "",
            };
            let available = area.width.saturating_sub(4) as usize;
            let mut text = String::new();
            let max = available.saturating_sub(marker.width());
            for grapheme in c.headword.graphemes(true) {
                if text.width() + grapheme.width()
                    > max.saturating_sub(usize::from(c.headword.width() > max))
                {
                    break;
                }
                text.push_str(grapheme);
            }
            if text != c.headword && max > 0 {
                text.push('…');
            }
            let query = crate::normalize(&app.input);
            let mut prefix_end = 0;
            if !query.is_empty() && c.key.starts_with(&query) {
                for (offset, g) in text.grapheme_indices(true) {
                    let normalized = crate::normalize(&text[..offset + g.len()]);
                    if query.starts_with(&normalized) {
                        prefix_end = offset + g.len();
                    }
                    if normalized == query {
                        break;
                    }
                }
            }
            ListItem::new(Line::from(vec![
                Span::styled(text[..prefix_end].to_string(), app.theme.heading()),
                Span::raw(text[prefix_end..].to_string()),
                Span::styled(marker, app.theme.dim()),
            ]))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(
            " Candidates · {}/{} ",
            if app.results.is_empty() {
                0
            } else {
                app.selected + 1
            },
            app.results.len()
        ))
        .border_style(app.theme.idle_border());
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_style(app.theme.selected())
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
    // The list writes back the first visible index, and single-line items map
    // one row per candidate, so a click row resolves to `offset + row`.
    pointer.candidates = Region::from(inner);
    pointer.candidates_offset = state.offset();
    pointer.candidates_len = app.results.len();
}

pub(super) fn scroll_percent(scroll: usize, max_scroll: usize) -> Option<usize> {
    if max_scroll == 0 {
        return None;
    }
    let percent = ((scroll as f64 / max_scroll as f64) * 100.0).round() as usize;
    Some(percent.min(100))
}

pub(super) fn render_definition(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    pointer: &mut Pointer,
) {
    let initial_block = Block::default().borders(Borders::ALL);
    let mut inner = initial_block.inner(area);
    if inner.width > 96 {
        inner.x += (inner.width - 96) / 2;
        inner.width = 96;
    }
    prepare_reading(app, inner.width as usize);
    let mut body = inner;
    if app.preview.is_some()
        && let Some(error) = &app.error
    {
        frame.render_widget(
            Paragraph::new(error.clone()).style(app.theme.error()),
            ratatui::layout::Rect::new(inner.x, inner.y, inner.width, 1),
        );
        body.y += 1;
        body.height = body.height.saturating_sub(1);
    }
    // A page keeps one line of overlap so no definition line is skipped.
    app.page = body.height.saturating_sub(1).max(1) as usize;
    let empty = if app.preview.is_none() {
        wrap(reading_lines(app), body.width as usize)
    } else {
        Vec::new()
    };
    let count = if app.preview.is_some() {
        app.reading.rows.len()
    } else {
        empty.len()
    };
    app.max_scroll = count.saturating_sub(body.height as usize);
    app.scroll = app.scroll.min(app.max_scroll);
    let mut lines = if app.preview.is_some() {
        styled_rows(app, app.scroll..app.scroll + body.height as usize)
    } else {
        empty
            .into_iter()
            .skip(app.scroll)
            .take(body.height as usize)
            .collect()
    };
    if app.loading && app.preview.is_some() {
        lines = stale_style(lines);
    }

    let mut title = match &app.preview {
        None => " Definition ".to_string(),
        Some(preview) => match scroll_percent(app.scroll, app.max_scroll) {
            None => format!(" Definition · {} ", preview.entry.headword),
            Some(pct) => format!(" Definition · {} · {}% ", preview.entry.headword, pct),
        },
    };
    if let Some(section) = app
        .reading
        .current_section(app.scroll)
        .and_then(|i| app.reading.sections.get(i))
    {
        title = title.trim_end().to_string() + " · " + &section.title + " ";
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(app.theme.border(app.focus == Focus::Definition));
    frame.render_widget(block, area);

    let visible: Vec<_> = lines.iter().collect();
    pointer.record_rows(body, &visible);
    let mut map = HashMap::new();
    app.labels.clear();
    if app.picking {
        for line in &visible {
            for token in line.text.split_word_bounds() {
                let key = crate::normalize(token);
                if token.graphemes(true).take(2).count() == 2
                    && token.graphemes(true).take(2).all(|g| g.width() == 1)
                    && app.contains(&key)
                    && !map.contains_key(&key)
                    && map.len() < 676
                {
                    let n = map.len();
                    let label = format!(
                        "{}{}",
                        (b'a' + (n / 26) as u8) as char,
                        (b'a' + (n % 26) as u8) as char
                    );
                    app.labels.push((label.clone(), key.clone()));
                    map.insert(key, label);
                }
            }
        }
    }
    let rendered: Vec<Line> = visible
        .iter()
        .map(|l| {
            if app.picking {
                label_line(l, &map, app.theme)
            } else {
                l.rendered()
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(rendered), body);
}
