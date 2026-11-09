//! Windows Firewall helper.
//!
//! The first time the companion binds its UDP ports, Windows will either pop a
//! prompt or silently drop inbound packets — and a silently blocked port looks
//! exactly like "the phone cannot find my PC". So we detect it and offer a fix,
//! rather than leaving the user to guess.

use std::os::windows::process::CommandExt;
use std::process::Command;

pub const RULE_NAME: &str = "PhonePad (UDP in)";

/// Don't flash a console window out of a GUI app.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleState {
    Present,
    Missing,
    Unknown(String),
}

pub fn rule_state() -> RuleState {
    let out = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            &format!("name={RULE_NAME}"),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match out {
        Ok(o) if o.status.success() => RuleState::Present,
        // netsh exits non-zero with "No rules match the specified criteria."
        Ok(_) => RuleState::Missing,
        Err(e) => RuleState::Unknown(format!("could not run netsh: {e}")),
    }
}

/// Adds PhonePad's inbound rule for both ports on **private and domain**
/// networks only. Must already be running elevated; the window gets there
/// through `elevate::run_self_elevated`.
///
/// Never `profile=any`. That was the change proposed for issue #2, and it would
/// open the ports on every public network the PC ever joins: café, hotel and
/// airport Wi-Fi, where anyone could reach the pairing window. A hotspot that
/// is the user's own is made private instead (see netprofile.rs), which scopes
/// the trust to that one network.
pub fn add_rule_elevated(discovery_port: u16, input_port: u16) -> Result<(), String> {
    if rule_state() == RuleState::Present {
        return Ok(());
    }
    let out = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={RULE_NAME}"),
            "dir=in",
            "action=allow",
            "protocol=UDP",
            &format!("localport={discovery_port},{input_port}"),
            "profile=private,domain",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not run netsh: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}
