use crate::{App, Focus};
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
    fn contains(&self, column: u16, row: u16) -> bool {
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
    pub(super) definition: Region,
    pub(super) candidates: Region,
    pub(super) candidates_offset: usize,
    pub(super) candidates_len: usize,
    rows: Vec<HitRow>,
}

impl Pointer {
    pub(super) fn reset(&mut self) {
        self.input = Region::default();
        self.definition = Region::default();
        self.candidates = Region::default();
        self.candidates_offset = 0;
        self.candidates_len = 0;
        self.rows.clear();
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
    if app.view.show_help {
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            app.view.show_help = false;
            return true;
        }
        return false;
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => return app.scroll_by(-(WHEEL_LINES as isize)),
        MouseEventKind::ScrollDown => return app.scroll_by(WHEEL_LINES as isize),
        MouseEventKind::Down(MouseButton::Left) => {}
        // Motion, drag, and other buttons are never read, so they must not
        // force a redraw either.
        _ => return false,
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
    if pointer.input.contains(mouse.column, mouse.row) && app.focus != Focus::Input {
        app.focus = Focus::Input;
        app.picking = false;
        app.label_input.clear();
        return true;
    }
    false
}
