//! Offline lookup engine and data pack builder.
//!
//! Internal indexes are deliberately not a public API:
//! ```compile_fail
//! use wordglide::index::Index;
//! ```
//! Appearance file I/O is also private:
//! ```compile_fail
//! use wordglide::config::ConfigStore;
//! ```
#![forbid(unsafe_code)]

mod app;
mod build;
mod config;
mod download;
mod entry_codec;
mod index;
mod model;
mod normalize;
mod store;
mod theme;
mod ui;

pub use app::{App, Focus, ReadingLayout, ReadingPreferences};
pub use build::build_pack;
pub use download::downloaded_data_path;
pub use model::{Candidate, Entry, Example, Group, MatchKind, Preview, Sense};
pub use normalize::normalize;
pub use store::{Dictionary, PackInfo, pack_info, verify_pack};
pub use theme::{Appearance, AppearanceOverrides, ThemePreset};
pub use ui::{
    RunOptions, download_data, download_data_with_cancel, draw, run, run_with_options,
    run_without_dictionary,
};
