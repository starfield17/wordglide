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
}

impl Action {
    pub(crate) const ALL: [Self; 21] = [
        Self::Commands,
        Self::Complete,
        Self::NewLookup,
        Self::Focus,
        Self::Accept,
        Self::Prediction,
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
        self.view.overlay = if self.view.overlay == panel {
            Overlay::None
        } else {
            panel
        };
        self.panel_query.clear();
        self.view.panel_row = 0;
        self.view.help_scroll = 0;
    }
    pub(crate) fn action_reason(&self, action: Action) -> Option<&'static str> {
        match action {
            Action::Accept | Action::Complete if self.results.is_empty() => Some("No candidate"),
            Action::Prediction if self.inline_candidate().is_none() => Some("No prediction"),
            Action::Follow | Action::Top | Action::Bottom | Action::Examples | Action::Ipa
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
                KeyCode::Esc | KeyCode::Enter => self.view.overlay = Overlay::None,
                KeyCode::Up => self.view.appearance_row = (self.view.appearance_row + 2) % 3,
                KeyCode::Down => self.view.appearance_row = (self.view.appearance_row + 1) % 3,
                KeyCode::Left => self.change_appearance(true),
                KeyCode::Right | KeyCode::Char(' ') => self.change_appearance(false),
                _ => {}
            },
            Overlay::Help => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') | KeyCode::Char(' ') => {
                    self.view.overlay = Overlay::None
                }
                KeyCode::Up => self.view.help_scroll = self.view.help_scroll.saturating_sub(1),
                KeyCode::Down => self.view.help_scroll = self.view.help_scroll.saturating_add(1),
                KeyCode::PageUp => {
                    self.view.help_scroll = self.view.help_scroll.saturating_sub(self.page)
                }
                KeyCode::PageDown => {
                    self.view.help_scroll = self.view.help_scroll.saturating_add(self.page)
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
                    self.panel_query.pop();
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
                        self.panel_query.pop();
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
                    if let Some((scroll, anchor, find, index)) = self.reading.find_original.take() {
                        self.scroll = anchor
                            .as_ref()
                            .and_then(|a| self.reading.row_for_anchor(a))
                            .unwrap_or(scroll)
                            .min(self.max_scroll);
                        self.reading.find = find;
                        self.reading.match_index = index;
                        self.reading.update_matches();
                    }
                    self.view.overlay = Overlay::None;
                }
                KeyCode::Enter => {
                    self.reading.find_original = None;
                    self.view.overlay = Overlay::None;
                }
                KeyCode::Backspace => {
                    self.panel_query.pop();
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
            Overlay::None => {}
        }
    }
}
