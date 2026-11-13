use super::pointer::{Pointer, Region};
use crate::{App, app::Overlay};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub(super) fn panel_area(screen: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(screen.width);
    let height = height.min(screen.height);
    Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    )
}

pub(super) fn render_commands(frame: &mut Frame, app: &mut App, pointer: &mut Pointer) {
    let area = panel_area(frame.area(), 78, 23);
    pointer.panel = Region::from(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Actions · Ctrl+G / F3 · Esc cancels ")
        .style(app.theme.surface())
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(format!("Filter: {}", app.panel_query)).style(app.theme.heading()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let commands = app.commands();
    let height = inner.height.saturating_sub(3) as usize;
    app.view.panel_row = app.view.panel_row.min(commands.len().saturating_sub(1));
    let offset = app.view.panel_row.saturating_sub(height.saturating_sub(1));
    for (i, &action) in commands.iter().enumerate().skip(offset).take(height) {
        let row = Rect::new(inner.x, inner.y + 2 + (i - offset) as u16, inner.width, 1);
        pointer.command_rows.push((Region::from(row), action, i));
        let state = app.action_state(action);
        let reason = app.action_reason(action);
        let detail = reason.unwrap_or(state);
        let style = if i == app.view.panel_row {
            app.theme.selected()
        } else if reason.is_some() {
            app.theme.dim()
        } else {
            app.theme.body()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(format!("{:<8} {}", action.shortcut(), action.title())),
                Span::styled(
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" · {detail}")
                    },
                    app.theme.dim(),
                ),
            ]))
            .style(style),
            row,
        );
    }
    if commands.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching actions").style(app.theme.dim()),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new("↑/↓ select · Enter run · Esc cancel").style(app.theme.dim()),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
    if app.view.overlay == Overlay::Commands {
        frame.set_cursor_position((
            inner.x
                + 8
                + unicode_width::UnicodeWidthStr::width(app.panel_query.as_str())
                    .min(inner.width.saturating_sub(9) as usize) as u16,
            inner.y,
        ));
    }
}

fn clip_width(text: &str, max_width: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut cols = 0;
    let budget = max_width.saturating_sub(1);
    for g in text.graphemes(true) {
        let w = g.width();
        if cols + w > budget {
            break;
        }
        out.push_str(g);
        cols += w;
    }
    out.push('…');
    out
}

fn fit_outline_row(title: &str, max_width: usize, sections: &[crate::app::Section]) -> String {
    use unicode_width::UnicodeWidthStr;
    if title.width() <= max_width {
        return title.to_string();
    }
    if title.starts_with("  ")
        && let Some(sec) = sections
            .iter()
            .find(|s| title.ends_with(&format!(" · {}", s.title)))
    {
        let suffix = format!(" · {}", sec.title);
        let suffix_w = suffix.width();
        if suffix_w + 6 <= max_width {
            let prefix_len = title.len() - suffix.len();
            let prefix = clip_width(&title[..prefix_len], max_width - suffix_w);
            return format!("{prefix}{suffix}");
        }
    }
    clip_width(title, max_width)
}

