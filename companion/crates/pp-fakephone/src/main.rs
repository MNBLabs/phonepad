//! A simulated phone, so the whole companion — discovery, pairing, session,
//! watchdog and the real virtual pad — can be exercised without an Android
//! device in the loop.
//!
//!   cargo run -p pp-fakephone                     # discover, pair if needed, sweep
//!   cargo run -p pp-fakephone -- --host 127.0.0.1 --code 123456
//!   cargo run -p pp-fakephone -- --rate 250 --seconds 30
//!   cargo run -p pp-fakephone -- --die-after 5    # stop sending, to watch the watchdog
//!
//! The pairing token is cached in the system temp dir so repeated runs do not
//! need a fresh code.

use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use pp_protocol::crypto::{ct_eq32, PairingKeys};
use pp_protocol::*;

struct Args {
    host: String,
    code: Option<String>,
    rate: u32,
    seconds: u64,
    die_after: Option<u64>,
}

fn parse_args() -> Args {
    let mut a = Args {
        host: "255.255.255.255".to_string(),
        code: None,
        rate: 250,
        seconds: 20,
        die_after: None,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let next = |i: usize| argv.get(i + 1).cloned().unwrap_or_default();
        match argv[i].as_str() {
            "--host" => {
                a.host = next(i);
                i += 1;
            }
            "--code" => {
                a.code = Some(next(i));
                i += 1;
            }
            "--rate" => {
                a.rate = next(i).parse().unwrap_or(250);
                i += 1;
            }
            "--seconds" => {
                a.seconds = next(i).parse().unwrap_or(20);
                i += 1;
            }
            "--die-after" => {
                a.die_after = next(i).parse().ok();
                i += 1;
            }
            "--help" | "-h" => {
                println!(
                    "pp-fakephone — drive a PhonePad companion without an Android device\n\n\
                     --host <ip>        companion address (default: broadcast)\n\
                     --code <123456>    pairing code, if not already paired\n\
                     --rate <hz>        input packets per second (default 250)\n\
                     --seconds <n>      how long to run (default 20)\n\
                     --die-after <n>    go silent after n seconds, to watch the watchdog"
                );
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument {other}"),
        }
        i += 1;
    }
    a
}

const DEVICE_ID: DeviceId = *b"pp-fakephone-01\0";

fn token_cache() -> std::path::PathBuf {
    std::env::temp_dir().join("phonepad-fakephone-token.hex")
}

fn load_token() -> Option<Token> {
    let text = std::fs::read_to_string(token_cache()).ok()?;
    let text = text.trim();
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn save_token(t: &Token) {
    let hex: String = t.iter().map(|b| format!("{b:02x}")).collect();
    if let Err(e) = std::fs::write(token_cache(), hex) {
        eprintln!("warning: could not cache the pairing token: {e}");
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("\nFAILED: {e}");
        std::process::exit(1);
    }
}

fn recv_typed(sock: &UdpSocket, want: u8, timeout: Duration) -> Option<(Vec<u8>, SocketAddr)> {
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        sock.set_read_timeout(Some(deadline.saturating_duration_since(Instant::now())))
            .ok()?;
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if peek_type(&buf[..n]) == Some(want) => {
                return Some((buf[..n].to_vec(), from))
            }
            Ok(_) => continue, // some other message on this socket; keep looking
            Err(_) => return None,
        }
    }
    None
}

