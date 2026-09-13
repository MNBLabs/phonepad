//! Diagnostics counters and live gauges.
//!
//! Every number here is measured, never estimated. Rejected packets are counted
//! by reason so a failure shows up as "42 bad MAC" rather than as silence.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const REL: Ordering = Ordering::Relaxed;

#[derive(Default, Debug)]
pub struct Counters {
    pub accepted: AtomicU64,
    pub rejected_malformed: AtomicU64,
    pub rejected_mac: AtomicU64,
    pub rejected_replay: AtomicU64,
    pub rejected_field: AtomicU64,
    pub rejected_unknown_session: AtomicU64,
    pub sessions_started: AtomicU64,
    pub sessions_dropped: AtomicU64,
    pub neutralisations: AtomicU64,
    pub backend_errors: AtomicU64,
}

#[derive(Default, Debug)]
pub struct Gauges {
    /// Accepted input packets per second, over the last completed second.
    pub pps: AtomicU32,
    pub loss_permille: AtomicU32,
    /// Mean inter-arrival deviation, microseconds.
    pub jitter_us: AtomicU32,
    /// Round-trip time as reported by the phone, microseconds. 0 = unknown.
    pub rtt_us: AtomicU32,
    pub connected: AtomicBool,
    /// True while the watchdog is holding the pad at neutral.
    pub neutralised: AtomicBool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StatsSnapshot {
    pub accepted: u64,
    pub rejected_malformed: u64,
    pub rejected_mac: u64,
    pub rejected_replay: u64,
    pub rejected_field: u64,
    pub rejected_unknown_session: u64,
    pub sessions_started: u64,
    pub sessions_dropped: u64,
    pub neutralisations: u64,
    pub backend_errors: u64,
    pub pps: u32,
    pub loss_permille: u32,
    pub jitter_us: u32,
    pub rtt_us: u32,
    pub connected: bool,
    pub neutralised: bool,
}

impl StatsSnapshot {
    pub fn total_rejected(&self) -> u64 {
        self.rejected_malformed
            + self.rejected_mac
            + self.rejected_replay
            + self.rejected_field
            + self.rejected_unknown_session
    }
}

#[derive(Default, Debug)]
pub struct Stats {
    pub counters: Counters,
    pub gauges: Gauges,
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        let c = &self.counters;
        let g = &self.gauges;
        StatsSnapshot {
            accepted: c.accepted.load(REL),
            rejected_malformed: c.rejected_malformed.load(REL),
            rejected_mac: c.rejected_mac.load(REL),
            rejected_replay: c.rejected_replay.load(REL),
            rejected_field: c.rejected_field.load(REL),
            rejected_unknown_session: c.rejected_unknown_session.load(REL),
            sessions_started: c.sessions_started.load(REL),
            sessions_dropped: c.sessions_dropped.load(REL),
            neutralisations: c.neutralisations.load(REL),
            backend_errors: c.backend_errors.load(REL),
            pps: g.pps.load(REL),
            loss_permille: g.loss_permille.load(REL),
            jitter_us: g.jitter_us.load(REL),
            rtt_us: g.rtt_us.load(REL),
            connected: g.connected.load(REL),
            neutralised: g.neutralised.load(REL),
        }
    }
}

/// Rolling per-second rate, loss and jitter for one session.
///
/// Kept entirely inside the input thread: no locks, no allocation, nothing the
/// hot path has to wait on.
pub struct RateTracker {
    window_start: Instant,
    accepted_this_window: u32,
    first_seq: u32,
    highest_seq: u32,
    accepted_total: u64,
    last_arrival: Option<Instant>,
    /// Exponentially smoothed |inter-arrival − mean|, in microseconds.
    jitter_us: f32,
    mean_gap_us: f32,
}

impl Default for RateTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RateTracker {
    pub fn new() -> Self {
        Self {
            window_start: Instant::now(),
            accepted_this_window: 0,
            first_seq: 0,
            highest_seq: 0,
            accepted_total: 0,
            last_arrival: None,
            jitter_us: 0.0,
            mean_gap_us: 0.0,
        }
    }

