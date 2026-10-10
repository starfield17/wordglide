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
            .sections
            .iter()
            .map(|s| s.title.clone())
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
            "Jump to a word / part of speech".into()
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
        frame.render_widget(
            Paragraph::new(title.clone()).style(if index == app.view.panel_row {
                app.theme.selected()
            } else {
                app.theme.body()
            }),
            row,
        );
    }
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching locations").style(app.theme.dim()),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );
    }
    frame.render_widget(
        Paragraph::new("↑/↓ select · Enter jump · Esc cancel").style(app.theme.dim()),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
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
    let mut gloss = String::new();
    let mut example = None;

    if let Some(preview) = &app.peek_preview {
        if preview.entry.key == candidate.key {
            let mut found = false;
            for group in &preview.entry.groups {
                for sense in &group.senses {
                    for g in &sense.glosses {
                        if !g.trim().is_empty() {
                            gloss = g.clone();
                            etym_number = group.etymology_number;
                            if !group.pos.is_empty() {
                                pos = &group.pos;
                            }
                            if let Some(first_ex) = sense.examples.first()
                                && !first_ex.text.trim().is_empty()
                            {
                                example = Some(shorten_example(&first_ex.text, 120));
                            }
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                if found {
                    break;
                }
            }
            if !found {
                for related in &preview.related {
                    for group in &related.groups {
                        for sense in &group.senses {
                            for g in &sense.glosses {
                                if !g.trim().is_empty() {
                                    gloss = g.clone();
                                    if !group.pos.is_empty() {
                                        pos = &group.pos;
                                    }
                                    if let Some(first_ex) = sense.examples.first()
                                        && !first_ex.text.trim().is_empty()
                                    {
                                        example = Some(shorten_example(&first_ex.text, 120));
                                    }
                                    found = true;
                                    break;
                                }
                            }
                            if found {
                                break;
                            }
                        }
                        if found {
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
            }
            if !found && !preview.entry.lemmas.is_empty() {
                gloss = format!("See {}", preview.entry.lemmas.join(", "));
            }
        } else {
            gloss = "Loading definition…".to_string();
        }
    } else {
        gloss = "Loading definition…".to_string();
    }
    if gloss.is_empty() {
        gloss = "No definition available.".to_string();
    }

    let mut lines = Vec::new();
    let mut header_spans = vec![Span::styled(headword.to_string(), app.theme.heading())];
    if let Some(n) = etym_number {
        header_spans.push(Span::styled(format!("  [Etym {n}]"), app.theme.dim()));
    }
    if !pos.is_empty() {
        header_spans.push(Span::styled(format!("  {pos}"), app.theme.dim()));
    }
    header_spans.push(Span::styled(
        format!("  ({}/{})", app.peek_index + 1, app.results.len()),
        app.theme.dim(),
    ));
    lines.push(Line::from(header_spans));
    lines.push(Line::raw(""));
    lines.push(Line::styled(format!("1. {gloss}"), app.theme.body()));
    if let Some(ex) = &example {
        lines.push(Line::raw(""));
        lines.push(Line::styled(format!("“{ex}”"), app.theme.example()));
    }

    let screen = frame.area();
    let width = (screen.width.saturating_sub(6)).clamp(36, 68);
    let inner_width = width.saturating_sub(2) as usize;
    let non_zero_width = std::num::NonZeroUsize::new(inner_width);
    let gloss_lines = non_zero_width.map_or(1, |w| (gloss.len() + 3).div_ceil(w.get()).max(1));
    let example_lines = if let Some(ex) = &example {
        non_zero_width.map_or(2, |w| 1 + (ex.len() + 2).div_ceil(w.get()).max(1))
    } else {
        0
    };
    let content_lines = 1 + 1 + gloss_lines + example_lines;
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
