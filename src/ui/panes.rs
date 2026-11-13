use crate::{App, Candidate, Focus, MatchKind};
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
use super::reading::{
    ensure_visible_spans, label_line_with_spans, prepare_reading, reading_lines, stale_style,
    styled_rows, wrap,
};

fn parts_summary(candidate: &Candidate, budget: usize) -> String {
    let budget = budget.min(10);
    let labels: Vec<_> = candidate
        .parts_of_speech
        .iter()
        .map(|p| match p.as_str() {
            "noun" => "n.",
            "verb" => "v.",
            "adj" => "adj.",
            "adv" => "adv.",
            "name" => "prop.n.",
            "pron" => "pron.",
            "prep" => "prep.",
            "conj" => "conj.",
            "intj" => "intj.",
            "det" => "det.",
            "num" => "num.",
            other => other,
        })
        .collect();
    let Some(first) = labels.first() else {
        return String::new();
    };
    let mut text = labels.iter().take(2).copied().collect::<Vec<_>>().join("/");
    if labels.len() > 2 {
        text.push_str("/…");
    }
    if text.width() + 2 > budget {
        text = (*first).to_string();
        if labels.len() > 1 {
            text.push_str("/…");
        }
    }
    if text.width() + 2 > budget {
        text = (*first).to_string();
        if labels.len() > 1 {
            text.push('…');
        }
    }
    if text.width() + 2 <= budget {
        format!("  {text}")
    } else {
        String::new()
    }
}

pub(super) fn render_candidates(frame: &mut Frame, app: &App, area: Rect, pointer: &mut Pointer) {
    let query = crate::normalize(&app.input);
    let ascii_query = query.is_ascii();
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
            let parts = parts_summary(c, available.saturating_sub(marker.width() + 8));
            let mut text = String::new();
            let max = available.saturating_sub(marker.width() + parts.width());
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
            let mut prefix_end = 0;
            if !query.is_empty() && c.key.starts_with(&query) {
                if ascii_query
                    && text.is_ascii()
                    && !text.starts_with(char::is_whitespace)
                    && !text.contains("  ")
                {
                    let qbytes = query.as_bytes();
                    let tbytes = text.as_bytes();
                    let mut matches = 0;
                    while matches < qbytes.len()
                        && matches < tbytes.len()
                        && tbytes[matches].to_ascii_lowercase() == qbytes[matches]
                    {
                        matches += 1;
                    }
                    if matches == qbytes.len() {
                        prefix_end = matches;
                    }
                } else {
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
            }
            ListItem::new(Line::from(vec![
                Span::styled(text[..prefix_end].to_string(), app.theme.heading()),
                Span::raw(text[prefix_end..].to_string()),
                Span::styled(marker, app.theme.dim()),
                Span::styled(parts, app.theme.dim()),
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
    let visible_end = (app.scroll + body.height as usize).min(count);
    let mut lines = if app.preview.is_some() {
        ensure_visible_spans(app, app.scroll..visible_end);
        styled_rows(app, app.scroll..visible_end)
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
    if app.preview.is_some() {
        pointer.record_cached_rows(body, &app.reading.rows[app.scroll..visible_end]);
    } else {
        pointer.record_rows(body, &visible);
    }
    let mut map = HashMap::new();
    app.labels.clear();
    if app.picking {
        use std::borrow::Cow;
        for (i, line) in visible.iter().enumerate() {
            let row_spans: Cow<'_, [(std::ops::Range<usize>, String)]> = if app.preview.is_some() {
                Cow::Borrowed(
                    app.reading.rows[app.scroll + i]
                        .spans
                        .as_deref()
                        .unwrap_or(&[]),
                )
            } else {
                Cow::Owned(line.lookup_spans())
            };
            for (range, key) in row_spans.iter() {
                let token = &line.text[range.clone()];
                if token.graphemes(true).take(2).count() == 2
                    && token.graphemes(true).take(2).all(|g| g.width() == 1)
                    && app.contains_normalized(key)
                    && !map.contains_key(key)
                    && map.len() < 676
                {
                    let n = map.len();
                    let label = format!(
                        "{}{}",
                        (b'a' + (n / 26) as u8) as char,
                        (b'a' + (n % 26) as u8) as char
                    );
                    app.labels.push((label.clone(), key.clone()));
                    map.insert(key.clone(), label);
                }
            }
        }
    }
    let rendered: Vec<Line> = if app.picking {
        use std::borrow::Cow;
        visible
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let row_spans: Cow<'_, [(std::ops::Range<usize>, String)]> =
                    if app.preview.is_some() {
                        Cow::Borrowed(
                            app.reading.rows[app.scroll + i]
                                .spans
                                .as_deref()
                                .unwrap_or(&[]),
                        )
                    } else {
                        Cow::Owned(l.lookup_spans())
                    };
                label_line_with_spans(l, &row_spans, &map, &app.label_input, app.theme)
            })
            .collect()
    } else {
        visible.iter().map(|l| l.rendered()).collect()
    };
    frame.render_widget(Paragraph::new(rendered), body);
}

#[cfg(test)]
mod candidate_tests {
    use super::*;

    #[test]
    fn narrow_summaries_never_hide_omitted_parts() {
        let mut candidate = Candidate {
            key: "example".into(),
            headword: "example".into(),
            score: 0,
            kind: MatchKind::Exact,
            parts_of_speech: vec!["adj".into(), "noun".into(), "verb".into()],
        };
        assert_eq!(parts_summary(&candidate, 6), "");
        assert_eq!(parts_summary(&candidate, 7), "  adj.…");
        assert_eq!(parts_summary(&candidate, 8), "  adj./…");
        candidate.parts_of_speech = vec!["unfamiliar-label".into(), "verb".into()];
        assert_eq!(parts_summary(&candidate, 8), "");
        candidate.parts_of_speech = vec!["noun".into(), "verb".into()];
        assert_eq!(parts_summary(&candidate, 7), "  n./v.");
    }
}
