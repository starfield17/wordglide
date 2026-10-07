mod completion;
mod history;
mod keys;
mod view;
mod worker;

use crate::{Appearance, Candidate, Dictionary, Preview, normalize, store::Lexicon, theme::Theme};
use crossterm::event::KeyEvent;
use std::{collections::VecDeque, sync::mpsc};

use completion::Completion;
pub(crate) use view::ViewOptions;
use worker::{Request, Response};

/// Cap on remembered navigation steps in either direction.
const HISTORY_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Input,
    Definition,
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
    pub(crate) page: usize,
    pub(crate) theme: Theme,
    pub(crate) view: ViewOptions,
    pub(crate) appearance_status: Option<String>,
    lexicon: Lexicon,
    history: VecDeque<Location>,
    forward: VecDeque<Location>,
    generation: u64,
    request: mpsc::Sender<Request>,
    response: mpsc::Receiver<Response>,
    completion: Option<Completion>,
    pending_completion: Vec<KeyEvent>,
}

impl App {
    pub fn new(dictionary: Dictionary, query: &str) -> Self {
        let lexicon = dictionary.lexicon();
        let (request, response) = worker::spawn(dictionary);
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
            page: 10,
            theme: Theme::colored(),
            view: ViewOptions::default(),
            appearance_status: None,
            lexicon,
            history: VecDeque::new(),
            forward: VecDeque::new(),
            generation: 0,
            request,
            response,
            completion: None,
            pending_completion: vec![],
        };
        app.search();
        app
    }

    /// Enable or suppress colors without changing appearance preferences.
    pub fn set_color(&mut self, color: bool) {
        self.theme.color = color;
    }

    /// Apply appearance preferences without modifying lookup or reading state.
    /// File persistence is owned by `run_with_options`, not by `App`.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.theme.appearance = appearance;
    }

    pub fn appearance(&self) -> Appearance {
        self.theme.appearance
    }

    pub(crate) fn change_appearance(&mut self, backwards: bool) {
        let appearance = &mut self.theme.appearance;
        match self.view.appearance_row {
            0 => appearance.color_theme = appearance.color_theme.cycle(backwards),
            1 => appearance.theme_background = !appearance.theme_background,
            _ => appearance.truecolor = !appearance.truecolor,
        }
    }

    /// Render without color; equivalent to `set_color(false)`.
    pub fn set_plain(&mut self) {
        self.set_color(false);
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn contains(&self, word: &str) -> bool {
        self.lexicon.contains(word)
    }

    /// Move the reading position by `lines` relative to the current offset,
    /// clamped to the content. Returns whether the position actually changed.
    pub(crate) fn scroll_by(&mut self, lines: isize) -> bool {
        let before = self.scroll;
        let target = if lines < 0 {
            self.scroll.saturating_sub(lines.unsigned_abs())
        } else {
            self.scroll.saturating_add(lines as usize)
        };
        self.scroll = target.min(self.max_scroll);
        self.scroll != before
    }

    fn search(&mut self) {
        self.completion = None;
        self.pending_completion.clear();
        // Any new lookup branches away from the states reached by going back.
        self.forward.clear();
        self.generation += 1;
        self.selected = 0;
        self.scroll = 0;
        self.max_scroll = 0;
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
        if !self.loading
            && !self.pending_completion.is_empty()
            && !self.view.show_help
            && !self.view.show_appearance
        {
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

    /// Select a candidate row, or accept it when it is already the highlighted
    /// one. Used by mouse clicks, where a second click on the same row follows
    /// the word like Enter. Returns whether anything changed.
    pub(crate) fn click_candidate(&mut self, index: usize) -> bool {
        if index >= self.results.len() {
            return false;
        }
        if index == self.selected {
            self.accept(true);
        } else {
            self.select(index);
        }
        true
    }

    fn select(&mut self, selected: usize) {
        if selected == self.selected || selected >= self.results.len() {
            return;
        }
        self.selected = selected;
        self.scroll = 0;
        self.max_scroll = 0;
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
        if self.history.len() > HISTORY_LIMIT {
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
}
