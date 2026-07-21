use super::*;

#[test]
fn unknown_term_falls_back_to_xterm_256color() {
    // A terminfo name the host cannot possibly have (and which is charset-valid)
    // must fall back so nethack doesn't abort with "Unknown terminal type".
    assert_eq!(
        effective_term("definitely-not-a-real-term-xyz"),
        "xterm-256color"
    );
}

#[test]
fn hostile_term_is_rejected_and_falls_back() {
    // Path-traversal / junk TERM never reaches the child's argv-env verbatim.
    assert_eq!(effective_term("../../etc/passwd"), "xterm-256color");
    assert_eq!(effective_term(""), "xterm-256color");
}

#[test]
fn supported_term_passes_through() {
    // xterm-256color ships in ncurses-base, so it is present anywhere tests run
    // (and on the host); it must pass through unchanged.
    assert_eq!(effective_term("xterm-256color"), "xterm-256color");
}
