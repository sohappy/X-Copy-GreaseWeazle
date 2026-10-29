// SPDX-License-Identifier: GPL-3.0-or-later

//! Greaseweazle and floppy-disk core, without any GUI dependency.
//!
//! | Module | Role |
//! |---|---|
//! | [`gw`] | `gw` commands and their arguments, running jobs (behind the [`gw::spawn::Spawner`] trait), parsing their output, finding `gw` |
//! | [`disk`] | Track selections, live disk map, formats and presets, image files, file naming |
//! | [`fs`] | Filesystems: blank disks for every machine (the [`fs::BlankDisk`] trait), AmigaDOS explorer and analysis |
//! | [`diag`] | Drive health, plain-language hints, Amiga write compatibility |

pub mod diag;
pub mod disk;
pub mod fs;
pub mod gw;

// The most used types, at the crate root.
pub use disk::diskmap::{DiskMap, MapMode, TrackInfo, TrackState};
pub use disk::trackset::TrackSet;
pub use gw::parse::{Output, OutputParser};
pub use gw::runner::{Job, JobEvent};
pub use gw::{Action, ActionKind, Drive, GwConfig};

/// Crate version, shown in the GUI title bar.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
