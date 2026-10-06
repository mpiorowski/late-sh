use super::*;

#[test]
fn keeps_alphanumerics_and_underscore() {
    assert_eq!(sanitize("late_9f3c1122"), "late_9f3c1122");
}

#[test]
fn strips_punctuation_and_shell_metachars() {
    assert_eq!(sanitize("bob; rm -rf /"), "bobrmrf");
    assert_eq!(sanitize("a b\tc"), "abc");
}

#[test]
fn caps_at_pl_nsiz() {
    let name = sanitize(&"x".repeat(100));
    assert_eq!(name.len(), PL_NSIZ_USABLE);
}

#[test]
fn empty_falls_back() {
    assert_eq!(sanitize(""), FALLBACK);
    assert_eq!(sanitize("!@#$"), FALLBACK);
}
