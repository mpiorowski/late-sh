use super::identity::*;
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

#[path = "../../../../../late-zork/src/identity.rs"]
mod host_identity;
#[test]
fn host_and_client_derive_identical_credentials() {
    assert_eq!(
        derive_client_key("zork-contract").public_key().key_data(),
        host_identity::derive_client_key("zork-contract")
            .public_key()
            .key_data()
    );
    assert_eq!(
        super::proxy::session_label(uuid::Uuid::from_u128(0xabcd)),
        "late_00000000000000000000abcd"
    );
}
