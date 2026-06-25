use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};

/// Domain separation for the derived client key. Distinct from the rebels door's
/// domain so the same configured secret could never produce a key valid for both
/// services.
const KEY_DOMAIN: &[u8] = b"late.sh/nethack/v1\0nethack\0";

/// Derive the single Ed25519 client key from the configured shared secret.
///
/// Unlike the rebels door, late.sh owns both ends of this connection, so we do
/// not need a per-user key: the key proves *authorization* (the connection came
/// from late-ssh, which holds the same secret), while the SSH username carries
/// *identity* (the NetHack `-u` playname). The server accepts exactly this one
/// derived public key; both ends recompute it from `LATE_NETHACK_SECRET`.
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

    // Known-answer test: this MUST match the identical KAT in late-ssh's
    // door::nethack::identity. If the two crates' KEY_DOMAIN or derivation ever
    // drift, the client derives a different key and the host rejects every
    // connection -- so pin the cross-crate contract to a fixed fingerprint here.
    #[test]
    fn known_answer_fingerprint_is_stable() {
        assert_eq!(fingerprint("late-nethack-kat-v1"), KAT_FINGERPRINT);
    }
}

#[cfg(test)]
const KAT_FINGERPRINT: &str = "SHA256:JA9AvdNoX1ZZMA43t1qMUzq73OW609Fme6rrle84UeU";
