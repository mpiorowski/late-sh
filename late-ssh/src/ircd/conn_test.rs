use super::nick_from_ban_mask;

#[test]
fn ban_mask_accepts_nick_identity_shape() {
    assert_eq!(nick_from_ban_mask("alice!*@*"), Some("alice"));
    assert_eq!(nick_from_ban_mask("Alice_123!*@*"), Some("Alice_123"));
}

#[test]
fn ban_mask_rejects_wildcards_hosts_and_plain_nicks() {
    assert_eq!(nick_from_ban_mask("*!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice!*@example.com"), None);
    assert_eq!(nick_from_ban_mask("alice@host!*@*"), None);
    assert_eq!(nick_from_ban_mask("alice"), None);
}
