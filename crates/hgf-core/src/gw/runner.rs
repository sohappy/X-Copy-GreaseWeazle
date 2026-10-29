// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs `gw` in the background, streams its output and supports cancellation.
//!
//! * Output is split on `\n` **and** `\r`, and unterminated text (e.g. `gw clean`, which
//!   prints progress without newlines) is delivered as [`JobEvent::Partial`].
//! * stdin is closed so gw can never block on a prompt.
//! * Cancelling sends SIGINT on Unix so gw deselects the drive and stops the motor cleanly;
//!   the process is killed if it has not exited 3 seconds later (immediately on Windows,
//!   where the device watchdog then releases the drive).

use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const KILL_GRACE: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobEvent {
    /// A complete output line.
    Line(String),
    /// Text printed without a line terminator yet (replaces the previous partial text).
    Partial(String),
    /// The process has exited. `code` is `None` if it was killed by a signal.
    Finished { code: Option<i32>, cancelled: bool },
}

struct Shared {
    child: Mutex<Child>,
    cancel_at: Mutex<Option<Instant>>,
    finished: AtomicBool,
}

/// A running `gw` process.
pub struct Job {
    rx: Receiver<JobEvent>,
    shared: Arc<Shared>,
}

impl Job {
    /// Next pending event, if any (non-blocking).
    pub fn try_next(&self) -> Option<JobEvent> {
        self.rx.try_recv().ok()
    }

    /// Blocking variant with a timeout (used by tests and on shutdown).
    pub fn next_timeout(&self, t: Duration) -> Option<JobEvent> {
        self.rx.recv_timeout(t).ok()
    }

    pub fn is_finished(&self) -> bool {
        self.shared.finished.load(Ordering::SeqCst)
    }

    pub fn is_cancelling(&self) -> bool {
        self.shared.cancel_at.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Ask gw to stop (idempotent).
    pub fn cancel(&self) {
        if self.is_finished() {
            return;
        }
        let Ok(mut at) = self.shared.cancel_at.lock() else { return };
        if at.is_some() {
            return;
        }
        *at = Some(Instant::now());
        drop(at);
        if let Ok(mut child) = self.shared.child.lock() {
            interrupt(&mut child);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// gw runs in its own process group so that the whole group can be signalled.
#[cfg(unix)]
fn signal_group(child: &mut Child, sig: libc::c_int) {
    // SAFETY: kill(2) on the process group we created; failure (already exited) is harmless.
    let rc = unsafe { libc::kill(-(child.id() as libc::pid_t), sig) };
    if rc != 0 {
        let _ = child.kill();
    }
}

#[cfg(unix)]
fn interrupt(child: &mut Child) {
    signal_group(child, libc::SIGINT);
}

#[cfg(unix)]
fn force_kill(child: &mut Child) {
    signal_group(child, libc::SIGKILL);
}

#[cfg(not(unix))]
fn interrupt(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(not(unix))]
fn force_kill(child: &mut Child) {
    let _ = child.kill();
}

/// Explain a spawn failure in user terms.
fn spawn_error(program: &str, e: std::io::Error) -> String {
    match e.kind() {
        ErrorKind::NotFound => format!(
            "Greaseweazle host tools not found ('{program}'). Install them or set the path to \
             gw in Settings."
        ),
        ErrorKind::PermissionDenied => format!("'{program}' is not executable (permission denied)."),
        _ => format!("Cannot start '{program}': {e}"),
    }
}

/// Start `program args…`.
pub fn spawn(program: &str, args: &[OsString]) -> Result<Job, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| spawn_error(program, e))?;
    let (tx, rx) = mpsc::channel();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let t = tx.clone();
        readers.push(thread::spawn(move || pump(out, t)));
    }
    if let Some(err) = child.stderr.take() {
        let t = tx.clone();
        readers.push(thread::spawn(move || pump(err, t)));
    }
    let shared = Arc::new(Shared {
        child: Mutex::new(child),
        cancel_at: Mutex::new(None),
        finished: AtomicBool::new(false),
    });
    let s = Arc::clone(&shared);
    thread::spawn(move || {
        let code = loop {
            let status = match s.child.lock() {
                Ok(mut c) => c.try_wait(),
                Err(_) => break None,
            };
            match status {
                Ok(Some(st)) => break st.code(),
                Ok(None) => {}
                Err(_) => break None,
            }
            let overdue = s.cancel_at.lock().ok().and_then(|g| *g).is_some_and(|t| t.elapsed() > KILL_GRACE);
            if overdue {
                if let Ok(mut c) = s.child.lock() {
                    force_kill(&mut c);
                }
            }
            thread::sleep(Duration::from_millis(30));
        };
        for r in readers {
            let _ = r.join();
        }
        let cancelled = s.cancel_at.lock().map(|g| g.is_some()).unwrap_or(false);
        s.finished.store(true, Ordering::SeqCst);
        let _ = tx.send(JobEvent::Finished { code, cancelled });
    });
    Ok(Job { rx, shared })
}

/// Run a short command to completion and return its combined output (e.g. `gw read --help`).
pub fn capture(program: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let job = spawn(program, &args)?;
    let deadline = Instant::now() + timeout;
    let mut text = String::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            job.cancel();
            return Err("timed out".into());
        }
        match job.next_timeout(left) {
            Some(JobEvent::Line(l)) => {
                text.push_str(&l);
                text.push('\n');
            }
            Some(JobEvent::Partial(_)) => {}
            Some(JobEvent::Finished { .. }) => return Ok(text),
            None => {}
        }
    }
}

