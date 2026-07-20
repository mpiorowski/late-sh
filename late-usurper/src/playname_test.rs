use crate::playname::*;

#[test]
fn keeps_alphanumerics_and_underscore() {
    assert_eq!(sanitize("Gnoll_Fan9"), "Gnoll_Fan9");
}

#[test]
fn strips_whitespace_and_metachars() {
    // A space would split the name into first/last in the dropfile.
    assert_eq!(sanitize("bob smith"), "bobsmith");
    assert_eq!(sanitize("a\nb; rm -rf /"), "abrmrf");
}

#[test]
fn caps_at_handle_limit() {
    assert_eq!(sanitize(&"x".repeat(100)).len(), 20);
}

#[test]
fn empty_falls_back() {
    assert_eq!(sanitize(""), "late");
    assert_eq!(sanitize("!@#$"), "late");
}
