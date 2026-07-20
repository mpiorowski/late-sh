use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Distinct from the nethack and
/// rebels doors' domains so the same configured secret could never produce a key
/// valid for another service.
const KEY_DOMAIN: &[u8] = b"late.sh/dopewars/v1\0dopewars\0";

/// Derive the single Ed25519 client key from the configured shared secret.
///
/// late.sh owns both ends of this connection, so there is no per-user key: the
/// key proves *authorization* (the connection came from late-ssh, which holds
/// the same secret). Unlike the nethack host the SSH username carries no
/// identity either -- dopewars single-player takes no `-u` name; the player
/// types their handle in-game and it lands in the shared high-score table. The
/// server accepts exactly this one derived public key; both ends recompute it
/// from `LATE_DOPEWARS_SECRET`.
pub fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh dopewars derived").expect("valid ed25519 key")
}

// CROSS-CRATE CONTRACT: `KEY_DOMAIN` and every derivation step above MUST stay
// byte-identical to late-ssh's `door::dopewars::identity::derive_client_key`.
// If they drift, the client derives a different key and the host rejects every
// connection. (The nethack crates pin this with a known-answer fingerprint test;
// mirror that here once test-running is back on.)
