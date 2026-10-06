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
