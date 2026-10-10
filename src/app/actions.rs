//! A fixed set of local actions shared by menus and clickable shortcuts.
use super::*;
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Find,
    Outline,
    Layout,
    Navigation,
    Commands,
    Complete,
    NewLookup,
    Focus,
    Accept,
    Prediction,
    Follow,
    Top,
    Bottom,
    Examples,
    Ipa,
    Back,
    Forward,
    Settings,
    Help,
    Mouse,
    Quit,
    Peek,
}

impl Action {
    pub(crate) const ALL: [Self; 22] = [
        Self::Commands,
        Self::Complete,
        Self::NewLookup,
        Self::Focus,
        Self::Accept,
        Self::Prediction,
        Self::Peek,
        Self::Find,
        Self::Outline,
        Self::Layout,
        Self::Navigation,
        Self::Follow,
        Self::Top,
        Self::Bottom,
        Self::Examples,
        Self::Ipa,
        Self::Back,
        Self::Forward,
        Self::Settings,
        Self::Help,
        Self::Mouse,
        Self::Quit,
    ];
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Find => "Find text in this definition",
            Self::Outline => "Definition outline",
            Self::Layout => "Reading layout: split / focus",
            Self::Navigation => "Session navigation",
            Self::Commands => "Search actions",
            Self::Complete => "Complete or cycle candidates",
            Self::NewLookup => "New lookup",
            Self::Focus => "Switch input / reading focus",
            Self::Accept => "Accept candidate and read",
            Self::Prediction => "Accept prediction",
            Self::Peek => "Quick peek candidate card",
            Self::Follow => "Follow a visible word",
            Self::Top => "Top of definition",
            Self::Bottom => "Bottom of definition",
            Self::Examples => "Expand examples and references",
            Self::Ipa => "Expand pronunciation (IPA)",
            Self::Back => "Navigate back",
            Self::Forward => "Navigate forward",
            Self::Settings => "Appearance settings",
            Self::Help => "Keyboard help",
            Self::Mouse => "Mouse capture / terminal text selection",
            Self::Quit => "Quit Wordglide",
        }
    }
    pub(crate) fn shortcut(self) -> &'static str {
        match self {
            Self::Find => "/",
            Self::Outline => "o",
            Self::Layout => "F4",
            Self::Navigation => "Ctrl+R",
            Self::Commands => "Ctrl+G",
            Self::Complete => "Tab",
            Self::NewLookup => "Ctrl+U",
            Self::Focus => "Ctrl+L",
            Self::Accept => "Enter",
            Self::Prediction => "Ctrl+F",
            Self::Peek => "Space",
            Self::Follow => "f",
            Self::Top => "Home",
            Self::Bottom => "End",
            Self::Examples => "e",
            Self::Ipa => "p",
            Self::Back => "Ctrl+Z",
            Self::Forward => "Ctrl+Y",
            Self::Settings => "F2",
            Self::Help => "F1",
            Self::Mouse => "",
            Self::Quit => "Ctrl+C",
        }
    }
}

