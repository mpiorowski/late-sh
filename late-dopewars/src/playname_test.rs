use super::*;

#[test]
fn keeps_valid_late_username_characters() {
    assert_eq!(sanitize("bob"), "bob");
    assert_eq!(sanitize("bob.smith-42_x"), "bob.smith-42_x");
}

#[test]
fn strips_punctuation_and_shell_metachars() {
    assert_eq!(sanitize("bob; rm -rf /"), "bobrm-rf");
    assert_eq!(sanitize("a b\tc"), "abc");
}

#[test]
fn caps_at_name_limit() {
    assert_eq!(sanitize(&"x".repeat(100)).len(), MAX_NAME_LENGTH);
}

#[test]
fn empty_falls_back() {
    assert_eq!(sanitize(""), FALLBACK);
    assert_eq!(sanitize("!@#$"), FALLBACK);
}
