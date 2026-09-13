//! End-to-end tests against a real running core over real UDP sockets.
//!
//! These drive the actual threads, sockets, pairing exchange and watchdog — the
//! parts that unit tests of pure functions cannot reach. The backend is `None`,
//! so no driver is needed; the pad itself is proven separately by
//! `pp-vigem-probe`.

use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use pp_core::server::{Core, DROP_AFTER, NEUTRAL_AFTER};
use pp_protocol::crypto::{ct_eq32, PairingKeys};
use pp_protocol::*;

/// Hand out an unused high port.
///
/// Ports come from one process-wide counter rather than from `bind(0)`. Asking
/// the OS for a free port means closing it again before `Core` can bind it, and
/// in that window a second harness running in parallel can be handed the very
/// same port — which shows up later as a mystery packet arriving at the wrong
/// test and failing a "zero rejected" assertion. A counter makes an
/// intra-process collision impossible.
///
/// The range stays well away from the real defaults so tests cannot disturb a
/// companion the user is actually running.
fn free_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    static BASE: OnceLock<u16> = OnceLock::new();

    let base = *BASE.get_or_init(|| 40_000 + (std::process::id() % 15_000) as u16);
    for _ in 0..2_000 {
        let offset = NEXT.fetch_add(1, Ordering::SeqCst);
        let port = 40_000 + ((base - 40_000 + offset) % 20_000);
        // Still confirm it is actually bindable, in case something outside this
        // process already holds it.
        if UdpSocket::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("no free port available");
}

struct Harness {
    core: Option<Core>,
    discovery: SocketAddr,
    input: SocketAddr,
    dir: PathBuf,
}

impl Harness {
    fn start() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pp-loopback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let discovery_port = free_port();
        let input_port = free_port();
        std::fs::write(
            &path,
            format!(
                r#"{{"server_id":"{}","discovery_port":{discovery_port},"input_port":{input_port},"backend":"none","devices":[]}}"#,
                "aa".repeat(16)
            ),
        )
        .unwrap();

        let core = Core::start_at(path);

