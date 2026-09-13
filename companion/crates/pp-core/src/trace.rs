//! Optional CSV capture of accepted input, for measuring how the phone's touch
//! controls actually behave.
//!
//! Tuning an on-screen stick by feel does not converge — the question "does a
//! small thumb movement produce a small axis value" needs the axis values. This
//! writes exactly what the pad was told to do, so a displacement sweep on the
//! phone can be plotted against its output.
//!
//! Disabled by default, and disabled costs one relaxed atomic load per packet.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use pp_protocol::packet::ControllerState;

#[derive(Default)]
pub struct Trace {
    on: AtomicBool,
    sink: Mutex<Option<Sink>>,
}

struct Sink {
    out: BufWriter<File>,
    start: Instant,
}

impl Trace {
    /// Begin writing to `path`. Replaces any capture already in progress.
    pub fn start(&self, path: &std::path::Path) -> Result<(), String> {
        let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut out = BufWriter::new(file);
        writeln!(out, "t_us,seq,lx,ly,rx,ry,lt,rt,buttons,rtt_us").map_err(|e| e.to_string())?;

        let mut slot = self.sink.lock().map_err(|_| "trace lock poisoned")?;
        *slot = Some(Sink {
            out,
            start: Instant::now(),
        });
        self.on.store(true, Ordering::Release);
        Ok(())
    }

    /// Flush and close. Safe to call when no capture is running.
    pub fn stop(&self) {
        self.on.store(false, Ordering::Release);
        if let Ok(mut slot) = self.sink.lock() {
            if let Some(mut s) = slot.take() {
                let _ = s.out.flush();
            }
        }
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// Record one accepted packet. A write failure stops the capture rather
    /// than logging once per packet at 250 Hz.
    pub fn record(&self, seq: u32, rtt_us: u32, st: &ControllerState) {
        if !self.on.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut slot) = self.sink.lock() else {
            return;
        };
        let Some(s) = slot.as_mut() else { return };

        let t = s.start.elapsed().as_micros();
        let line = format!(
            "{t},{seq},{},{},{},{},{},{},{},{rtt_us}",
            st.lx, st.ly, st.rx, st.ry, st.lt, st.rt, st.buttons
        );
        if writeln!(s.out, "{line}").is_err() {
            self.on.store(false, Ordering::Release);
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pp-trace-{name}-{}.csv", std::process::id()));
        p
    }

    #[test]
    fn disabled_by_default_and_records_nothing() {
        let t = Trace::default();
        assert!(!t.is_on());
        t.record(1, 0, &ControllerState::NEUTRAL); // must not panic
    }

    #[test]
    fn writes_a_header_and_one_row_per_packet() {
        let path = temp("rows");
        let t = Trace::default();
        t.start(&path).unwrap();
        let mut st = ControllerState::NEUTRAL;
        st.lx = -1234;
        st.rt = 200;
        t.record(7, 5000, &st);
        t.stop();

        let body = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines[0], "t_us,seq,lx,ly,rx,ry,lt,rt,buttons,rtt_us");
        assert_eq!(lines.len(), 2);
        let cells: Vec<&str> = lines[1].split(',').collect();
        assert_eq!(cells[1], "7");
        assert_eq!(cells[2], "-1234");
        assert_eq!(cells[7], "200");
        assert_eq!(cells[9], "5000");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stop_is_idempotent_and_record_after_stop_is_inert() {
        let path = temp("stop");
        let t = Trace::default();
        t.start(&path).unwrap();
        t.stop();
        t.stop();
        t.record(1, 0, &ControllerState::NEUTRAL);
        assert!(!t.is_on());
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