fn run() -> Result<(), String> {
    let args = parse_args();

    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("cannot open a socket: {e}"))?;
    sock.set_broadcast(true)
        .map_err(|e| format!("cannot enable broadcast: {e}"))?;

    // --- discovery -----------------------------------------------------------
    let target: SocketAddr = (args.host.as_str(), DISCOVERY_PORT)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {}: {e}", args.host))?
        .next()
        .ok_or_else(|| format!("no address for {}", args.host))?;

    println!("Looking for a PhonePad companion via {target}...");
    let req = DiscoverReq {
        device_id: DEVICE_ID,
        nonce: 0xF00D_BEEF,
        name: "pp-fakephone".to_string(),
    };
    sock.send_to(&encode_discover_req(&req), target)
        .map_err(|e| format!("discovery send failed: {e}"))?;

    let (raw, server_addr) = recv_typed(&sock, msg::DISCOVER_RESP, Duration::from_secs(3))
        .ok_or("no companion answered — is it running, and is UDP allowed through the firewall?")?;
    let resp = decode_discover_resp(&raw).map_err(|e| format!("bad discovery reply: {e}"))?;

    println!(
        "Found \"{}\" at {} — backend: {}",
        resp.hostname,
        server_addr.ip(),
        resp.backend
    );
    let discovery_addr = SocketAddr::new(server_addr.ip(), DISCOVERY_PORT);
    let input_addr = SocketAddr::new(server_addr.ip(), resp.input_port);

    // --- pairing -------------------------------------------------------------
    let token = match (resp.already_paired, load_token()) {
        (true, Some(t)) => {
            println!("Already paired; reusing the cached token.");
            t
        }
        _ => {
            if !resp.pairing_mode {
                return Err(
                    "the companion is not in pairing mode — click \"Pair a phone\" in its window"
                        .into(),
                );
            }
            let code = match args.code {
                Some(c) => c,
                None => {
                    print!("Enter the 6-digit code shown by the companion: ");
                    std::io::stdout().flush().ok();
                    let mut line = String::new();
                    std::io::stdin()
                        .read_line(&mut line)
                        .map_err(|e| format!("cannot read the code: {e}"))?;
                    line.trim().to_string()
                }
            };
            let t = pair(&sock, discovery_addr, &code)?;
            save_token(&t);
            println!("Paired.");
            t
        }
    };

    // --- session -------------------------------------------------------------
    let client_nonce = crypto::random_bytes::<16>();
    let sreq = SessionReq {
        device_id: DEVICE_ID,
        client_nonce,
    };
    let mut out = [0u8; SESSION_REQ_LEN];
    encode_session_req(&sreq, &token, &mut out);
    sock.send_to(&out, input_addr)
        .map_err(|e| format!("session send failed: {e}"))?;

    let (raw, _) = recv_typed(&sock, msg::SESSION_RESP, Duration::from_secs(3))
        .ok_or("the companion did not answer the session request (is this phone still paired?)")?;
    let sresp = decode_session_resp(&raw, &token)
        .map_err(|e| format!("session reply failed authentication: {e}"))?;
    if sresp.status != SessionStatus::Ok {
        return Err(format!("companion refused the session: {:?}", sresp.status));
    }
    let session_key = crypto::derive_session_key(&token, &client_nonce, &sresp.server_nonce);
    println!("Session {} open on {input_addr}.\n", sresp.session_id);

    // --- drive ---------------------------------------------------------------
    // Non-blocking, not a short read timeout. Windows rounds SO_RCVTIMEO up to
    // the ~15.6 ms scheduler tick, so a "1 ms" timeout actually costs 15 ms per
    // drain and caps the send rate at ~65 Hz however high `--rate` is set.
    sock.set_read_timeout(None)
        .map_err(|e| format!("cannot clear read timeout: {e}"))?;
    sock.set_nonblocking(true)
        .map_err(|e| format!("cannot set non-blocking: {e}"))?;

    let interval = Duration::from_secs_f64(1.0 / args.rate.max(1) as f64);
    let start = Instant::now();
    let mut seq: u32 = 0;
    let mut next_send = Instant::now();
    let mut rtt_us: u32 = 0;
    let mut last_echo_ms = u32::MAX;
    let mut feedback_seen = 0u64;
    let mut last_report = Instant::now();
    let mut sent_this_second = 0u32;

    println!(
        "Driving the controller at {} Hz. Open joy.cpl to watch.",
        args.rate
    );
    if let Some(d) = args.die_after {
        println!(
            "Will stop sending after {d}s so you can watch the watchdog release the controls."
        );
    }

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(start);
        if elapsed.as_secs() >= args.seconds {
            break;
        }

        let stop_sending = args.die_after.is_some_and(|d| elapsed.as_secs() >= d);

        if !stop_sending && now >= next_send {
            next_send += interval;
            // Guard against drift after a scheduling hiccup.
            if next_send < now {
                next_send = now + interval;
            }

            seq += 1;
            sent_this_second += 1;
            let t = elapsed.as_secs_f32();
            let (s, c) = (t * 1.5).sin_cos();
            let amp = 30000.0;
            let tri = ((t * 0.5).fract() * 2.0 - 1.0).abs();
            let cycle = [
                buttons::A,
                buttons::B,
                buttons::X,
                buttons::Y,
                buttons::LB,
                buttons::RB,
                buttons::DPAD_UP,
                buttons::DPAD_RIGHT,
                buttons::DPAD_DOWN,
                buttons::DPAD_LEFT,
                buttons::START,
                buttons::BACK,
            ];

            let packet = InputPacket {
                session_id: sresp.session_id,
                seq,
                client_time_ms: elapsed.as_millis() as u32,
                rtt_us,
                flags: 0,
                state: ControllerState {
                    buttons: cycle[(t as usize) % cycle.len()],
                    lx: (c * amp) as i16,
                    ly: (s * amp) as i16,
                    rx: (s * amp) as i16,
                    ry: (c * amp) as i16,
                    lt: (tri * 255.0) as u8,
                    rt: 255 - (tri * 255.0) as u8,
                },
            };
            let mut buf = [0u8; INPUT_LEN];
            encode_input(&packet, &session_key, &mut buf);
            if let Err(e) = sock.send_to(&buf, input_addr) {
                eprintln!("send failed: {e}");
            }
        }

        // Drain any feedback and turn the echo into a real RTT measurement.
        let mut rbuf = [0u8; 256];
        while let Ok((n, _)) = sock.recv_from(&mut rbuf) {
            if let Ok(fb) = decode_feedback(&rbuf[..n], &session_key) {
                feedback_seen += 1;
                // Only a *new* echo says anything about the current round trip.
                // Re-measuring against a stale echo makes RTT climb forever
                // once the phone stops sending, which is a lie, not a reading.
                if fb.echo_client_time_ms != last_echo_ms {
                    last_echo_ms = fb.echo_client_time_ms;
                    let now_ms = start.elapsed().as_millis() as u32;
                    rtt_us = now_ms.saturating_sub(fb.echo_client_time_ms) * 1000;
                }
            }
        }

        if now.duration_since(last_report) >= Duration::from_secs(1) {
            last_report = now;
            println!(
                "  t={:>3}s  sent {:>4}/s  seq {:<7} rtt {:.1} ms  feedback {}{}",
                elapsed.as_secs(),
                sent_this_second,
                seq,
                rtt_us as f32 / 1000.0,
                feedback_seen,
                if stop_sending {
                    "   [SILENT — watchdog should release]"
                } else {
                    ""
                }
            );
            sent_this_second = 0;
        }

        // Yield rather than sleep. `thread::sleep` on Windows is also quantised
        // to the scheduler tick, so sleeping "200 us" would cost ~15 ms and make
        // accurate pacing impossible. Spinning costs a core on a dev tool, which
        // is the right trade for honest rate control.
        std::thread::yield_now();
    }

    // Say goodbye properly rather than relying on the watchdog.
    let mut bye = [0u8; BYE_LEN];
    encode_bye(sresp.session_id, &session_key, &mut bye);
    let _ = sock.send_to(&bye, input_addr);
    println!("\nDone. Sent {seq} packets, received {feedback_seen} feedback packets.");
    Ok(())
}

