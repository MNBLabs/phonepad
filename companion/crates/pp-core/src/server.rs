//! The companion core: two UDP threads, one active session, one watchdog.
//!
//! Deliberately has no async runtime. The input path is a plain blocking socket
//! on a dedicated OS thread with a short read timeout, which keeps the hot path
//! to `recv_from -> verify -> decode -> apply` and gives the watchdog a
//! guaranteed tick even while packets are pouring in.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pp_protocol::crypto::{ct_eq32, generate_pairing_code, random_bytes, PairingKeys};
use pp_protocol::*;

use crate::backend::{self, BackendKind, VirtualPad};
use crate::log::{Level, Log};
use crate::stats::{RateTracker, Stats};
use crate::store::{self, Config};

/// Idle time after which every control is forced back to neutral. Short enough
/// that a dropped connection never reads as a held button, long enough to
/// survive a couple of missed packets at 120 Hz.
pub const NEUTRAL_AFTER: Duration = Duration::from_millis(120);
/// Idle time after which the session is abandoned entirely.
pub const DROP_AFTER: Duration = Duration::from_secs(2);
/// How often the PC tells the phone what it is seeing.
pub const FEEDBACK_EVERY: Duration = Duration::from_millis(50);
/// Pairing codes stop being accepted after this long.
pub const PAIRING_WINDOW: Duration = Duration::from_secs(120);

const INPUT_READ_TIMEOUT: Duration = Duration::from_millis(15);
const DISCOVERY_READ_TIMEOUT: Duration = Duration::from_millis(200);
const REL: Ordering = Ordering::Relaxed;

// --- shared state ------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub backend_kind: Option<BackendKind>,
    pub backend_label: String,
    pub backend_ready: bool,
    pub backend_error: Option<String>,
    pub user_index: Option<u32>,
    pub active_device_name: Option<String>,
    pub active_device_addr: Option<String>,
    pub discovery_bound: Option<String>,
    pub input_bound: Option<String>,
    pub bind_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairOutcome {
    Paired(String),
    Refused(String),
}

struct PendingPair {
    keys: PairingKeys,
    client_pub: [u8; 32],
    token: Token,
    name: String,
    expected_client_confirm: [u8; 32],
    expires: Instant,
}

#[derive(Default)]
pub struct PairingState {
    pub active: bool,
    pub code: String,
    pub expires_at: Option<Instant>,
    pub last_outcome: Option<PairOutcome>,
    pending: HashMap<DeviceId, PendingPair>,
}

impl PairingState {
    pub fn begin(&mut self) {
        self.active = true;
        self.code = generate_pairing_code();
        self.expires_at = Some(Instant::now() + PAIRING_WINDOW);
        self.last_outcome = None;
        self.pending.clear();
    }

    pub fn end(&mut self) {
        self.active = false;
        self.code.clear();
        self.expires_at = None;
        self.pending.clear();
    }

    /// Pairing mode expiring is normal, not an error.
    fn expire_if_due(&mut self, now: Instant) {
        if self.active && self.expires_at.is_some_and(|t| now >= t) {
            self.end();
        }
        self.pending.retain(|_, p| now < p.expires);
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.expires_at
            .map(|t| t.saturating_duration_since(Instant::now()))
    }
}

pub struct Shared {
    /// Where `config` is persisted. Injectable so tests never touch the real
    /// `%APPDATA%` file.
    pub config_path: std::path::PathBuf,
    pub config: Mutex<Config>,
    pub stats: Stats,
    pub status: Mutex<Status>,
    pub log: Mutex<Log>,
    pub pairing: Mutex<PairingState>,
    /// Optional CSV capture of accepted input, for tuning the phone's controls.
    pub trace: crate::trace::Trace,
    running: AtomicBool,
}

