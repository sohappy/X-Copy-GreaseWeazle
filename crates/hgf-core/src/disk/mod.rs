// SPDX-License-Identifier: GPL-3.0-or-later

//! What is on (or meant for) a disk: track selections ([`trackset`]), the live per-track map
//! ([`diskmap`]), disk formats and presets ([`formats`]), image files and their options
//! ([`image`], [`imageopts`]) and output file naming ([`naming`]).

pub mod diskmap;
pub mod formats;
pub mod image;
pub mod imageopts;
pub mod naming;
pub mod trackset;
