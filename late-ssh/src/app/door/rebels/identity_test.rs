use uuid::Uuid;
use crate::app::door::rebels::identity::*;
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
