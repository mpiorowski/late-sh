use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
use uuid::Uuid;

const KEY_DOMAIN: &[u8] = b"late.sh/rebels/v1\0rebels\0";
const USER_DOMAIN: &[u8] = b"late.sh/rebels/user/v1";

/// rebels requires usernames of 3..=16 chars; we emit exactly 12 hex chars
/// (the first 6 bytes of the username hash, hex-encoded).
const USERNAME_BYTES: usize = 6;

pub struct RebelsIdentity {
    pub username: String,
    pub key: PrivateKey,
}

/// Turn the configured secret (any length) into a 32-byte blake3 keyed-hash key.
fn master(secret: &str) -> [u8; 32] {
    *blake3::hash(secret.as_bytes()).as_bytes()
}

/// Domain-separated keyed hash of `domain || user_id` under the master key.
fn derive(master: &[u8; 32], domain: &[u8], user_id: Uuid) -> blake3::Hash {
    blake3::Hasher::new_keyed(master)
        .update(domain)
        .update(user_id.as_bytes())
        .finalize()
}

/// Derive a stable (username, Ed25519 key) for a late.sh account. The key is
/// forwarded to rebels via authenticate_publickey; rebels hashes its
/// `pk.to_string()` per username, so a stable key persists the save.
pub fn derive_identity(secret: &str, user_id: Uuid) -> RebelsIdentity {
    let master = master(secret);

    let seed = derive(&master, KEY_DOMAIN, user_id);
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    let key = PrivateKey::new(KeypairData::from(kp), "late.sh rebels derived")
        .expect("valid ed25519 key");

    let username = hex::encode(&derive(&master, USER_DOMAIN, user_id).as_bytes()[..USERNAME_BYTES]);

    RebelsIdentity { username, key }
}


