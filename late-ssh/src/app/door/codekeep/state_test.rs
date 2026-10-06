use super::*;

fn disabled_state() -> State {
    State::new(
        uuid::Uuid::nil(),
        "127.0.0.1".to_string(),
        2328,
        String::new(),
        "xterm-256color".to_string(),
        false,
        None,
    )
}

#[test]
fn connect_is_a_no_op_when_disabled() {
    let mut state = disabled_state();
    state.connect();
    assert!(state.proxy().is_none());
    assert_eq!(state.mode(), Mode::Launcher);
}

#[test]
fn strip_input_noise_drops_mouse_and_paste_markers() {
    assert_eq!(strip_input_noise(b"\x1b[<35;10;5MJ"), b"J");
    assert_eq!(strip_input_noise(b"a\x1b[Mabcb"), b"ab");
    assert_eq!(strip_input_noise(b"\x1b[200~hi\x1b[201~"), b"hi");
}

#[test]
fn strip_input_noise_keeps_keys_and_arrows() {
    assert_eq!(strip_input_noise(b"qhjkl"), b"qhjkl");
    assert_eq!(strip_input_noise(b"\x1b[A\x1b[B"), b"\x1b[A\x1b[B");
}

#[test]
fn closed_proxy_returns_to_launcher() {
    let mut state = disabled_state();
    state.mode = Mode::Running;
    state.tick();
    assert_eq!(state.mode(), Mode::Launcher);
}
