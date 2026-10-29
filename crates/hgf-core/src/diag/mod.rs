// SPDX-License-Identifier: GPL-3.0-or-later

//! Diagnostics: drive health verdicts ([`health`]), plain-language explanations of failures
//! ([`hints`]) and Amiga write compatibility checks ([`compat`]).

pub mod compat;
pub mod health;
pub mod hints;
