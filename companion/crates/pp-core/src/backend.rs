//! Virtual controller backends.
//!
//! Everything that talks to a driver lives behind `VirtualPad`. ViGEmBus is
//! end-of-life (archived 2023-11-02), so the day a successor arrives, only this
//! file changes — see `docs/ADR-001-architecture.md`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use pp_protocol::ControllerState;
use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    /// Xbox 360 pad via ViGEmBus. Visible to XInput, DirectInput, GameInput and
    /// the browser Gamepad API — which is what Xbox Cloud Gaming reads.
    Xbox360,
    /// No driver touched. For testing the network path in isolation.
    None,
}

impl BackendKind {
    pub const ALL: [BackendKind; 2] = [BackendKind::Xbox360, BackendKind::None];

    pub fn label(&self) -> &'static str {
        match self {
            BackendKind::Xbox360 => "Xbox 360 (ViGEmBus)",
            BackendKind::None => "None (network test only)",
        }
    }
}

pub trait VirtualPad: Send {
    fn kind(&self) -> BackendKind;

    /// Push a state to the driver. Called up to 250 times a second.
    fn apply(&mut self, state: &ControllerState) -> Result<(), String>;

    /// Which XInput slot Windows assigned, once known.
    fn user_index(&mut self) -> Option<u32> {
        None
    }

    /// Rumble the game has asked for, if this backend can observe it.
    ///
    /// Returns `None` on backends that cannot observe rumble at all, which is
    /// different from `Some((0, 0))` — a game that has stopped rumbling.
    fn poll_rumble(&mut self) -> Option<(u8, u8)> {
        None
    }

    /// Release everything. Called by the watchdog the moment a connection dies,
    /// so a held stick or trigger can never outlive its session.
    fn neutralise(&mut self) -> Result<(), String> {
        self.apply(&ControllerState::NEUTRAL)
    }
}

pub fn create(kind: BackendKind) -> Result<Box<dyn VirtualPad>, String> {
    match kind {
        BackendKind::Xbox360 => Ok(Box::new(ViGEmX360::new()?)),
        BackendKind::None => Ok(Box::new(NullPad::default())),
    }
}

// --- ViGEm Xbox 360 ----------------------------------------------------------

struct ViGEmX360 {
    pad: Xbox360Wired<Client>,
    user_index: Option<u32>,

    /// Latest rumble the game has asked for, written by the driver's
    /// notification thread and read by the input thread.
    ///
    /// Packed into one atomic rather than two so the pair can never be torn:
    /// reading a large motor from one notification and a small motor from the
    /// next would make the phone buzz at a strength no game ever requested.
    /// A sentinel above the packed range means "nothing seen yet".
    rumble: Arc<AtomicU32>,
}

const RUMBLE_NONE: u32 = 0xFFFF_FFFF;

impl ViGEmX360 {
    fn new() -> Result<Self, String> {
        let client = Client::connect().map_err(|e| {
            format!(
                "cannot reach the ViGEmBus driver ({e:?}). Install ViGEmBus 1.22.0 from \
                 https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0 and try again."
            )
        })?;
        let mut pad = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        pad.plugin()
            .map_err(|e| format!("could not plug in the virtual pad: {e:?}"))?;
        pad.wait_ready()
            .map_err(|e| format!("virtual pad never became ready: {e:?}"))?;

        // Rumble arrives as driver notifications, not as a value that can be
        // polled, so it needs a thread parked on the IOCTL. The thread ends on
        // its own when the target is dropped, which is what re-attaching does.
        //
        // A failure here is not fatal: it costs rumble, not the controller.
        let rumble = Arc::new(AtomicU32::new(RUMBLE_NONE));
        match pad.request_notification() {
            Ok(req) => {
                let sink = Arc::clone(&rumble);
                req.spawn_thread(move |_, n| {
                    sink.store(
                        (n.large_motor as u32) << 8 | n.small_motor as u32,
                        Ordering::Relaxed,
                    );
                });
            }
            Err(_) => rumble.store(RUMBLE_NONE, Ordering::Relaxed),
        }

        Ok(Self {
            pad,
            user_index: None,
            rumble,
        })
    }
}

impl VirtualPad for ViGEmX360 {
    fn kind(&self) -> BackendKind {
        BackendKind::Xbox360
    }

    fn apply(&mut self, state: &ControllerState) -> Result<(), String> {
        // Button bits are defined to match XInput's `wButtons` exactly, so this
        // is a copy rather than a translation table.
        let gamepad = XGamepad {
            buttons: XButtons { raw: state.buttons },
            left_trigger: state.lt,
            right_trigger: state.rt,
            thumb_lx: state.lx,
            thumb_ly: state.ly,
            thumb_rx: state.rx,
            thumb_ry: state.ry,
        };
        self.pad
            .update(&gamepad)
            .map_err(|e| format!("virtual pad update failed: {e:?}"))
    }

    fn user_index(&mut self) -> Option<u32> {
        if self.user_index.is_none() {
            self.user_index = self.pad.get_user_index().ok();
        }
        self.user_index
    }

    fn poll_rumble(&mut self) -> Option<(u8, u8)> {
        match self.rumble.load(Ordering::Relaxed) {
            RUMBLE_NONE => None,
            packed => Some(((packed >> 8) as u8, packed as u8)),
        }
    }
}

// --- null --------------------------------------------------------------------

/// Accepts everything and drives nothing. Lets the whole network, pairing and
/// watchdog path be exercised on a machine with no driver installed.
#[derive(Default)]
struct NullPad {
    last: ControllerState,
}

impl VirtualPad for NullPad {
    fn kind(&self) -> BackendKind {
        BackendKind::None
    }

    fn apply(&mut self, state: &ControllerState) -> Result<(), String> {
        self.last = *state;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pp_protocol::buttons;

    #[test]
    fn null_backend_accepts_and_neutralises() {
        let mut pad = create(BackendKind::None).unwrap();
        assert_eq!(pad.kind(), BackendKind::None);
        pad.apply(&ControllerState {
            buttons: buttons::A,
            lx: 1000,
            ..Default::default()
        })
        .unwrap();
        pad.neutralise().unwrap();
        assert!(pad.poll_rumble().is_none());
    }

    #[test]
    fn every_backend_kind_has_a_label() {
        for k in BackendKind::ALL {
            assert!(!k.label().is_empty());
        }
    }
}
