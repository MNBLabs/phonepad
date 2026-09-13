//! M1 gate: prove that Windows genuinely sees a working virtual Xbox controller.
//!
//! This does not merely check that a device node appeared. It plugs in a ViGEm
//! Xbox 360 target, writes known states to it, and reads them back through
//! Windows' own XInput API, asserting that what Windows reports matches what we
//! wrote. Anything less would not actually prove the pipeline.
//!
//!   cargo run -p pp-vigem-probe            # automated verification, exits 0/1
//!   cargo run -p pp-vigem-probe -- --sweep # continuous motion for joy.cpl
//!   cargo run -p pp-vigem-probe -- --rumble # ask the pad to rumble, as a game would

use std::time::{Duration, Instant};

use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows_sys::Win32::UI::Input::XboxController::{
    XInputGetState, XInputSetState, XINPUT_STATE, XINPUT_VIBRATION,
};

const ERROR_SUCCESS: u32 = 0;

/// How long to wait for a written state to become observable through XInput
/// before declaring the control broken.
const SETTLE_TIMEOUT: Duration = Duration::from_millis(750);

type Pad = Xbox360Wired<Client>;

/// Outcome of one check: whether XInput reported what we wrote, and how long
/// it took to become observable.
struct Outcome {
    matched: bool,
    latency: Option<Duration>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sweep = args.iter().any(|a| a == "--sweep");

    // Read-only mode: plugs in nothing, just reports what Windows currently
    // sees. Used to verify that *something else* — the companion driven by a
    // phone — is really moving the controller.
    // Drive rumble into whatever pad is already present, exactly the way a
    // game does. The point is to prove the whole return path — game to
    // ViGEmBus to the companion to the phone's motor — with no game involved.
    if args.iter().any(|a| a == "--rumble") {
        let seconds = args
            .iter()
            .position(|a| a == "--seconds")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(6);
        rumble(seconds);
        return;
    }

    if args.iter().any(|a| a == "--watch") {
        let seconds = args
            .iter()
            .position(|a| a == "--seconds")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);
        watch(seconds);
        return;
    }

    println!("PhonePad — ViGEm virtual controller probe (M1 gate)");
    println!("===================================================\n");

    if let Err(e) = run(sweep) {
        eprintln!("\nFAILED: {e}");
        eprintln!("\n{}", remediation(&e));
        std::process::exit(1);
    }
}

fn remediation(err: &str) -> String {
    if err.contains("connect") || err.contains("BusNotFound") || err.contains("driver") {
        "The ViGEmBus driver could not be reached.\n\
         Install ViGEmBus 1.22.0 from the official release:\n  \
         https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0\n\
         then reboot if prompted and re-run this probe."
            .to_string()
    } else {
        "See docs/ADR-001-architecture.md for the virtual-controller design and \
         fallback options."
            .to_string()
    }
}