        // Wait until both sockets are actually bound before any test sends.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let st = core.shared.status_snapshot();
            if st.discovery_bound.is_some() && st.input_bound.is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "core never bound its sockets");
            std::thread::sleep(Duration::from_millis(10));
        }

        Self {
            core: Some(core),
            discovery: format!("127.0.0.1:{discovery_port}").parse().unwrap(),
            input: format!("127.0.0.1:{input_port}").parse().unwrap(),
            dir,
        }
    }

    fn core(&self) -> &Core {
        self.core.as_ref().unwrap()
    }

    fn pairing_code(&self) -> String {
        self.core().shared.pairing.lock().unwrap().code.clone()
    }

    /// Block until `f` holds, or fail. Avoids sleeping for a fixed guessed
    /// duration and then hoping.
    fn wait_for(&self, what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for {what}");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// A minimal phone: enough protocol to exercise the server honestly.
struct FakePhone {
    sock: UdpSocket,
    device_id: DeviceId,
    token: Option<Token>,
    session_id: u32,
    session_key: SessionKey,
    seq: u32,
}

impl FakePhone {
    fn new(device_id: DeviceId) -> Self {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(1500)))
            .unwrap();
        Self {
            sock,
            device_id,
            token: None,
            session_id: 0,
            session_key: [0u8; 32],
            seq: 0,
        }
    }

    /// Send, then read until a datagram of the expected type turns up.
    ///
    /// This socket also receives FEEDBACK from any live session, so a real
    /// client has to demultiplex by message type rather than assume the next
    /// datagram is the reply it wanted. The fake phone does the same.
    fn round_trip(&self, to: SocketAddr, payload: &[u8], want: u8) -> Option<Vec<u8>> {
        self.sock.send_to(payload, to).ok()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut buf = [0u8; 512];
        while Instant::now() < deadline {
            let (n, _) = self.sock.recv_from(&mut buf).ok()?;
            if peek_type(&buf[..n]) == Some(want) {
                return Some(buf[..n].to_vec());
            }
        }
        None
    }

    fn discover(&self, to: SocketAddr) -> DiscoverResp {
        let req = DiscoverReq {
            device_id: self.device_id,
            nonce: 0x1234_5678,
            name: "Test Phone".into(),
        };
        let raw = self
            .round_trip(to, &encode_discover_req(&req), msg::DISCOVER_RESP)
            .expect("no discovery reply");
        decode_discover_resp(&raw).expect("malformed discovery reply")
    }

    /// Full X25519 + code exchange. Returns the status the PC ended on.
    fn pair(&mut self, to: SocketAddr, code: &str) -> PairStatus {
        let keys = PairingKeys::generate();
        let req = PairReq {
            device_id: self.device_id,
            client_pub: keys.public,
            name: "Test Phone".into(),
        };
        let raw = self
            .round_trip(to, &encode_pair_req(&req), msg::PAIR_RESP)
            .expect("no pair reply");
        let resp = decode_pair_resp(&raw).expect("malformed pair reply");
        if resp.status != PairStatus::Ok {
            return resp.status;
        }

        let secrets = keys.agree(&resp.server_pub, &self.device_id);

        // The phone verifies the PC knew the code before revealing anything.
        let expected = secrets.server_confirm(&keys.public, &resp.server_pub, code);
        if !ct_eq32(&expected, &resp.server_confirm) {
            return PairStatus::CodeMismatch;
        }

        let confirm = PairConfirm {
            device_id: self.device_id,
            client_confirm: secrets.client_confirm(&keys.public, &resp.server_pub, code),
        };
        let raw = self
            .round_trip(to, &encode_pair_confirm(&confirm), msg::PAIR_RESULT)
            .expect("no pair result");
        let status = decode_pair_result(&raw).expect("malformed pair result");
        if status == PairStatus::Ok {
            self.token = Some(secrets.token);
        }
        status
    }

    fn open_session(&mut self, to: SocketAddr) -> SessionStatus {
        let token = self.token.expect("must pair before opening a session");
        let client_nonce = [0x5c; 16];
        let req = SessionReq {
            device_id: self.device_id,
            client_nonce,
        };
        let mut out = [0u8; SESSION_REQ_LEN];
        encode_session_req(&req, &token, &mut out);

        let raw = self
            .round_trip(to, &out, msg::SESSION_RESP)
            .expect("no session reply");
        let resp = decode_session_resp(&raw, &token).expect("malformed session reply");
        if resp.status == SessionStatus::Ok {
            self.session_id = resp.session_id;
            self.session_key =
                crypto::derive_session_key(&token, &client_nonce, &resp.server_nonce);
            self.seq = 0;
        }
        resp.status
    }

    fn send_input(&mut self, to: SocketAddr, state: ControllerState) {
        self.seq += 1;
        self.send_input_seq(to, state, self.seq);
    }

    fn send_input_seq(&self, to: SocketAddr, state: ControllerState, seq: u32) {
        let p = InputPacket {
            session_id: self.session_id,
            seq,
            client_time_ms: 1000,
            rtt_us: 5000,
            flags: 0,
            state,
        };
        let mut out = [0u8; INPUT_LEN];
        encode_input(&p, &self.session_key, &mut out);
        self.sock.send_to(&out, to).unwrap();
    }
}

fn pressed(buttons: u16) -> ControllerState {
    ControllerState {
        buttons,
        lx: 12000,
        ..Default::default()
    }
}

// -----------------------------------------------------------------------------

#[test]
fn discovery_reports_an_unpaired_phone_and_open_pairing_mode() {
    let h = Harness::start();
    let phone = FakePhone::new([0x01; 16]);

    let resp = phone.discover(h.discovery);
    assert_eq!(resp.nonce, 0x1234_5678, "nonce must be echoed");
    assert_eq!(resp.input_port, h.input.port());
    assert!(!resp.already_paired);
    // A fresh install opens the pairing window automatically.
    assert!(resp.pairing_mode, "first run should be pairable");
    assert!(!resp.hostname.is_empty());
}

