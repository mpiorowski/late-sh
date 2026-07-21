use super::*;

fn disabled_state() -> State {
    State::new(
        uuid::Uuid::nil(),
        "127.0.0.1".to_string(),
        2323,
        String::new(),
        "xterm".to_string(),
        false,
        None,
        None,
        None,
    )
}

#[test]
fn connect_is_a_no_op_when_disabled() {
    let mut state = disabled_state();
    assert!(!state.is_enabled());
    state.connect();
    assert!(state.proxy().is_none());
    assert_eq!(state.mode(), Mode::Launcher);
}

#[test]
fn forward_input_without_proxy_is_a_no_op() {
    let state = disabled_state();
    // Must not panic when nothing is running.
    state.forward_input(b"hjkl");
}

#[test]
fn strip_input_noise_drops_mouse_keeps_keys() {
    // The `?` survives a motion report glued to it, which is exactly the
    // case that used to cancel the help menu.
    assert_eq!(strip_input_noise(b"\x1b[<35;10;5M?"), b"?");
    assert_eq!(strip_input_noise(b"?\x1b[<35;10;5m"), b"?");
    // Legacy X10 mouse and paste markers go too.
    assert_eq!(strip_input_noise(b"a\x1b[Mabcb"), b"ab");
    assert_eq!(strip_input_noise(b"\x1b[200~hi\x1b[201~"), b"hi");
}

#[test]
fn strip_input_noise_passes_keys_and_arrows() {
    assert_eq!(strip_input_noise(b"hjkl"), b"hjkl");
    // Arrow keys (ESC [ A …) must not be mistaken for mouse.
    assert_eq!(strip_input_noise(b"\x1b[A\x1b[B"), b"\x1b[A\x1b[B");
}

#[test]
fn f1_is_consumed_and_other_keys_pass_through() {
    let state = disabled_state();
    // F1 (both encodings) is consumed: late.sh remaps it to nethack's `?`
    // help, so it must not also be forwarded as the raw escape.
    assert!(state.intercept_input(b"\x1bOP"));
    assert!(state.intercept_input(b"\x1b[11~"));
    // Everything else falls through to be forwarded to nethack verbatim,
    // including a literal `?` (nethack's own help key).
    assert!(!state.intercept_input(b"?"));
    assert!(!state.intercept_input(b"hjkl"));
}

#[test]
fn exit_grace_opens_on_close_and_counts_down() {
    let mut state = disabled_state();
    // Simulate a game that has exited: in Running with no proxy, the next
    // tick returns to the Launcher and opens the input grace.
    state.mode = Mode::Running;
    assert!(!state.in_exit_grace());
    state.tick();
    assert_eq!(state.mode(), Mode::Launcher);
    assert!(state.in_exit_grace());
    // The grace counts down once per tick and eventually clears, so the
    // launcher does not swallow input forever.
    for _ in 0..EXIT_GRACE_TICKS {
        assert!(state.in_exit_grace());
        state.tick();
    }
    assert!(!state.in_exit_grace());
}

#[test]
fn is_f1_matches_both_encodings() {
    assert!(is_f1(b"\x1bOP"));
    assert!(is_f1(b"\x1b[11~"));
    assert!(!is_f1(b"\x1b[A"));
    assert!(!is_f1(b"?"));
}
