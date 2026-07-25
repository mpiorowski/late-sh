use russh::keys::HashAlg;

use crate::identity::*;

const KAT_FINGERPRINT: &str = "SHA256:lee3nvup/gosJ6lByidqJqfW/mzkNy7c0JEN+50Ppyc";

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

// Known-answer test: this MUST match the identical KAT in the late-ssh
// brogue client. Determinism alone would pass even if KEY_DOMAIN or a
// derivation step drifted on one side only.
#[test]
fn known_answer_fingerprint_is_stable() {
    assert_eq!(fingerprint("late-brogue-kat-v1"), KAT_FINGERPRINT);
}
