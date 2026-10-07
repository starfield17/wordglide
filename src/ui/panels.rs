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
