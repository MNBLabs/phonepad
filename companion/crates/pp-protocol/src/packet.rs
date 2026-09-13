//! Fixed-layout little-endian packet codec. See `protocol/PROTOCOL.md`.

use crate::crypto::{mac8, verify_mac8, SessionKey};
use crate::{buttons, msg, ControlCommand, DecodeError, DeviceId, MAGIC, VERSION};

pub const INPUT_LEN: usize = 40;
pub const FEEDBACK_LEN: usize = 28;
pub const BYE_LEN: usize = 16;
pub const SESSION_REQ_LEN: usize = 44;
pub const SESSION_RESP_LEN: usize = 33;
pub const HEADER_LEN: usize = 4;

/// Longest UTF-8 name we will encode or accept, in bytes.
pub const MAX_NAME_LEN: usize = 64;

// --- little-endian helpers ---------------------------------------------------

#[inline]
fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_i16(b: &mut [u8], off: usize, v: i16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
#[inline]
fn get_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
#[inline]
fn get_i16(b: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([b[off], b[off + 1]])
}
#[inline]
fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn put_header(b: &mut [u8], msg_type: u8, flags: u8) {
    b[0] = MAGIC;
    b[1] = VERSION;
    b[2] = msg_type;
    b[3] = flags;
}

/// Validate the common header and overall length before touching any field.
fn check(buf: &[u8], expected_type: u8, expected_len: usize) -> Result<(), DecodeError> {
    if buf.len() < expected_len {
        return Err(DecodeError::TooShort {
            expected: expected_len,
            got: buf.len(),
        });
    }
    if buf[0] != MAGIC {
        return Err(DecodeError::BadMagic(buf[0]));
    }
    if buf[1] != VERSION {
        return Err(DecodeError::BadVersion(buf[1]));
    }
    if buf[2] != expected_type {
        return Err(DecodeError::WrongType {
            expected: expected_type,
            got: buf[2],
        });
    }
    Ok(())
}

/// Peek the message type without validating anything else, so a dispatcher can
/// route a datagram before it knows which key to verify it with.
pub fn peek_type(buf: &[u8]) -> Option<u8> {
    if buf.len() >= HEADER_LEN && buf[0] == MAGIC && buf[1] == VERSION {
        Some(buf[2])
    } else {
        None
    }
}

fn write_name(out: &mut Vec<u8>, name: &str) {
    // Truncate on a char boundary so we never emit invalid UTF-8.
    let mut end = name.len().min(MAX_NAME_LEN);
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = &name.as_bytes()[..end];
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
}

fn read_name(buf: &[u8], off: &mut usize) -> Result<String, DecodeError> {
    if *off >= buf.len() {
        return Err(DecodeError::TooShort {
            expected: *off + 1,
            got: buf.len(),
        });
    }
    let len = buf[*off] as usize;
    *off += 1;
    if len > MAX_NAME_LEN {
        return Err(DecodeError::BadField("name too long"));
    }
    if *off + len > buf.len() {
        return Err(DecodeError::TooShort {
            expected: *off + len,
            got: buf.len(),
        });
    }
    let s = String::from_utf8_lossy(&buf[*off..*off + len]).into_owned();
    *off += len;
    Ok(s)
}

// --- controller state --------------------------------------------------------

/// The complete controller state carried by one INPUT packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControllerState {
    pub buttons: u16,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    pub lt: u8,
    pub rt: u8,
}

impl ControllerState {
    /// Everything released, sticks centred, triggers zero. This is what the
    /// watchdog pushes when a connection dies.
    pub const NEUTRAL: Self = Self {
        buttons: 0,
        lx: 0,
        ly: 0,
        rx: 0,
        ry: 0,
        lt: 0,
        rt: 0,
    };

    pub fn is_neutral(&self) -> bool {
        *self == Self::NEUTRAL
    }
}

// --- INPUT -------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputPacket {
    pub session_id: u32,
    pub seq: u32,
    /// The phone's monotonic clock. The PC never compares this against its own
    /// clock — it only echoes it back so the phone can compute its own RTT.
    pub client_time_ms: u32,
    /// Round-trip time in microseconds as most recently measured *by the phone*
    /// from FEEDBACK echoes. Zero means "not yet known". Carrying it here lets
    /// the companion display real end-to-end latency without needing a clock
    /// shared between the two devices.
    pub rtt_us: u32,
    pub flags: u8,
    pub state: ControllerState,
}

