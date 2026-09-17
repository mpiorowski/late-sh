use super::write_session_modes_off;

#[test]
fn session_end_turns_off_every_mode_the_server_turns_on() {
    let mut out = Vec::new();
    write_session_modes_off(&mut out).expect("write");
    let out = String::from_utf8(out).expect("ascii");
    for mode in ["[?1000l", "[?1003l", "[?1006l", "[?2004l", "[?25h", "]111\x1b\\"] {
        assert!(
            out.contains(&format!("\x1b{mode}")),
            "{mode:?} missing from {out:?}"
        );
    }
    assert!(
        !out.contains("?1049l"),
        "leaving the alt screen twice would move the cursor: {out:?}"
    );
}
