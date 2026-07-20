use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Distinct from the nethack,
/// dcss, dopewars, and rebels doors' domains so the same configured secret
/// could never produce a key valid for another service.
const KEY_DOMAIN: &[u8] = b"late.sh/usurper/v1\0usurper\0";

/// Derive the single Ed25519 client key from the configured shared secret.
///
/// late.sh owns both ends of this connection, so we do not need a per-user key:
/// the key proves *authorization* (the connection came from late-ssh, which
/// holds the same secret), while the SSH username carries *identity* (the
/// arcade handle the host writes into the session's DOOR32.SYS dropfile). The
/// server accepts exactly this one derived public key; both ends recompute it
/// from `LATE_USURPER_SECRET`.
pub fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh usurper derived").expect("valid ed25519 key")
}

// CROSS-CRATE CONTRACT: `KEY_DOMAIN` and every derivation step above MUST stay
// byte-identical to late-usurper's `identity::derive_client_key`. If they
// drift, the client derives a different key and the host rejects every
// connection, so the contract is pinned by the known-answer test in
// identity_test.rs, which must match the identical KAT in late-usurper.
