use super::*;

#[test]
fn hostile_term_falls_back() {
    assert_eq!(effective_term("../../etc/passwd"), "xterm-256color");
    assert_eq!(effective_term(""), "xterm-256color");
}

#[test]
fn safe_term_passes_through() {
    assert_eq!(effective_term("xterm-ghostty"), "xterm-ghostty");
}
