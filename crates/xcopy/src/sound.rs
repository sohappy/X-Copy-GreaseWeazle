// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Retro Computing Club (RCC)

//! End-of-operation sounds, played with the system's own player: `afplay` on macOS, the .NET
//! SoundPlayer on Windows, `paplay`/`aplay` on Linux. No audio library is linked.
//!
//! * copy done and error: the X-COPY "Boing" samples (`assets/sounds/*.wav`, embedded in the
//!   executable);
//! * abort: a short square-wave jingle synthesised here.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Operation completed.
    Done,
    /// Stopped or cancelled by the user.
    Abort,
    /// Operation failed.
    Error,
}

const RATE: u32 = 22_050;

/// (frequency Hz, milliseconds); 0 Hz = silence.
fn notes(s: Sound) -> &'static [(f32, u32)] {
    match s {
        Sound::Done => &[(523.3, 70), (659.3, 70), (784.0, 70), (1046.5, 180)],
        Sound::Abort => &[(784.0, 90), (0.0, 30), (523.3, 160)],
        Sound::Error => &[(220.0, 140), (0.0, 50), (174.6, 140), (0.0, 50), (130.8, 260)],
    }
}

/// 8-bit mono WAV of the jingle.
/// X-COPY sample played when an operation completes.
const DONE_WAV: &[u8] = include_bytes!("../assets/sounds/done.wav");
/// X-COPY sample played when an operation fails.
const ERROR_WAV: &[u8] = include_bytes!("../assets/sounds/error.wav");

/// WAV file of a sound: the X-COPY samples for done/error, a synthesised jingle otherwise.
pub fn wav(s: Sound) -> Vec<u8> {
    match s {
        Sound::Done => DONE_WAV.to_vec(),
        Sound::Error => ERROR_WAV.to_vec(),
        Sound::Abort => synth(s),
    }
}

/// 8-bit mono WAV of a square-wave jingle.
fn synth(s: Sound) -> Vec<u8> {
    let mut pcm: Vec<u8> = Vec::new();
    for &(f, ms) in notes(s) {
        let n = (RATE * ms / 1000) as usize;
        for i in 0..n {
            let v = if f == 0.0 {
                0.0
            } else {
                let phase = (i as f32 * f / RATE as f32).fract();
                let sq = if phase < 0.5 { 1.0 } else { -1.0 };
                // short attack and release to avoid clicks
                let env = (i as f32 / 200.0).min(1.0) * ((n - i) as f32 / 400.0).min(1.0);
                sq * env * 0.35
            };
            pcm.push((128.0 + v * 127.0) as u8);
        }
    }
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&RATE.to_le_bytes()); // byte rate
    w.extend_from_slice(&1u16.to_le_bytes()); // block align
    w.extend_from_slice(&8u16.to_le_bytes()); // bits
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(&pcm);
    w
}

fn path_for(s: Sound) -> PathBuf {
    std::env::temp_dir().join(format!("xcopy-modern-{}-{:?}.wav", std::process::id(), s).to_lowercase())
}

fn file_for(s: Sound) -> Option<PathBuf> {
    let p = path_for(s);
    if !p.exists() {
        std::fs::write(&p, wav(s)).ok()?;
    }
    Some(p)
}

/// Play in the background; silently does nothing if no player is available.
pub fn play(s: Sound) {
    let Some(path) = file_for(s) else { return };
    std::thread::spawn(move || {
        use std::process::{Command, Stdio};
        let p = path.to_string_lossy().into_owned();
        let tries: Vec<(&str, Vec<String>)> = if cfg!(target_os = "macos") {
            vec![("afplay", vec![p])]
        } else if cfg!(windows) {
            let ps = format!("(New-Object Media.SoundPlayer '{}').PlaySync()", p.replace('\'', "''"));
            vec![(
                "powershell",
                vec!["-NoProfile".into(), "-WindowStyle".into(), "Hidden".into(), "-Command".into(), ps],
            )]
        } else {
            vec![("paplay", vec![p.clone()]), ("aplay", vec!["-q".into(), p.clone()]), ("pw-play", vec![p])]
        };
        for (cmd, args) in tries {
            let mut c = Command::new(cmd);
            c.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
            if let Ok(status) = c.status() {
                if status.success() {
                    return;
                }
            }
        }
    });
}

pub fn cleanup() {
    for s in [Sound::Done, Sound::Abort, Sound::Error] {
        let _ = std::fs::remove_file(path_for(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_consistent() {
        let w = synth(Sound::Abort);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize, w.len() - 8);
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize, w.len() - 44);
    }

    #[test]
    fn embedded_samples_are_pcm_wav() {
        for s in [Sound::Done, Sound::Error] {
            let w = wav(s);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(&w[8..12], b"WAVE");
            assert_eq!(u16::from_le_bytes([w[20], w[21]]), 1, "PCM, playable by every system player");
        }
    }
}