/// Encode into a caller-owned buffer. No allocation: this runs up to 250 times
/// a second on the phone and must not create GC pressure on either side.
pub fn encode_input(p: &InputPacket, key: &SessionKey, out: &mut [u8; INPUT_LEN]) {
    put_header(out, msg::INPUT, p.flags);
    put_u32(out, 4, p.session_id);
    put_u32(out, 8, p.seq);
    put_u32(out, 12, p.client_time_ms);
    put_u16(out, 16, p.state.buttons);
    put_i16(out, 18, p.state.lx);
    put_i16(out, 20, p.state.ly);
    put_i16(out, 22, p.state.rx);
    put_i16(out, 24, p.state.ry);
    out[26] = p.state.lt;
    out[27] = p.state.rt;
    put_u32(out, 28, p.rtt_us);
    let tag = mac8(key, &out[..32]);
    out[32..40].copy_from_slice(&tag);
}

/// Read the session id without verifying anything, so the receiver can look up
/// which key this packet should be checked against.
pub fn peek_session_id(buf: &[u8]) -> Option<u32> {
    if buf.len() >= 8 && buf[0] == MAGIC && buf[1] == VERSION {
        Some(get_u32(buf, 4))
    } else {
        None
    }
}

pub fn decode_input(buf: &[u8], key: &SessionKey) -> Result<InputPacket, DecodeError> {
    check(buf, msg::INPUT, INPUT_LEN)?;
    if !verify_mac8(key, &buf[..32], &buf[32..40]) {
        return Err(DecodeError::BadMac);
    }

    let raw_buttons = get_u16(buf, 16);
    if raw_buttons & buttons::RESERVED != 0 {
        return Err(DecodeError::BadField("reserved button bit set"));
    }
    // i16::MIN has no positive counterpart, which makes downstream negation and
    // scaling asymmetric. Reject it at the boundary rather than let it surprise
    // the stick maths later.
    for (v, name) in [
        (get_i16(buf, 18), "lx"),
        (get_i16(buf, 20), "ly"),
        (get_i16(buf, 22), "rx"),
        (get_i16(buf, 24), "ry"),
    ] {
        if v == i16::MIN {
            return Err(DecodeError::BadField(name));
        }
    }

    Ok(InputPacket {
        session_id: get_u32(buf, 4),
        seq: get_u32(buf, 8),
        client_time_ms: get_u32(buf, 12),
        rtt_us: get_u32(buf, 28),
        flags: buf[3],
        state: ControllerState {
            buttons: raw_buttons,
            lx: get_i16(buf, 18),
            ly: get_i16(buf, 20),
            rx: get_i16(buf, 22),
            ry: get_i16(buf, 24),
            lt: buf[26],
            rt: buf[27],
        },
    })
}

// --- FEEDBACK ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeedbackPacket {
    pub session_id: u32,
    pub echo_client_time_ms: u32,
    pub rumble_large: u8,
    pub rumble_small: u8,
    pub accepted_pps: u16,
    pub loss_permille: u16,
}

pub fn encode_feedback(p: &FeedbackPacket, key: &SessionKey, out: &mut [u8; FEEDBACK_LEN]) {
    put_header(out, msg::FEEDBACK, 0);
    put_u32(out, 4, p.session_id);
    put_u32(out, 8, p.echo_client_time_ms);
    out[12] = p.rumble_large;
    out[13] = p.rumble_small;
    put_u16(out, 14, p.accepted_pps);
    put_u16(out, 16, p.loss_permille);
    put_u16(out, 18, 0); // reserved
    let tag = mac8(key, &out[..20]);
    out[20..28].copy_from_slice(&tag);
}

pub fn decode_feedback(buf: &[u8], key: &SessionKey) -> Result<FeedbackPacket, DecodeError> {
    check(buf, msg::FEEDBACK, FEEDBACK_LEN)?;
    if !verify_mac8(key, &buf[..20], &buf[20..28]) {
        return Err(DecodeError::BadMac);
    }
    Ok(FeedbackPacket {
        session_id: get_u32(buf, 4),
        echo_client_time_ms: get_u32(buf, 8),
        rumble_large: buf[12],
        rumble_small: buf[13],
        accepted_pps: get_u16(buf, 14),
        loss_permille: get_u16(buf, 16),
    })
}

