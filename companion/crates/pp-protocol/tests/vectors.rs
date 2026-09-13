//! Assert the codec still produces the exact bytes committed in
//! `protocol/vectors.json`. The Dart implementation asserts against the same
//! file, so this is what stops the two codecs from silently drifting apart.
//!
//! If a change here is intentional, regenerate with:
//!   cargo run -p pp-protocol --example gen_vectors > ../protocol/vectors.json
//! and update the Dart side in the same commit.

use pp_protocol::crypto::PairingKeys;
use pp_protocol::*;
use serde_json::Value;

fn hexstr(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn key32(v: &Value, field: &str) -> [u8; 32] {
    let bytes = unhex(
        v[field]
            .as_str()
            .unwrap_or_else(|| panic!("missing {field}")),
    );
    bytes.try_into().expect("32-byte key")
}

fn arr16(v: &Value, field: &str) -> [u8; 16] {
    let bytes = unhex(
        v[field]
            .as_str()
            .unwrap_or_else(|| panic!("missing {field}")),
    );
    bytes.try_into().expect("16-byte field")
}

fn load() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../protocol/vectors.json"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    serde_json::from_str(&text).expect("vectors.json is valid JSON")
}

#[test]
fn committed_vectors_still_match() {
    let doc = load();
    assert_eq!(
        doc["protocolVersion"].as_u64().unwrap(),
        VERSION as u64,
        "vectors.json was generated for a different protocol version"
    );

    let cases = doc["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "no vectors to check");

    let mut checked = 0;
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let expected = unhex(c["bytes"].as_str().unwrap());

        let actual: Vec<u8> = match c["type"].as_str().unwrap() {
            "input" => {
                let p = InputPacket {
                    session_id: c["sessionId"].as_u64().unwrap() as u32,
                    seq: c["seq"].as_u64().unwrap() as u32,
                    client_time_ms: c["clientTimeMs"].as_u64().unwrap() as u32,
                    rtt_us: c["rttUs"].as_u64().unwrap() as u32,
                    flags: c["flags"].as_u64().unwrap() as u8,
                    state: ControllerState {
                        buttons: c["buttons"].as_u64().unwrap() as u16,
                        lx: c["lx"].as_i64().unwrap() as i16,
                        ly: c["ly"].as_i64().unwrap() as i16,
                        rx: c["rx"].as_i64().unwrap() as i16,
                        ry: c["ry"].as_i64().unwrap() as i16,
                        lt: c["lt"].as_u64().unwrap() as u8,
                        rt: c["rt"].as_u64().unwrap() as u8,
                    },
                };
                let mut buf = [0u8; INPUT_LEN];
                encode_input(&p, &key32(c, "sessionKey"), &mut buf);
                // Round-trip too, not just encode.
                assert_eq!(
                    decode_input(&buf, &key32(c, "sessionKey")).unwrap(),
                    p,
                    "{name}: decode did not recover the packet"
                );
                buf.to_vec()
            }
            "feedback" => {
                let p = FeedbackPacket {
                    session_id: c["sessionId"].as_u64().unwrap() as u32,
                    echo_client_time_ms: c["echoClientTimeMs"].as_u64().unwrap() as u32,
                    rumble_large: c["rumbleLarge"].as_u64().unwrap() as u8,
                    rumble_small: c["rumbleSmall"].as_u64().unwrap() as u8,
                    accepted_pps: c["acceptedPps"].as_u64().unwrap() as u16,
                    loss_permille: c["lossPermille"].as_u64().unwrap() as u16,
                };
                let mut buf = [0u8; FEEDBACK_LEN];
                encode_feedback(&p, &key32(c, "sessionKey"), &mut buf);
                buf.to_vec()
            }
            "sessionReq" => {
                let p = SessionReq {
                    device_id: arr16(c, "deviceId"),
                    client_nonce: arr16(c, "clientNonce"),
                };
                let mut buf = [0u8; SESSION_REQ_LEN];
                encode_session_req(&p, &key32(c, "token"), &mut buf);
                buf.to_vec()
            }
            "sessionResp" => {
                let p = SessionResp {
                    status: SessionStatus::from_u8(c["status"].as_u64().unwrap() as u8).unwrap(),
                    session_id: c["sessionId"].as_u64().unwrap() as u32,
                    server_nonce: arr16(c, "serverNonce"),
                };
                let mut buf = [0u8; SESSION_RESP_LEN];
                encode_session_resp(&p, &key32(c, "token"), &mut buf);
                buf.to_vec()
            }
            "control" => {
                let p = ControlPacket {
                    session_id: c["sessionId"].as_u64().unwrap() as u32,
                    control_seq: c["controlSeq"].as_u64().unwrap() as u32,
                    command: ControlCommand::from_u8(c["command"].as_u64().unwrap() as u8).unwrap(),
                };
                let mut buf = [0u8; CONTROL_LEN];
                encode_control(&p, &key32(c, "sessionKey"), &mut buf);
                assert_eq!(decode_control(&buf, &key32(c, "sessionKey")).unwrap(), p);
                buf.to_vec()
            }
            "bye" => {
                let mut buf = [0u8; BYE_LEN];
                encode_bye(
                    c["sessionId"].as_u64().unwrap() as u32,
                    &key32(c, "sessionKey"),
                    &mut buf,
                );
                buf.to_vec()
            }
            "discoverReq" => encode_discover_req(&DiscoverReq {
                device_id: arr16(c, "deviceId"),
                nonce: c["nonce"].as_u64().unwrap() as u32,
                name: c["deviceName"].as_str().unwrap().to_string(),
            }),
            "discoverResp" => encode_discover_resp(&DiscoverResp {
                nonce: c["nonce"].as_u64().unwrap() as u32,
                server_id: arr16(c, "serverId"),
                input_port: c["inputPort"].as_u64().unwrap() as u16,
                already_paired: c["alreadyPaired"].as_bool().unwrap(),
                pairing_mode: c["pairingMode"].as_bool().unwrap(),
                hostname: c["hostname"].as_str().unwrap().to_string(),
                backend: c["backend"].as_str().unwrap().to_string(),
            }),
            "pairing" => {
                // No wire bytes; this vector pins the key agreement itself.
                let client = PairingKeys::from_scalar(key32(c, "clientScalar"));
                let server = PairingKeys::from_scalar(key32(c, "serverScalar"));
                assert_eq!(hexstr(&client.public), c["clientPub"], "{name}: clientPub");
                assert_eq!(hexstr(&server.public), c["serverPub"], "{name}: serverPub");

                let device_id = arr16(c, "deviceId");
                let code = c["code"].as_str().unwrap();
                let cs = client.agree(&server.public, &device_id);
                let ss = server.agree(&client.public, &device_id);

                assert_eq!(cs.token, ss.token, "{name}: sides disagree on the token");
                assert_eq!(hexstr(&cs.token), c["token"], "{name}: token");
                assert_eq!(
                    hexstr(&ss.server_confirm(&client.public, &server.public, code)),
                    c["serverConfirm"],
                    "{name}: serverConfirm"
                );
                assert_eq!(
                    hexstr(&cs.client_confirm(&client.public, &server.public, code)),
                    c["clientConfirm"],
                    "{name}: clientConfirm"
                );
                assert_eq!(
                    hexstr(&crypto::derive_session_key(
                        &cs.token,
                        &arr16(c, "clientNonce"),
                        &arr16(c, "serverNonce")
                    )),
                    c["sessionKey"],
                    "{name}: sessionKey"
                );
                checked += 1;
                continue;
            }
            other => panic!("{name}: unknown vector type {other}"),
        };

        assert_eq!(
            actual, expected,
            "{name}: encoded bytes differ from the committed vector"
        );
        checked += 1;
    }

    assert_eq!(checked, cases.len());
}
