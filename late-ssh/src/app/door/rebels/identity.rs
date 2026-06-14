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

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::HashAlg;

    fn fingerprint(id: &RebelsIdentity) -> String {
        id.key.public_key().fingerprint(HashAlg::Sha256).to_string()
    }

    #[test]
    fn username_is_stable_and_within_rebels_bounds() {
        let id = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let a = derive_identity("secret", id);
        let b = derive_identity("secret", id);
        assert_eq!(a.username, b.username);
        assert!((3..=16).contains(&a.username.len()));
        assert_eq!(a.username.len(), 12);
        assert!(a.username.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn key_is_deterministic_for_same_user_and_secret() {
        let id = Uuid::from_u128(99);
        assert_eq!(
            fingerprint(&derive_identity("s", id)),
            fingerprint(&derive_identity("s", id))
        );
    }

    #[test]
    fn different_users_get_different_keys_and_usernames() {
        let a = derive_identity("secret", Uuid::from_u128(1));
        let b = derive_identity("secret", Uuid::from_u128(2));
        assert_ne!(a.username, b.username);
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn secret_changes_key_and_username() {
        let id = Uuid::from_u128(7);
        let a = derive_identity("a", id);
        let b = derive_identity("b", id);
        assert_ne!(a.username, b.username);
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }
}
