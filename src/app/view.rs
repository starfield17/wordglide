/// Display toggles that do not affect lookup or ranking.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ViewOptions {
    pub(crate) expand_ipa: bool,
    pub(crate) expand_examples: bool,
    pub(crate) show_help: bool,
}
