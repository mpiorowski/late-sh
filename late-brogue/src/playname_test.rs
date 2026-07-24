use super::*;

#[test]
fn keeps_alphanumerics_and_underscore() {
    assert_eq!(sanitize("Gnoll_Fan9"), "Gnoll_Fan9");
}

#[test]
fn strips_path_and_shell_metachars() {
    // Dots and slashes must never survive into a path component.
    assert_eq!(sanitize("../../etc/passwd"), "etcpasswd");
    assert_eq!(sanitize("bob; rm -rf /"), "bobrmrf");
    assert_eq!(sanitize("a b\tc"), "abc");
}

#[test]
fn caps_at_name_limit() {
    let name = sanitize(&"x".repeat(100));
    assert_eq!(name.len(), MAX_NAME_LENGTH);
}

#[test]
fn empty_falls_back() {
    assert_eq!(sanitize(""), FALLBACK);
    assert_eq!(sanitize("!@#$"), FALLBACK);
}