fn pump<R: Read>(mut r: R, tx: Sender<JobEvent>) {
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    let mut last_was_cr = false;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        for &b in &buf[..n] {
            match b {
                b'\n' if last_was_cr => {}
                b'\n' | b'\r' => {
                    let line = String::from_utf8_lossy(&pending).into_owned();
                    pending.clear();
                    if tx.send(JobEvent::Line(line)).is_err() {
                        return;
                    }
                }
                _ => pending.push(b),
            }
            last_was_cr = b == b'\r';
        }
        if !pending.is_empty() {
            let _ = tx.send(JobEvent::Partial(String::from_utf8_lossy(&pending).into_owned()));
        }
    }
    if !pending.is_empty() {
        let _ = tx.send(JobEvent::Line(String::from_utf8_lossy(&pending).into_owned()));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn collect(job: &Job) -> (Vec<String>, Option<i32>, bool) {
        let mut lines = Vec::new();
        loop {
            match job.next_timeout(Duration::from_secs(10)) {
                Some(JobEvent::Line(l)) => lines.push(l),
                Some(JobEvent::Partial(_)) => {}
                Some(JobEvent::Finished { code, cancelled }) => return (lines, code, cancelled),
                None => panic!("timeout"),
            }
        }
    }

    #[test]
    fn streams_lines_and_exit_code() {
        let job =
            spawn("sh", &["-c".into(), "printf 'a\\r\\nb\\rc\\n'; echo err >&2; exit 3".into()]).unwrap();
        let (mut lines, code, cancelled) = collect(&job);
        lines.sort();
        assert_eq!(lines, ["a", "b", "c", "err"]);
        assert_eq!(code, Some(3));
        assert!(!cancelled);
    }

    #[test]
    fn partial_output_is_delivered() {
        let job = spawn("sh", &["-c".into(), "printf 'Pass 1: 0 1 2 '; sleep 0.3; echo".into()]).unwrap();
        let mut saw_partial = false;
        loop {
            match job.next_timeout(Duration::from_secs(5)) {
                Some(JobEvent::Partial(p)) => saw_partial |= p.starts_with("Pass 1"),
                Some(JobEvent::Finished { .. }) => break,
                Some(_) => {}
                None => panic!("timeout"),
            }
        }
        assert!(saw_partial);
    }

    #[test]
    fn cancel_interrupts() {
        let job = spawn("sh", &["-c".into(), "sleep 30".into()]).unwrap();
        thread::sleep(Duration::from_millis(100));
        let t = Instant::now();
        job.cancel();
        let (_, _, cancelled) = collect(&job);
        assert!(cancelled);
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn missing_program_is_explained() {
        let e = spawn("/nonexistent/gw", &[]).err().unwrap();
        assert!(e.contains("not found"));
    }
}