// --- BYE ---------------------------------------------------------------------

pub fn encode_bye(session_id: u32, key: &SessionKey, out: &mut [u8; BYE_LEN]) {
    put_header(out, msg::BYE, 0);
    put_u32(out, 4, session_id);
    put_u32(out, 8, 0); // reserved, keeps the MAC 8-byte aligned
    let tag = mac8(key, &out[..8]);
    out[8..16].copy_from_slice(&tag);
}

pub fn decode_bye(buf: &[u8], key: &SessionKey) -> Result<u32, DecodeError> {
    check(buf, msg::BYE, BYE_LEN)?;
    if !verify_mac8(key, &buf[..8], &buf[8..16]) {
        return Err(DecodeError::BadMac);
    }
    Ok(get_u32(buf, 4))
}

// --- CONTROL -----------------------------------------------------------------

pub const CONTROL_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlPacket {
    pub session_id: u32,
    /// Monotonic, counted separately from input sequence numbers so the two
    /// streams cannot invalidate each other's replay checks.
    pub control_seq: u32,
    pub command: ControlCommand,
}

pub fn encode_control(p: &ControlPacket, key: &SessionKey, out: &mut [u8; CONTROL_LEN]) {
    put_header(out, msg::CONTROL, p.command as u8);
    put_u32(out, 4, p.session_id);
    put_u32(out, 8, p.control_seq);
    put_u32(out, 12, 0); // reserved
    let tag = mac8(key, &out[..16]);
    out[16..24].copy_from_slice(&tag);
}

pub fn decode_control(buf: &[u8], key: &SessionKey) -> Result<ControlPacket, DecodeError> {
    check(buf, msg::CONTROL, CONTROL_LEN)?;
    if !verify_mac8(key, &buf[..16], &buf[16..24]) {
        return Err(DecodeError::BadMac);
    }
    let command = ControlCommand::from_u8(buf[3]).ok_or(DecodeError::BadField("command"))?;
    Ok(ControlPacket {
        session_id: get_u32(buf, 4),
        control_seq: get_u32(buf, 8),
        command,
    })
}

// --- SESSION -----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionReq {
    pub device_id: DeviceId,
    pub client_nonce: [u8; 16],
}

pub fn encode_session_req(p: &SessionReq, token: &crate::Token, out: &mut [u8; SESSION_REQ_LEN]) {
    put_header(out, msg::SESSION_REQ, 0);
    out[4..20].copy_from_slice(&p.device_id);
    out[20..36].copy_from_slice(&p.client_nonce);
    let tag = mac8(token, &out[..36]);
    out[36..44].copy_from_slice(&tag);
}

/// The device id must be read before the MAC can be checked, because the id is
/// what selects the token. So this returns the claim, and the caller verifies.
pub fn peek_session_req_device(buf: &[u8]) -> Result<DeviceId, DecodeError> {
    check(buf, msg::SESSION_REQ, SESSION_REQ_LEN)?;
    let mut id = [0u8; 16];
    id.copy_from_slice(&buf[4..20]);
    Ok(id)
}

