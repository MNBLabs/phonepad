//! Noticing when a cloud gaming client starts.
//!
//! A cloud client binds a controller to a streaming session at the moment it
//! observes one *arrive*. A pad that already existed when the stream started
//! produces no arrival event, so the session ends up with no controller bound
//! even though the client can read the pad perfectly well. The signature is
//! very specific: the client's own menus respond, and the game ignores
//! everything.
//!
//! Re-attaching fixes it, but only if it happens *after* the client starts.
//! Re-attaching on connect instead would yank the pad out from under a game
//! that was already working, which is why that is not the answer.
//!
//! So: watch for a known client appearing, and re-attach once, shortly after.
//! This is a heuristic over process names and it is treated as one — it is off
//! until the user turns it on, it fires at most once per launch, and the manual
//! path stays one gesture away on the phone.

use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Executable names that bind a controller on arrival.
///
/// Browsers are here because xbox.com/play runs in one. That makes the check
/// coarse — opening a browser for anything at all counts — which is survivable
/// only because a spurious re-attach costs a 400 ms gap in a pad nothing is
/// currently reading, and because this is off by default.
const CLIENTS: &[&str] = &[
    "xboxpcapp.exe",
    "gamingservices.exe",
    "geforcenow.exe",
    "steamwebhelper.exe",
    "msedge.exe",
    "chrome.exe",
    "firefox.exe",
];

/// How long after a client appears before re-attaching.
///
/// Long enough for the client to have finished starting and to be listening for
/// controllers, short enough that the user is probably still on a menu rather
/// than mid-race.
pub const SETTLE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct CloudWatch {
    seen: HashSet<String>,
    primed: bool,
    /// `None` until the first scan, so the very first poll is never rate-limited
    /// away — it is the one that records what was already running.
    last_poll: Option<Instant>,
    pending: Option<Instant>,
}

impl CloudWatch {
    /// Called every loop iteration. Returns true exactly once when a newly
    /// started client has settled and the pad should be re-attached.
    ///
    /// `enabled` is read every call rather than at construction, so turning the
    /// setting off mid-session takes effect immediately.
    pub fn poll(&mut self, now: Instant, enabled: bool) -> bool {
        if !enabled {
            // Forget everything, so re-enabling does not immediately fire on
            // clients that were already running while it was off.
            self.primed = false;
            self.pending = None;
            self.last_poll = None;
            self.seen.clear();
            return false;
        }

        if let Some(at) = self.pending {
            if now >= at {
                self.pending = None;
                return true;
            }
        }

        if let Some(last) = self.last_poll {
            if now.duration_since(last) < Duration::from_secs(1) {
                return false;
            }
        }
        self.last_poll = Some(now);

        let running = running_clients();

        // The first poll only records what was already running. Without this,
        // enabling the setting while the Xbox app is open would re-attach
        // immediately — into a game that may already be working.
        if !self.primed {
            self.primed = true;
            self.seen = running;
            return false;
        }

        let started = running.difference(&self.seen).next().is_some();
        self.seen = running;
        if started && self.pending.is_none() {
            self.pending = Some(now + SETTLE);
        }
        false
    }

    /// Name of a client currently running, for the log line. Cheap: reads the
    /// set the last poll already built.
    pub fn any_seen(&self) -> Option<&str> {
        self.seen.iter().next().map(|s| s.as_str())
    }
}

#[cfg(windows)]
fn running_clients() -> HashSet<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut found = HashSet::new();

    // SAFETY: a process snapshot with no owner filter; the handle is closed on
    // every path out, and PROCESSENTRY32W is zeroed with dwSize set as the API
    // requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if CLIENTS.contains(&name.as_str()) {
                    found.insert(name);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    found
}

#[cfg(not(windows))]
fn running_clients() -> HashSet<String> {
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_fires() {
        let mut w = CloudWatch::default();
        let t = Instant::now();
        assert!(!w.poll(t, false));
        assert!(!w.poll(t + Duration::from_secs(10), false));
    }

    #[test]
    fn the_first_poll_only_records_what_was_already_running() {
        // Enabling the setting with a client already open must not re-attach:
        // that client may be mid-game and working.
        let mut w = CloudWatch::default();
        let t = Instant::now();
        assert!(!w.poll(t, true));
        assert!(w.primed);
        assert!(w.pending.is_none());
    }

    #[test]
    fn a_started_client_fires_once_after_settling() {
        let mut w = CloudWatch::default();
        let t = Instant::now();
        w.primed = true;
        w.last_poll = Some(t - Duration::from_secs(2));
        w.seen = HashSet::new();

        // Stand in for the process scan, which cannot be driven from a test.
        w.pending = Some(t + SETTLE);

        assert!(!w.poll(t, true), "must not fire before settling");
        assert!(w.poll(t + SETTLE, true), "must fire once settled");
        assert!(!w.poll(t + SETTLE * 2, true), "must not fire twice");
    }

    #[test]
    fn turning_it_off_clears_a_pending_reattach() {
        let mut w = CloudWatch::default();
        let t = Instant::now();
        w.pending = Some(t);
        assert!(!w.poll(t, false));
        assert!(w.pending.is_none());
        // And re-enabling re-primes rather than firing straight away.
        assert!(!w.poll(t, true));
    }
}
