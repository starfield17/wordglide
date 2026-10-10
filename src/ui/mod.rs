mod appearance;
mod help;
mod panels;
mod panes;
mod pointer;
mod reading;
mod terminal;

pub use terminal::{
    RunOptions, download_data, download_data_with_cancel, run, run_with_options,
    run_without_dictionary,
};

use crate::{App, Focus, ReadingLayout, app::Overlay};
#[cfg(test)]
use help::footer_help;
use help::{render_footer, render_help};
use panes::{render_candidates, render_definition};
use pointer::{Pointer, Region};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_segmentation::UnicodeSegmentation;
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
    frame.render_widget(Block::default().style(app.theme.surface()), area);
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
        .border_type(BorderType::Rounded)
        .title(" Wordglide · English ")
        .border_style(app.theme.border(app.focus == Focus::Input));
    let available = rows[0].width.saturating_sub(3) as usize;
    let before = &app.input[..app.cursor];
    let mut start = 0;
    while before[start..].width() > available.saturating_sub(1) {
        start += before[start..].graphemes(true).next().map_or(0, str::len);
    }
    let mut column = rows[0].x + 1;
    pointer.input_positions.push((column, start));
    for (byte, grapheme) in app.input[start..].grapheme_indices(true) {
        column = column.saturating_add(grapheme.width() as u16);
        if column >= rows[0].right().saturating_sub(1) {
            break;
        }
        pointer
            .input_positions
            .push((column, start + byte + grapheme.len()));
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(app.input[start..].to_string()),
            Span::styled(app.inline_suffix().unwrap_or_default(), app.theme.dim()),
        ]))
        .block(input_block),
        rows[0],
    );
    if app.focus == Focus::Input && !app.picking && !app.view.modal() {
        frame.set_cursor_position((
            rows[0].x + 1 + before[start..].width() as u16,
            rows[0].y + 1,
        ));
    }
    let focused = app.focus == Focus::Definition
        && app.reading_preferences().reading_layout == ReadingLayout::Focus;
    let panes = if focused {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(0), Constraint::Min(1)])
            .split(rows[1])
    } else if area.width >= 80 {
        let width = ((area.width as usize * 24) / 100).clamp(20, 36) as u16;
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
    if !focused {
        render_candidates(frame, app, panes[0], pointer);
    }
    render_definition(frame, app, panes[1], pointer);
    render_footer(frame, app, rows[2], pointer);
    if app.view.show_help() {
        render_help(frame, app);
    }
    if matches!(
        app.view.overlay,
        Overlay::Outline | Overlay::History | Overlay::Find
    ) {
        panels::render_reading_panel(frame, app, pointer);
    }
    if app.view.overlay == Overlay::Commands {
        panels::render_commands(frame, app, pointer);
    }
    if app.view.show_appearance() {
        appearance::render_appearance(frame, app, pointer);
    }
    if app.view.overlay == Overlay::Download {
        appearance::render_download(frame, app, pointer);
    }
    if app.view.overlay == Overlay::Peek {
        panels::render_peek(frame, app, pointer);
    }
}
