/// Display toggles that do not affect lookup or ranking.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ViewOptions {
    pub(crate) reading_layout: ReadingLayout,
    pub(crate) expand_ipa: bool,
    pub(crate) expand_examples: bool,
    pub(crate) overlay: Overlay,
    pub(crate) appearance_row: usize,
    pub(crate) panel_row: usize,
    pub(crate) help_scroll: usize,
    pub(crate) help_page: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Overlay {
    #[default]
    None,
    Help,
    Appearance,
    Commands,
    Outline,
    History,
    Find,
}

impl ViewOptions {
    pub(crate) fn show_help(self) -> bool {
        self.overlay == Overlay::Help
    }
    pub(crate) fn show_appearance(self) -> bool {
        self.overlay == Overlay::Appearance
    }
    pub(crate) fn modal(self) -> bool {
        self.overlay != Overlay::None
    }
}

/// Layout used while the definition has focus. Input always shows candidates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingLayout {
    #[default]
    Split,
    Focus,
}

/// Display preferences only; never contain query text or navigation history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReadingPreferences {
    pub reading_layout: ReadingLayout,
    pub expand_examples: bool,
    pub expand_ipa: bool,
}