pub(super) fn render_reading_panel(frame: &mut Frame, app: &mut App, pointer: &mut Pointer) {
    if app.view.overlay == Overlay::Find {
        let screen = frame.area();
        let height = if screen.height < 12 { 1 } else { 2 };
        let area = Rect::new(screen.x, screen.bottom() - height, screen.width, height);
        frame.render_widget(Clear, area);
        frame.render_widget(Block::default().style(app.theme.surface()), area);
        let count = app.reading.matches.len();
        let status = if count == 0 {
            "No matches".to_string()
        } else {
            format!("{}/{}", app.reading.match_index + 1, count)
        };
        let prompt = format!("Find /{} · {status}", app.panel_query);
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(prompt, app.theme.heading()),
                Line::styled("↑/↓ match · Enter keep · Esc restore", app.theme.dim()),
            ]),
            area,
        );
        frame.set_cursor_position((
            area.x
                + 6
                + unicode_width::UnicodeWidthStr::width(app.panel_query.as_str())
                    .min(area.width.saturating_sub(7) as usize) as u16,
            area.y,
        ));
        return;
    }
    let outline = app.view.overlay == Overlay::Outline;
    let rows = if outline {
        app.reading
            .outline(&app.panel_query)
            .into_iter()
            .map(|(title, _)| title)
            .collect::<Vec<_>>()
    } else {
        app.navigation_locations()
            .into_iter()
            .map(|(_, title)| title)
            .collect()
    };
    let area = panel_area(frame.area(), 78, 23);
    pointer.panel = Region::from(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if outline {
            " Outline · Esc cancels "
        } else {
            " Session navigation · Esc cancels "
        })
        .style(app.theme.surface())
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(if outline {
            format!("Filter senses: {}", app.panel_query)
        } else {
            format!("Filter: {}", app.panel_query)
        })
        .style(app.theme.dim()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let height = inner.height.saturating_sub(3) as usize;
    app.view.panel_row = app.view.panel_row.min(rows.len().saturating_sub(1));
    let offset = app.view.panel_row.saturating_sub(height.saturating_sub(1));
    for (index, title) in rows.iter().enumerate().skip(offset).take(height) {
        let row = Rect::new(
            inner.x,
            inner.y + 2 + (index - offset) as u16,
            inner.width,
            1,
        );
        pointer.navigation_rows.push((Region::from(row), index));
        let rendered = if outline {
            fit_outline_row(title, inner.width as usize, &app.reading.sections)
        } else {
            clip_width(title, inner.width as usize)
        };
        frame.render_widget(
            Paragraph::new(rendered).style(if index == app.view.panel_row {
                app.theme.selected()
            } else {
                app.theme.body()
            }),
            row,
        );
    }
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching locations or senses").style(app.theme.dim()),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new("↑/↓ select · Enter jump · Esc cancel").style(app.theme.dim()),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
    let prefix_len = if outline { 15 } else { 8 };
    frame.set_cursor_position((
        inner.x
            + prefix_len
            + unicode_width::UnicodeWidthStr::width(app.panel_query.as_str())
                .min(inner.width.saturating_sub(prefix_len + 1) as usize) as u16,
        inner.y,
    ));
}

fn shorten_example(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_len.saturating_sub(1)).collect();
    if let Some(last_space) = truncated.rfind(' ')
        && last_space > max_len / 2
    {
        truncated.truncate(last_space);
    }
    truncated.push('…');
    truncated
}

struct PeekSnippet<'a> {
    pos: &'a str,
    etym_number: Option<u32>,
    ipa: Option<&'a str>,
    gloss: String,
    example: Option<String>,
}

fn first_sense_snippet(entry: &crate::Entry) -> Option<PeekSnippet<'_>> {
    let any_ipa = entry
        .groups
        .iter()
        .find_map(|g| g.pronunciations.first())
        .map(|p| p.ipa.as_str());
    for group in &entry.groups {
        let group_ipa = group
            .pronunciations
            .first()
            .map(|p| p.ipa.as_str())
            .or(any_ipa);
        for sense in &group.senses {
            for g in &sense.glosses {
                if !g.trim().is_empty() {
                    let example = sense
                        .examples
                        .first()
                        .filter(|ex| !ex.text.trim().is_empty())
                        .map(|ex| shorten_example(&ex.text, 120));
                    return Some(PeekSnippet {
                        pos: &group.pos,
                        etym_number: group.etymology_number,
                        ipa: group_ipa,
                        gloss: g.clone(),
                        example,
                    });
                }
            }
        }
    }
    None
}

