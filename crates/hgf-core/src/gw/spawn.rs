// SPDX-License-Identifier: GPL-3.0-or-later

//! How `gw` processes are started, behind two small traits so that the code driving them
//! (a GUI, a copy loop…) does not depend on real processes:
//!
//! * [`ProcessSpawner`] starts the real `gw` ([`super::runner::spawn`]);
//! * [`ScriptedSpawner`] replays captured output instantly, for tests and demos.

use super::runner::{self, Job, JobEvent};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

/// A running job: a real `gw` process or a replay.
pub trait JobControl {
    /// Next pending event, if any (non-blocking).
    fn try_next(&self) -> Option<JobEvent>;
    fn is_finished(&self) -> bool;
    /// Ask the job to stop (idempotent).
    fn cancel(&self);
}

impl JobControl for Job {
    fn try_next(&self) -> Option<JobEvent> {
        Job::try_next(self)
    }
    fn is_finished(&self) -> bool {
        Job::is_finished(self)
    }
    fn cancel(&self) {
        Job::cancel(self)
    }
}

/// Starts jobs.
pub trait Spawner {
    fn spawn(&self, program: &str, args: &[OsString]) -> Result<Box<dyn JobControl>, String>;
}

/// Starts the real `gw` program.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessSpawner;

impl Spawner for ProcessSpawner {
    fn spawn(&self, program: &str, args: &[OsString]) -> Result<Box<dyn JobControl>, String> {
        Ok(Box::new(runner::spawn(program, args)?))
    }
}

/// Output of one scripted `gw` run.
#[derive(Clone, Debug)]
pub struct Script {
    pub lines: Vec<String>,
    pub exit_code: i32,
}

impl Script {
    pub fn new(text: &str, exit_code: i32) -> Self {
        Self { lines: text.lines().map(str::to_string).collect(), exit_code }
    }
}

/// Replays one [`Script`] per spawned job, in order, and records every command line.
/// Clones share the same queue and log, so a test keeps a clone to inspect the calls.
#[derive(Clone, Debug, Default)]
pub struct ScriptedSpawner {
    scripts: Arc<Mutex<VecDeque<Script>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl ScriptedSpawner {
    pub fn new(scripts: impl IntoIterator<Item = Script>) -> Self {
        Self { scripts: Arc::new(Mutex::new(scripts.into_iter().collect())), calls: Arc::default() }
    }

    /// Command lines received so far (`gw read --drive A …`).
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

struct ScriptedJob {
    events: Mutex<VecDeque<JobEvent>>,
    cancelled: Mutex<bool>,
}

impl JobControl for ScriptedJob {
    fn try_next(&self) -> Option<JobEvent> {
        if *self.cancelled.lock().ok()? {
            let mut ev = self.events.lock().ok()?;
            if ev.is_empty() {
                return None;
            }
            ev.clear();
            return Some(JobEvent::Finished { code: None, cancelled: true });
        }
        self.events.lock().ok()?.pop_front()
    }
    fn is_finished(&self) -> bool {
        self.events.lock().map(|e| e.is_empty()).unwrap_or(true)
    }
    fn cancel(&self) {
        if let Ok(mut c) = self.cancelled.lock() {
            *c = true;
        }
    }
}

impl Spawner for ScriptedSpawner {
    fn spawn(&self, program: &str, args: &[OsString]) -> Result<Box<dyn JobControl>, String> {
        let line = std::iter::once(program.to_string())
            .chain(args.iter().map(|a| a.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        if let Ok(mut c) = self.calls.lock() {
            c.push(line);
        }
        let script = self
            .scripts
            .lock()
            .ok()
            .and_then(|mut s| s.pop_front())
            .ok_or_else(|| format!("no script left for '{program}'"))?;
        let mut events: VecDeque<JobEvent> = script.lines.into_iter().map(JobEvent::Line).collect();
        events.push_back(JobEvent::Finished { code: Some(script.exit_code), cancelled: false });
        Ok(Box::new(ScriptedJob { events: Mutex::new(events), cancelled: Mutex::new(false) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_scripts_in_order_and_logs_calls() {
        let s = ScriptedSpawner::new([Script::new("one\ntwo", 0), Script::new("", 1)]);
        let probe = s.clone();
        let job = s.spawn("gw", &["info".into()]).unwrap();
        assert_eq!(job.try_next(), Some(JobEvent::Line("one".into())));
        assert_eq!(job.try_next(), Some(JobEvent::Line("two".into())));
        assert_eq!(job.try_next(), Some(JobEvent::Finished { code: Some(0), cancelled: false }));
        assert!(job.is_finished());
        let job = s.spawn("gw", &["rpm".into()]).unwrap();
        assert_eq!(job.try_next(), Some(JobEvent::Finished { code: Some(1), cancelled: false }));
        assert!(s.spawn("gw", &[]).is_err());
        assert_eq!(probe.calls(), vec!["gw info", "gw rpm", "gw"]);
    }

    #[test]
    fn cancel_ends_the_job() {
        let s = ScriptedSpawner::new([Script::new("a\nb\nc", 0)]);
        let job = s.spawn("gw", &[]).unwrap();
        assert!(job.try_next().is_some());
        job.cancel();
        assert_eq!(job.try_next(), Some(JobEvent::Finished { code: None, cancelled: true }));
        assert_eq!(job.try_next(), None);
    }
}