impl App {
    pub(crate) fn open_panel(&mut self, panel: Overlay) {
        if self.view.overlay == Overlay::Find {
            self.cancel_find();
        }

        self.view.overlay = if self.view.overlay == panel {
            Overlay::None
        } else {
            panel
        };
        self.panel_query.clear();
        self.view.panel_row = if panel == Overlay::Outline {
            self.reading.current_section(self.scroll).unwrap_or(0)
        } else {
            0
        };
        self.view.help_scroll = 0;
    }
    pub(crate) fn action_reason(&self, action: Action) -> Option<&'static str> {
        match action {
            Action::Accept | Action::Complete | Action::Peek if self.results.is_empty() => {
                Some("No candidate")
            }
            Action::Prediction if self.inline_candidate().is_none() => Some("No prediction"),
            Action::Follow
            | Action::Find
            | Action::Outline
            | Action::Layout
            | Action::Top
            | Action::Bottom
            | Action::Examples
            | Action::Ipa
                if self.preview.is_none() || self.loading =>
            {
                Some("No ready definition")
            }
            Action::Navigation if self.history.is_empty() && self.forward.is_empty() => {
                Some("No navigation locations")
            }
            Action::Back if self.history.is_empty() => Some("No previous location"),
            Action::Forward if self.forward.is_empty() => Some("No next location"),
            _ => None,
        }
    }
    pub(crate) fn action_state(&self, action: Action) -> &'static str {
        match action {
            Action::Layout => {
                if self.view.reading_layout == ReadingLayout::Focus {
                    "focus"
                } else {
                    "split"
                }
            }
            Action::Examples => {
                if self.view.expand_examples {
                    "on"
                } else {
                    "off"
                }
            }
            Action::Ipa => {
                if self.view.expand_ipa {
                    "on"
                } else {
                    "off"
                }
            }
            Action::Mouse => {
                if self.mouse_enabled {
                    "on"
                } else {
                    "off"
                }
            }
            _ => "",
        }
    }
    pub(crate) fn commands(&self) -> Vec<Action> {
        let query = self.panel_query.to_lowercase();
        Action::ALL
            .into_iter()
            .filter(|a| {
                let text = format!("{} {}", a.title(), a.shortcut()).to_lowercase();
                query.split_whitespace().all(|word| text.contains(word))
            })
            .collect()
    }
    pub(crate) fn execute_action(&mut self, action: Action) {
        if self.action_reason(action).is_some() {
            return;
        }
        self.view.overlay = Overlay::None;
        match action {
            Action::Find => self.find_start(),
            Action::Outline => {
                self.open_panel(Overlay::Outline);
                self.focus = Focus::Definition;
            }
            Action::Navigation => self.open_panel(Overlay::History),
            Action::Layout => {
                self.view.reading_layout = if self.view.reading_layout == ReadingLayout::Split {
                    ReadingLayout::Focus
                } else {
                    ReadingLayout::Split
                };
                self.focus = Focus::Definition;
            }
            Action::Commands => self.open_panel(Overlay::Commands),
            Action::Complete => {
                self.focus = Focus::Input;
                self.cycle_completion(false);
            }
            Action::NewLookup => {
                self.focus = Focus::Input;
                self.input.clear();
                self.cursor = 0;
                self.search();
            }
            Action::Focus => {
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
            Action::Accept => self.accept(true),
            Action::Prediction => self.accept_inline(),
            Action::Peek => self.open_peek(),
            Action::Follow => {
                self.focus = Focus::Definition;
                self.picking = true;
                self.label_input.clear();
            }
            Action::Top => self.scroll = 0,
            Action::Bottom => self.scroll = self.max_scroll,
            Action::Examples => self.view.expand_examples = !self.view.expand_examples,
            Action::Ipa => self.view.expand_ipa = !self.view.expand_ipa,
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Settings => self.open_panel(Overlay::Appearance),
            Action::Help => self.open_panel(Overlay::Help),
            Action::Mouse => self.mouse_enabled = !self.mouse_enabled,
            Action::Quit => self.exit = true,
        }
    }
    pub(crate) fn open_peek(&mut self) {
        if self.results.is_empty() {
            return;
        }
        if self.view.overlay == Overlay::Peek {
            self.close_peek();
            return;
        }
        if self.view.overlay == Overlay::Find {
            self.cancel_find();
        }
        self.panel_query.clear();
        self.view.overlay = Overlay::Peek;
        self.peek_index = self.selected;
        self.fetch_peek(self.selected);
    }
    pub(crate) fn close_peek(&mut self) {
        if self.view.overlay == Overlay::Peek {
            self.view.overlay = Overlay::None;
            self.peek_preview = None;
        }
    }
    pub(crate) fn accept_peek(&mut self, index: usize) {
        if index >= self.results.len() {
            self.close_peek();
            return;
        }
        let peek_preview = self.peek_preview.take();
        self.close_peek();
        if index == self.selected {
            self.accept(true);
            self.focus = Focus::Definition;
        } else {
            self.selected = index;
            self.reading.clear_find();
            self.reading.restore_anchor = None;
            self.scroll = 0;
            self.max_scroll = 0;
            self.error = None;
            self.picking = false;
            self.focus = Focus::Definition;
            if let Some(preview) = peek_preview
                && preview.entry.key == self.results[index].key
            {
                self.preview = Some(preview);
                self.loading = false;
            } else {
                self.loading = true;
                if self
                    .send_request(Request::Preview(
                        self.generation,
                        self.results[index].clone(),
                    ))
                    .is_err()
                {
                    self.worker_error();
                }
            }
        }
    }
    pub(crate) fn set_peek_index(&mut self, index: usize) {
        if index >= self.results.len() {
            return;
        }
        self.peek_index = index;
        self.fetch_peek(index);
    }
    fn fetch_peek(&mut self, index: usize) {
        if let Some(candidate) = self.results.get(index) {
            if self
                .preview
                .as_ref()
                .is_some_and(|p| p.entry.key == candidate.key)
            {
                self.peek_preview = self.preview.clone();
            } else {
                self.peek_preview = None;
                let _ = self.send_request(Request::Peek(self.generation, index, candidate.clone()));
            }
        }
    }
    pub(crate) fn panel_paste(&mut self, text: &str) {
        self.panel_query.extend(
            text.chars()
                .filter(|c| !c.is_control())
                .take(256usize.saturating_sub(self.panel_query.chars().count())),
        );
        self.view.panel_row = 0;
        if self.view.overlay == Overlay::Find {
            self.find_update();
        }
    }
    pub(crate) fn accept_panel_row(&mut self) {
        if self.view.overlay == Overlay::Outline {
            if let Some(row) = self.reading.section_row(self.view.panel_row) {
                self.scroll = row.min(self.max_scroll);
            }
        } else if let Some((distance, _)) = self.navigation_locations().get(self.view.panel_row) {
            self.navigate_distance(*distance);
        }
        self.view.overlay = Overlay::None;
    }
    pub(crate) fn panel_key(&mut self, key: KeyEvent) {
        match self.view.overlay {
            Overlay::Appearance => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') if self.view.appearance_row == 6 => {
                    self.start_download()
                }
                KeyCode::Esc | KeyCode::Enter => self.view.overlay = Overlay::None,
                KeyCode::Up => self.view.appearance_row = (self.view.appearance_row + 6) % 7,
                KeyCode::Down => self.view.appearance_row = (self.view.appearance_row + 1) % 7,
                KeyCode::Left => self.change_appearance(true),
                KeyCode::Right | KeyCode::Char(' ') => self.change_appearance(false),
                _ => {}
            },
            Overlay::Download => match key.code {
                KeyCode::Esc if self.download.state.is_cancellable() => {
                    self.download.state = DownloadState::Cancelling;
                    self.download.message = "Cancelling download…".into();
                }
                KeyCode::Esc | KeyCode::Enter if !self.download.state.is_active() => {
                    self.view.overlay = Overlay::Appearance;
                }
                _ => {}
            },
            Overlay::Help => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char(' ') => {
                    self.view.overlay = Overlay::None
                }
                KeyCode::Up => self.view.help_scroll = self.view.help_scroll.saturating_sub(1),
                KeyCode::Down => self.view.help_scroll = self.view.help_scroll.saturating_add(1),
                KeyCode::PageUp => {
                    self.view.help_scroll = self
                        .view
                        .help_scroll
                        .saturating_sub(self.view.help_page.max(1))
                }
                KeyCode::PageDown => {
                    self.view.help_scroll = self
                        .view
                        .help_scroll
                        .saturating_add(self.view.help_page.max(1))
                }
                KeyCode::Home => self.view.help_scroll = 0,
                KeyCode::End => self.view.help_scroll = usize::MAX,
                _ => {}
            },
            Overlay::Commands => match key.code {
                KeyCode::Esc => self.view.overlay = Overlay::None,
                KeyCode::Up => self.view.panel_row = self.view.panel_row.saturating_sub(1),
                KeyCode::Down => {
                    self.view.panel_row =
                        (self.view.panel_row + 1).min(self.commands().len().saturating_sub(1))
                }
                KeyCode::Home => self.view.panel_row = 0,
                KeyCode::End => self.view.panel_row = self.commands().len().saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(&action) = self.commands().get(self.view.panel_row) {
                        self.execute_action(action);
                    }
                }
                KeyCode::Backspace => {
                    {
                        use unicode_segmentation::UnicodeSegmentation;
                        let start = self
                            .panel_query
                            .grapheme_indices(true)
                            .next_back()
                            .map_or(0, |(byte, _)| byte);
                        self.panel_query.truncate(start);
                    }
                    self.view.panel_row = 0;
                }
                KeyCode::Char(c)
                    if key.modifiers.is_empty()
                        || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
                {
                    self.panel_paste(&c.to_string())
                }
                _ => {}
            },
            Overlay::Outline | Overlay::History => {
                let count = if self.view.overlay == Overlay::Outline {
                    self.reading.sections.len()
                } else {
                    self.navigation_locations().len()
                };
                match key.code {
                    KeyCode::Esc => self.view.overlay = Overlay::None,
                    KeyCode::Up => self.view.panel_row = self.view.panel_row.saturating_sub(1),
                    KeyCode::Down => {
                        self.view.panel_row = (self.view.panel_row + 1).min(count.saturating_sub(1))
                    }
                    KeyCode::PageUp => self.view.panel_row = self.view.panel_row.saturating_sub(10),
                    KeyCode::PageDown => {
                        self.view.panel_row =
                            (self.view.panel_row + 10).min(count.saturating_sub(1))
                    }
                    KeyCode::Home => self.view.panel_row = 0,
                    KeyCode::End => self.view.panel_row = count.saturating_sub(1),
                    KeyCode::Enter => self.accept_panel_row(),
                    KeyCode::Backspace if self.view.overlay == Overlay::History => {
                        {
                            use unicode_segmentation::UnicodeSegmentation;
                            let start = self
                                .panel_query
                                .grapheme_indices(true)
                                .next_back()
                                .map_or(0, |(byte, _)| byte);
                            self.panel_query.truncate(start);
                        }
                        self.view.panel_row = 0;
                    }
                    KeyCode::Char(c)
                        if self.view.overlay == Overlay::History
                            && (key.modifiers.is_empty()
                                || key.modifiers == crossterm::event::KeyModifiers::SHIFT) =>
                    {
                        self.panel_paste(&c.to_string())
                    }
                    _ => {}
                }
            }
            Overlay::Find => match key.code {
                KeyCode::Esc => {
                    self.cancel_find();
                    self.view.overlay = Overlay::None;
                }
                KeyCode::Enter => {
                    self.reading.find_original = None;
                    self.view.overlay = Overlay::None;
                }
                KeyCode::Backspace => {
                    {
                        use unicode_segmentation::UnicodeSegmentation;
                        let start = self
                            .panel_query
                            .grapheme_indices(true)
                            .next_back()
                            .map_or(0, |(byte, _)| byte);
                        self.panel_query.truncate(start);
                    }
                    self.find_update();
                }
                KeyCode::Down => self.next_match(false),
                KeyCode::Up => self.next_match(true),
                KeyCode::Char(c)
                    if key.modifiers.is_empty()
                        || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
                {
                    self.panel_paste(&c.to_string())
                }
                _ => {}
            },
            Overlay::Peek => {
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                {
                    match key.code {
                        KeyCode::Char('p') => {
                            if self.peek_index > 0 {
                                self.set_peek_index(self.peek_index - 1);
                            }
                        }
                        KeyCode::Char('n') => {
                            if self.peek_index + 1 < self.results.len() {
                                self.set_peek_index(self.peek_index + 1);
                            }
                        }
                        _ => self.close_peek(),
                    }
                    return;
                }
                match key.code {
                    KeyCode::Esc | KeyCode::Char(' ') => self.close_peek(),
                    KeyCode::Enter => self.accept_peek(self.peek_index),
                    KeyCode::Up | KeyCode::BackTab => {
                        if self.peek_index > 0 {
                            self.set_peek_index(self.peek_index - 1);
                        }
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        if self.peek_index + 1 < self.results.len() {
                            self.set_peek_index(self.peek_index + 1);
                        }
                    }
                    KeyCode::Home => self.set_peek_index(0),
                    KeyCode::End => self.set_peek_index(self.results.len().saturating_sub(1)),
                    _ => self.close_peek(),
                }
            }
            Overlay::None => {}
        }
    }
}