fn run(sweep: bool) -> Result<(), String> {
    // Snapshot which XInput slots are already in use so we can identify ours.
    let before = connected_slots();
    println!("XInput slots in use before plugin: {before:?}");

    let client = Client::connect().map_err(|e| format!("Client::connect failed: {e:?}"))?;
    let mut pad = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);

    pad.plugin().map_err(|e| format!("plugin failed: {e:?}"))?;
    pad.wait_ready()
        .map_err(|e| format!("wait_ready failed: {e:?}"))?;
    println!("Virtual Xbox 360 target plugged in and ready.");

    let slot = find_new_slot(&before)
        .ok_or_else(|| "target plugged in, but no new XInput slot appeared".to_string())?;
    println!("Windows exposes it on XInput slot {slot}.\n");

    if sweep {
        return sweep_forever(&mut pad, slot);
    }

    let mut results: Vec<(String, Outcome)> = Vec::new();

    // --- buttons, one at a time -------------------------------------------
    let buttons: [(&str, u16); 15] = [
        ("A", XButtons::A),
        ("B", XButtons::B),
        ("X", XButtons::X),
        ("Y", XButtons::Y),
        ("LB", XButtons::LB),
        ("RB", XButtons::RB),
        ("LS press", XButtons::LTHUMB),
        ("RS press", XButtons::RTHUMB),
        ("Start/Menu", XButtons::START),
        ("Back/View", XButtons::BACK),
        ("Guide", XButtons::GUIDE),
        ("Dpad up", XButtons::UP),
        ("Dpad down", XButtons::DOWN),
        ("Dpad left", XButtons::LEFT),
        ("Dpad right", XButtons::RIGHT),
    ];

    let has_ex = get_state_ex().is_some();
    println!(
        "Read-back path: {}",
        if has_ex {
            "XInputGetStateEx (ordinal 100) — Guide button observable"
        } else {
            "XInputGetState — Guide button not observable, will be skipped"
        }
    );

    println!("\nTesting buttons...");
    for (name, mask) in buttons {
        if mask == XButtons::GUIDE && !has_ex {
            println!("  [skip] {name:<20} (XInputGetStateEx unavailable on this system)");
            continue;
        }
        let want = XGamepad {
            buttons: XButtons { raw: mask },
            ..Default::default()
        };
        let outcome = apply_and_verify(&mut pad, slot, want)?;
        report(name, &outcome);
        results.push((name.to_string(), outcome));
        // Release before the next one so a stuck button cannot cascade.
        apply_and_verify(&mut pad, slot, XGamepad::default())?;
    }

    // --- triggers ----------------------------------------------------------
    println!("\nTesting triggers...");
    for level in [0u8, 64, 128, 192, 255] {
        let want = XGamepad {
            left_trigger: level,
            right_trigger: 255 - level,
            ..Default::default()
        };
        let outcome = apply_and_verify(&mut pad, slot, want)?;
        let name = format!("LT={level} RT={}", 255 - level);
        report(&name, &outcome);
        results.push((name, outcome));
    }

    // --- sticks ------------------------------------------------------------
    println!("\nTesting analogue sticks...");
    let positions: [(i16, i16); 6] = [
        (0, 0),
        (i16::MAX, 0),
        (i16::MIN + 1, 0),
        (0, i16::MAX),
        (0, i16::MIN + 1),
        (12345, -23456),
    ];
    for (x, y) in positions {
        let want = XGamepad {
            thumb_lx: x,
            thumb_ly: y,
            thumb_rx: -x,
            thumb_ry: -y,
            ..Default::default()
        };
        let outcome = apply_and_verify(&mut pad, slot, want)?;
        let name = format!("L=({x},{y}) R=({},{})", -x, -y);
        report(&name, &outcome);
        results.push((name, outcome));
    }

    // Always leave the pad neutral.
    apply_and_verify(&mut pad, slot, XGamepad::default())?;

    // --- summary -----------------------------------------------------------
    let failures: Vec<&str> = results
        .iter()
        .filter(|(_, o)| !o.matched)
        .map(|(n, _)| n.as_str())
        .collect();
    let latencies: Vec<Duration> = results.iter().filter_map(|(_, o)| o.latency).collect();

    println!("\n---------------------------------------------------");
    println!(
        "{} / {} checks passed",
        results.len() - failures.len(),
        results.len()
    );

    if !latencies.is_empty() {
        let total: Duration = latencies.iter().sum();
        let avg = total / latencies.len() as u32;
        let max = latencies.iter().max().copied().unwrap_or_default();
        println!(
            "write -> observable via XInput: avg {:.2} ms, max {:.2} ms",
            avg.as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0
        );
    }

    if failures.is_empty() {
        println!("\nM1 GATE: PASS — Windows sees a real, fully working Xbox controller.");
        println!("Cross-check visually with:  joy.cpl");
        Ok(())
    } else {
        Err(format!(
            "controls did not read back correctly: {failures:?}"
        ))
    }
}

/// Write `want` to the virtual pad and poll XInput until Windows reports the
/// same state.
fn apply_and_verify(pad: &mut Pad, slot: u32, want: XGamepad) -> Result<Outcome, String> {
    let start = Instant::now();
    pad.update(&want)
        .map_err(|e| format!("update failed: {e:?}"))?;

    loop {
        if let Some(got) = read_slot(slot) {
            if got == want {
                return Ok(Outcome {
                    matched: true,
                    latency: Some(start.elapsed()),
                });
            }
        }
        if start.elapsed() > SETTLE_TIMEOUT {
            eprintln!("    wanted {want:?}");
            eprintln!("    got    {:?}", read_slot(slot));
            return Ok(Outcome {
                matched: false,
                latency: None,
            });
        }
        std::thread::sleep(Duration::from_micros(500));
    }
}

fn report(name: &str, outcome: &Outcome) {
    match outcome.latency {
        Some(d) if outcome.matched => {
            println!("  [ ok ] {name:<20} ({:.2} ms)", d.as_secs_f64() * 1000.0)
        }
        _ => println!("  [FAIL] {name}"),
    }
}

// --- XInput read-back --------------------------------------------------------

/// The public `XInputGetState` deliberately strips the Guide button (0x0400)
/// out of `wButtons`. The only way to observe it is `XInputGetStateEx`, which
/// Microsoft exports from xinput1_4.dll by ordinal 100 and never declared in a
/// header. We resolve it dynamically purely so the probe can *verify* Guide is
/// really reaching Windows; the product itself never needs it.
type XInputGetStateExFn = unsafe extern "system" fn(u32, *mut XINPUT_STATE) -> u32;

