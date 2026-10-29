// SPDX-License-Identifier: GPL-3.0-or-later

//! Talking to the Greaseweazle host tools (`gw`): the command model and its arguments
//! ([`Action`] and its options), starting and stopping jobs ([`runner`], [`spawn`]),
//! understanding their output ([`parse`]), finding `gw` ([`locate`]) and the serial ports
//! ([`ports`]).

mod action;
pub mod locate;
pub mod parse;
pub mod ports;
pub mod runner;
pub mod spawn;

pub use action::*;
