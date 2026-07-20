use crate::identity::*;
use russh::keys::HashAlg;

const KAT_FINGERPRINT: &str = "SHA256:EOzwdMGl+YxD1ERqkpMknze4OddMl4M6J2j8PdbZOYo";

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
// usurper client. Determinism alone would pass even if KEY_DOMAIN or a
// derivation step drifted on one side only.
#[test]
fn known_answer_fingerprint_is_stable() {
    assert_eq!(fingerprint("late-usurper-kat-v1"), KAT_FINGERPRINT);
}
