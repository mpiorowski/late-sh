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

    // Known-answer test: this MUST match the identical KAT in the late-nethack
    // crate's identity module. If the two crates' KEY_DOMAIN or derivation ever
    // drift, this client derives a different key and the host rejects every
    // connection -- so pin the cross-crate contract to a fixed fingerprint here.
    #[test]
    fn known_answer_fingerprint_is_stable() {
        assert_eq!(
            fingerprint("late-nethack-kat-v1"),
            "SHA256:JA9AvdNoX1ZZMA43t1qMUzq73OW609Fme6rrle84UeU"
        );
    }
}
