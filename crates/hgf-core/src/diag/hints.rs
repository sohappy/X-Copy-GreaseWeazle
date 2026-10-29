// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-language explanations of gw failures and suspicious results.
//!
//! The matched texts are those of the Greaseweazle host tools (`usb.py` `Ack.str`, `tools/util.py`,
//! `tools/write.py`) and of the errors seen on real hardware.

use crate::diag::health::ScanStats;
use crate::gw::ActionKind;

/// A button offered next to the hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintAction {
    OpenHealth,
    OpenBandwidth,
    OpenUpdate,
    OpenSettings,
    /// Re-read only the tracks that failed.
    RereadBad,
}

impl HintAction {
    pub fn label(self) -> &'static str {
        match self {
            HintAction::OpenHealth => "Run the drive health check",
            HintAction::OpenBandwidth => "Test the USB bandwidth",
            HintAction::OpenUpdate => "Open firmware update",
            HintAction::OpenSettings => "Open Settings",
            HintAction::RereadBad => "Re-read the failed tracks",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub title: String,
    pub steps: Vec<String>,
    pub action: Option<HintAction>,
}

fn hint(title: &str, steps: &[&str], action: Option<HintAction>) -> Hint {
    Hint { title: title.into(), steps: steps.iter().map(|s| s.to_string()).collect(), action }
}

/// Explanation for a failed operation, from gw's error message.
pub fn for_failure(kind: ActionKind, message: &str) -> Option<Hint> {
    let m = message.to_lowercase();
    let writes = matches!(kind, ActionKind::Write | ActionKind::Erase);
    let h = if m.contains("no index") {
        hint(
            "The drive does not see the disk turning (no index pulse).",
            &[
                "Insert the disk until it clicks, metal shutter first.",
                "Check that the drive's 4-pin power connector is plugged in.",
                "Twisted PC cable: Drive A is the connector after the twist, Drive B the middle one.",
                "A drive LED that stays lit all the time means the cable is upside down.",
            ],
            Some(HintAction::OpenHealth),
        )
    } else if m.contains("track 0 not found") {
        hint(
            "The head could not find track 0.",
            &[
                "The drive is probably not selected or not powered: check the drive choice (A/B or Unit 0-3).",
                "Check the cable orientation (red wire on pin 1 at both ends).",
            ],
            None,
        )
    } else if m.contains("write protected") {
        hint(
            "The disk is write-protected.",
            &[
                "3.5\": slide the tab to close the square hole.",
                "5.25\": cover the side notch with an opaque sticker.",
            ],
            None,
        )
    } else if m.contains("failed to verify") {
        hint(
            "What was written could not be read back correctly.",
            &[
                "Try another disk: worn media is the most common cause.",
                "Use DD media for DD formats (Amiga 880K, PC 720K); on an HD disk, tape over the density hole.",
                "Clean the drive heads, then try again.",
                "If several disks fail, check the drive with the health check.",
            ],
            Some(HintAction::OpenHealth),
        )
    } else if m.contains("flux overflow") || m.contains("flux underflow") || m.contains("garbled") {
        hint(
            "The USB link could not keep up with the flux stream.",
            &[
                "Plug the Greaseweazle directly into the computer, not through a hub.",
                "Use a short, good-quality USB cable.",
                "Close programs that use USB heavily (backups, video capture).",
            ],
            Some(HintAction::OpenBandwidth),
        )
    } else if m.contains("no drive unit selected") || m.contains("no bus type") || m.contains("invalid unit")
    {
        hint(
            "The drive selection does not match the cable.",
            &[
                "PC twisted cable: choose Drive A or Drive B.",
                "Straight (Shugart) cable or Amiga drive: choose Unit 0 or Unit 1.",
            ],
            None,
        )
    } else if m.contains("firmware update mode") {
        hint(
            "The Greaseweazle is in firmware update mode.",
            &[
                "If you did not mean to update: unplug it, remove the update jumper, plug it back in.",
                "Otherwise run the firmware update.",
            ],
            Some(HintAction::OpenUpdate),
        )
    } else if m.contains("is unsupported") || m.contains("update the firmware") {
        hint(
            "The device firmware is too old for these host tools.",
            &["Update the firmware, then try again."],
            Some(HintAction::OpenUpdate),
        )
    } else if m.contains("is not executable") {
        hint(
            "The program set as gw cannot be run.",
            &[
                "Check the gw path in Settings, or use Auto-detect.",
                "Linux/macOS: make it executable (chmod +x).",
            ],
            Some(HintAction::OpenSettings),
        )
    } else if m.contains("permission denied") {
        hint(
            "The Greaseweazle's serial port cannot be opened.",
            &[
                "Linux: add yourself to the dialout group (sudo usermod -aG dialout $USER), then log out and back in.",
                "Close any other program using the device.",
            ],
            None,
        )
    } else if m.contains("cannot find the greaseweazle") || m.contains("no greaseweazle device found") {
        hint(
            "No Greaseweazle is connected.",
            &[
                "Check the USB cable and try another port.",
                "With several devices connected, set the port (--device) in Settings.",
            ],
            Some(HintAction::OpenSettings),
        )
    } else if m.contains("host tools") && m.contains("not found") {
        hint(
            "The Greaseweazle host tools (gw) are not installed or not found.",
            &[
                "Windows: unzip the official release next to HappyGreaseFloppy.",
                "Linux/macOS: pip install greaseweazle.",
                "Then use Settings → Auto-detect.",
            ],
            Some(HintAction::OpenSettings),
        )
    } else if writes && m.contains("verify") {
        hint(
            "Verification failed.",
            &["Try another disk, then clean the heads."],
            Some(HintAction::OpenHealth),
        )
    } else {
        return None;
    };
    Some(h)
}

/// Explanation for a completed read with problems.
pub fn for_read(stats: &ScanStats, bad_tracks: usize, format_set: bool) -> Option<Hint> {
    if !format_set || stats.total == 0 {
        return None;
    }
    if stats.ratio() < 0.10 {
        return Some(hint(
            "Almost nothing could be decoded in this format.",
            &[
                "The disk may be blank, erased, or only quick-formatted.",
                "Check that the format matches the disk (e.g. Amiga vs PC: an Amiga can also write PC disks).",
                "If the disk should contain data, check the drive with the health check.",
            ],
            Some(HintAction::OpenHealth),
        ));
    }
    if bad_tracks > 0 {
        let title = if bad_tracks == 1 {
            "1 track could not be read completely.".to_string()
        } else {
            format!("{bad_tracks} tracks could not be read completely.")
        };
        return Some(Hint {
            title,
            steps: vec![
                "Re-read them with more retries: a weak sector often comes back on a later pass.".into(),
                "If they stay red, that part of the surface is damaged; clean the heads and try another drive.".into(),
            ],
            action: Some(HintAction::RereadBad),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title(k: ActionKind, m: &str) -> String {
        for_failure(k, m).map(|h| h.title).unwrap_or_default()
    }

    #[test]
    fn real_gw_messages() {
        assert!(title(ActionKind::Erase, "GetFluxStatus: No Index").contains("index"));
        assert!(title(ActionKind::Write, "Failed to verify Track 0.1").contains("read back"));
        assert!(title(ActionKind::Write, "Disk is Write Protected").contains("write-protected"));
        assert!(title(ActionKind::Read, "Track 0 not found").contains("track 0"));
        assert!(title(ActionKind::Read, "Flux Overflow").contains("USB"));
        assert!(title(ActionKind::Read, "USB read data garbled (Device -> Host)").contains("USB"));
        assert!(title(ActionKind::Info, "Device is in Firmware Update Mode").contains("update mode"));
        assert!(title(ActionKind::Read, "Device firmware version 0.27 is unsupported").contains("too old"));
        assert!(title(ActionKind::Read, "No drive unit selected").contains("selection"));
        assert!(title(ActionKind::Info, "No Greaseweazle device found. Check the USB cable.")
            .contains("No Greaseweazle"));
        assert!(title(ActionKind::Read, "could not open port /dev/ttyACM0: [Errno 13] Permission denied")
            .contains("serial"));
        assert!(title(ActionKind::Info, "'/opt/gw' is not executable (permission denied).")
            .contains("cannot be run"));
        assert_eq!(for_failure(ActionKind::Read, "something else"), None);
        assert_eq!(for_failure(ActionKind::Write, "No Index").unwrap().action, Some(HintAction::OpenHealth));
    }

    #[test]
    fn read_results() {
        let erased = ScanStats { good: 18, total: 1760, ..Default::default() };
        assert!(for_read(&erased, 160, true).unwrap().title.contains("Almost nothing"));
        let worn = ScanStats { good: 1745, total: 1760, ..Default::default() };
        let h = for_read(&worn, 13, true).unwrap();
        assert!(h.title.starts_with("13 tracks"));
        assert_eq!(h.action, Some(HintAction::RereadBad));
        let good = ScanStats { good: 1760, total: 1760, ..Default::default() };
        assert_eq!(for_read(&good, 0, true), None);
        assert_eq!(for_read(&erased, 160, false), None);
    }
}
