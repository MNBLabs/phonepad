//! Regenerate `protocol/vectors.json`, the fixtures both the Rust and the Dart
//! codec are asserted against.
//!
//!   cargo run -p pp-protocol --example gen_vectors > ../protocol/vectors.json
//!
//! Keys and field values here are fixed, never random — the whole point is that
//! the output is byte-stable so a drift shows up as a failing test.

use pp_protocol::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let key: SessionKey = [0x5a; 32];
    let token: Token = [0x77; 32];

    let mut cases: Vec<String> = Vec::new();

    // --- INPUT ---------------------------------------------------------------
    let inputs = [
        (
            "input_neutral",
            InputPacket {
                session_id: 1,
                seq: 1,
                client_time_ms: 0,
                rtt_us: 0,
                flags: 0,
                state: ControllerState::NEUTRAL,
            },
        ),
        (
            "input_mixed",
            InputPacket {
                session_id: 0xDEAD_BEEF,
                seq: 12345,
                client_time_ms: 987_654,
                rtt_us: 4321,
                flags: 1,
                state: ControllerState {
                    buttons: buttons::A | buttons::DPAD_LEFT | buttons::RB,
                    lx: -12345,
                    ly: 23456,
                    rx: 1,
                    ry: -1,
                    lt: 200,
                    rt: 55,
                },
            },
        ),
        (
            "input_extremes",
            InputPacket {
                session_id: u32::MAX,
                seq: u32::MAX,
                client_time_ms: u32::MAX,
                rtt_us: u32::MAX,
                flags: 0,
                state: ControllerState {
                    buttons: 0xF7FF, // every defined bit, reserved bit clear
                    lx: i16::MAX,
                    ly: -32767,
                    rx: -32767,
                    ry: i16::MAX,
                    lt: 255,
                    rt: 255,
                },
            },
        ),
    ];

    for (name, p) in inputs {
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&p, &key, &mut buf);
        cases.push(format!(
            r#"    {{
      "name": "{name}",
      "type": "input",
      "sessionKey": "{}",
      "sessionId": {},
      "seq": {},
      "clientTimeMs": {},
      "rttUs": {},
      "flags": {},
      "buttons": {},
      "lx": {}, "ly": {}, "rx": {}, "ry": {},
      "lt": {}, "rt": {},
      "bytes": "{}"
    }}"#,
            hex(&key),
            p.session_id,
            p.seq,
            p.client_time_ms,
            p.rtt_us,
            p.flags,
            p.state.buttons,
            p.state.lx,
            p.state.ly,
            p.state.rx,
            p.state.ry,
            p.state.lt,
            p.state.rt,
            hex(&buf)
        ));
    }

    // --- FEEDBACK ------------------------------------------------------------
    let fb = FeedbackPacket {
        session_id: 0x0102_0304,
        echo_client_time_ms: 555_666,
        rumble_large: 200,
        rumble_small: 30,
        accepted_pps: 250,
        loss_permille: 7,
    };
    let mut fbuf = [0u8; FEEDBACK_LEN];
    encode_feedback(&fb, &key, &mut fbuf);
    cases.push(format!(
        r#"    {{
      "name": "feedback_basic",
      "type": "feedback",
      "sessionKey": "{}",
      "sessionId": {},
      "echoClientTimeMs": {},
      "rumbleLarge": {}, "rumbleSmall": {},
      "acceptedPps": {}, "lossPermille": {},
      "bytes": "{}"
    }}"#,
        hex(&key),
        fb.session_id,
        fb.echo_client_time_ms,
        fb.rumble_large,
        fb.rumble_small,
        fb.accepted_pps,
        fb.loss_permille,
        hex(&fbuf)
    ));

    // --- SESSION -------------------------------------------------------------
    let sreq = SessionReq {
        device_id: [0x11; 16],
        client_nonce: [0x22; 16],
    };
    let mut sbuf = [0u8; SESSION_REQ_LEN];
    encode_session_req(&sreq, &token, &mut sbuf);
    cases.push(format!(
        r#"    {{
      "name": "session_req",
      "type": "sessionReq",
      "token": "{}",
      "deviceId": "{}",
      "clientNonce": "{}",
      "bytes": "{}"
    }}"#,
        hex(&token),
        hex(&sreq.device_id),
        hex(&sreq.client_nonce),
        hex(&sbuf)
    ));

    let sresp = SessionResp {
        status: SessionStatus::Ok,
        session_id: 0x0BAD_F00D,
        server_nonce: [0x33; 16],
    };
    let mut rbuf = [0u8; SESSION_RESP_LEN];
    encode_session_resp(&sresp, &token, &mut rbuf);
    cases.push(format!(
        r#"    {{
      "name": "session_resp",
      "type": "sessionResp",
      "token": "{}",
      "status": 0,
      "sessionId": {},
      "serverNonce": "{}",
      "bytes": "{}"
    }}"#,
        hex(&token),
        sresp.session_id,
        hex(&sresp.server_nonce),
        hex(&rbuf)
    ));

    // --- BYE -----------------------------------------------------------------
    let mut byebuf = [0u8; BYE_LEN];
    encode_bye(0x1234_5678, &key, &mut byebuf);
    cases.push(format!(
        r#"    {{
      "name": "bye",
      "type": "bye",
      "sessionKey": "{}",
      "sessionId": {},
      "bytes": "{}"
    }}"#,
        hex(&key),
        0x1234_5678u32,
        hex(&byebuf)
    ));

    // --- CONTROL -------------------------------------------------------------
    for (name, command) in [
        ("control_reattach", ControlCommand::ReattachPad),
        ("control_release", ControlCommand::ReleaseAll),
    ] {
        let p = ControlPacket {
            session_id: 0x0102_0304,
            control_seq: 7,
            command,
        };
        let mut buf = [0u8; CONTROL_LEN];
        encode_control(&p, &key, &mut buf);
        cases.push(format!(
            r#"    {{
      "name": "{name}",
      "type": "control",
      "sessionKey": "{}",
      "sessionId": {},
      "controlSeq": {},
      "command": {},
      "bytes": "{}"
    }}"#,
            hex(&key),
            p.session_id,
            p.control_seq,
            command as u8,
            hex(&buf)
        ));
    }

    // --- DISCOVERY -----------------------------------------------------------
    let dreq = DiscoverReq {
        device_id: [0x44; 16],
        nonce: 0xABCD_1234,
        name: "Galaxy S24 Ultra".to_string(),
    };
    cases.push(format!(
        r#"    {{
      "name": "discover_req",
      "type": "discoverReq",
      "deviceId": "{}",
      "nonce": {},
      "deviceName": "{}",
      "bytes": "{}"
    }}"#,
        hex(&dreq.device_id),
        dreq.nonce,
        dreq.name,
        hex(&encode_discover_req(&dreq))
    ));

    let dresp = DiscoverResp {
        nonce: 0xABCD_1234,
        server_id: [0x55; 16],
        input_port: INPUT_PORT,
        already_paired: true,
        pairing_mode: false,
        hostname: "DESKTOP-PHONEPAD".to_string(),
        backend: "Xbox 360 (ViGEm)".to_string(),
    };
    cases.push(format!(
        r#"    {{
      "name": "discover_resp",
      "type": "discoverResp",
      "nonce": {},
      "serverId": "{}",
      "inputPort": {},
      "alreadyPaired": true,
      "pairingMode": false,
      "hostname": "{}",
      "backend": "{}",
      "bytes": "{}"
    }}"#,
        dresp.nonce,
        hex(&dresp.server_id),
        dresp.input_port,
        dresp.hostname,
        dresp.backend,
        hex(&encode_discover_resp(&dresp))
    ));

    // --- PAIRING (deterministic keys) ---------------------------------------
    // Fixed scalars so the exchange is reproducible. This is what stops the
    // Rust and Dart pairing crypto from disagreeing at runtime, where the only
    // symptom would be an unexplained "wrong code".
    let client_scalar = [0x11u8; 32];
    let server_scalar = [0x22u8; 32];
    let device_id: DeviceId = [0x66; 16];
    let code = "314159";

    let client = PairingKeys::from_scalar(client_scalar);
    let server = PairingKeys::from_scalar(server_scalar);
    let cs = client.agree(&server.public, &device_id);
    let ss = server.agree(&client.public, &device_id);
    assert_eq!(cs.token, ss.token, "vector generation is self-inconsistent");

    let client_nonce = [0x88u8; 16];
    let server_nonce = [0x99u8; 16];
    let session_key = crypto::derive_session_key(&cs.token, &client_nonce, &server_nonce);

    cases.push(format!(
        r#"    {{
      "name": "pairing_exchange",
      "type": "pairing",
      "clientScalar": "{}",
      "serverScalar": "{}",
      "clientPub": "{}",
      "serverPub": "{}",
      "deviceId": "{}",
      "code": "{code}",
      "serverConfirm": "{}",
      "clientConfirm": "{}",
      "token": "{}",
      "clientNonce": "{}",
      "serverNonce": "{}",
      "sessionKey": "{}",
      "bytes": ""
    }}"#,
        hex(&client_scalar),
        hex(&server_scalar),
        hex(&client.public),
        hex(&server.public),
        hex(&device_id),
        hex(&ss.server_confirm(&client.public, &server.public, code)),
        hex(&cs.client_confirm(&client.public, &server.public, code)),
        hex(&cs.token),
        hex(&client_nonce),
        hex(&server_nonce),
        hex(&session_key),
    ));

    println!("{{");
    println!(
        r#"  "comment": "Generated by: cargo run -p pp-protocol --example gen_vectors. Do not hand-edit — both the Rust and Dart codecs assert against these exact bytes.","#
    );
    println!(r#"  "protocolVersion": {VERSION},"#);
    println!(r#"  "cases": ["#);
    println!("{}", cases.join(",\n"));
    println!("  ]");
    println!("}}");
}
