//! Offline lookup engine and data pack builder.
//!
//! F1 ← S2 / F4 ← S2: private modules keep pack writing and configuration out of
//! the public API; the compile-fail doc tests below are part of the boundary
//! check named in `SPEC.md`.
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
mod entry_storage;
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