pub(super) fn render_peek(frame: &mut Frame, app: &mut App, pointer: &mut Pointer) {
    let Some(candidate) = app.results.get(app.peek_index) else {
        return;
    };
    let headword = &candidate.headword;
    let mut pos = candidate
        .parts_of_speech
        .first()
        .map(|s| s.as_str())
        .unwrap_or("");
    let mut etym_number = None;
    let mut ipa: Option<&str> = None;
    let mut body_lines: Vec<(String, ratatui::style::Style)> = Vec::new();

    if let Some(preview) = &app.peek_preview
        && preview.entry.key == candidate.key
    {
        let own = first_sense_snippet(&preview.entry);
        let fallback_ipa = preview
            .entry
            .groups
            .iter()
            .find_map(|g| g.pronunciations.first())
            .map(|p| p.ipa.as_str());
        if let Some(ref snippet) = own {
            etym_number = snippet.etym_number;
            if !snippet.pos.is_empty() {
                pos = snippet.pos;
            }
            ipa = snippet.ipa;
        } else {
            ipa = fallback_ipa;
        }
        let related_match = preview.related.iter().find_map(|rel| {
            first_sense_snippet(rel).map(|snippet| (rel.headword.as_str(), snippet))
        });

        if preview.entry.preview_lemmas
            && let Some((rel_headword, rel)) = related_match
        {
            if let Some(ref snippet) = own {
                body_lines.push((snippet.gloss.clone(), app.theme.dim()));
                if pos.is_empty() && !rel.pos.is_empty() {
                    pos = rel.pos;
                }
                if ipa.is_none() {
                    ipa = rel.ipa;
                }
            }
            body_lines.push((format!("→ {rel_headword}: {}", rel.gloss), app.theme.body()));
            let example = own.and_then(|s| s.example).or(rel.example);
            if let Some(ex) = example {
                body_lines.push((String::new(), ratatui::style::Style::default()));
                body_lines.push((format!("“{ex}”"), app.theme.example()));
            }
        } else if let Some(snippet) = own {
            body_lines.push((format!("1. {}", snippet.gloss), app.theme.body()));
            if let Some(ex) = snippet.example {
                body_lines.push((String::new(), ratatui::style::Style::default()));
                body_lines.push((format!("“{ex}”"), app.theme.example()));
            }
        } else if let Some((rel_headword, rel)) = related_match {
            if pos.is_empty() && !rel.pos.is_empty() {
                pos = rel.pos;
            }
            if ipa.is_none() {
                ipa = rel.ipa;
            }
            body_lines.push((format!("→ {rel_headword}: {}", rel.gloss), app.theme.body()));
            if let Some(ex) = rel.example {
                body_lines.push((String::new(), ratatui::style::Style::default()));
                body_lines.push((format!("“{ex}”"), app.theme.example()));
            }
        } else if !preview.entry.lemmas.is_empty() {
            body_lines.push((
                format!("1. See {}", preview.entry.lemmas.join(", ")),
                app.theme.body(),
            ));
        } else {
            body_lines.push(("1. No definition available.".to_string(), app.theme.body()));
        }
    } else {
        body_lines.push(("1. Loading definition…".to_string(), app.theme.body()));
    }

    let mut lines = Vec::new();
    let mut header_text = headword.to_string();
    let mut header_spans = vec![Span::styled(headword.to_string(), app.theme.heading())];
    if let Some(n) = etym_number {
        let part = format!("  [Etym {n}]");
        header_text.push_str(&part);
        header_spans.push(Span::styled(part, app.theme.dim()));
    }
    if !pos.is_empty() {
        let part = format!("  {pos}");
        header_text.push_str(&part);
        header_spans.push(Span::styled(part, app.theme.dim()));
    }
    if let Some(ipa_text) = ipa {
        let part = format!("  {ipa_text}");
        header_text.push_str(&part);
        header_spans.push(Span::styled(part, app.theme.dim()));
    }
    let counter = format!("  ({}/{})", app.peek_index + 1, app.results.len());
    header_text.push_str(&counter);
    header_spans.push(Span::styled(counter, app.theme.dim()));

    lines.push(Line::from(header_spans));
    lines.push(Line::raw(""));
    for (text, style) in &body_lines {
        if text.is_empty() {
            lines.push(Line::raw(""));
        } else {
            lines.push(Line::styled(text.clone(), *style));
        }
    }

    let screen = frame.area();
    let width = (screen.width.saturating_sub(6)).clamp(36, 68);
    let inner_width = (width.saturating_sub(2) as usize).max(1);
    let mut wrap_input = Vec::with_capacity(body_lines.len() + 2);
    wrap_input.push(super::reading::ReadingLine::new(
        header_text,
        ratatui::style::Style::default(),
    ));
    wrap_input.push(super::reading::ReadingLine::new(
        String::new(),
        ratatui::style::Style::default(),
    ));
    for (text, _) in &body_lines {
        wrap_input.push(super::reading::ReadingLine::new(
            text.clone(),
            ratatui::style::Style::default(),
        ));
    }
    let content_lines = super::reading::wrap(wrap_input, inner_width).len();
    let needed_height = (content_lines + 2) as u16;
    let height = needed_height
        .clamp(6, 12)
        .min(screen.height.saturating_sub(2));
    let area = panel_area(screen, width, height);

    pointer.panel = Region::from(area);
    pointer.peek = Region::from(area);

    frame.render_widget(Clear, area);
    let title = if area.width >= 50 {
        " Peek · ↑/↓ compare · Enter read · Esc close "
    } else if area.width >= 34 {
        " Peek · ↑/↓ compare · Esc "
    } else {
        " Peek "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .style(app.theme.surface())
        .border_style(app.theme.focused_border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(
        Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
        inner,
    );
}