#[test]
fn pairing_then_input_reaches_the_core() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x02; 16]);
    let code = h.pairing_code();

    assert_eq!(phone.pair(h.discovery, &code), PairStatus::Ok);
    assert_eq!(phone.open_session(h.input), SessionStatus::Ok);

    for _ in 0..20 {
        phone.send_input(h.input, pressed(buttons::A));
        std::thread::sleep(Duration::from_millis(4));
    }

    h.wait_for("inputs to be accepted", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().accepted >= 20
    });

    let snap = h.core().shared.stats.snapshot();
    assert_eq!(
        snap.total_rejected(),
        0,
        "clean traffic must not be rejected"
    );
    assert!(snap.connected);
    assert_eq!(snap.rtt_us, 5000, "phone-reported RTT should surface");

    // And the device is now remembered.
    let after = phone.discover(h.discovery);
    assert!(after.already_paired);
}

#[test]
fn a_wrong_code_does_not_pair() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x03; 16]);
    let real = h.pairing_code();
    let wrong = format!("{:06}", (real.parse::<u32>().unwrap() + 1) % 1_000_000);

    assert_eq!(phone.pair(h.discovery, &wrong), PairStatus::CodeMismatch);
    assert!(phone.token.is_none());
    assert!(!phone.discover(h.discovery).already_paired);
}

#[test]
fn an_unpaired_phone_cannot_open_a_session() {
    let h = Harness::start();
    let phone = FakePhone::new([0x04; 16]);

    // Forge a session request with a token we invented.
    let bogus: Token = [0xEE; 32];
    let req = SessionReq {
        device_id: phone.device_id,
        client_nonce: [0; 16],
    };
    let mut out = [0u8; SESSION_REQ_LEN];
    encode_session_req(&req, &bogus, &mut out);
    phone.sock.send_to(&out, h.input).unwrap();

    // The server must not answer an unknown device at all.
    let mut buf = [0u8; 256];
    assert!(
        phone.sock.recv_from(&mut buf).is_err(),
        "server replied to an unpaired device"
    );
    assert!(!h.core().shared.stats.snapshot().connected);
}

#[test]
fn forged_input_is_rejected() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x05; 16]);
    let code = h.pairing_code();
    phone.pair(h.discovery, &code);
    phone.open_session(h.input);

    phone.send_input(h.input, pressed(buttons::A));
    h.wait_for("first packet", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().accepted >= 1
    });

    // Same session id, right shape, wrong key.
    let attacker = FakePhone {
        sock: UdpSocket::bind("127.0.0.1:0").unwrap(),
        device_id: [0xFF; 16],
        token: None,
        session_id: phone.session_id,
        session_key: [0x99; 32],
        seq: 0,
    };
    for seq in 100..110 {
        attacker.send_input_seq(h.input, pressed(buttons::B), seq);
    }

    h.wait_for("forgeries to be counted", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().rejected_mac >= 10
    });
    assert_eq!(h.core().shared.stats.snapshot().accepted, 1);
}

#[test]
fn replayed_packets_are_rejected() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x06; 16]);
    let code = h.pairing_code();
    phone.pair(h.discovery, &code);
    phone.open_session(h.input);

    for seq in 1..=10 {
        phone.send_input_seq(h.input, pressed(buttons::X), seq);
    }
    h.wait_for("originals", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().accepted >= 10
    });

    // Capture-and-resend the exact same packets.
    for seq in 1..=10 {
        phone.send_input_seq(h.input, pressed(buttons::X), seq);
    }
    h.wait_for("replays to be caught", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().rejected_replay >= 10
    });
    assert_eq!(h.core().shared.stats.snapshot().accepted, 10);
}