impl Shared {
    pub fn log(&self, level: Level, text: impl Into<String>) {
        if let Ok(mut l) = self.log.lock() {
            l.push(level, text);
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn set_status(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    pub fn status_snapshot(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Persist the current config, reporting failures to the log rather than
    /// dropping them.
    fn persist(&self, cfg: &Config) {
        if let Err(e) = store::save_to(&self.config_path, cfg) {
            self.log(Level::Warn, format!("could not save config: {e}"));
        }
    }
}

#[derive(Debug)]
pub enum Command {
    SetBackend(BackendKind),
    DisconnectSession,
    /// Unplug the virtual pad and plug it straight back in.
    ///
    /// Some clients bind a controller to a session only when they observe it
    /// *arrive*. A browser page, for instance, gets `gamepadconnected` on
    /// arrival and nothing at all for a pad that was already present — so a
    /// cloud-gaming stream started before the pad existed can end up with no
    /// controller attached to the game, even though the page itself can read
    /// the pad perfectly well. Re-attaching produces a real arrival event
    /// without touching the phone or the network session.
    ReattachPad,
    Shutdown,
}

// --- the running core --------------------------------------------------------

pub struct Core {
    pub shared: Arc<Shared>,
    cmd_tx: Sender<Command>,
    threads: Vec<JoinHandle<()>>,
}

impl Core {
    pub fn start() -> Self {
        Self::start_at(store::config_path())
    }

    /// Start with an explicit config file. Tests use this with a temporary path
    /// and non-default ports so they never collide with a running companion.
    pub fn start_at(config_path: std::path::PathBuf) -> Self {
        let (config, warning) = store::load_from(&config_path);
        let backend_kind = config.backend;
        let discovery_port = config.discovery_port;
        let input_port = config.input_port;
        let start_paired = !config.devices.is_empty();

        let shared = Arc::new(Shared {
            config: Mutex::new(config),
            config_path: config_path.clone(),
            stats: Stats::default(),
            status: Mutex::new(Status::default()),
            log: Mutex::new(Log::default()),
            pairing: Mutex::new(PairingState::default()),
            trace: crate::trace::Trace::default(),
            running: AtomicBool::new(true),
        });

        if let Some(w) = warning {
            shared.log(Level::Warn, w);
        }
        shared.log(Level::Info, format!("config: {}", config_path.display()));

        // A phone can only connect once it is paired, so on a fresh install
        // open the pairing window immediately rather than making the first run
        // a scavenger hunt.
        if !start_paired {
            if let Ok(mut p) = shared.pairing.lock() {
                p.begin();
                shared.log(
                    Level::Info,
                    format!("no paired phones yet — pairing code {}", p.code),
                );
            }
        }

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let mut threads = Vec::new();

        threads.push({
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("pp-input".into())
                .spawn(move || input_thread(shared, cmd_rx, input_port, backend_kind))
                .expect("spawn input thread")
        });

        threads.push({
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("pp-discovery".into())
                .spawn(move || discovery_thread(shared, discovery_port))
                .expect("spawn discovery thread")
        });

        Self {
            shared,
            cmd_tx,
            threads,
        }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn shutdown(self) {
        self.shared.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(Command::Shutdown);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

// --- input thread ------------------------------------------------------------

struct Session {
    id: u32,
    key: SessionKey,
    device_id: DeviceId,
    name: String,
    addr: SocketAddr,
    replay: ReplayWindow,
    rates: RateTracker,
    last_packet: Instant,
    last_client_time_ms: u32,
    neutralised: bool,
    /// Highest control sequence accepted. Counted separately from input
    /// sequences so the two streams cannot invalidate each other.
    last_control_seq: u32,
}

fn bind_udp(port: u16, timeout: Duration) -> Result<UdpSocket, String> {
    let sock = UdpSocket::bind(("0.0.0.0", port)).map_err(|e| {
        format!(
            "cannot bind UDP port {port}: {e}. Another program may be using it, \
             or Windows Firewall may be blocking it."
        )
    })?;
    sock.set_read_timeout(Some(timeout))
        .map_err(|e| format!("cannot set read timeout on port {port}: {e}"))?;
    sock.set_broadcast(true)
        .map_err(|e| format!("cannot enable broadcast on port {port}: {e}"))?;
    Ok(sock)
}

fn install_backend(shared: &Shared, kind: BackendKind) -> Option<Box<dyn VirtualPad>> {
    match backend::create(kind) {
        Ok(mut pad) => {
            let index = pad.user_index();
            shared.set_status(|s| {
                s.backend_kind = Some(kind);
                s.backend_label = kind.label().to_string();
                s.backend_ready = true;
                s.backend_error = None;
                s.user_index = index;
            });
            match index {
                Some(i) => shared.log(
                    Level::Info,
                    format!("{} ready on XInput slot {i}", kind.label()),
                ),
                None => shared.log(Level::Info, format!("{} ready", kind.label())),
            }
            Some(pad)
        }
        Err(e) => {
            shared.set_status(|s| {
                s.backend_kind = Some(kind);
                s.backend_label = kind.label().to_string();
                s.backend_ready = false;
                s.backend_error = Some(e.clone());
                s.user_index = None;
            });
            shared.log(Level::Error, e);
            None
        }
    }
}

fn input_thread(
    shared: Arc<Shared>,
    cmd_rx: Receiver<Command>,
    port: u16,
    initial_backend: BackendKind,
) {
    let sock = match bind_udp(port, INPUT_READ_TIMEOUT) {
        Ok(s) => {
            let bound = s.local_addr().map(|a| a.to_string()).ok();
            shared.set_status(|st| st.input_bound = bound.clone());
            shared.log(
                Level::Info,
                format!("input socket listening on {}", bound.unwrap_or_default()),
            );
            s
        }
        Err(e) => {
            shared.set_status(|st| st.bind_error = Some(e.clone()));
            shared.log(Level::Error, e);
            return;
        }
    };

    let mut pad = install_backend(&shared, initial_backend);

    // Set while the pad is deliberately unplugged mid-re-attach; the loop plugs
    // it back in when this passes. See `begin_reattach`.
    let mut reattach_at: Option<Instant> = None;
    let mut cloud = crate::cloud::CloudWatch::default();
    let mut session: Option<Session> = None;
    let mut buf = [0u8; 256];
    let mut last_feedback = Instant::now();

    while shared.is_running() {
        // --- commands from the UI -----------------------------------------
        loop {
            match cmd_rx.try_recv() {
                Ok(Command::Shutdown) => {
                    finish_session(&shared, &mut session, &mut pad, "shutting down");
                    return;
                }
                Ok(Command::DisconnectSession) => {
                    finish_session(&shared, &mut session, &mut pad, "disconnected from the PC");
                }
                Ok(Command::ReattachPad) => {
                    reattach_at = begin_reattach(&shared, &mut pad, "companion window");
                }
                Ok(Command::SetBackend(kind)) => {
                    finish_session(&shared, &mut session, &mut pad, "backend changed");
                    // Drop the old one first: two ViGEm targets would otherwise
                    // briefly coexist and Windows would show two controllers.
                    drop(pad.take());
                    pad = install_backend(&shared, kind);
                    if let Ok(mut c) = shared.config.lock() {
                        c.backend = kind;
                        shared.persist(&c);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        // --- a cloud client just started ------------------------------------
        // Only while a phone is actually connected: re-attaching a pad nobody
        // is driving achieves nothing and only risks disturbing another one.
        if session.is_some() {
            let want = shared
                .config
                .lock()
                .map(|c| c.auto_reattach)
                .unwrap_or(false);
            if cloud.poll(Instant::now(), want) {
                let who = cloud.any_seen().unwrap_or("a cloud client").to_string();
                reattach_at = begin_reattach(&shared, &mut pad, &format!("{who} starting"));
            }
        }

        // --- finish a re-attach whose gap has elapsed ------------------------
        if let Some(at) = reattach_at {
            if Instant::now() >= at {
                reattach_at = None;
                finish_reattach(&shared, &mut pad, &mut session);
            }
        }

        // --- receive --------------------------------------------------------
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => handle_input_datagram(
                &shared,
                &sock,
                &buf[..n],
                from,
                &mut session,
                &mut pad,
                &mut reattach_at,
            ),
            Err(e) if is_timeout(&e) => {}
            Err(e) => shared.log(Level::Warn, format!("input socket recv failed: {e}")),
        }

        let now = Instant::now();

        // --- watchdog: this runs every iteration, including while packets are
        // flowing, so a stall is caught within one loop rather than one timeout.
        if let Some(s) = session.as_mut() {
            let idle = now.duration_since(s.last_packet);
            if idle > DROP_AFTER {
                let name = s.name.clone();
                finish_session(
                    &shared,
                    &mut session,
                    &mut pad,
                    &format!("no packets from {name} for {:.1}s", idle.as_secs_f32()),
                );
                shared.stats.counters.sessions_dropped.fetch_add(1, REL);
            } else if idle > NEUTRAL_AFTER && !s.neutralised {
                s.neutralised = true;
                shared.stats.gauges.neutralised.store(true, REL);
                shared.stats.counters.neutralisations.fetch_add(1, REL);
                neutralise(&shared, &mut pad);
                shared.log(
                    Level::Warn,
                    format!(
                        "input stalled ({} ms) — controls released",
                        idle.as_millis()
                    ),
                );
            }
        }

        // --- periodic feedback to the phone ---------------------------------
        if now.duration_since(last_feedback) >= FEEDBACK_EVERY {
            last_feedback = now;
            if let Some(s) = session.as_mut() {
                s.rates.tick(now, &shared.stats.gauges);
                let rumble = pad.as_mut().and_then(|p| p.poll_rumble()).unwrap_or((0, 0));
                let fb = FeedbackPacket {
                    session_id: s.id,
                    echo_client_time_ms: s.last_client_time_ms,
                    rumble_large: rumble.0,
                    rumble_small: rumble.1,
                    accepted_pps: shared.stats.gauges.pps.load(REL).min(u16::MAX as u32) as u16,
                    loss_permille: shared
                        .stats
                        .gauges
                        .loss_permille
                        .load(REL)
                        .min(u16::MAX as u32) as u16,
                };
                let mut out = [0u8; FEEDBACK_LEN];
                encode_feedback(&fb, &s.key, &mut out);
                if let Err(e) = sock.send_to(&out, s.addr) {
                    shared.log(Level::Warn, format!("could not send feedback: {e}"));
                }
            }
        }
    }

    finish_session(&shared, &mut session, &mut pad, "core stopped");
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
}

/// How long the pad stays unplugged during a re-attach.
///
/// The gap has to be real: without it the host coalesces the removal and the
/// arrival into no change at all, and the arrival event is the entire point.
const REATTACH_GAP: Duration = Duration::from_millis(400);

/// Unplug the virtual pad, and say when to plug it back in.
///
/// Split in two rather than sleeping through the gap. This runs on the input
/// thread, so sleeping here would stop the socket being drained and stop the
/// watchdog ticking for the whole 400 ms — during which a phone at 250 Hz
/// sends a hundred packets into a socket buffer nobody is reading.
///
/// Reachable from the companion window and from the phone, because the moment
/// it is most needed is mid-game with the companion behind a fullscreen window.
fn begin_reattach(
    shared: &Shared,
    pad: &mut Option<Box<dyn VirtualPad>>,
    source: &str,
) -> Option<Instant> {
    shared.log(
        Level::Info,
        format!("re-attaching the virtual controller (requested from {source})"),
    );
    drop(pad.take());
    Some(Instant::now() + REATTACH_GAP)
}

fn finish_reattach(
    shared: &Shared,
    pad: &mut Option<Box<dyn VirtualPad>>,
    session: &mut Option<Session>,
) {
    let kind = shared
        .status_snapshot()
        .backend_kind
        .unwrap_or(BackendKind::Xbox360);
    *pad = install_backend(shared, kind);

    // The gap was our doing, so don't let the watchdog report it as the phone
    // stalling.
    if let Some(s) = session.as_mut() {
        s.last_packet = Instant::now();
        s.neutralised = false;
    }
    shared.stats.gauges.neutralised.store(false, REL);
}

fn neutralise(shared: &Shared, pad: &mut Option<Box<dyn VirtualPad>>) {
    if let Some(p) = pad.as_mut() {
        if let Err(e) = p.neutralise() {
            shared.stats.counters.backend_errors.fetch_add(1, REL);
            shared.log(Level::Error, format!("could not neutralise the pad: {e}"));
        }
    }
}

/// End a session and put the controller back to neutral. Every exit path from a
/// session goes through here so there is exactly one place that can forget to
/// release the controls.
fn finish_session(
    shared: &Shared,
    session: &mut Option<Session>,
    pad: &mut Option<Box<dyn VirtualPad>>,
    reason: &str,
) {
    if let Some(s) = session.take() {
        shared.log(
            Level::Info,
            format!("session with {} ended: {reason}", s.name),
        );
        // Record when we last actually heard from this phone, not just when it
        // connected, so the device list shows something meaningful.
        if let Ok(mut cfg) = shared.config.lock() {
            cfg.touch_device(&s.device_id, crate::now_iso());
            shared.persist(&cfg);
        }
    }
    neutralise(shared, pad);
    shared.stats.gauges.connected.store(false, REL);
    shared.stats.gauges.neutralised.store(false, REL);
    shared.stats.gauges.pps.store(0, REL);
    shared.stats.gauges.rtt_us.store(0, REL);
    shared.set_status(|st| {
        st.active_device_name = None;
        st.active_device_addr = None;
    });
}

fn handle_input_datagram(
    shared: &Shared,
    sock: &UdpSocket,
    buf: &[u8],
    from: SocketAddr,
    session: &mut Option<Session>,
    pad: &mut Option<Box<dyn VirtualPad>>,
    reattach_at: &mut Option<Instant>,
) {
    let Some(msg_type) = peek_type(buf) else {
        shared.stats.counters.rejected_malformed.fetch_add(1, REL);
        return;
    };

    match msg_type {
        msg::INPUT => handle_input(shared, buf, from, session, pad),
        msg::SESSION_REQ => handle_session_req(shared, sock, buf, from, session, pad),
        msg::BYE => {
            if let Some(s) = session.as_ref() {
                if decode_bye(buf, &s.key).is_ok_and(|id| id == s.id) {
                    finish_session(shared, session, pad, "phone said goodbye");
                } else {
                    shared.stats.counters.rejected_mac.fetch_add(1, REL);
                }
            }
        }
        msg::CONTROL => handle_control(shared, buf, session, pad, reattach_at),
        _ => {
            shared.stats.counters.rejected_malformed.fetch_add(1, REL);
        }
    }
}

/// Out-of-band requests from the phone: re-attach the pad, or release
/// everything. Authenticated with the same session key as input, so these are
/// no easier to forge than a button press.
fn handle_control(
    shared: &Shared,
    buf: &[u8],
    session: &mut Option<Session>,
    pad: &mut Option<Box<dyn VirtualPad>>,
    reattach_at: &mut Option<Instant>,
) {
    let Some(s) = session.as_mut() else {
        shared
            .stats
            .counters
            .rejected_unknown_session
            .fetch_add(1, REL);
        return;
    };

    let packet = match decode_control(buf, &s.key) {
        Ok(p) => p,
        Err(DecodeError::BadMac) => {
            shared.stats.counters.rejected_mac.fetch_add(1, REL);
            return;
        }
        Err(DecodeError::BadField(_)) => {
            shared.stats.counters.rejected_field.fetch_add(1, REL);
            return;
        }
        Err(_) => {
            shared.stats.counters.rejected_malformed.fetch_add(1, REL);
            return;
        }
    };

    if packet.session_id != s.id {
        shared
            .stats
            .counters
            .rejected_unknown_session
            .fetch_add(1, REL);
        return;
    }

    // Strictly increasing. The phone re-sends control messages a few times to
    // survive loss, so duplicates are expected and must be ignored rather than
    // acted on twice — re-attaching four times over would be disruptive.
    if packet.control_seq <= s.last_control_seq {
        shared.stats.counters.rejected_replay.fetch_add(1, REL);
        return;
    }
    s.last_control_seq = packet.control_seq;

    match packet.command {
        ControlCommand::ReattachPad => *reattach_at = begin_reattach(shared, pad, "the phone"),
        ControlCommand::ReleaseAll => {
            shared.log(Level::Info, "phone requested a full release");
            if let Some(s) = session.as_mut() {
                s.neutralised = true;
            }
            shared.stats.gauges.neutralised.store(true, REL);
            shared.stats.counters.neutralisations.fetch_add(1, REL);
            neutralise(shared, pad);
        }
    }
}

fn handle_input(
    shared: &Shared,
    buf: &[u8],
    from: SocketAddr,
    session: &mut Option<Session>,
    pad: &mut Option<Box<dyn VirtualPad>>,
) {
    let Some(s) = session.as_mut() else {
        shared
            .stats
            .counters
            .rejected_unknown_session
            .fetch_add(1, REL);
        return;
    };

    if peek_session_id(buf) != Some(s.id) {
        shared
            .stats
            .counters
            .rejected_unknown_session
            .fetch_add(1, REL);
        return;
    }

    let packet = match decode_input(buf, &s.key) {
        Ok(p) => p,
        Err(DecodeError::BadMac) => {
            shared.stats.counters.rejected_mac.fetch_add(1, REL);
            return;
        }
        Err(DecodeError::BadField(_)) => {
            shared.stats.counters.rejected_field.fetch_add(1, REL);
            return;
        }
        Err(_) => {
            shared.stats.counters.rejected_malformed.fetch_add(1, REL);
            return;
        }
    };

    if !s.replay.accept(packet.seq) {
        shared.stats.counters.rejected_replay.fetch_add(1, REL);
        return;
    }

    // The MAC already proved this came from the paired phone, so a changed
    // source address just means the phone moved networks. Follow it rather than
    // forcing a reconnect.
    if from != s.addr {
        shared.log(
            Level::Info,
            format!("{} moved from {} to {from}", s.name, s.addr),
        );
        s.addr = from;
    }

    let now = Instant::now();
    s.last_packet = now;
    s.last_client_time_ms = packet.client_time_ms;
    s.rates.record(packet.seq, now);

    if s.neutralised {
        s.neutralised = false;
        shared.stats.gauges.neutralised.store(false, REL);
        shared.log(Level::Info, "input resumed");
    }

    shared.stats.counters.accepted.fetch_add(1, REL);
    shared.stats.gauges.rtt_us.store(packet.rtt_us, REL);
    shared
        .trace
        .record(packet.seq, packet.rtt_us, &packet.state);

    if let Some(p) = pad.as_mut() {
        if let Err(e) = p.apply(&packet.state) {
            shared.stats.counters.backend_errors.fetch_add(1, REL);
            shared.log(Level::Error, e);
        }
    }
}

fn handle_session_req(
    shared: &Shared,
    sock: &UdpSocket,
    buf: &[u8],
    from: SocketAddr,
    session: &mut Option<Session>,
    pad: &mut Option<Box<dyn VirtualPad>>,
) {
    let device_id = match peek_session_req_device(buf) {
        Ok(id) => id,
        Err(_) => {
            shared.stats.counters.rejected_malformed.fetch_add(1, REL);
            return;
        }
    };

    let (token, name) = {
        let Ok(cfg) = shared.config.lock() else {
            return;
        };
        (
            cfg.token_for(&device_id),
            cfg.name_for(&device_id)
                .unwrap_or("unknown phone")
                .to_string(),
        )
    };

    let Some(token) = token else {
        shared.log(
            Level::Warn,
            format!("{from} tried to connect but is not paired"),
        );
        // No token means no key to sign a reply with, so the phone learns this
        // from the discovery response's `alreadyPaired` flag instead.
        shared
            .stats
            .counters
            .rejected_unknown_session
            .fetch_add(1, REL);
        return;
    };

    let req = match decode_session_req(buf, &token) {
        Ok(r) => r,
        Err(_) => {
            shared.stats.counters.rejected_mac.fetch_add(1, REL);
            shared.log(
                Level::Warn,
                format!("session request from {from} failed authentication"),
            );
            return;
        }
    };

    let status = if pad.as_ref().is_some() {
        SessionStatus::Ok
    } else {
        SessionStatus::NoBackend
    };

    let server_nonce = random_bytes::<16>();
    let session_id = u32::from_le_bytes(random_bytes::<4>()).max(1);

    let resp = SessionResp {
        status,
        session_id,
        server_nonce,
    };
    let mut out = [0u8; SESSION_RESP_LEN];
    encode_session_resp(&resp, &token, &mut out);
    if let Err(e) = sock.send_to(&out, from) {
        shared.log(
            Level::Warn,
            format!("could not reply to session request: {e}"),
        );
        return;
    }

    if status != SessionStatus::Ok {
        shared.log(
            Level::Warn,
            format!("refused {name}: no virtual controller backend is available"),
        );
        return;
    }

    // Replacing an existing session must release the old one's controls first.
    if session.is_some() {
        finish_session(shared, session, pad, "replaced by a new session");
    }

    let key = crypto::derive_session_key(&token, &req.client_nonce, &server_nonce);
    *session = Some(Session {
        id: session_id,
        key,
        device_id,
        name: name.clone(),
        addr: from,
        replay: ReplayWindow::new(),
        rates: RateTracker::new(),
        last_packet: Instant::now(),
        last_client_time_ms: 0,
        neutralised: false,
        last_control_seq: 0,
    });

    shared.stats.counters.sessions_started.fetch_add(1, REL);
    shared.stats.gauges.connected.store(true, REL);
    shared.set_status(|st| {
        st.active_device_name = Some(name.clone());
        st.active_device_addr = Some(from.to_string());
    });
    shared.log(Level::Info, format!("{name} connected from {from}"));

    if let Ok(mut cfg) = shared.config.lock() {
        cfg.touch_device(&device_id, crate::now_iso());
        shared.persist(&cfg);
    }
}

// --- discovery + pairing thread ----------------------------------------------

fn discovery_thread(shared: Arc<Shared>, port: u16) {
    let sock = match bind_udp(port, DISCOVERY_READ_TIMEOUT) {
        Ok(s) => {
            let bound = s.local_addr().map(|a| a.to_string()).ok();
            shared.set_status(|st| st.discovery_bound = bound.clone());
            shared.log(
                Level::Info,
                format!("discovery listening on {}", bound.unwrap_or_default()),
            );
            s
        }
        Err(e) => {
            shared.set_status(|st| st.bind_error = Some(e.clone()));
            shared.log(Level::Error, e);
            return;
        }
    };

    let mut buf = [0u8; 512];

    while shared.is_running() {
        if let Ok(mut p) = shared.pairing.lock() {
            p.expire_if_due(Instant::now());
        }

        let (n, from) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e) if is_timeout(&e) => continue,
            Err(e) => {
                shared.log(Level::Warn, format!("discovery recv failed: {e}"));
                continue;
            }
        };

        match peek_type(&buf[..n]) {
            Some(msg::DISCOVER_REQ) => {
                let hostname = advertised_name(&shared);
                on_discover(&shared, &sock, &buf[..n], from, &hostname)
            }
            Some(msg::PAIR_REQ) => on_pair_req(&shared, &sock, &buf[..n], from),
            Some(msg::PAIR_CONFIRM) => on_pair_confirm(&shared, &sock, &buf[..n], from),
            _ => {
                shared.stats.counters.rejected_malformed.fetch_add(1, REL);
            }
        }
    }
}

/// What the phone lists this PC as. A configured name wins; otherwise the
/// computer name, which is what people recognise their own machine by.
fn advertised_name(shared: &Shared) -> String {
    let configured = shared
        .config
        .lock()
        .ok()
        .map(|c| c.name.trim().to_string())
        .unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".to_string())
}

fn on_discover(shared: &Shared, sock: &UdpSocket, buf: &[u8], from: SocketAddr, hostname: &str) {
    let Ok(req) = decode_discover_req(buf) else {
        shared.stats.counters.rejected_malformed.fetch_add(1, REL);
        return;
    };

    let (server_id, input_port, already_paired) = {
        let Ok(cfg) = shared.config.lock() else {
            return;
        };
        (
            cfg.server_id_bytes(),
            cfg.input_port,
            cfg.is_paired(&req.device_id),
        )
    };
    let pairing_mode = shared.pairing.lock().map(|p| p.active).unwrap_or(false);
    let backend = shared.status_snapshot().backend_label;

    let resp = DiscoverResp {
        nonce: req.nonce,
        server_id,
        input_port,
        already_paired,
        pairing_mode,
        hostname: hostname.to_string(),
        backend,
    };

    if let Err(e) = sock.send_to(&encode_discover_resp(&resp), from) {
        shared.log(Level::Warn, format!("could not answer discovery: {e}"));
    }
}

fn on_pair_req(shared: &Shared, sock: &UdpSocket, buf: &[u8], from: SocketAddr) {
    let Ok(req) = decode_pair_req(buf) else {
        shared.stats.counters.rejected_malformed.fetch_add(1, REL);
        return;
    };

    let Ok(mut pairing) = shared.pairing.lock() else {
        return;
    };

    if !pairing.active {
        let _ = sock.send_to(
            &encode_pair_resp(&PairResp {
                status: PairStatus::NotInPairingMode,
                server_pub: [0u8; 32],
                server_confirm: [0u8; 32],
            }),
            from,
        );
        shared.log(
            Level::Warn,
            format!("{} tried to pair but pairing mode is off", req.name),
        );
        return;
    }

    let keys = PairingKeys::generate();
    let secrets = keys.agree(&req.client_pub, &req.device_id);
    let server_confirm = secrets.server_confirm(&req.client_pub, &keys.public, &pairing.code);
    let expected_client_confirm =
        secrets.client_confirm(&req.client_pub, &keys.public, &pairing.code);

    let resp = PairResp {
        status: PairStatus::Ok,
        server_pub: keys.public,
        server_confirm,
    };

    pairing.pending.insert(
        req.device_id,
        PendingPair {
            client_pub: req.client_pub,
            token: secrets.token,
            name: req.name.clone(),
            expected_client_confirm,
            expires: Instant::now() + PAIRING_WINDOW,
            keys,
        },
    );
    drop(pairing);

    if let Err(e) = sock.send_to(&encode_pair_resp(&resp), from) {
        shared.log(Level::Warn, format!("could not answer pair request: {e}"));
        return;
    }
    shared.log(Level::Info, format!("pairing started with {}", req.name));
}

fn on_pair_confirm(shared: &Shared, sock: &UdpSocket, buf: &[u8], from: SocketAddr) {
    let Ok(conf) = decode_pair_confirm(buf) else {
        shared.stats.counters.rejected_malformed.fetch_add(1, REL);
        return;
    };

    let Ok(mut pairing) = shared.pairing.lock() else {
        return;
    };
    let Some(pending) = pairing.pending.remove(&conf.device_id) else {
        let _ = sock.send_to(&encode_pair_result(PairStatus::Rejected), from);
        return;
    };

    if !ct_eq32(&pending.expected_client_confirm, &conf.client_confirm) {
        // Wrong code typed. Keep pairing mode open so the user can retry, but
        // discard this attempt.
        pairing.last_outcome = Some(PairOutcome::Refused(pending.name.clone()));
        drop(pairing);
        let _ = sock.send_to(&encode_pair_result(PairStatus::CodeMismatch), from);
        shared.log(
            Level::Warn,
            format!("{} entered the wrong pairing code", pending.name),
        );
        return;
    }

    pairing.last_outcome = Some(PairOutcome::Paired(pending.name.clone()));
    pairing.end();
    drop(pairing);

    if let Ok(mut cfg) = shared.config.lock() {
        cfg.upsert_device(&conf.device_id, &pending.name, &pending.token);
        shared.persist(&cfg);
    }

    let _ = sock.send_to(&encode_pair_result(PairStatus::Ok), from);
    shared.log(Level::Info, format!("paired with {}", pending.name));

    // `keys` and `client_pub` have done their job; naming them here documents
    // that they are intentionally dropped rather than forgotten.
    let _ = (pending.keys, pending.client_pub);
}
