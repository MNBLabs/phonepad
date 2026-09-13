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

/// Adds an inbound allow rule for both ports. Requires elevation, so this
/// triggers a UAC prompt; the user is told that up front by the button label.
pub fn add_rule(discovery_port: u16, input_port: u16) -> Result<(), String> {
    let args = format!(
        "advfirewall firewall add rule name=\"\"{RULE_NAME}\"\" dir=in action=allow \
         protocol=UDP localport={discovery_port},{input_port} profile=private,domain"
    );

    let script = format!(
        "$p = Start-Process -FilePath netsh -ArgumentList '{args}' -Verb RunAs -Wait -PassThru; \
         exit $p.ExitCode"
    );

    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not launch the elevation prompt: {e}"))?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = stderr.trim();
        Err(if detail.is_empty() {
            "the firewall rule was not added (the elevation prompt may have been declined)".into()
        } else {
            detail.to_string()
        })
    }
}
