use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// CROSS-CRATE CONTRACT: this domain and derivation must stay byte-identical to
/// the peer CodeKeep identity module.
const KEY_DOMAIN: &[u8] = b"late.sh/codekeep/v1\0codekeep\0";

/// Derive the single Ed25519 client key from `LATE_CODEKEEP_SECRET`.
pub(crate) fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh codekeep derived").expect("valid ed25519 key")
}
