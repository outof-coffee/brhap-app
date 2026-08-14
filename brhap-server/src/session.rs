//! One game at a time.
//!
//! Holds the child process, applies the symlink plan before spawning, and
//! reports state changes. A launch request while something is already running
//! is rejected rather than queued.
//!
//! This module must not know that Tauri exists. Events go out through a
//! listener supplied by the caller, so the desktop wrapper can forward them to
//! a webview while a terminal caller just prints them.

use std::fmt;
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde::Serialize;

use crate::launch::LaunchPlan;
use crate::launcher::{apply_plan, spawn_game};

/// How often the watcher thread checks whether the game has exited.
const POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    State { running: bool, pid: Option<u32> },
    Linked { removed: usize, created: usize },
    Spawned { pid: u32 },
    Exited { code: Option<i32> },
    Error { message: String },
}

#[derive(Debug)]
pub enum SessionError {
    /// The request does not apply to the current state.
    Conflict(String),
    Io(std::io::Error),
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(message) => write!(formatter, "{message}"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<std::io::Error> for SessionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a successful launch did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Launched {
    pub pid: u32,
    pub removed: usize,
    pub created: usize,
}

/// Anything that wants to hear about the running game.
pub type Listener = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Session {
    child: Arc<Mutex<Option<Child>>>,
    listener: Listener,
}

impl Session {
    pub fn new(listener: Listener) -> Self {
        Self { child: Arc::new(Mutex::new(None)), listener }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Option<Child>> {
        self.child.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn running(&self) -> bool {
        self.held().is_some()
    }

    pub fn pid(&self) -> Option<u32> {
        self.held().as_ref().map(Child::id)
    }

    /// Create the symlinks the plan describes, then spawn the game. Rejected
    /// rather than queued when something is already running.
    pub fn launch(
        &self,
        plan: &LaunchPlan,
        workshop_dir: &Path,
    ) -> Result<Launched, SessionError> {
        if self.running() {
            return Err(SessionError::Conflict("a game is already running".into()));
        }

        let applied = apply_plan(plan, workshop_dir)?;
        (self.listener)(Event::Linked {
            removed: applied.removed.len(),
            created: applied.created.len(),
        });

        let child = spawn_game(plan)?;
        let pid = child.id();
        *self.held() = Some(child);

        (self.listener)(Event::Spawned { pid });
        (self.listener)(Event::State { running: true, pid: Some(pid) });

        self.watch();
        Ok(Launched { pid, removed: applied.removed.len(), created: applied.created.len() })
    }

    /// Watch for the game exiting, so an exit nobody asked for still reports.
    fn watch(&self) {
        let child = Arc::clone(&self.child);
        let listener = Arc::clone(&self.listener);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(POLL);
                let mut held = child.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let Some(process) = held.as_mut() else {
                    return;
                };
                match process.try_wait() {
                    Ok(Some(status)) => {
                        *held = None;
                        drop(held);
                        listener(Event::Exited { code: status.code() });
                        listener(Event::State { running: false, pid: None });
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        *held = None;
                        drop(held);
                        listener(Event::Error { message: error.to_string() });
                        listener(Event::State { running: false, pid: None });
                        return;
                    }
                }
            }
        });
    }

    /// Ask the game to close. The exit arrives through the listener.
    ///
    /// Sends SIGTERM so the game shuts down the way it does under the Node
    /// implementation. `Child::kill` is hardcoded to SIGKILL, which would give
    /// the process no chance to close cleanly.
    pub fn stop(&self) -> Result<(), SessionError> {
        let pid = self.pid().ok_or_else(|| SessionError::Conflict("nothing is running".into()))?;
        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|error| SessionError::Io(std::io::Error::other(error)))
    }
}

/// A listener that prints events, for the terminal binary and for tests.
pub fn printing_listener() -> Listener {
    Arc::new(|event| println!("  event: {}", serde_json::to_string(&event).unwrap_or_default()))
}

/// A listener that records events, so a caller can assert on them.
pub fn recording_listener() -> (Listener, Arc<Mutex<Vec<Event>>>) {
    let log: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&log);
    let listener: Listener = Arc::new(move |event| {
        sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(event);
    });
    (listener, log)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_new_session_is_idle() {
        let session = Session::new(printing_listener());
        assert!(!session.running());
        assert_eq!(session.pid(), None);
    }

    #[test]
    fn stopping_nothing_is_a_conflict_not_a_crash() {
        let session = Session::new(printing_listener());
        match session.stop() {
            Err(SessionError::Conflict(message)) => assert!(message.contains("nothing is running")),
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn events_serialize_in_the_shape_the_webview_expects() {
        let json = serde_json::to_string(&Event::Spawned { pid: 42 }).expect("serializes");
        assert_eq!(json, r#"{"type":"spawned","pid":42}"#);

        let json = serde_json::to_string(&Event::State { running: false, pid: None })
            .expect("serializes");
        assert_eq!(json, r#"{"type":"state","running":false,"pid":null}"#);
    }

    /// Exercises spawn, the watcher thread, and stop against `/bin/sleep`
    /// rather than the game. Ignored by default so the normal suite stays
    /// pure logic and starts no processes.
    ///
    /// Run with: cargo test -- --ignored
    #[test]
    #[ignore]
    fn spawns_watches_and_stops_a_real_process() {
        let plan = LaunchPlan {
            executable: PathBuf::from("/bin/sleep"),
            cwd: std::env::temp_dir(),
            args: vec!["30".into()],
            env: Default::default(),
            symlinks: Vec::new(),
            preview: String::new(),
        };

        let (listener, log) = recording_listener();
        let session = Session::new(listener);
        let launched = session.launch(&plan, Path::new("/nonexistent/workshop")).expect("launches");
        assert!(session.running());
        assert_eq!(session.pid(), Some(launched.pid));

        session.stop().expect("stops");

        // The watcher polls, so give it a few cycles to notice.
        for _ in 0..40 {
            if !session.running() {
                break;
            }
            std::thread::sleep(POLL);
        }
        assert!(!session.running(), "watcher should have cleared the child");

        let seen = log.lock().expect("lock").clone();
        assert!(seen.iter().any(|event| matches!(event, Event::Spawned { .. })));
        assert!(seen.iter().any(|event| matches!(event, Event::Exited { .. })));
        assert!(seen.iter().any(|event| matches!(event, Event::State { running: false, .. })));
    }

    #[test]
    fn a_recording_listener_captures_what_is_emitted() {
        let (listener, log) = recording_listener();
        listener(Event::Linked { removed: 2, created: 3 });
        let held = log.lock().expect("lock");
        assert_eq!(held.as_slice(), &[Event::Linked { removed: 2, created: 3 }]);
    }
}
