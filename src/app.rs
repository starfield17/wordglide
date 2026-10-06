use crate::{Candidate, Dictionary, Preview, index::Index, normalize};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::VecDeque,
    sync::{Arc, mpsc},
    thread,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Input,
    Definition,
}

enum Request {
    Search(u64, String),
    Preview(u64, Candidate),
}
enum Response {
    Search(u64, Result<(Vec<Candidate>, Option<Preview>), String>),
    Preview(u64, String, Result<Preview, String>),
}

#[derive(Clone)]
struct Location {
    input: String,
    cursor: usize,
    results: Vec<Candidate>,
    selected: usize,
    preview: Option<Preview>,
    scroll: usize,
    focus: Focus,
    loading: bool,
}

struct Completion {
    original: Location,
    index: Option<usize>,
}

pub struct App {
    pub input: String,
    pub cursor: usize,
    pub results: Vec<Candidate>,
    pub selected: usize,
    pub preview: Option<Preview>,
    pub scroll: usize,
    pub focus: Focus,
    pub loading: bool,
    pub error: Option<String>,
    pub picking: bool,
    pub label_input: String,
    pub labels: Vec<(String, String)>,
    pub exit: bool,
    pub(crate) max_scroll: usize,
    lexicon: Arc<Index>,
    history: VecDeque<Location>,
    generation: u64,
    request: mpsc::Sender<Request>,
    response: mpsc::Receiver<Response>,
    completion: Option<Completion>,
    pending_completion: Vec<KeyEvent>,
}

impl App {
    pub fn new(mut dictionary: Dictionary, query: &str) -> Self {
        let lexicon = dictionary.lexicon();
        let (tx, requests) = mpsc::channel();
        let (responses, rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(mut request) = requests.recv() {
                // Coalesce queued keystrokes/selection changes, never publish an old result in the UI.
                for newer in requests.try_iter() {
                    request = newer;
                }
                let response = match request {
                    Request::Search(id, text) => {
                        let result = (|| {
                            let candidates = dictionary.search(&text)?;
                            let preview = candidates
                                .first()
                                .map(|c| dictionary.preview(c))
                                .transpose()?;
                            Ok::<_, anyhow::Error>((candidates, preview))
                        })()
                        .map_err(|e| format!("{e:#}"));
                        Response::Search(id, result)
                    }
                    Request::Preview(id, candidate) => Response::Preview(
                        id,
                        candidate.key.clone(),
                        dictionary.preview(&candidate).map_err(|e| format!("{e:#}")),
                    ),
                };
                if responses.send(response).is_err() {
                    break;
                }
            }
        });
        let mut app = Self {
            input: query.into(),
            cursor: query.len(),
            results: vec![],
            selected: 0,
            preview: None,
            scroll: 0,
            focus: Focus::Input,
            loading: false,
            error: None,
            picking: false,
            label_input: String::new(),
            labels: vec![],
            exit: false,
            max_scroll: 0,
            lexicon,
            history: VecDeque::new(),
            generation: 0,
            request: tx,
            response: rx,
            completion: None,
            pending_completion: vec![],
        };
        app.search();
        app
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn contains(&self, word: &str) -> bool {
        self.lexicon.exact(&normalize(word)).is_some()
    }

    fn search(&mut self) {
        self.completion = None;
        self.pending_completion.clear();
        self.generation += 1;
        self.selected = 0;
        self.scroll = 0;
        self.max_scroll = 0;
        self.preview = None;
        self.results.clear();
        self.error = None;
        self.picking = false;
        self.labels.clear();
        self.loading = !normalize(&self.input).is_empty();
        if self
            .request
            .send(Request::Search(self.generation, self.input.clone()))
            .is_err()
        {
            self.worker_error();
        }
    }

    fn worker_error(&mut self) {
        self.loading = false;
        self.error = Some("Dictionary worker stopped".into());
    }

