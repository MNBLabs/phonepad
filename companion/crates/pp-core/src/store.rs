//! Persisted configuration and the trusted-device list.
//!
//! Lives at `%APPDATA%\PhonePad\config.json`. It holds pairing tokens, so it is
//! written to the per-user roaming profile and never anywhere world-readable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pp_protocol::{DeviceId, Token};
use serde::{Deserialize, Serialize};

use crate::backend::BackendKind;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevice {
    /// Hex-encoded 16-byte device id.
    pub id: String,
    pub name: String,
    /// Hex-encoded 32-byte long-term token, derived during pairing.
    pub token: String,
    /// RFC-3339-ish local timestamp, purely informational.
    #[serde(default)]
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Hex-encoded 16-byte id for this PC, stable across restarts.
    pub server_id: String,
    pub discovery_port: u16,
    pub input_port: u16,
    pub backend: BackendKind,
    #[serde(default)]
    pub devices: Vec<TrustedDevice>,
    /// Re-attach the pad automatically when a cloud gaming client starts.
    ///
    /// Off unless the user asks for it: it is a heuristic over process names,
    /// and a wrong guess unplugs a controller somebody may be using.
    #[serde(default)]
    pub auto_reattach: bool,
    /// The name the phone shows for this PC. Empty means the computer name.
    #[serde(default)]
    pub name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_id: hex(&pp_protocol::crypto::random_bytes::<16>()),
            discovery_port: pp_protocol::DISCOVERY_PORT,
            input_port: pp_protocol::INPUT_PORT,
            backend: BackendKind::Xbox360,
            devices: Vec::new(),
            auto_reattach: false,
            name: String::new(),
        }
    }
}

impl Config {
    pub fn server_id_bytes(&self) -> DeviceId {
        unhex::<16>(&self.server_id).unwrap_or([0u8; 16])
    }

    /// Token lookup for an incoming SESSION_REQ.
    pub fn token_for(&self, id: &DeviceId) -> Option<Token> {
        let wanted = hex(id);
        self.devices
            .iter()
            .find(|d| d.id == wanted)
            .and_then(|d| unhex::<32>(&d.token))
    }

    pub fn name_for(&self, id: &DeviceId) -> Option<&str> {
        let wanted = hex(id);
        self.devices
            .iter()
            .find(|d| d.id == wanted)
            .map(|d| d.name.as_str())
    }

    pub fn is_paired(&self, id: &DeviceId) -> bool {
        let wanted = hex(id);
        self.devices.iter().any(|d| d.id == wanted)
    }

    /// Add or replace. Re-pairing an existing phone rotates its token rather
    /// than creating a duplicate entry.
    pub fn upsert_device(&mut self, id: &DeviceId, name: &str, token: &Token) {
        let id_hex = hex(id);
        let entry = TrustedDevice {
            id: id_hex.clone(),
            name: name.to_string(),
            token: hex(token),
            last_seen: None,
        };
        match self.devices.iter_mut().find(|d| d.id == id_hex) {
            Some(existing) => {
                // Keep the existing `last_seen`; re-pairing rotates the token,
                // it does not make the device newly unseen.
                existing.name = entry.name;
                existing.token = entry.token;
            }
            None => self.devices.push(entry),
        }
    }

    pub fn forget_device(&mut self, id_hex: &str) {
        self.devices.retain(|d| d.id != id_hex);
    }

    pub fn touch_device(&mut self, id: &DeviceId, when: String) {
        let id_hex = hex(id);
        if let Some(d) = self.devices.iter_mut().find(|d| d.id == id_hex) {
            d.last_seen = Some(when);
        }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("PhonePad")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load() -> (Config, Option<String>) {
    load_from(&config_path())
}

/// Returns the config plus a warning if an existing file could not be used.
/// A corrupt file must not silently discard someone's pairings, so the old file
/// is kept as `.bad` rather than overwritten.
pub fn load_from(path: &Path) -> (Config, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Config::default(), None),
        Err(e) => {
            return (
                Config::default(),
                Some(format!("could not read {}: {e}", path.display())),
            )
        }
    };

