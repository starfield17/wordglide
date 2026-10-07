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
mod index;
mod model;
mod normalize;
mod store;
mod theme;
mod ui;

pub use app::{App, Focus};
pub use build::build_pack;
pub use model::{Candidate, Entry, Example, Group, MatchKind, Preview, Sense};
pub use normalize::normalize;
pub use store::{Dictionary, PackInfo, pack_info, verify_pack};
pub use theme::{Appearance, AppearanceOverrides, ThemePreset};
pub use ui::{RunOptions, draw, run, run_with_options};
