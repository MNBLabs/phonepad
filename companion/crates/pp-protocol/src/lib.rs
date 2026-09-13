//! PhonePad wire protocol v1 — see `protocol/PROTOCOL.md`.
//!
//! Pure logic: no sockets, no threads, no clock. That keeps every rule in here
//! directly unit-testable, which matters most for the parts that are easy to get
//! subtly wrong — MAC verification, replay windows and packet validation.

#![forbid(unsafe_code)]

pub mod crypto;
pub mod packet;
pub mod replay;

pub use crypto::{PairingKeys, SessionKey, Token};
pub use packet::*;
pub use replay::ReplayWindow;

/// First byte of every packet: `'P'`.
pub const MAGIC: u8 = 0x50;
/// Protocol version. Bumped on any incompatible layout change.
pub const VERSION: u8 = 1;

pub const DISCOVERY_PORT: u16 = 47800;
pub const INPUT_PORT: u16 = 47801;

/// Message type codes.
pub mod msg {
    pub const DISCOVER_REQ: u8 = 0x01;
    pub const DISCOVER_RESP: u8 = 0x02;
    pub const PAIR_REQ: u8 = 0x03;
    pub const PAIR_RESP: u8 = 0x04;
    pub const SESSION_REQ: u8 = 0x05;
    pub const SESSION_RESP: u8 = 0x06;
    pub const PAIR_CONFIRM: u8 = 0x07;
    pub const PAIR_RESULT: u8 = 0x08;
    pub const INPUT: u8 = 0x10;
    pub const FEEDBACK: u8 = 0x11;
    pub const BYE: u8 = 0x12;
    pub const CONTROL: u8 = 0x13;
}

/// Out-of-band actions the phone can ask the PC to perform.
///
/// These exist because the companion window is unreachable behind a fullscreen
/// game — the moment you most need to fix the controller is the moment you
/// cannot get to the PC's UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCommand {
    /// Unplug the virtual pad and plug it back in, so a game or stream that
    /// binds controllers on arrival sees one arrive.
    ReattachPad = 1,
    /// Force every control to neutral immediately.
    ReleaseAll = 2,
}

impl ControlCommand {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::ReattachPad,
            2 => Self::ReleaseAll,
            _ => return None,
        })
    }
}

/// Button bits. Deliberately identical to XInput's `wButtons` so the companion
/// copies the field straight through with no translation step.
pub mod buttons {
    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LS: u16 = 0x0040;
    pub const RS: u16 = 0x0080;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;

    /// Bits that carry no meaning in v1. Rejected so that a future version
    /// cannot be misread as a valid v1 packet.
    pub const RESERVED: u16 = 0x0800;
}

/// A 16-byte stable identifier for a phone or a PC.
pub type DeviceId = [u8; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Buffer shorter than the fixed layout requires.
    TooShort {
        expected: usize,
        got: usize,
    },
    BadMagic(u8),
    /// Wire version we do not speak.
    BadVersion(u8),
    /// Correct shape, but not the message type the caller asked for.
    WrongType {
        expected: u8,
        got: u8,
    },
    /// MAC did not verify. The packet is forged, corrupt, or keyed to a
    /// different session.
    BadMac,
    /// A field held a value outside its permitted range.
    BadField(&'static str),
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { expected, got } => {
                write!(f, "packet too short: expected {expected} bytes, got {got}")
            }
            Self::BadMagic(m) => write!(f, "bad magic byte 0x{m:02x}"),
            Self::BadVersion(v) => write!(f, "unsupported protocol version {v}"),
            Self::WrongType { expected, got } => {
                write!(
                    f,
                    "wrong message type: expected 0x{expected:02x}, got 0x{got:02x}"
                )
            }
            Self::BadMac => write!(f, "MAC verification failed"),
            Self::BadField(name) => write!(f, "field out of range: {name}"),
        }
    }
}

impl std::error::Error for DecodeError {}
