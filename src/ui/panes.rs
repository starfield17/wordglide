use crate::{App, Focus, MatchKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::pointer::{Pointer, Region};
use super::reading::{label_line, reading_lines, stale_style, wrap};

pub(super) fn render_candidates(frame: &mut Frame, app: &App, area: Rect, pointer: &mut Pointer) {
    let items: Vec<_> = app
        .results
        .iter()
        .map(|c| {
            let hint = match c.kind {
                MatchKind::Fuzzy => " ≈",
                MatchKind::Inflection => " →",
                _ => "",
            };
            ListItem::new(format!("{}{}", c.headword, hint))
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" Candidates · {} ", app.results.len()))
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
    let inner = initial_block.inner(area);
    // A page keeps one line of overlap so no definition line is skipped.
    app.page = inner.height.saturating_sub(1).max(1) as usize;
    let mut content = reading_lines(app);
    if app.loading && app.preview.is_some() {
        content = stale_style(content);
    }
    let lines = wrap(content, inner.width as usize);
    app.max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.scroll = app.scroll.min(app.max_scroll);

    let title = match &app.preview {
        None => " Definition ".to_string(),
        Some(preview) => match scroll_percent(app.scroll, app.max_scroll) {
            None => format!(" Definition · {} ", preview.entry.headword),
            Some(pct) => format!(" Definition · {} · {}% ", preview.entry.headword, pct),
        },
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(app.theme.border(app.focus == Focus::Definition));
    frame.render_widget(block, area);

    let visible: Vec<_> = lines
        .iter()
        .skip(app.scroll)
        .take(inner.height as usize)
        .collect();
    pointer.record_rows(inner, &visible);
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
    frame.render_widget(Paragraph::new(rendered), inner);
}
