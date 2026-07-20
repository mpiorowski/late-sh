use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Must match late-nethack's
/// `identity::KEY_DOMAIN`; distinct from the rebels door's domain so the same
/// configured secret can never produce a key valid for both services.
const KEY_DOMAIN: &[u8] = b"late.sh/nethack/v1\0nethack\0";

/// Derive the Ed25519 client key from the configured shared secret. late.sh owns
/// both ends of this connection, so a single shared key is enough: it proves the
/// connection came from late-ssh, while the SSH username carries the playname.
/// The late-nethack host derives the same key and accepts only its public half.
pub fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh nethack derived").expect("valid ed25519 key")
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;