    /// Apply only responses belonging to the current query and selected candidate.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(response) = self.response.try_recv() {
            match response {
                Response::Search(id, result) if id == self.generation => {
                    self.loading = false;
                    match result {
                        Ok((results, preview)) => {
                            self.results = results;
                            self.selected = 0;
                            self.preview = preview;
                        }
                        Err(error) => self.error = Some(error),
                    }
                    changed = true;
                }
                Response::Preview(id, key, result)
                    if id == self.generation
                        && self
                            .results
                            .get(self.selected)
                            .is_some_and(|c| c.key == key) =>
                {
                    self.loading = false;
                    match result {
                        Ok(preview) => self.preview = Some(preview),
                        Err(error) => self.error = Some(error),
                    }
                    changed = true;
                }
                _ => {}
            }
        }
        if !self.loading && !self.pending_completion.is_empty() {
            let pending = std::mem::take(&mut self.pending_completion);
            let generation = self.generation;
            for key in pending {
                self.handle_key(key);
                if self.generation != generation {
                    break;
                }
            }
            changed = true;
        }
        changed
    }

    fn select(&mut self, selected: usize) {
        if selected == self.selected || selected >= self.results.len() {
            return;
        }
        self.selected = selected;
        self.scroll = 0;
        self.max_scroll = 0;
        self.preview = None;
        self.error = None;
        self.loading = true;
        self.picking = false;
        if self
            .request
            .send(Request::Preview(
                self.generation,
                self.results[selected].clone(),
            ))
            .is_err()
        {
            self.worker_error();
        }
    }

    pub fn jump_to(&mut self, word: &str) {
        if !self.contains(word) {
            return;
        }
        self.history.push_back(self.location());
        if self.history.len() > 64 {
            self.history.pop_front();
        }
        self.input = normalize(word);
        self.cursor = self.input.len();
        self.focus = Focus::Definition;
        self.search();
    }

    fn location(&self) -> Location {
        Location {
            input: self.input.clone(),
            cursor: self.cursor,
            results: self.results.clone(),
            selected: self.selected,
            preview: self.preview.clone(),
            scroll: self.scroll,
            focus: self.focus,
            loading: self.loading,
        }
    }

    pub fn back(&mut self) {
        let Some(old) = self.history.pop_back() else {
            return;
        };
        self.restore(old);
    }

    fn restore(&mut self, old: Location) {
        self.completion = None;
        self.pending_completion.clear();
        self.generation += 1;
        self.input = old.input;
        self.cursor = old.cursor;
        self.results = old.results;
        self.selected = old.selected;
        self.preview = old.preview;
        self.scroll = old.scroll;
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
            if self.request.send(request).is_err() {
                self.worker_error();
            }
        }
    }

    pub(crate) fn inline_suffix(&self) -> Option<String> {
        if self.focus != Focus::Input
            || self.cursor != self.input.len()
            || self.input.is_empty()
            || self.input.ends_with(char::is_whitespace)
        {
            return None;
        }
        let candidate = self.results.get(self.selected)?;
        let query = normalize(&self.input);
        let suffix = candidate.key.strip_prefix(&query)?;
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.into())
        }
    }

    fn accept(&mut self, reading: bool) {
        let Some(candidate) = self.results.get(self.selected) else {
            return;
        };
        let word = candidate.headword.clone();
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

    fn cycle_completion(&mut self, reverse: bool) {
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
            if let Some(common) = self.lexicon.common_prefix(&query)
                && common.len() > query.len()
            {
                self.input = common;
                self.cursor = self.input.len();
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

    pub fn handle_key(&mut self, key: KeyEvent) {
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
                KeyCode::Char('o') => self.back(),
                KeyCode::Char('u') => {
                    self.input.clear();
                    self.cursor = 0;
                    self.focus = Focus::Input;
                    self.search();
                }
                KeyCode::Char('n') => self.select(self.selected.saturating_add(1)),
                KeyCode::Char('p') => self.select(self.selected.saturating_sub(1)),
                KeyCode::Char('a') => self.cursor = 0,
                KeyCode::Char('e') => self.cursor = self.input.len(),
                KeyCode::Char('f') if self.inline_suffix().is_some() => self.accept(false),
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
        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Left {
            self.back();
            return;
        }
        if self.picking {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => {
                    self.picking = false;
                    self.label_input.clear();
                }
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
            KeyCode::PageDown => self.scroll = (self.scroll + 10).min(self.max_scroll),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
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
            KeyCode::Char('f') if self.focus == Focus::Definition && self.preview.is_some() => {
                self.picking = true;
                self.label_input.clear();
            }
            KeyCode::Char('j') if self.focus == Focus::Definition => {
                self.scroll = (self.scroll + 1).min(self.max_scroll)
            }
            KeyCode::Char('k') if self.focus == Focus::Definition => {
                self.scroll = self.scroll.saturating_sub(1)
            }
            KeyCode::Left if self.focus == Focus::Input => {
                self.cursor = self.input[..self.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i)
            }
            KeyCode::Right if self.focus == Focus::Input => {
                if self.inline_suffix().is_some() {
                    self.accept(false);
                } else if let Some(c) = self.input[self.cursor..].chars().next() {
                    self.cursor += c.len_utf8();
                }
            }
            KeyCode::Home if self.focus == Focus::Input => self.cursor = 0,
            KeyCode::End if self.focus == Focus::Input => self.cursor = self.input.len(),
            KeyCode::Backspace if self.focus == Focus::Input && self.cursor > 0 => {
                let previous = self.input[..self.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i);
                self.input.drain(previous..self.cursor);
                self.cursor = previous;
                self.search();
            }
            KeyCode::Delete if self.focus == Focus::Input => {
                if let Some(c) = self.input[self.cursor..].chars().next() {
                    self.input.drain(self.cursor..self.cursor + c.len_utf8());
                    self.search();
                }
            }
            KeyCode::Char(c) if self.focus == Focus::Input => {
                self.input.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.search();
            }
            _ => {}
        }
    }

    pub fn paste(&mut self, text: &str) {
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
            self.search();
        }
    }
}
