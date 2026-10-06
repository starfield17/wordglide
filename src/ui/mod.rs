mod help;
mod panes;
mod pointer;
mod reading;
mod terminal;

pub use terminal::run;

use crate::{App, Focus};
use help::{footer_help, render_help};
use panes::{render_candidates, render_definition};
use pointer::{Pointer, Region};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

// The moved unit tests refer to module items by their former bare names.
#[cfg(test)]
use panes::*;
#[cfg(test)]
use pointer::*;
#[cfg(test)]
use reading::*;

#[cfg(test)]
mod tests;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let mut pointer = Pointer::default();
    render(frame, app, &mut pointer);
}

pub(in crate::ui) fn render(frame: &mut Frame, app: &mut App, pointer: &mut Pointer) {
    pointer.reset();
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Resize to at least 30 × 10. Ctrl+C exits."),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(if area.height < 12 { 1 } else { 2 }),
        ])
        .split(area);
    pointer.input = Region::from(rows[0]);
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" Wordglide · English ")
        .border_style(app.theme.border(app.focus == Focus::Input));
    let available = rows[0].width.saturating_sub(3) as usize;
    let before = &app.input[..app.cursor];
    let mut start = 0;
    while before[start..].width() > available.saturating_sub(1) {
        start += before[start..].chars().next().map_or(0, char::len_utf8);
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(app.input[start..].to_string()),
            Span::styled(app.inline_suffix().unwrap_or_default(), app.theme.dim()),
        ]))
        .block(input_block),
        rows[0],
    );
    if app.focus == Focus::Input && !app.picking {
        frame.set_cursor_position((
            rows[0].x + 1 + before[start..].width() as u16,
            rows[0].y + 1,
        ));
    }
    let panes = if area.width >= 80 {
        let longest = app
            .results
            .iter()
            .map(|c| c.headword.width())
            .max()
            .unwrap_or(0);
        let min_w = (area.width as usize * 18) / 100;
        let max_w = (area.width as usize * 40) / 100;
        let width = ((longest + 4).clamp(min_w, max_w).max(16)) as u16;
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(width), Constraint::Min(1)])
            .split(rows[1])
    } else {
        let max_height = (rows[1].height / 2)
            .max(4)
            .min(rows[1].height.saturating_sub(3));
        let min_height = 4.min(max_height);
        let height = ((app.results.len() + 2) as u16).clamp(min_height, max_height);
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(height), Constraint::Min(1)])
            .split(rows[1])
    };
    pointer.definition = Region::from(panes[1]);
    render_candidates(frame, app, panes[0], pointer);
    render_definition(frame, app, panes[1], pointer);
    let help_lines = footer_help(app, rows[2].width as usize, rows[2].height as usize);
    frame.render_widget(
        Paragraph::new(help_lines.join("\n")).style(app.theme.dim()),
        rows[2],
    );
    if app.show_help {
        render_help(frame, app);
    }
}
