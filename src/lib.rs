//! Offline lookup engine and data pack builder.
//!
//! Internal indexes are deliberately not a public API:
//! ```compile_fail
//! use local_english_dict::index::Index;
//! ```
#![forbid(unsafe_code)]

mod app;
mod build;
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
pub use store::{Dictionary, verify_pack};
pub use ui::{draw, run};