    pub fn record(&mut self, seq: u32, now: Instant) {
        if self.first_seq == 0 {
            self.first_seq = seq;
        }
        if seq > self.highest_seq {
            self.highest_seq = seq;
        }
        self.accepted_this_window += 1;
        self.accepted_total += 1;

        if let Some(prev) = self.last_arrival {
            let gap_us = now.duration_since(prev).as_secs_f32() * 1e6;
            if self.mean_gap_us == 0.0 {
                self.mean_gap_us = gap_us;
            } else {
                // RFC 3550-style smoothing: cheap, stable, and good enough to
                // spot a genuinely jittery link.
                self.mean_gap_us += (gap_us - self.mean_gap_us) / 16.0;
                self.jitter_us += ((gap_us - self.mean_gap_us).abs() - self.jitter_us) / 16.0;
            }
        }
        self.last_arrival = Some(now);
    }

    /// Publish once a second. Returns `true` if the gauges were updated.
    pub fn tick(&mut self, now: Instant, gauges: &Gauges) -> bool {
        let elapsed = now.duration_since(self.window_start);
        if elapsed < Duration::from_secs(1) {
            return false;
        }
        // Divide by the window that actually elapsed rather than assuming it was
        // exactly one second. This is only polled on the feedback tick, so the
        // window overshoots slightly and the rate would read high.
        let rate = (self.accepted_this_window as f64 / elapsed.as_secs_f64()).round();
        gauges.pps.store(rate as u32, REL);
        gauges.jitter_us.store(self.jitter_us as u32, REL);

        // Loss is measured against the sender's own sequence numbers, so it
        // reflects packets that genuinely never arrived.
        let expected = self.highest_seq.saturating_sub(self.first_seq) as u64 + 1;
        let permille = if expected > 0 && expected >= self.accepted_total {
            ((expected - self.accepted_total) * 1000 / expected) as u32
        } else {
            0
        };
        gauges.loss_permille.store(permille, REL);

        self.window_start = now;
        self.accepted_this_window = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reflects_counters() {
        let s = Stats::default();
        s.counters.accepted.fetch_add(5, REL);
        s.counters.rejected_mac.fetch_add(2, REL);
        s.gauges.connected.store(true, REL);
        let snap = s.snapshot();
        assert_eq!(snap.accepted, 5);
        assert_eq!(snap.rejected_mac, 2);
        assert_eq!(snap.total_rejected(), 2);
        assert!(snap.connected);
    }

    #[test]
    fn perfect_stream_reports_no_loss() {
        let g = Gauges::default();
        let mut t = RateTracker::new();
        let start = Instant::now();
        for seq in 1..=100 {
            t.record(seq, start);
        }
        assert!(t.tick(start + Duration::from_secs(1), &g));
        assert_eq!(g.pps.load(REL), 100);
        assert_eq!(g.loss_permille.load(REL), 0);
    }

    #[test]
    fn dropped_packets_show_up_as_loss() {
        let g = Gauges::default();
        let mut t = RateTracker::new();
        let start = Instant::now();
        // Sequences 2..=11 never arrive; 1 and 12..=100 do. That keeps the
        // highest sequence at 100 so "expected" is unambiguously 100, and
        // exactly 10 of them are missing.
        for seq in (1..=100u32).filter(|s| !(2..=11).contains(s)) {
            t.record(seq, start);
        }
        t.tick(start + Duration::from_secs(1), &g);
        assert_eq!(g.pps.load(REL), 90);
        assert_eq!(g.loss_permille.load(REL), 100); // 10.0%
    }

    #[test]
    fn tick_does_nothing_before_a_full_second() {
        let g = Gauges::default();
        let mut t = RateTracker::new();
        let start = Instant::now();
        t.record(1, start);
        assert!(!t.tick(start + Duration::from_millis(999), &g));
        assert_eq!(g.pps.load(REL), 0);
    }

    #[test]
    fn steady_arrivals_report_low_jitter() {
        let g = Gauges::default();
        let mut t = RateTracker::new();
        let start = Instant::now();
        for i in 1..=200u32 {
            t.record(i, start + Duration::from_micros(4000 * i as u64));
        }
        t.tick(start + Duration::from_secs(1), &g);
        // Perfectly spaced arrivals must not read as jittery.
        assert!(
            g.jitter_us.load(REL) < 500,
            "jitter {}",
            g.jitter_us.load(REL)
        );
    }
}
