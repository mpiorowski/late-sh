use super::*;

#[test]
fn canonical_dm_pair_orders_smaller_first() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    assert_eq!(canonical_dm_pair(a, b), (a, b));
    assert_eq!(canonical_dm_pair(b, a), (a, b));
}

#[test]
fn canonical_dm_pair_equal_uuids() {
    let a = Uuid::from_u128(42);
    let (x, y) = canonical_dm_pair(a, a);
    assert_eq!(x, a);
    assert_eq!(y, a);
}

#[test]
fn normalize_topic_slug_slugifies_room_names() {
    assert_eq!(
        normalize_topic_slug("  Rust Nerds  ").unwrap(),
        "rust-nerds"
    );
    assert_eq!(normalize_topic_slug("room\nname").unwrap(), "room-name");
    assert_eq!(normalize_topic_slug("vps/d9d0").unwrap(), "vps-d9d0");
    assert_eq!(normalize_topic_slug("a___b...c").unwrap(), "a-b-c");
}

#[test]
fn normalize_topic_slug_rejects_empty_or_reserved_names() {
    assert!(normalize_topic_slug("   ").is_err());
    assert!(normalize_topic_slug("!!!").is_err());
    assert!(normalize_topic_slug("lounge").is_err());
}

#[test]
fn normalize_room_slug_allows_lounge_for_non_creation_paths() {
    assert_eq!(normalize_room_slug(" Lounge ").unwrap(), "lounge");
}
