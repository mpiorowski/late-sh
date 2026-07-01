use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Distinct from the nethack and
/// rebels doors' domains so the same configured secret could never produce a key
/// valid for another service.
///
/// CROSS-CRATE CONTRACT: this and every derivation step below MUST stay
/// byte-identical to the host's `late-dopewars/src/identity.rs`. If they drift,
/// the client derives a different key and the host rejects every connection.
const KEY_DOMAIN: &[u8] = b"late.sh/dopewars/v1\0dopewars\0";

/// Derive the single Ed25519 client key from the configured shared secret. Both
/// ends recompute it from `LATE_DOPEWARS_SECRET`; the host accepts exactly this
/// one derived public key. See the cross-crate note on `KEY_DOMAIN`.
pub fn derive_client_key(secret: &str) -> PrivateKey {
    let master = blake3::hash(secret.as_bytes());
    let seed = blake3::Hasher::new_keyed(master.as_bytes())
        .update(KEY_DOMAIN)
        .finalize();
    let kp = Ed25519Keypair::from_seed(seed.as_bytes());
    PrivateKey::new(KeypairData::from(kp), "late.sh dopewars derived").expect("valid ed25519 key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::HashAlg;

    fn fingerprint(secret: &str) -> String {
        derive_client_key(secret)
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string()
    }

    #[test]
    fn key_is_deterministic_for_same_secret() {
        assert_eq!(fingerprint("s3cret"), fingerprint("s3cret"));
    }

    #[test]
    fn different_secrets_yield_different_keys() {
        assert_ne!(fingerprint("a"), fingerprint("b"));
    }
}
