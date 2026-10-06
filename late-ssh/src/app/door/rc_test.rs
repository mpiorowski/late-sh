use super::*;

#[test]
fn sanitize_keeps_structure_and_drops_hostile_controls() {
    // CRLF and bare CR normalize to LF; tabs survive (real rc files use them).
    assert_eq!(
        sanitize_rc_paste("OPTIONS=color\r\nOPTIONS=\tautopickup\rdone"),
        "OPTIONS=color\nOPTIONS=\tautopickup\ndone"
    );
    // ESC loses its escape byte so a paste cannot smuggle terminal sequences;
    // other control bytes vanish outright.
    assert_eq!(sanitize_rc_paste("a\x1b[31mred\x07b\x7f"), "a[31mredb");
}