    match serde_json::from_str::<Config>(&text) {
        Ok(c) => (c, None),
        Err(e) => {
            let backup = path.with_extension("bad");
            let note = match std::fs::rename(path, &backup) {
                Ok(()) => format!(
                    "config was unreadable ({e}); previous file kept at {}",
                    backup.display()
                ),
                Err(re) => format!("config was unreadable ({e}) and could not be set aside: {re}"),
            };
            (Config::default(), Some(note))
        }
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    save_to(&config_path(), config)
}

/// Write via a temporary file and rename, so an interrupted save cannot leave a
/// half-written trusted-device list behind.
pub fn save_to(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

/// Fast token lookup snapshot for the input thread, so the hot path never has
/// to hold the config lock or do hex parsing.
pub fn token_map(config: &Config) -> HashMap<DeviceId, Token> {
    config
        .devices
        .iter()
        .filter_map(|d| Some((unhex::<16>(&d.id)?, unhex::<32>(&d.token)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 15, 16, 127, 128, 254, 255, 3, 3, 3, 3, 3, 3, 3, 3];
        let s = hex(&bytes);
        assert_eq!(s.len(), 32);
        assert_eq!(unhex::<16>(&s), Some(bytes));
        assert_eq!(unhex::<16>("nothex"), None);
        assert_eq!(unhex::<16>(&s[..30]), None);
        assert_eq!(unhex::<16>("zz000000000000000000000000000000"), None);
    }

    #[test]
    fn upsert_replaces_rather_than_duplicating() {
        let mut c = Config::default();
        let id: DeviceId = [1u8; 16];
        c.upsert_device(&id, "Phone", &[2u8; 32]);
        c.upsert_device(&id, "Phone renamed", &[3u8; 32]);
        assert_eq!(c.devices.len(), 1);
        assert_eq!(c.devices[0].name, "Phone renamed");
        assert_eq!(c.token_for(&id), Some([3u8; 32]));
        assert!(c.is_paired(&id));
    }

    #[test]
    fn unknown_device_has_no_token() {
        let c = Config::default();
        assert_eq!(c.token_for(&[9u8; 16]), None);
        assert!(!c.is_paired(&[9u8; 16]));
    }

    #[test]
    fn forget_removes_only_the_named_device() {
        let mut c = Config::default();
        c.upsert_device(&[1u8; 16], "A", &[0u8; 32]);
        c.upsert_device(&[2u8; 16], "B", &[0u8; 32]);
        c.forget_device(&hex(&[1u8; 16]));
        assert_eq!(c.devices.len(), 1);
        assert_eq!(c.devices[0].name, "B");
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("pp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let mut c = Config::default();
        c.upsert_device(&[4u8; 16], "S24 Ultra", &[5u8; 32]);
        save_to(&path, &c).unwrap();

        let (loaded, warn) = load_from(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.server_id, c.server_id);
        assert_eq!(loaded.token_for(&[4u8; 16]), Some([5u8; 32]));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_config_is_set_aside_not_silently_lost() {
        let dir = std::env::temp_dir().join(format!("pp-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        let (cfg, warn) = load_from(&path);
        assert!(warn.is_some(), "a corrupt config must be reported");
        assert!(cfg.devices.is_empty());
        assert!(path.with_extension("bad").exists(), "old file must be kept");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_config_is_not_an_error() {
        let path = std::env::temp_dir().join("pp-definitely-absent-config.json");
        std::fs::remove_file(&path).ok();
        let (_, warn) = load_from(&path);
        assert!(warn.is_none());
    }

    #[test]
    fn token_map_skips_malformed_entries() {
        let mut c = Config::default();
        c.upsert_device(&[1u8; 16], "good", &[2u8; 32]);
        c.devices.push(TrustedDevice {
            id: "not-hex".into(),
            name: "bad".into(),
            token: "also-not-hex".into(),
            last_seen: None,
        });
        let map = token_map(&c);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&[1u8; 16]), Some(&[2u8; 32]));
    }
}
