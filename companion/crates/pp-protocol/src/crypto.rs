//! Pairing and per-packet authentication.
//!
//! Threat model: any device on the same LAN. It must not be able to inject
//! controller input, replay a captured stream, or man-in-the-middle pairing.
//! It is *not* a defence against an attacker who can read the phone's or the
//! PC's local storage — at that point they own the machine anyway.

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::DeviceId;

type HmacSha256 = Hmac<Sha256>;

/// Long-term credential for one phone/PC pair. Derived independently on both
/// sides during pairing and **never transmitted**.
pub type Token = [u8; 32];

/// Per-session key, derived from the token plus both nonces. A fresh key each
/// session means a captured stream cannot be replayed into a later one even
/// though sequence numbers restart.
pub type SessionKey = [u8; 32];

const HKDF_SALT: &[u8] = b"phonepad-v1";

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

/// Truncated HMAC-SHA256 tag as carried on the wire.
pub fn mac8(key: &[u8; 32], data: &[u8]) -> [u8; 8] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    let full = mac.finalize().into_bytes();
    let mut out = [0u8; 8];
    out.copy_from_slice(&full[..8]);
    out
}

/// Constant-time verification. Uses the `hmac` crate's own comparison rather
/// than `==` so a timing side channel cannot leak the expected tag.
pub fn verify_mac8(key: &[u8; 32], data: &[u8], tag: &[u8]) -> bool {
    if tag.len() != 8 {
        return false;
    }
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.verify_truncated_left(tag).is_ok()
}

fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = [0u8; 32];
    hk.expand(info, &mut out)
        .expect("32 bytes is a valid HKDF output length");
    out
}

pub fn derive_session_key(
    token: &Token,
    client_nonce: &[u8; 16],
    server_nonce: &[u8; 16],
) -> SessionKey {
    let mut salt = [0u8; 32];
    salt[..16].copy_from_slice(client_nonce);
    salt[16..].copy_from_slice(server_nonce);
    hkdf32(token, &salt, b"session")
}

/// One side's ephemeral X25519 state during a pairing exchange.
pub struct PairingKeys {
    secret: StaticSecret,
    pub public: [u8; 32],
}

/// Everything derived once both public keys are known.
pub struct PairingSecrets {
    /// Key for the two confirmation tags.
    confirm_key: [u8; 32],
    /// The long-term credential.
    pub token: Token,
}

impl PairingKeys {
    pub fn generate() -> Self {
        Self::from_scalar(random_bytes::<32>())
    }

    /// Deterministic construction. Used only to generate the conformance
    /// vectors that keep the Rust and Dart pairing code in agreement — real
    /// pairings always come from [`PairingKeys::generate`].
    pub fn from_scalar(scalar: [u8; 32]) -> Self {
        let secret = StaticSecret::from(scalar);
        let public = PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    /// Complete the Diffie–Hellman and derive both the confirmation key and the
    /// long-term token.
    pub fn agree(&self, their_public: &[u8; 32], device_id: &DeviceId) -> PairingSecrets {
        let shared = self
            .secret
            .diffie_hellman(&PublicKey::from(*their_public))
            .to_bytes();

        let mut token_info = Vec::with_capacity(5 + device_id.len());
        token_info.extend_from_slice(b"token");
        token_info.extend_from_slice(device_id);

        PairingSecrets {
            confirm_key: hkdf32(&shared, HKDF_SALT, b"pair"),
            token: hkdf32(&shared, HKDF_SALT, &token_info),
        }
    }
}

impl PairingSecrets {
    fn confirm(
        &self,
        label: &[u8],
        client_pub: &[u8; 32],
        server_pub: &[u8; 32],
        code: &str,
    ) -> [u8; 32] {
        let mut mac = HmacSha256::new_from_slice(&self.confirm_key).expect("valid key length");
        mac.update(label);
        mac.update(client_pub);
        mac.update(server_pub);
        mac.update(code.as_bytes());
        mac.finalize().into_bytes().into()
    }

