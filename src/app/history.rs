use super::*;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

impl App {
    pub(crate) fn navigation_labels(&self, word_columns: usize) -> Vec<(String, Action)> {
        let mut labels = Vec::new();
        for (location, arrow, shortcut, action) in [
            (self.history.back(), "←", "Ctrl+Z", Action::Back),
            (self.forward.back(), "→", "Ctrl+Y", Action::Forward),
        ] {
            if let Some(location) = location {
                let word = location
                    .preview
                    .as_ref()
                    .map_or(location.input.as_str(), |p| p.entry.headword.as_str());
                let mut label = String::new();
                for g in word.graphemes(true) {
                    if label.width() + g.width() > word_columns.saturating_sub(1) {
                        break;
                    }
                    label.push_str(g);
                }
                if label != word {
                    label.push('…');
                }
                let word = label;
                labels.push((format!("{arrow} {word} ({shortcut})"), action));
            }
        }
        labels
    }

    pub(super) fn push_history(&mut self, location: Location) {
        if location.preview.is_none() && normalize(&location.input).is_empty() {
            return;
        }
        if let Some(last) = self.history.back()
            && last.preview.as_ref().map(|p| p.entry.key.as_str())
                == location.preview.as_ref().map(|p| p.entry.key.as_str())
            && normalize(&last.input) == normalize(&location.input)
            && last.selected == location.selected
            && last.scroll == location.scroll
        {
            return;
        }
        self.history.push_back(location);
        if self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
    }

    pub(super) fn capture_studied_origin(&mut self) {
        if self.studied && self.preview.is_some() {
            self.studied_origin = Some(self.location());
            self.studied = false;
        }
    }

    pub(super) fn confirm_studied(&mut self, next_key: Option<&str>, next_input: &str) {
        if let Some(origin) = self.studied_origin.take() {
            let origin_key = origin.preview.as_ref().map(|p| p.entry.key.as_str());
            let same_entry = match (origin_key, next_key) {
                (Some(a), Some(b)) => a == b,
                _ => normalize(&origin.input) == normalize(next_input),
            };
            if !same_entry {
                self.push_history(origin);
            }
        }
        self.studied = true;
    }

    pub(crate) fn focus_input(&mut self) {
        self.capture_studied_origin();
        self.cancel_picking();
        self.focus = Focus::Input;
    }

    pub(crate) fn focus_definition(&mut self) {
        if self.preview.is_some() && !self.loading {
            let key = self.preview.as_ref().map(|p| p.entry.key.clone());
            let input = self.input.clone();
            self.confirm_studied(key.as_deref(), &input);
        }
        self.focus = Focus::Definition;
    }

    pub fn back(&mut self) {
        let origin = self.studied_origin.take().filter(|origin| {
            let origin_key = origin.preview.as_ref().map(|p| p.entry.key.as_str());
            let same_entry = if self.loading {
                normalize(&origin.input) == normalize(&self.input)
                    && (self.results.is_empty()
                        || origin_key == self.results.get(self.selected).map(|c| c.key.as_str()))
            } else {
                match (
                    origin_key,
                    self.preview.as_ref().map(|p| p.entry.key.as_str()),
                ) {
                    (Some(a), Some(b)) => a == b,
                    _ => normalize(&origin.input) == normalize(&self.input),
                }
            };
            !same_entry
        });
        let Some(old) = origin.or_else(|| self.history.pop_back()) else {
            return;
        };
        self.forward.push_back(self.location());
        if self.forward.len() > HISTORY_LIMIT {
            self.forward.pop_front();
        }
        self.restore(old);
    }

    /// Undo the last `back` and return to the word left behind. A new lookup or
    /// follow clears this forward stack.
    pub fn forward(&mut self) {
        let Some(next) = self.forward.pop_back() else {
            return;
        };
        self.push_history(self.location());
        self.restore(next);
    }

    pub(super) fn restore(&mut self, old: Location) {
        self.close_peek();
        self.completion = None;
        self.pending_completion.clear();
        self.generation += 1;
        self.input = old.input;
        self.cursor = old.cursor;
        self.results = old.results;
        self.selected = old.selected;
        self.preview = old.preview;
        self.scroll = old.scroll;
        self.reading.clear_find();
        self.reading.restore_anchor = old.anchor;
        self.reading.restore_scroll = old.scroll;
        self.focus = old.focus;
        self.studied = old.focus == Focus::Definition && self.preview.is_some();
        self.studied_origin = None;
        self.error = None;
        self.cancel_picking();
        self.labels.clear();
        self.loading = old.loading || (self.preview.is_none() && !self.results.is_empty());
        if self.loading {
            let request = if let Some(candidate) = self.results.get(self.selected) {
                Request::Preview(self.generation, candidate.clone())
            } else {
                Request::Search(self.generation, self.input.clone())
            };
            if self.send_request(request).is_err() {
                self.worker_error();
            }
        }
    }
}
