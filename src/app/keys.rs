use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

use super::*;

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Byte offset of the start of the word before `cursor`, skipping whitespace.
fn prev_word_boundary(text: &str, cursor: usize) -> usize {
    let mut index = cursor.min(text.len());
    while index > 0 {
        let previous = text[..index].graphemes(true).next_back().unwrap();
        if previous.chars().all(char::is_whitespace) {
            index -= previous.len();
        } else {
            break;
        }
    }
    while index > 0 {
        let previous = text[..index].graphemes(true).next_back().unwrap();
        if previous.chars().any(word_char) {
            index -= previous.len();
        } else {
            break;
        }
    }
    index
}

/// Byte offset of the end of the next word after `cursor`, skipping whitespace.
fn next_word_boundary(text: &str, cursor: usize) -> usize {
    let mut index = cursor.min(text.len());
    while index < text.len() {
        let next = text[index..].graphemes(true).next().unwrap();
        if next.chars().all(char::is_whitespace) {
            index += next.len();
        } else {
            break;
        }
    }
    while index < text.len() {
        let next = text[index..].graphemes(true).next().unwrap();
        if next.chars().any(word_char) {
            index += next.len();
        } else {
            break;
        }
    }
    index
}

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Overlays own input before completion can queue it or lookup can act.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.exit = true;
            return;
        }
        if !self.view.modal() && key.code == KeyCode::F(4) {
            self.execute_action(Action::Layout);
            return;
        }
        if self.view.overlay == Overlay::Download {
            self.panel_key(key);
            return;
        }
        if !self.view.modal()
            && key.code == KeyCode::Char('r')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.execute_action(Action::Navigation);
            return;
        }
        let panel = match key.code {
            KeyCode::F(1) => Some(Overlay::Help),
            KeyCode::F(2) => Some(Overlay::Appearance),
            KeyCode::F(3) => Some(Overlay::Commands),
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Overlay::Commands)
            }
            _ => None,
        };
        if let Some(panel) = panel {
            self.open_panel(panel);
            return;
        }
        if self.view.modal() {
            self.panel_key(key);
            return;
        }
        let completion_key = matches!(key.code, KeyCode::Tab | KeyCode::BackTab | KeyCode::Enter)
            || (key.code == KeyCode::Right && self.cursor == self.input.len())
            || (key.code == KeyCode::Char('f') && key.modifiers.contains(KeyModifiers::CONTROL));
        if self.focus == Focus::Input && self.loading && self.results.is_empty() && completion_key {
            if self.pending_completion.len() < 64 {
                self.pending_completion.push(key);
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.exit = true,
                KeyCode::Char('z') => self.back(),
                KeyCode::Char('y') => self.forward(),
                KeyCode::Char('u') if self.focus == Focus::Input => {
                    self.input.clear();
                    self.cursor = 0;
                    self.focus = Focus::Input;
                    self.search();
                }
                KeyCode::Char('n') => self.select(self.selected.saturating_add(1)),
                KeyCode::Char('p') => self.select(self.selected.saturating_sub(1)),
                KeyCode::Char('a') if self.focus == Focus::Input => self.cursor = 0,
                KeyCode::Char('e') if self.focus == Focus::Input => self.cursor = self.input.len(),
                KeyCode::Char('f') if self.focus == Focus::Input => self.accept_inline(),
                KeyCode::Char('w') if self.focus == Focus::Input => {
                    let previous = prev_word_boundary(&self.input, self.cursor);
                    self.input.drain(previous..self.cursor);
                    self.cursor = previous;
                    self.snap_cursor();
                    self.search();
                }
                KeyCode::Char('j') if self.focus == Focus::Definition => {
                    self.scroll_by(1);
                }
                KeyCode::Char('k') if self.focus == Focus::Definition => {
                    self.scroll_by(-1);
                }
                KeyCode::Char('k') if self.focus == Focus::Input => {
                    self.input.truncate(self.cursor);
                    self.search();
                }
                KeyCode::Left if self.focus == Focus::Input => {
                    self.cursor = prev_word_boundary(&self.input, self.cursor);
                }
                KeyCode::Right if self.focus == Focus::Input => {
                    self.cursor = next_word_boundary(&self.input, self.cursor);
                }
                KeyCode::Char('l') => {
                    if self.completion.is_some() {
                        self.accept(false);
                    }
                    self.pending_completion.clear();
                    self.focus = if self.focus == Focus::Input {
                        Focus::Definition
                    } else {
                        Focus::Input
                    };
                }
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => self.back(),
                KeyCode::Right => self.forward(),
                KeyCode::Char('p') => self.execute_action(Action::Peek),
                KeyCode::Backspace if self.focus == Focus::Input => {
                    let previous = prev_word_boundary(&self.input, self.cursor);
                    self.input.drain(previous..self.cursor);
                    self.cursor = previous;
                    self.snap_cursor();
                    self.search();
                }
                _ => {}
            }
            return;
        }
        if self.picking {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => {
                    self.picking = false;
                    self.label_input.clear();
                }
                // Scrolling keeps hints usable on definitions longer than the
                // pane; labels are rebuilt for the new lines on the next frame.
                KeyCode::PageDown => {
                    self.scroll_by(self.page as isize);
                }
                KeyCode::PageUp => {
                    self.scroll_by(-(self.page as isize));
                }
                KeyCode::Home => self.scroll = 0,
                KeyCode::End => self.scroll = self.max_scroll,
                KeyCode::Backspace => {
                    self.label_input.pop();
                }
                KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                    self.label_input.push(c.to_ascii_lowercase());
                    if self.label_input.len() == 2 {
                        let target = self
                            .labels
                            .iter()
                            .find(|(label, _)| label == &self.label_input)
                            .map(|(_, word)| word.clone());
                        self.label_input.clear();
                        self.picking = false;
                        if let Some(word) = target {
                            self.jump_to(&word);
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Tab if self.focus == Focus::Input => self.cycle_completion(false),
            KeyCode::BackTab if self.focus == Focus::Input => self.cycle_completion(true),
            KeyCode::Enter if self.focus == Focus::Input => self.accept(true),
            KeyCode::Down if self.completion.is_some() => self.cycle_completion(false),
            KeyCode::Up if self.completion.is_some() => self.cycle_completion(true),
            KeyCode::Down => self.select(self.selected.saturating_add(1)),
            KeyCode::Up => self.select(self.selected.saturating_sub(1)),
            KeyCode::PageDown => {
                self.scroll_by(self.page as isize);
            }
            KeyCode::PageUp => {
                self.scroll_by(-(self.page as isize));
            }
            KeyCode::Esc => {
                self.pending_completion.clear();
                if self.focus == Focus::Input {
                    if let Some(completion) = self.completion.take() {
                        self.restore(completion.original);
                    }
                } else {
                    self.focus = Focus::Input;
                }
            }
            KeyCode::Char('/') if self.focus == Focus::Definition => {
                self.execute_action(Action::Find)
            }
            KeyCode::Char('o') if self.focus == Focus::Definition => {
                self.execute_action(Action::Outline)
            }
            KeyCode::Char('n') if self.focus == Focus::Definition => self.next_match(false),
            KeyCode::Char('N') if self.focus == Focus::Definition => self.next_match(true),
            KeyCode::Char('[') if self.focus == Focus::Definition => self.move_section(true),
            KeyCode::Char(']') if self.focus == Focus::Definition => self.move_section(false),
            KeyCode::Char('f') if self.focus == Focus::Definition && self.preview.is_some() => {
                self.picking = true;
                self.label_input.clear();
            }
            KeyCode::Char('p') if self.focus == Focus::Definition && self.preview.is_some() => {
                self.view.expand_ipa = !self.view.expand_ipa;
            }
            KeyCode::Char('e') if self.focus == Focus::Definition && self.preview.is_some() => {
                self.view.expand_examples = !self.view.expand_examples;
            }
            KeyCode::Char('?') if self.focus == Focus::Definition => {
                self.open_panel(Overlay::Help);
            }
            KeyCode::Char(' ') if self.focus == Focus::Definition => {
                self.execute_action(Action::Peek);
            }
            KeyCode::Home if self.focus == Focus::Definition => self.scroll = 0,
            KeyCode::End if self.focus == Focus::Definition => self.scroll = self.max_scroll,
            KeyCode::Left if self.focus == Focus::Input => {
                self.cursor = self.input[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(i, _)| i)
            }
            KeyCode::Right if self.focus == Focus::Input => {
                if self.inline_candidate().is_some() {
                    self.accept_inline();
                } else if let Some(c) = self.input[self.cursor..].graphemes(true).next() {
                    self.cursor += c.len();
                }
            }
            KeyCode::Home if self.focus == Focus::Input => self.cursor = 0,
            KeyCode::End if self.focus == Focus::Input => self.cursor = self.input.len(),
            KeyCode::Backspace if self.focus == Focus::Input && self.cursor > 0 => {
                let previous = self.input[..self.cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(i, _)| i);
                self.input.drain(previous..self.cursor);
                self.cursor = previous;
                self.snap_cursor();
                self.search();
            }
            KeyCode::Delete if self.focus == Focus::Input => {
                if let Some(c) = self.input[self.cursor..].graphemes(true).next() {
                    self.input.drain(self.cursor..self.cursor + c.len());
                    self.snap_cursor();
                    self.search();
                }
            }
            KeyCode::Char(c) if self.focus == Focus::Input => {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.snap_cursor();
                self.search();
            }
            _ => {}
        }
    }

    fn snap_cursor(&mut self) {
        if let Some((byte, grapheme)) = self
            .input
            .grapheme_indices(true)
            .find(|(byte, g)| *byte < self.cursor && self.cursor < *byte + g.len())
        {
            self.cursor = byte + grapheme.len();
        }
    }

    pub fn paste(&mut self, text: &str) {
        if self.view.modal() {
            if matches!(
                self.view.overlay,
                Overlay::Commands | Overlay::History | Overlay::Find
            ) {
                self.panel_paste(text);
            }
            return;
        }
        if self.focus == Focus::Input {
            let clean: String = text
                .chars()
                .filter_map(|c| {
                    if c.is_whitespace() {
                        Some(' ')
                    } else if c.is_control() {
                        None
                    } else {
                        Some(c)
                    }
                })
                .collect();
            self.input.insert_str(self.cursor, &clean);
            self.cursor += clean.len();
            self.snap_cursor();
            self.search();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundaries_skip_whitespace_and_punctuation() {
        let text = "take off now";
        assert_eq!(prev_word_boundary(text, text.len()), 9);
        assert_eq!(prev_word_boundary(text, 9), 5);
        assert_eq!(prev_word_boundary(text, 5), 0);
        assert_eq!(prev_word_boundary(text, 0), 0);
        assert_eq!(next_word_boundary(text, 0), 4);
        assert_eq!(next_word_boundary(text, 4), 8);
        assert_eq!(next_word_boundary(text, 8), 12);
        assert_eq!(next_word_boundary(text, text.len()), text.len());
    }

    #[test]
    fn word_boundaries_are_utf8_safe() {
        let text = "café résumé";
        let end = text.len();
        let resume = prev_word_boundary(text, end);
        assert_eq!(&text[resume..end], "résumé");
        let before_resume = prev_word_boundary(text, resume);
        assert_eq!(before_resume, 0);
        assert_eq!(next_word_boundary(text, 0), "café".len());
    }
}
