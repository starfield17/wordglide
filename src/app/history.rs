use super::*;

impl App {
    pub fn back(&mut self) {
        let Some(old) = self.history.pop_back() else {
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
        self.history.push_back(self.location());
        if self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.restore(next);
    }

    pub(super) fn restore(&mut self, old: Location) {
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
        self.error = None;
        self.picking = false;
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