    /// Proves the PC knows the code the user is looking at.
    pub fn server_confirm(
        &self,
        client_pub: &[u8; 32],
        server_pub: &[u8; 32],
        code: &str,
    ) -> [u8; 32] {
        self.confirm(b"pp-srv", client_pub, server_pub, code)
    }

    /// Proves the phone's user typed the same code.
    pub fn client_confirm(
        &self,
        client_pub: &[u8; 32],
        server_pub: &[u8; 32],
        code: &str,
    ) -> [u8; 32] {
        self.confirm(b"pp-cli", client_pub, server_pub, code)
    }
}

/// Constant-time equality for 32-byte confirmation tags.
pub fn ct_eq32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// A 6-digit pairing code, uniformly distributed over 000000..=999999.
pub fn generate_pairing_code() -> String {
    // Rejection sampling: taking `u32 % 1_000_000` directly would bias the low
    // codes slightly. Cheap to do correctly, so do it correctly.
    let limit = u32::MAX - (u32::MAX % 1_000_000);
    loop {
        let v = OsRng.next_u32();
        if v < limit {
            return format!("{:06}", v % 1_000_000);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_produces_matching_tokens_when_codes_agree() {
        let device_id: DeviceId = [7u8; 16];
        let code = "314159";

        let client = PairingKeys::generate();
        let server = PairingKeys::generate();

        let cs = client.agree(&server.public, &device_id);
        let ss = server.agree(&client.public, &device_id);

        // Both sides land on the same long-term credential without it ever
        // touching the wire.
        assert_eq!(cs.token, ss.token);

        let srv_tag = ss.server_confirm(&client.public, &server.public, code);
        assert!(ct_eq32(
            &srv_tag,
            &cs.server_confirm(&client.public, &server.public, code)
        ));

        let cli_tag = cs.client_confirm(&client.public, &server.public, code);
        assert!(ct_eq32(
            &cli_tag,
            &ss.client_confirm(&client.public, &server.public, code)
        ));
    }

    #[test]
    fn wrong_code_fails_confirmation() {
        let device_id: DeviceId = [1u8; 16];
        let client = PairingKeys::generate();
        let server = PairingKeys::generate();
        let cs = client.agree(&server.public, &device_id);
        let ss = server.agree(&client.public, &device_id);

        let real = ss.server_confirm(&client.public, &server.public, "123456");
        let typed = cs.server_confirm(&client.public, &server.public, "123457");
        assert!(!ct_eq32(&real, &typed));
    }

    #[test]
    fn mitm_with_a_substituted_key_fails_confirmation() {
        let device_id: DeviceId = [2u8; 16];
        let code = "999000";
        let client = PairingKeys::generate();
        let server = PairingKeys::generate();
        let attacker = PairingKeys::generate();

        // Attacker sits in the middle and swaps in its own public key.
        let client_view = client.agree(&attacker.public, &device_id);
        let server_view = server.agree(&attacker.public, &device_id);

        // The confirmation is bound to the public keys the client actually saw,
        // so the tags cannot line up.
        let from_server = server_view.server_confirm(&client.public, &server.public, code);
        let expected_by_client = client_view.server_confirm(&client.public, &server.public, code);
        assert!(!ct_eq32(&from_server, &expected_by_client));
    }

    #[test]
    fn session_keys_differ_per_session() {
        let token: Token = [9u8; 32];
        let a = derive_session_key(&token, &[1u8; 16], &[2u8; 16]);
        let b = derive_session_key(&token, &[1u8; 16], &[3u8; 16]);
        assert_ne!(a, b);
    }

    #[test]
    fn mac_rejects_tampering() {
        let key = [3u8; 32];
        let data = b"hello controller";
        let tag = mac8(&key, data);
        assert!(verify_mac8(&key, data, &tag));
        assert!(!verify_mac8(&key, b"hello controlleR", &tag));
        assert!(!verify_mac8(&[4u8; 32], data, &tag));
        assert!(!verify_mac8(&key, data, &tag[..7]));
    }

    #[test]
    fn pairing_codes_are_six_digits() {
        for _ in 0..200 {
            let c = generate_pairing_code();
            assert_eq!(c.len(), 6);
            assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        }
    }
}