fn get_state_ex() -> Option<XInputGetStateExFn> {
    use std::sync::OnceLock;
    static CACHED: OnceLock<Option<usize>> = OnceLock::new();

    let addr = *CACHED.get_or_init(|| unsafe {
        let module = LoadLibraryA(c"xinput1_4.dll".to_bytes_with_nul().as_ptr());
        if module.is_null() {
            return None;
        }
        // Ordinal 100, passed where a name pointer would normally go.
        GetProcAddress(module, 100 as *const u8).map(|p| p as usize)
    });
    // SAFETY: the address came from GetProcAddress for a known XInput export
    // whose signature matches XInputGetStateExFn.
    addr.map(|a| unsafe { std::mem::transmute::<usize, XInputGetStateExFn>(a) })
}

fn read_slot(slot: u32) -> Option<XGamepad> {
    let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
    // SAFETY: `state` is a correctly sized, zero-initialised XINPUT_STATE and
    // both entry points only write into it.
    let rc = unsafe {
        match get_state_ex() {
            Some(f) => f(slot, &mut state),
            None => XInputGetState(slot, &mut state),
        }
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let g = state.Gamepad;
    Some(XGamepad {
        buttons: XButtons { raw: g.wButtons },
        left_trigger: g.bLeftTrigger,
        right_trigger: g.bRightTrigger,
        thumb_lx: g.sThumbLX,
        thumb_ly: g.sThumbLY,
        thumb_rx: g.sThumbRX,
        thumb_ry: g.sThumbRY,
    })
}

fn connected_slots() -> Vec<u32> {
    (0..4).filter(|&s| read_slot(s).is_some()).collect()
}

fn find_new_slot(before: &[u32]) -> Option<u32> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(slot) = connected_slots().into_iter().find(|s| !before.contains(s)) {
            return Some(slot);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

// --- read-only watch ---------------------------------------------------------

/// Report what XInput reports, changing nothing. Proves an *external* source is
/// genuinely driving the pad.
/// Ramp both motors on the first connected pad, then stop.
///
/// `XInputSetState` is the only way a game asks for rumble, so this is the
/// real path rather than an approximation of it.
fn rumble(seconds: u64) {
    let Some(slot) = connected_slots().first().copied() else {
        eprintln!("no XInput controller connected — start the companion and connect a phone");
        std::process::exit(1);
    };
    println!(
        "rumbling slot {slot} for {seconds}s
"
    );

    let started = Instant::now();
    let total = Duration::from_secs(seconds);
    while started.elapsed() < total {
        // A triangle over the run, so a phone on the other end should feel the
        // strength rise and fall rather than just switch on.
        let phase = started.elapsed().as_secs_f32() / total.as_secs_f32();
        let level = (1.0 - (phase * 2.0 - 1.0).abs()).clamp(0.0, 1.0);
        let large = (level * 65535.0) as u16;
        let small = (level * 40000.0) as u16;

        let v = XINPUT_VIBRATION {
            wLeftMotorSpeed: large,
            wRightMotorSpeed: small,
        };
        // SAFETY: `v` is a correctly initialised XINPUT_VIBRATION and `slot` is
        // a slot XInput just reported as connected.
        let rc = unsafe { XInputSetState(slot, &v) };
        if rc != 0 {
            eprintln!("XInputSetState failed with {rc}");
            std::process::exit(1);
        }
        print!("\r  large {large:>5}  small {small:>5}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_millis(100));
    }

    let off = XINPUT_VIBRATION {
        wLeftMotorSpeed: 0,
        wRightMotorSpeed: 0,
    };
    // SAFETY: as above.
    unsafe { XInputSetState(slot, &off) };
    println!("\r  stopped                          ");
}

fn watch(seconds: u64) {
    println!("Watching XInput slots for {seconds}s (read-only, nothing plugged in).\n");
    let start = Instant::now();
    let mut last: Option<XGamepad> = None;
    let mut changes = 0u64;
    let mut samples = 0u64;
    let mut distinct_buttons = std::collections::BTreeSet::new();
    let mut axis_extent: (i16, i16) = (0, 0);
    let mut trigger_max = 0u8;
    let mut slot_seen = None;

    while start.elapsed() < Duration::from_secs(seconds) {
        for s in 0..4u32 {
            if let Some(g) = read_slot(s) {
                slot_seen = Some(s);
                samples += 1;
                if last != Some(g) {
                    changes += 1;
                    last = Some(g);
                }
                if g.buttons.raw != 0 {
                    distinct_buttons.insert(g.buttons.raw);
                }
                axis_extent.0 = axis_extent.0.min(g.thumb_lx);
                axis_extent.1 = axis_extent.1.max(g.thumb_lx);
                trigger_max = trigger_max.max(g.left_trigger.max(g.right_trigger));
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }

    match slot_seen {
        None => println!("No controller present on any XInput slot."),
        Some(slot) => {
            println!("Slot {slot}: {samples} samples, {changes} distinct state changes");

            // Name the buttons rather than printing a bitmask: during an
            // acceptance run the question is "did LB actually arrive?", and a
            // hex number does not answer it.
            let mut seen_names: std::collections::BTreeSet<&str> = Default::default();
            for pattern in &distinct_buttons {
                for (mask, name) in BUTTON_NAMES {
                    if pattern & mask != 0 {
                        seen_names.insert(name);
                    }
                }
            }
            println!(
                "  buttons observed     : {}",
                if seen_names.is_empty() {
                    "none".to_string()
                } else {
                    seen_names.iter().copied().collect::<Vec<_>>().join(", ")
                }
            );
            println!(
                "  left stick X range   : {} .. {}",
                axis_extent.0, axis_extent.1
            );
            println!("  max trigger value    : {trigger_max}");

            let stick_moved = axis_extent.0 < -1000 || axis_extent.1 > 1000;
            println!(
                "\n{}",
                if changes > 5 {
                    format!(
                        "An external source is driving the pad. Buttons: {}. \
                         Sticks: {}. Triggers: {}.",
                        if seen_names.is_empty() { "no" } else { "yes" },
                        if stick_moved { "yes" } else { "no" },
                        if trigger_max > 0 { "yes" } else { "no" },
                    )
                } else {
                    "No meaningful movement observed.".to_string()
                }
            );
        }
    }
}

/// XInput `wButtons` bits, in the order a person would look for them.
const BUTTON_NAMES: [(u16, &str); 15] = [
    (XButtons::A, "A"),
    (XButtons::B, "B"),
    (XButtons::X, "X"),
    (XButtons::Y, "Y"),
    (XButtons::LB, "LB"),
    (XButtons::RB, "RB"),
    (XButtons::LTHUMB, "L3"),
    (XButtons::RTHUMB, "R3"),
    (XButtons::START, "Menu"),
    (XButtons::BACK, "View"),
    (XButtons::GUIDE, "Guide"),
    (XButtons::UP, "Dpad-Up"),
    (XButtons::DOWN, "Dpad-Down"),
    (XButtons::LEFT, "Dpad-Left"),
    (XButtons::RIGHT, "Dpad-Right"),
];

// --- visual sweep ------------------------------------------------------------

fn sweep_forever(pad: &mut Pad, slot: u32) -> Result<(), String> {
    println!("Sweeping continuously on slot {slot}. Open joy.cpl to watch. Ctrl+C to stop.");
    const BUTTONS: [u16; 14] = [
        XButtons::A,
        XButtons::B,
        XButtons::X,
        XButtons::Y,
        XButtons::LB,
        XButtons::RB,
        XButtons::UP,
        XButtons::RIGHT,
        XButtons::DOWN,
        XButtons::LEFT,
        XButtons::START,
        XButtons::BACK,
        XButtons::LTHUMB,
        XButtons::RTHUMB,
    ];

    let start = Instant::now();
    let mut frame: u64 = 0;
    loop {
        let t = start.elapsed().as_secs_f32();
        let (s, c) = (t * 1.5).sin_cos();
        let amp = i16::MAX as f32 * 0.95;
        // 0..1..0 triangle wave over two seconds, for the triggers.
        let tri = ((t * 0.5).fract() * 2.0 - 1.0).abs();

        let state = XGamepad {
            buttons: XButtons {
                raw: BUTTONS[(t as usize) % BUTTONS.len()],
            },
            left_trigger: (tri * 255.0) as u8,
            right_trigger: 255 - (tri * 255.0) as u8,
            thumb_lx: (c * amp) as i16,
            thumb_ly: (s * amp) as i16,
            thumb_rx: (s * amp) as i16,
            thumb_ry: (c * amp) as i16,
        };
        pad.update(&state)
            .map_err(|e| format!("update failed: {e:?}"))?;

        frame += 1;
        if frame % 250 == 0 {
            println!(
                "  {frame} updates sent; XInput reads back {:?}",
                read_slot(slot)
            );
        }
        std::thread::sleep(Duration::from_millis(4)); // ~250 Hz
    }
}