fn pair(sock: &UdpSocket, addr: SocketAddr, code: &str) -> Result<Token, String> {
    let keys = PairingKeys::generate();
    let req = PairReq {
        device_id: DEVICE_ID,
        client_pub: keys.public,
        name: "pp-fakephone".to_string(),
    };
    sock.send_to(&encode_pair_req(&req), addr)
        .map_err(|e| format!("pair send failed: {e}"))?;

    let (raw, _) = recv_typed(sock, msg::PAIR_RESP, Duration::from_secs(5))
        .ok_or("no pairing reply from the companion")?;
    let resp = decode_pair_resp(&raw).map_err(|e| format!("bad pairing reply: {e}"))?;
    if resp.status != PairStatus::Ok {
        return Err(format!("companion refused pairing: {:?}", resp.status));
    }

    let secrets = keys.agree(&resp.server_pub, &DEVICE_ID);
    // Check the PC knew the code before we commit to anything.
    if !ct_eq32(
        &secrets.server_confirm(&keys.public, &resp.server_pub, code),
        &resp.server_confirm,
    ) {
        return Err("wrong code, or someone is interfering with the pairing exchange".into());
    }

    let confirm = PairConfirm {
        device_id: DEVICE_ID,
        client_confirm: secrets.client_confirm(&keys.public, &resp.server_pub, code),
    };
    sock.send_to(&encode_pair_confirm(&confirm), addr)
        .map_err(|e| format!("confirm send failed: {e}"))?;

    let (raw, _) = recv_typed(sock, msg::PAIR_RESULT, Duration::from_secs(5))
        .ok_or("no pairing result from the companion")?;
    match decode_pair_result(&raw).map_err(|e| format!("bad pairing result: {e}"))? {
        PairStatus::Ok => Ok(secrets.token),
        other => Err(format!("pairing refused: {other:?}")),
    }
}
