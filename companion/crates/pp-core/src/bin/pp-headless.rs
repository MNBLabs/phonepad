//! The companion core with no window: useful for benchmarking, scripted tests
//! and running on a machine where the UI is not wanted.
//!
//!   cargo run -p pp-core --bin pp-headless
//!   cargo run -p pp-core --bin pp-headless -- --pair   # open the pairing window
//!   cargo run -p pp-core --bin pp-headless -- --trace-csv stick.csv

use std::time::{Duration, Instant};

use pp_core::server::{Command, Core};

/// Parse `--flag <value>` from the command line.
fn arg_value(flag: &str) -> Option<String> {
    let argv: Vec<String> = std::env::args().collect();
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1).cloned())
}

fn main() {
    println!("PhonePad companion (headless)");
    println!("=============================\n");

    let core = Core::start();
    let shared = core.shared.clone();

    // With a phone already trusted, pairing mode stays closed so a stranger
    // cannot pair unprompted. This is the headless equivalent of the GUI's
    // "Pair a phone" button.
    if std::env::args().any(|a| a == "--pair") {
        if let Ok(mut p) = shared.pairing.lock() {
            p.begin();
        }
    }

    // Exercise the re-attach path without a UI, so the removal/arrival a
    // browser needs to observe can be verified from a script.
    let reattach_at = arg_value("--reattach-after")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| {
            println!("  will re-attach the virtual pad after {secs}s");
            Instant::now() + Duration::from_secs(secs)
        });
    let mut reattached = false;

    // Capture every accepted packet to CSV. This is how the phone's stick
    // response gets measured rather than guessed: sweep a known displacement on
    // the phone and read back what the pad was actually told.
    if let Some(path) = arg_value("--trace-csv") {
        let path = std::path::PathBuf::from(path);
        match shared.trace.start(&path) {
            Ok(()) => println!("  tracing accepted input to {}", path.display()),
            Err(e) => {
                eprintln!("  could not open trace file: {e}");
                std::process::exit(2);
            }
        }
    }

    // Report what the core logged during startup, including any bind failure.
    std::thread::sleep(Duration::from_millis(300));

    // The log is a bounded ring, so a running index into it would skip lines
    // once it wraps. `total` counts everything ever pushed, which is what
    // "have I printed this yet" actually depends on.
    let mut printed_total = 0usize;

    loop {
        if let Some(at) = reattach_at {
            if !reattached && Instant::now() >= at {
                reattached = true;
                core.send(Command::ReattachPad);
            }
        }

        {
            let log = shared.log.lock().unwrap();
            let total = log.total as usize;
            let unseen = total.saturating_sub(printed_total).min(log.len());
            for line in log.iter().skip(log.len() - unseen) {
                println!("[{}] {:<5} {}", line.at, line.level.label(), line.text);
            }
            printed_total = total;
        }

        let st = shared.status_snapshot();
        let s = shared.stats.snapshot();

        if let Ok(p) = shared.pairing.lock() {
            if p.active {
                let left = p.remaining().unwrap_or_default().as_secs();
                println!("  PAIRING CODE {}  ({}s left)", p.code, left);
            }
        }

        println!(
            "  {} | {} | {} pps | loss {:.1}% | jitter {:.1} ms | rtt {:.1} ms | accepted {} | rejected {}",
            if s.connected { "CONNECTED" } else { "waiting" },
            st.active_device_name.as_deref().unwrap_or("no phone"),
            s.pps,
            s.loss_permille as f32 / 10.0,
            s.jitter_us as f32 / 1000.0,
            s.rtt_us as f32 / 1000.0,
            s.accepted,
            s.total_rejected(),
        );

        // A bare rejection total says something is wrong but not what. The
        // reason is the whole diagnosis, so print it whenever it is non-zero.
        if s.total_rejected() > 0 {
            println!(
                "    rejected: malformed {} | mac {} | replay {} | field {} | unknown-session {}",
                s.rejected_malformed,
                s.rejected_mac,
                s.rejected_replay,
                s.rejected_field,
                s.rejected_unknown_session,
            );
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
