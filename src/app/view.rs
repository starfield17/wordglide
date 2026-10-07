/// Display toggles that do not affect lookup or ranking.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ViewOptions {
    pub(crate) expand_ipa: bool,
    pub(crate) expand_examples: bool,
    pub(crate) overlay: Overlay,
    pub(crate) appearance_row: usize,
    pub(crate) panel_row: usize,
    pub(crate) help_scroll: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Overlay {
    #[default]
    None,
    Help,
    Appearance,
    Commands,
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
