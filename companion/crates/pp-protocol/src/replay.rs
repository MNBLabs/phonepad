//! Anti-replay sliding window.
//!
//! UDP reorders, so "reject anything not strictly newer" would throw away
//! perfectly good input. This is the standard IPsec-style approach: track the
//! highest sequence seen plus a bitmap of the 64 slots below it, accepting
//! out-of-order packets within that window exactly once each.

/// Number of slots below the highest sequence that remain acceptable.
pub const WINDOW: u32 = 64;

#[derive(Debug, Clone)]
pub struct ReplayWindow {
    highest: u32,
    /// Bit *n* set means `highest - n` has already been accepted.
    bitmap: u64,
    started: bool,
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayWindow {
    pub const fn new() -> Self {
        Self {
            highest: 0,
            bitmap: 0,
            started: false,
        }
    }

    pub fn highest(&self) -> u32 {
        self.highest
    }

    /// Number of sequence numbers that were skipped between the first and the
    /// highest accepted packet. Combined with the accepted count this gives a
    /// true loss figure rather than an estimate.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Returns `true` if this sequence is fresh and should be processed.
    pub fn accept(&mut self, seq: u32) -> bool {
        // Sequences start at 1; zero means an uninitialised sender.
        if seq == 0 {
            return false;
        }

        if !self.started {
            self.started = true;
            self.highest = seq;
            self.bitmap = 1;
            return true;
        }

        if seq > self.highest {
            let shift = seq - self.highest;
            self.bitmap = if shift >= 64 {
                // The whole window has moved past; nothing below is still valid.
                0
            } else {
                self.bitmap << shift
            };
            self.bitmap |= 1;
            self.highest = seq;
            return true;
        }

        let diff = self.highest - seq;
        if diff >= WINDOW {
            // Too old to distinguish from a replay.
            return false;
        }
        let mask = 1u64 << diff;
        if self.bitmap & mask != 0 {
            return false; // already seen
        }
        self.bitmap |= mask;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_monotonic_stream_once_each() {
        let mut w = ReplayWindow::new();
        for seq in 1..=1000 {
            assert!(w.accept(seq), "seq {seq} rejected");
        }
        assert_eq!(w.highest(), 1000);
    }

    #[test]
    fn rejects_exact_duplicates() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(1));
        assert!(!w.accept(1));
        assert!(w.accept(2));
        assert!(!w.accept(2));
        assert!(!w.accept(1));
    }

    #[test]
    fn rejects_sequence_zero() {
        let mut w = ReplayWindow::new();
        assert!(!w.accept(0));
    }

    #[test]
    fn accepts_reordering_inside_the_window() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(10));
        // These arrived late but are still fresh.
        assert!(w.accept(7));
        assert!(w.accept(9));
        assert!(w.accept(8));
        // ...and none of them a second time.
        assert!(!w.accept(7));
        assert!(!w.accept(8));
        assert!(!w.accept(9));
        assert_eq!(w.highest(), 10);
    }

    #[test]
    fn rejects_packets_older_than_the_window() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(100));
        assert!(w.accept(100 - WINDOW + 1)); // just inside
        assert!(!w.accept(100 - WINDOW)); // exactly at the edge, too old
        assert!(!w.accept(1));
    }

    #[test]
    fn a_large_forward_jump_clears_the_window() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(1));
        assert!(w.accept(10_000));
        // Everything from before the jump is now unverifiable, so rejected.
        assert!(!w.accept(2));
        assert!(!w.accept(9_000));
        // But the new region still works normally.
        assert!(w.accept(9_999));
        assert!(!w.accept(9_999));
    }

    #[test]
    fn a_replayed_burst_is_fully_rejected() {
        let mut w = ReplayWindow::new();
        let burst: Vec<u32> = (1..=50).collect();
        for &s in &burst {
            assert!(w.accept(s));
        }
        // An attacker captures and re-sends the whole burst.
        for &s in &burst {
            assert!(!w.accept(s), "replayed seq {s} was accepted");
        }
    }

    #[test]
    fn reset_starts_clean() {
        let mut w = ReplayWindow::new();
        assert!(w.accept(500));
        w.reset();
        // A new session restarts numbering; the window must not block it.
        assert!(w.accept(1));
    }
}
