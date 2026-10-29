// SPDX-License-Identifier: GPL-3.0-or-later

//! Serial ports where a Greaseweazle may be connected (for the `--device` choice).

/// A serial port; `greaseweazle` when its USB description says so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub description: String,
    pub greaseweazle: bool,
}

/// List the serial ports of this computer, Greaseweazles first.
pub fn list() -> Vec<Port> {
    let mut ports = platform();
    ports.sort_by(|a, b| {
        b.greaseweazle.cmp(&a.greaseweazle).then_with(|| natural(&a.name).cmp(&natural(&b.name)))
    });
    ports.dedup_by(|a, b| a.name == b.name);
    ports
}

/// "COM10" after "COM9".
fn natural(s: &str) -> (String, u32) {
    let digits: String =
        s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    (s[..s.len() - digits.len()].to_string(), digits.parse().unwrap_or(0))
}

#[cfg(target_os = "linux")]
fn platform() -> Vec<Port> {
    let mut out = Vec::new();
    // udev names such as usb-Keir_Fraser_Greaseweazle_GW...-if00 -> ../../ttyACM0
    if let Ok(dir) = std::fs::read_dir("/dev/serial/by-id") {
        for e in dir.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            if let Ok(target) = std::fs::canonicalize(e.path()) {
                out.push(Port {
                    name: target.to_string_lossy().into_owned(),
                    greaseweazle: id.to_lowercase().contains("greaseweazle"),
                    description: id.trim_start_matches("usb-").replace('_', " "),
                });
            }
        }
    }
    if let Ok(dir) = std::fs::read_dir("/dev") {
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("ttyACM") || n.starts_with("ttyUSB") {
                let name = format!("/dev/{n}");
                if !out.iter().any(|p| p.name == name) {
                    out.push(Port { name, description: String::new(), greaseweazle: false });
                }
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn platform() -> Vec<Port> {
    let mut out = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/dev") {
        for e in dir.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("cu.usbmodem") || n.starts_with("cu.usbserial") {
                out.push(Port { name: format!("/dev/{n}"), description: String::new(), greaseweazle: false });
            }
        }
    }
    out
}

#[cfg(windows)]
fn platform() -> Vec<Port> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Values such as "\Device\USBSER000    REG_SZ    COM3".
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\HARDWARE\DEVICEMAP\SERIALCOMM"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(out) = out else { return Vec::new() };
    parse_serialcomm(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform() -> Vec<Port> {
    Vec::new()
}

/// Parse `reg query HKLM\HARDWARE\DEVICEMAP\SERIALCOMM`. USB CDC ports (USBSER) are flagged as
/// possible Greaseweazles: Windows gives no product name there.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_serialcomm(text: &str) -> Vec<Port> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split("REG_SZ");
            let key = parts.next()?.trim();
            let name = parts.next()?.trim();
            (!name.is_empty()).then(|| Port {
                name: name.to_string(),
                greaseweazle: key.to_uppercase().contains("USBSER"),
                description: if key.to_uppercase().contains("USBSER") {
                    "USB serial".into()
                } else {
                    key.trim_start_matches(r"\Device\").into()
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_registry_output() {
        let text = "\r\nHKEY_LOCAL_MACHINE\\HARDWARE\\DEVICEMAP\\SERIALCOMM\r\n    \\Device\\Serial0    REG_SZ    COM1\r\n    \\Device\\USBSER000    REG_SZ    COM10\r\n    \\Device\\USBSER001    REG_SZ    COM3\r\n";
        let p = parse_serialcomm(text);
        assert_eq!(p.len(), 3);
        assert_eq!(p[1].name, "COM10");
        assert!(p[1].greaseweazle && !p[0].greaseweazle);
        assert!(natural("COM10") > natural("COM9"));
    }

    #[test]
    fn listing_does_not_fail() {
        let _ = list();
    }
}