pub fn decode_session_req(buf: &[u8], token: &crate::Token) -> Result<SessionReq, DecodeError> {
    check(buf, msg::SESSION_REQ, SESSION_REQ_LEN)?;
    if !verify_mac8(token, &buf[..36], &buf[36..44]) {
        return Err(DecodeError::BadMac);
    }
    let mut device_id = [0u8; 16];
    device_id.copy_from_slice(&buf[4..20]);
    let mut client_nonce = [0u8; 16];
    client_nonce.copy_from_slice(&buf[20..36]);
    Ok(SessionReq {
        device_id,
        client_nonce,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Ok = 0,
    UnknownDevice = 1,
    BadMac = 2,
    ServerBusy = 3,
    NoBackend = 4,
}

impl SessionStatus {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Ok,
            1 => Self::UnknownDevice,
            2 => Self::BadMac,
            3 => Self::ServerBusy,
            4 => Self::NoBackend,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionResp {
    pub status: SessionStatus,
    pub session_id: u32,
    pub server_nonce: [u8; 16],
}

pub fn encode_session_resp(
    p: &SessionResp,
    token: &crate::Token,
    out: &mut [u8; SESSION_RESP_LEN],
) {
    put_header(out, msg::SESSION_RESP, 0);
    out[4] = p.status as u8;
    put_u32(out, 5, p.session_id);
    out[9..25].copy_from_slice(&p.server_nonce);
    let tag = mac8(token, &out[..25]);
    out[25..33].copy_from_slice(&tag);
}

pub fn decode_session_resp(buf: &[u8], token: &crate::Token) -> Result<SessionResp, DecodeError> {
    check(buf, msg::SESSION_RESP, SESSION_RESP_LEN)?;
    if !verify_mac8(token, &buf[..25], &buf[25..33]) {
        return Err(DecodeError::BadMac);
    }
    let status = SessionStatus::from_u8(buf[4]).ok_or(DecodeError::BadField("status"))?;
    let mut server_nonce = [0u8; 16];
    server_nonce.copy_from_slice(&buf[9..25]);
    Ok(SessionResp {
        status,
        session_id: get_u32(buf, 5),
        server_nonce,
    })
}

// --- DISCOVERY ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverReq {
    pub device_id: DeviceId,
    pub nonce: u32,
    pub name: String,
}

pub fn encode_discover_req(p: &DiscoverReq) -> Vec<u8> {
    let mut out = vec![0u8; 24];
    put_header(&mut out, msg::DISCOVER_REQ, 0);
    out[4..20].copy_from_slice(&p.device_id);
    put_u32(&mut out, 20, p.nonce);
    write_name(&mut out, &p.name);
    out
}

pub fn decode_discover_req(buf: &[u8]) -> Result<DiscoverReq, DecodeError> {
    check(buf, msg::DISCOVER_REQ, 25)?;
    let mut device_id = [0u8; 16];
    device_id.copy_from_slice(&buf[4..20]);
    let mut off = 24;
    let name = read_name(buf, &mut off)?;
    Ok(DiscoverReq {
        device_id,
        nonce: get_u32(buf, 20),
        name,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoverResp {
    pub nonce: u32,
    pub server_id: DeviceId,
    pub input_port: u16,
    pub already_paired: bool,
    pub pairing_mode: bool,
    pub hostname: String,
    pub backend: String,
}

pub fn encode_discover_resp(p: &DiscoverResp) -> Vec<u8> {
    let mut out = vec![0u8; 27];
    put_header(&mut out, msg::DISCOVER_RESP, 0);
    put_u32(&mut out, 4, p.nonce);
    out[8..24].copy_from_slice(&p.server_id);
    put_u16(&mut out, 24, p.input_port);
    out[26] = (p.already_paired as u8) | ((p.pairing_mode as u8) << 1);
    write_name(&mut out, &p.hostname);
    write_name(&mut out, &p.backend);
    out
}

pub fn decode_discover_resp(buf: &[u8]) -> Result<DiscoverResp, DecodeError> {
    check(buf, msg::DISCOVER_RESP, 28)?;
    let mut server_id = [0u8; 16];
    server_id.copy_from_slice(&buf[8..24]);
    let state = buf[26];
    let mut off = 27;
    let hostname = read_name(buf, &mut off)?;
    let backend = read_name(buf, &mut off)?;
    Ok(DiscoverResp {
        nonce: get_u32(buf, 4),
        server_id,
        input_port: get_u16(buf, 24),
        already_paired: state & 1 != 0,
        pairing_mode: state & 2 != 0,
        hostname,
        backend,
    })
}

// --- PAIRING -----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairReq {
    pub device_id: DeviceId,
    pub client_pub: [u8; 32],
    pub name: String,
}

pub fn encode_pair_req(p: &PairReq) -> Vec<u8> {
    let mut out = vec![0u8; 52];
    put_header(&mut out, msg::PAIR_REQ, 0);
    out[4..20].copy_from_slice(&p.device_id);
    out[20..52].copy_from_slice(&p.client_pub);
    write_name(&mut out, &p.name);
    out
}

pub fn decode_pair_req(buf: &[u8]) -> Result<PairReq, DecodeError> {
    check(buf, msg::PAIR_REQ, 53)?;
    let mut device_id = [0u8; 16];
    device_id.copy_from_slice(&buf[4..20]);
    let mut client_pub = [0u8; 32];
    client_pub.copy_from_slice(&buf[20..52]);
    let mut off = 52;
    let name = read_name(buf, &mut off)?;
    Ok(PairReq {
        device_id,
        client_pub,
        name,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairStatus {
    Ok = 0,
    NotInPairingMode = 1,
    CodeMismatch = 2,
    Rejected = 3,
}

impl PairStatus {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Ok,
            1 => Self::NotInPairingMode,
            2 => Self::CodeMismatch,
            3 => Self::Rejected,
            _ => return None,
        })
    }
}

pub const PAIR_RESP_LEN: usize = 69;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairResp {
    pub status: PairStatus,
    pub server_pub: [u8; 32],
    pub server_confirm: [u8; 32],
}

pub fn encode_pair_resp(p: &PairResp) -> [u8; PAIR_RESP_LEN] {
    let mut out = [0u8; PAIR_RESP_LEN];
    put_header(&mut out, msg::PAIR_RESP, 0);
    out[4] = p.status as u8;
    out[5..37].copy_from_slice(&p.server_pub);
    out[37..69].copy_from_slice(&p.server_confirm);
    out
}

pub fn decode_pair_resp(buf: &[u8]) -> Result<PairResp, DecodeError> {
    check(buf, msg::PAIR_RESP, PAIR_RESP_LEN)?;
    let status = PairStatus::from_u8(buf[4]).ok_or(DecodeError::BadField("status"))?;
    let mut server_pub = [0u8; 32];
    server_pub.copy_from_slice(&buf[5..37]);
    let mut server_confirm = [0u8; 32];
    server_confirm.copy_from_slice(&buf[37..69]);
    Ok(PairResp {
        status,
        server_pub,
        server_confirm,
    })
}

pub const PAIR_CONFIRM_LEN: usize = 52;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairConfirm {
    pub device_id: DeviceId,
    pub client_confirm: [u8; 32],
}

pub fn encode_pair_confirm(p: &PairConfirm) -> [u8; PAIR_CONFIRM_LEN] {
    let mut out = [0u8; PAIR_CONFIRM_LEN];
    put_header(&mut out, msg::PAIR_CONFIRM, 0);
    out[4..20].copy_from_slice(&p.device_id);
    out[20..52].copy_from_slice(&p.client_confirm);
    out
}

pub fn decode_pair_confirm(buf: &[u8]) -> Result<PairConfirm, DecodeError> {
    check(buf, msg::PAIR_CONFIRM, PAIR_CONFIRM_LEN)?;
    let mut device_id = [0u8; 16];
    device_id.copy_from_slice(&buf[4..20]);
    let mut client_confirm = [0u8; 32];
    client_confirm.copy_from_slice(&buf[20..52]);
    Ok(PairConfirm {
        device_id,
        client_confirm,
    })
}

pub const PAIR_RESULT_LEN: usize = 5;

pub fn encode_pair_result(status: PairStatus) -> [u8; PAIR_RESULT_LEN] {
    let mut out = [0u8; PAIR_RESULT_LEN];
    put_header(&mut out, msg::PAIR_RESULT, 0);
    out[4] = status as u8;
    out
}

pub fn decode_pair_result(buf: &[u8]) -> Result<PairStatus, DecodeError> {
    check(buf, msg::PAIR_RESULT, PAIR_RESULT_LEN)?;
    PairStatus::from_u8(buf[4]).ok_or(DecodeError::BadField("status"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: SessionKey = [0x5au8; 32];

    fn sample_input() -> InputPacket {
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
        }
    }

    #[test]
    fn input_round_trips() {
        let p = sample_input();
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&p, &KEY, &mut buf);
        assert_eq!(decode_input(&buf, &KEY).unwrap(), p);
    }

    #[test]
    fn input_layout_is_exactly_forty_bytes() {
        // The wire format is a published contract; a silent size change would
        // desync the Dart side.
        assert_eq!(INPUT_LEN, 40);
        assert_eq!(FEEDBACK_LEN, 28);
        assert_eq!(BYE_LEN, 16);
    }

    #[test]
    fn tampered_input_is_rejected() {
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&sample_input(), &KEY, &mut buf);
        for bit in 0..8 {
            let mut t = buf;
            t[16] ^= 1 << bit; // flip a button bit
            assert_eq!(decode_input(&t, &KEY), Err(DecodeError::BadMac));
        }
    }

    #[test]
    fn wrong_key_is_rejected() {
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&sample_input(), &KEY, &mut buf);
        assert_eq!(decode_input(&buf, &[0u8; 32]), Err(DecodeError::BadMac));
    }

    #[test]
    fn malformed_inputs_are_rejected_not_panicking() {
        // Truncation at every length must produce an error, never a panic.
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&sample_input(), &KEY, &mut buf);
        for len in 0..INPUT_LEN {
            assert!(
                decode_input(&buf[..len], &KEY).is_err(),
                "len {len} accepted"
            );
        }

        let mut bad_magic = buf;
        bad_magic[0] = b'X';
        assert_eq!(
            decode_input(&bad_magic, &KEY),
            Err(DecodeError::BadMagic(b'X'))
        );

        let mut bad_version = buf;
        bad_version[1] = 99;
        assert_eq!(
            decode_input(&bad_version, &KEY),
            Err(DecodeError::BadVersion(99))
        );

        let mut bad_type = buf;
        bad_type[2] = msg::FEEDBACK;
        assert!(matches!(
            decode_input(&bad_type, &KEY),
            Err(DecodeError::WrongType { .. })
        ));
    }

    #[test]
    fn reserved_button_bit_is_rejected() {
        let mut p = sample_input();
        p.state.buttons |= buttons::RESERVED;
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&p, &KEY, &mut buf);
        assert_eq!(
            decode_input(&buf, &KEY),
            Err(DecodeError::BadField("reserved button bit set"))
        );
    }

    #[test]
    fn i16_min_axis_is_rejected() {
        let mut p = sample_input();
        p.state.lx = i16::MIN;
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&p, &KEY, &mut buf);
        assert_eq!(decode_input(&buf, &KEY), Err(DecodeError::BadField("lx")));
    }

    #[test]
    fn rtt_field_round_trips_including_zero() {
        for rtt in [0u32, 1, 4321, u32::MAX] {
            let mut p = sample_input();
            p.rtt_us = rtt;
            let mut buf = [0u8; INPUT_LEN];
            encode_input(&p, &KEY, &mut buf);
            assert_eq!(decode_input(&buf, &KEY).unwrap().rtt_us, rtt);
        }
    }

    #[test]
    fn peek_session_id_matches_decode() {
        let p = sample_input();
        let mut buf = [0u8; INPUT_LEN];
        encode_input(&p, &KEY, &mut buf);
        assert_eq!(peek_session_id(&buf), Some(p.session_id));
        assert_eq!(peek_type(&buf), Some(msg::INPUT));
    }

    #[test]
    fn feedback_round_trips() {
        let p = FeedbackPacket {
            session_id: 42,
            echo_client_time_ms: 777,
            rumble_large: 128,
            rumble_small: 64,
            accepted_pps: 250,
            loss_permille: 3,
        };
        let mut buf = [0u8; FEEDBACK_LEN];
        encode_feedback(&p, &KEY, &mut buf);
        assert_eq!(decode_feedback(&buf, &KEY).unwrap(), p);

        buf[12] ^= 0xFF;
        assert_eq!(decode_feedback(&buf, &KEY), Err(DecodeError::BadMac));
    }

    #[test]
    fn control_round_trips_and_rejects_forgery() {
        for command in [ControlCommand::ReattachPad, ControlCommand::ReleaseAll] {
            let p = ControlPacket {
                session_id: 4242,
                control_seq: 9,
                command,
            };
            let mut buf = [0u8; CONTROL_LEN];
            encode_control(&p, &KEY, &mut buf);
            assert_eq!(decode_control(&buf, &KEY).unwrap(), p);

            // A control message must be no easier to forge than an input.
            assert_eq!(decode_control(&buf, &[1u8; 32]), Err(DecodeError::BadMac));
            let mut tampered = buf;
            tampered[8] ^= 0x01;
            assert_eq!(decode_control(&tampered, &KEY), Err(DecodeError::BadMac));
        }
    }

    #[test]
    fn unknown_control_command_is_rejected() {
        let p = ControlPacket {
            session_id: 1,
            control_seq: 1,
            command: ControlCommand::ReattachPad,
        };
        let mut buf = [0u8; CONTROL_LEN];
        encode_control(&p, &KEY, &mut buf);
        // Re-sign with a command value we do not define, so the MAC is valid and
        // only the field check can catch it.
        buf[3] = 99;
        let tag = crate::crypto::mac8(&KEY, &buf[..16]);
        buf[16..24].copy_from_slice(&tag);
        assert_eq!(
            decode_control(&buf, &KEY),
            Err(DecodeError::BadField("command"))
        );
    }

    #[test]
    fn bye_round_trips() {
        let mut buf = [0u8; BYE_LEN];
        encode_bye(9001, &KEY, &mut buf);
        assert_eq!(decode_bye(&buf, &KEY).unwrap(), 9001);
    }

    #[test]
    fn session_round_trips() {
        let token = [7u8; 32];
        let req = SessionReq {
            device_id: [3u8; 16],
            client_nonce: [4u8; 16],
        };
        let mut buf = [0u8; SESSION_REQ_LEN];
        encode_session_req(&req, &token, &mut buf);
        assert_eq!(peek_session_req_device(&buf).unwrap(), req.device_id);
        assert_eq!(decode_session_req(&buf, &token).unwrap(), req);
        assert_eq!(
            decode_session_req(&buf, &[0u8; 32]),
            Err(DecodeError::BadMac)
        );

        let resp = SessionResp {
            status: SessionStatus::Ok,
            session_id: 555,
            server_nonce: [8u8; 16],
        };
        let mut rbuf = [0u8; SESSION_RESP_LEN];
        encode_session_resp(&resp, &token, &mut rbuf);
        assert_eq!(decode_session_resp(&rbuf, &token).unwrap(), resp);
    }

    #[test]
    fn discovery_round_trips() {
        let req = DiscoverReq {
            device_id: [1u8; 16],
            nonce: 0xABCD_1234,
            name: "Galaxy S24 Ultra".to_string(),
        };
        let bytes = encode_discover_req(&req);
        assert_eq!(decode_discover_req(&bytes).unwrap(), req);

        let resp = DiscoverResp {
            nonce: 0xABCD_1234,
            server_id: [2u8; 16],
            input_port: INPUT_PORT_FOR_TEST,
            already_paired: true,
            pairing_mode: false,
            hostname: "DESKTOP-TEST".to_string(),
            backend: "Xbox 360 (ViGEm)".to_string(),
        };
        let bytes = encode_discover_resp(&resp);
        assert_eq!(decode_discover_resp(&bytes).unwrap(), resp);
    }

    const INPUT_PORT_FOR_TEST: u16 = 47801;

    #[test]
    fn overlong_names_are_truncated_on_a_char_boundary() {
        // A multi-byte char straddling the limit must not produce broken UTF-8.
        let name = "é".repeat(50); // 100 bytes
        let req = DiscoverReq {
            device_id: [0u8; 16],
            nonce: 0,
            name,
        };
        let bytes = encode_discover_req(&req);
        let back = decode_discover_req(&bytes).unwrap();
        assert!(back.name.len() <= MAX_NAME_LEN);
        assert!(back.name.chars().all(|c| c == 'é'));
    }

    #[test]
    fn truncated_variable_length_packets_error_cleanly() {
        let bytes = encode_discover_req(&DiscoverReq {
            device_id: [0u8; 16],
            nonce: 0,
            name: "test".into(),
        });
        for len in 0..bytes.len() {
            assert!(decode_discover_req(&bytes[..len]).is_err(), "len {len}");
        }
    }

    #[test]
    fn pairing_messages_round_trip() {
        let req = PairReq {
            device_id: [5u8; 16],
            client_pub: [6u8; 32],
            name: "phone".into(),
        };
        assert_eq!(decode_pair_req(&encode_pair_req(&req)).unwrap(), req);

        let resp = PairResp {
            status: PairStatus::Ok,
            server_pub: [7u8; 32],
            server_confirm: [8u8; 32],
        };
        assert_eq!(decode_pair_resp(&encode_pair_resp(&resp)).unwrap(), resp);

        let conf = PairConfirm {
            device_id: [5u8; 16],
            client_confirm: [9u8; 32],
        };
        assert_eq!(
            decode_pair_confirm(&encode_pair_confirm(&conf)).unwrap(),
            conf
        );

        assert_eq!(
            decode_pair_result(&encode_pair_result(PairStatus::CodeMismatch)).unwrap(),
            PairStatus::CodeMismatch
        );
    }

    #[test]
    fn neutral_state_is_all_zero() {
        assert!(ControllerState::NEUTRAL.is_neutral());
        assert!(ControllerState::default().is_neutral());
        assert!(!ControllerState {
            buttons: buttons::A,
            ..Default::default()
        }
        .is_neutral());
    }
}