#[test]
fn malformed_datagrams_are_counted_not_swallowed() {
    let h = Harness::start();
    let phone = FakePhone::new([0x07; 16]);

    for junk in [
        &b""[..],
        &b"P"[..],
        &b"not a phonepad packet at all"[..],
        &[0x50, 99, 0x10, 0][..], // right magic, wrong version
        &[0x50, 1, 0xAB, 0][..],  // unknown type
    ] {
        phone.sock.send_to(junk, h.input).unwrap();
    }

    h.wait_for("junk to be counted", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().rejected_malformed >= 4
    });
}

#[test]
fn watchdog_neutralises_then_drops_a_dead_connection() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x08; 16]);
    let code = h.pairing_code();
    phone.pair(h.discovery, &code);
    phone.open_session(h.input);

    // Hold a button, then vanish — the worst-case failure this system can have.
    phone.send_input(h.input, pressed(buttons::A | buttons::RB));
    h.wait_for("input accepted", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().accepted >= 1
    });
    assert!(h.core().shared.stats.snapshot().connected);

    let died = Instant::now();
    h.wait_for("controls to be released", Duration::from_secs(1), || {
        h.core().shared.stats.snapshot().neutralised
    });
    let took = died.elapsed();
    assert!(
        took < NEUTRAL_AFTER + Duration::from_millis(150),
        "neutralised after {took:?}, expected close to {NEUTRAL_AFTER:?}"
    );
    assert!(h.core().shared.stats.snapshot().neutralisations >= 1);

    h.wait_for(
        "session to be dropped",
        DROP_AFTER + Duration::from_secs(2),
        || !h.core().shared.stats.snapshot().connected,
    );
    assert!(h.core().shared.stats.snapshot().sessions_dropped >= 1);
}

#[test]
fn input_resumes_cleanly_after_a_stall() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x09; 16]);
    let code = h.pairing_code();
    phone.pair(h.discovery, &code);
    phone.open_session(h.input);

    phone.send_input(h.input, pressed(buttons::A));
    h.wait_for("neutralised", Duration::from_secs(1), || {
        h.core().shared.stats.snapshot().neutralised
    });

    // A brief Wi-Fi hiccup must not require re-pairing or a new session.
    phone.send_input(h.input, pressed(buttons::B));
    h.wait_for("recovery", Duration::from_secs(1), || {
        !h.core().shared.stats.snapshot().neutralised
    });
    assert!(h.core().shared.stats.snapshot().connected);
}

#[test]
fn reconnecting_replaces_the_old_session() {
    let h = Harness::start();
    let mut phone = FakePhone::new([0x0A; 16]);
    let code = h.pairing_code();
    phone.pair(h.discovery, &code);

    assert_eq!(phone.open_session(h.input), SessionStatus::Ok);
    let first = phone.session_id;
    phone.send_input(h.input, pressed(buttons::A));

    assert_eq!(phone.open_session(h.input), SessionStatus::Ok);
    assert_ne!(phone.session_id, first, "each session needs a fresh id");

    // Sequence numbers restart, and the replay window must not block them.
    for seq in 1..=5 {
        phone.send_input_seq(h.input, pressed(buttons::Y), seq);
    }
    h.wait_for("post-reconnect input", Duration::from_secs(2), || {
        h.core().shared.stats.snapshot().accepted >= 6
    });
    assert_eq!(h.core().shared.stats.snapshot().rejected_replay, 0);
}

#[test]
fn pairing_is_refused_once_the_window_is_closed() {
    let h = Harness::start();
    let mut first = FakePhone::new([0x0B; 16]);
    let code = h.pairing_code();
    assert_eq!(first.pair(h.discovery, &code), PairStatus::Ok);

    // A successful pairing closes the window, so a second phone cannot slip in.
    let mut second = FakePhone::new([0x0C; 16]);
    assert_eq!(
        second.pair(h.discovery, &code),
        PairStatus::NotInPairingMode
    );

    assert!(!h.core().shared.pairing.lock().unwrap().active);
}

#[test]
fn shutdown_stops_the_threads() {
    let h = Harness::start();
    let shared = h.core().shared.clone();
    drop(h); // triggers Core::shutdown
    assert!(!shared.is_running());
    assert!(!shared.stats.gauges.connected.load(Relaxed));
}
