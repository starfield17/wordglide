use crate::{
    App, Focus,
    app::{Action, Overlay},
};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::reading::ReadingLine;

/// Lines moved per wheel notch. Terminals send one event per notch, so this
/// stays small enough to feel continuous.
pub(super) const WHEEL_LINES: usize = 3;

#[derive(Default, Clone, Copy)]
pub(super) struct Region {
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) width: u16,
    pub(super) height: u16,
}

impl Region {
    pub(super) fn contains(&self, column: u16, row: u16) -> bool {
        self.width > 0
            && self.height > 0
            && column >= self.x
            && column < self.x + self.width
            && row >= self.y
            && row < self.y + self.height
    }
}

impl From<Rect> for Region {
    fn from(area: Rect) -> Self {
        Self {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        }
    }
}

struct HitToken {
    start: u16,
    end: u16,
    word: String,
}

struct HitRow {
    y: u16,
    tokens: Vec<HitToken>,
}

#[derive(Default)]
pub(super) struct Pointer {
    pub(super) input: Region,
    pub(super) input_positions: Vec<(u16, usize)>,
    pub(super) panel: Region,
    pub(super) navigation_rows: Vec<(Region, usize)>,
    pub(super) command_rows: Vec<(Region, Action, usize)>,
    pub(super) footer: Vec<(Region, Action)>,
    pub(super) definition: Region,
    pub(super) candidates: Region,
    pub(super) candidates_offset: usize,
    pub(super) candidates_len: usize,
    rows: Vec<HitRow>,
    pub(super) appearance_rows: [Region; 6],
}

impl Pointer {
    pub(super) fn reset(&mut self) {
        self.input = Region::default();
        self.input_positions.clear();
        self.panel = Region::default();
        self.command_rows.clear();
        self.navigation_rows.clear();
        self.footer.clear();
        self.definition = Region::default();
        self.candidates = Region::default();
        self.candidates_offset = 0;
        self.candidates_len = 0;
        self.rows.clear();
        self.appearance_rows = [Region::default(); 6];
    }

    pub(super) fn record_rows(&mut self, inner: Rect, visible: &[&ReadingLine]) {
        self.rows.clear();
        for (i, line) in visible.iter().enumerate() {
            let y = inner.y + i as u16;
            let mut tokens = Vec::new();
            let mut column = inner.x;
            for token in line.text.split_word_bounds() {
                let width = token.width() as u16;
                // Whitespace and punctuation cannot be looked up; skip allocating
                // hit targets for them.
                if width > 0 && token.chars().any(char::is_alphanumeric) {
                    tokens.push(HitToken {
                        start: column,
                        end: column + width,
                        word: token.to_string(),
                    });
                }
                column += width;
            }
            self.rows.push(HitRow { y, tokens });
        }
    }

    fn definition_word(&self, column: u16, row: u16) -> Option<String> {
        self.rows
            .iter()
            .find(|r| r.y == row)?
            .tokens
            .iter()
            .find(|t| column >= t.start && column < t.end)
            .map(|t| t.word.clone())
    }

    #[cfg(test)]
    pub(super) fn word_position(&self, word: &str) -> Option<(u16, u16)> {
        for row in &self.rows {
            if let Some(token) = row.tokens.iter().find(|t| t.word == word) {
                return Some((token.start, row.y));
            }
        }
        None
    }
}

/// Handle a mouse event against the last rendered layout. Returns whether the
/// visible state changed. Motion and drag are ignored so `?1003h` traffic never
/// forces a redraw.
pub(super) fn on_mouse(app: &mut App, pointer: &Pointer, mouse: MouseEvent) -> bool {
    if app.view.show_appearance() {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.view.appearance_row = (app.view.appearance_row + 5) % 6,
            MouseEventKind::ScrollDown => {
                app.view.appearance_row = (app.view.appearance_row + 1) % 6
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = pointer
                    .appearance_rows
                    .iter()
                    .position(|row| row.contains(mouse.column, mouse.row))
                {
                    app.view.appearance_row = index;
                    app.change_appearance(false);
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        return true;
    }
    if app.view.show_help() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                app.view.help_scroll = app.view.help_scroll.saturating_sub(WHEEL_LINES)
            }
            MouseEventKind::ScrollDown => {
                app.view.help_scroll = app.view.help_scroll.saturating_add(WHEEL_LINES)
            }
            MouseEventKind::Down(MouseButton::Left) => app.view.overlay = Overlay::None,
            _ => return false,
        }
        return true;
    }
    if app.view.overlay == Overlay::Commands {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.view.panel_row = app.view.panel_row.saturating_sub(1),
            MouseEventKind::ScrollDown => {
                app.view.panel_row =
                    (app.view.panel_row + 1).min(app.commands().len().saturating_sub(1))
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, action, index)) = pointer
                    .command_rows
                    .iter()
                    .find(|(region, _, _)| region.contains(mouse.column, mouse.row))
                {
                    app.view.panel_row = *index;
                    app.execute_action(*action);
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        return true;
    }
    if matches!(app.view.overlay, Overlay::Outline | Overlay::History) {
        let count = if app.view.overlay == Overlay::Outline {
            app.reading.sections.len()
        } else {
            app.navigation_locations().len()
        };
        match mouse.kind {
            MouseEventKind::ScrollUp => app.view.panel_row = app.view.panel_row.saturating_sub(1),
            MouseEventKind::ScrollDown => {
                app.view.panel_row = (app.view.panel_row + 1).min(count.saturating_sub(1))
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, index)) = pointer
                    .navigation_rows
                    .iter()
                    .find(|(region, _)| region.contains(mouse.column, mouse.row))
                {
                    app.view.panel_row = *index;
                    app.accept_panel_row();
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        return true;
    }
    if app.view.overlay == Overlay::Find {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.next_match(true),
            MouseEventKind::ScrollDown => app.next_match(false),
            _ => return false,
        }
        return true;
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => return app.scroll_by(-(WHEEL_LINES as isize)),
        MouseEventKind::ScrollDown => return app.scroll_by(WHEEL_LINES as isize),
        MouseEventKind::Down(MouseButton::Left) => {}
        // Motion, drag, and other buttons are never read, so they must not
        // force a redraw either.
        _ => return false,
    }
    if let Some((_, action)) = pointer
        .footer
        .iter()
        .find(|(region, _)| region.contains(mouse.column, mouse.row))
    {
        app.execute_action(*action);
        return true;
    }
    if pointer.definition.contains(mouse.column, mouse.row) {
        if app.picking {
            app.picking = false;
            app.label_input.clear();
            return true;
        }
        if app.focus != Focus::Definition {
            app.focus = Focus::Definition;
            return true;
        }
        if let Some(word) = pointer.definition_word(mouse.column, mouse.row)
            && app.contains(&word)
            && crate::normalize(&word) != crate::normalize(&app.input)
        {
            app.jump_to(&word);
            return true;
        }
        return false;
    }
    if pointer.candidates.contains(mouse.column, mouse.row) {
        let row = (mouse.row - pointer.candidates.y) as usize;
        let index = pointer.candidates_offset + row;
        return index < pointer.candidates_len && app.click_candidate(index);
    }
    if pointer.input.contains(mouse.column, mouse.row) {
        let position = pointer
            .input_positions
            .iter()
            .min_by_key(|(column, _)| column.abs_diff(mouse.column))
            .map_or(app.cursor, |(_, byte)| *byte);
        let changed = app.focus != Focus::Input || app.cursor != position || app.picking;
        app.cursor = position;
        app.focus = Focus::Input;
        app.picking = false;
        app.label_input.clear();
        return changed;
    }
    false
}
