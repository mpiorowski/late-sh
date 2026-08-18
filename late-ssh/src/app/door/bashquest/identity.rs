use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Distinct from every other
/// door's domain so the same configured secret could never produce a key
/// valid for another service.
const KEY_DOMAIN: &[u8] = b"late.sh/bashquest/v1\0bashquest\0";

/// Derive the single Ed25519 client key from the configured shared secret.
///
/// late.sh owns both ends of this connection, so there is no per-user key: the
/// key proves *authorization* (the connection came from late-ssh, which holds
/// the same secret). Identity is separate: `proxy::ProcessConfig::playname`
/// (the account's arcade handle) is sent as the SSH username, and the host
/// hands it to bashquest.sh as `BASHQUEST_AUTOLOGIN`. Both ends recompute this
/// key from `LATE_BASHQUEST_SECRET`.
pub fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh bashquest derived").expect("valid ed25519 key")
}

// CROSS-CRATE CONTRACT: `KEY_DOMAIN` and every derivation step above MUST stay
// byte-identical to late-bashquest's `identity::derive_client_key`. If they
// drift, this client derives a different key and the host rejects every
// connection. A known-answer fingerprint test pins this (see identity_test.rs).

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
