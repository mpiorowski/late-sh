use crate::identity::*;
use russh::keys::HashAlg;

const KAT_FINGERPRINT: &str = "SHA256:9NHbIJzzfj+WQ4YoYYlgWtjvH7N+FE2m1KYAp3X/73c";

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
// door::bashquest::identity. If the two crates' KEY_DOMAIN or derivation ever
// drift, the client derives a different key and the host rejects every
// connection -- so pin the cross-crate contract to a fixed fingerprint here.
#[test]
fn known_answer_fingerprint_is_stable() {
    assert_eq!(fingerprint("late-bashquest-kat-v1"), KAT_FINGERPRINT);
}
