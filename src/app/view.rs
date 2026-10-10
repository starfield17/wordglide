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
    Download,
    Peek,
}

/// Lifecycle state of the dictionary downloader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DownloadState {
    #[default]
    Disabled,
    Idle,
    Requested,
    Running,
    Cancelling,
    Finished,
}

impl DownloadState {
    /// True while a download task is requested, running, or completing cancellation.
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Requested | Self::Running | Self::Cancelling)
    }

    /// True if the user can request cancellation (task is requested or running).
    pub(crate) fn is_cancellable(self) -> bool {
        matches!(self, Self::Requested | Self::Running)
    }

    /// True if download capability is available in this session.
    pub(crate) fn is_enabled(self) -> bool {
        self != Self::Disabled
    }
}

#[derive(Debug, Default)]
pub(crate) struct DownloadView {
    pub(crate) state: DownloadState,
    pub(crate) message: String,
    pub(crate) downloaded: u64,
    pub(crate) total: u64,
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
