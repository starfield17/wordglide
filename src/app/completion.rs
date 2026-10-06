use super::*;

pub(super) struct Completion {
    pub(super) original: Location,
    pub(super) index: Option<usize>,
}

impl App {
    pub(super) fn inline_candidate(&self) -> Option<&Candidate> {
        if self.focus != Focus::Input
            || self.cursor != self.input.len()
            || self.input.is_empty()
            || self.input.ends_with(char::is_whitespace)
        {
            return None;
        }
        let query = normalize(&self.input);
        let extends = |candidate: &&Candidate| {
            candidate.key.len() > query.len() && candidate.key.starts_with(&query)
        };
        // Prefer a plain single word over hyphenated or multi-word compounds so
        // the ghost does not predict an obscure `house-like` over `household`.
        let plain =
            |candidate: &&Candidate| extends(candidate) && !candidate.key.contains(['-', ' ']);
        let candidate = self
            .results
            .get(self.selected)
            .filter(|candidate| plain(candidate))
            .or_else(|| self.results.iter().find(|candidate| plain(candidate)))
            .or_else(|| self.results.get(self.selected).filter(extends))
            .or_else(|| self.results.iter().find(extends))?;
        // Suppress a suggestion that is not more useful than the exact match.
        if let Some(exact) = self.results.iter().find(|candidate| candidate.key == query)
            && candidate.score <= exact.score
        {
            return None;
        }
        Some(candidate)
    }

    pub(crate) fn inline_suffix(&self) -> Option<String> {
        let candidate = self.inline_candidate()?;
        Some(candidate.key[normalize(&self.input).len()..].into())
    }

    pub(super) fn accept_inline(&mut self) {
        if let Some(candidate) = self.inline_candidate() {
            let word = candidate.headword.clone();
            self.accept_word(word, false);
        }
    }

    pub(super) fn accept(&mut self, reading: bool) {
        let Some(candidate) = self.results.get(self.selected) else {
            return;
        };
        let word = candidate.headword.clone();
        self.accept_word(word, reading);
    }

    fn accept_word(&mut self, word: String, reading: bool) {
        let restart = self.input != word || self.completion.is_some();
        self.input = word;
        self.cursor = self.input.len();
        if restart {
            self.search();
        }
        if reading {
            self.focus = Focus::Definition;
        }
    }

    pub(super) fn cycle_completion(&mut self, reverse: bool) {
        if self.results.is_empty() {
            return;
        }
        if self.completion.is_none() {
            self.completion = Some(Completion {
                original: self.location(),
                index: None,
            });
            if self.results.len() == 1 {
                self.fill_completion(0);
                return;
            }
            let query = normalize(&self.input);
            match self.lexicon.common_prefix(&query) {
                Ok(Some(common)) if common.len() > query.len() => {
                    self.input = common;
                    self.cursor = self.input.len();
                }
                Err(error) => {
                    self.error = Some(format!("{error:#}"));
                    self.completion = None;
                    return;
                }
                _ => {}
            }
            if !reverse {
                return;
            }
        }
        let current = self.completion.as_ref().unwrap().index;
        let count = self.results.len();
        let next = match current {
            None => {
                if reverse {
                    count - 1
                } else {
                    0
                }
            }
            Some(i) => {
                if reverse {
                    (i + count - 1) % count
                } else {
                    (i + 1) % count
                }
            }
        };
        self.fill_completion(next);
    }

    fn fill_completion(&mut self, index: usize) {
        self.completion.as_mut().unwrap().index = Some(index);
        self.input = self.results[index].headword.clone();
        self.cursor = self.input.len();
        self.select(index);
    }
}
