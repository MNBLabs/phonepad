//! PhonePad companion core.
//!
//! Owns the network, the sessions, the watchdog and the virtual pad. Knows
//! nothing about any UI — `pp-companion` is a reader of the shared state this
//! crate publishes, never a participant in the input path.

pub mod backend;
pub mod cloud;
pub mod log;
pub mod server;
pub mod stats;
pub mod store;
pub mod trace;

pub use backend::{BackendKind, VirtualPad};
pub use server::{Command, Core, PairOutcome, PairingState, Shared, Status};
pub use stats::StatsSnapshot;
pub use store::Config;

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, and the seconds-into-the-day.
fn epoch_parts() -> (i64, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (secs.div_euclid(86_400), secs.rem_euclid(86_400) as u32)
}

/// `HH:MM:SS` in UTC. Only used for log line prefixes, so a timezone database
/// would be more machinery than the job deserves.
pub fn now_hms() -> String {
    let (_, sod) = epoch_parts();
    format!("{:02}:{:02}:{:02}", sod / 3600, (sod / 60) % 60, sod % 60)
}

/// `YYYY-MM-DD HH:MM:SSZ` in UTC, for the "last seen" column.
pub fn now_iso() -> String {
    let (days, sod) = epoch_parts();
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60
    )
}

/// Howard Hinnant's `civil_from_days`. Exact for all dates we can encounter,
/// and avoids pulling in a date library for two format strings.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(1), (1970, 1, 2));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        // 2000-03-01, just past a leap day in a leap century.
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        // 2026-08-15
        assert_eq!(civil_from_days(20_680), (2026, 8, 15));
    }

    #[test]
    fn timestamps_are_well_formed() {
        let hms = now_hms();
        assert_eq!(hms.len(), 8);
        assert_eq!(hms.matches(':').count(), 2);

        let iso = now_iso(); // "YYYY-MM-DD HH:MM:SSZ"
        assert_eq!(iso.len(), 20, "{iso}");
        assert!(iso.ends_with('Z'));
    }
}
