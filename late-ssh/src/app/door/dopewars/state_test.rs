use super::*;

fn disabled_state() -> State {
    State::new(
        uuid::Uuid::nil(),
        "127.0.0.1".to_string(),
        2324,
        String::new(),
        "xterm".to_string(),
        false,
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
    state.forward_input(b"hjkl");
}

#[test]
fn strip_input_noise_drops_mouse_keeps_keys() {
    assert_eq!(strip_input_noise(b"\x1b[<35;10;5MJ"), b"J");
    assert_eq!(strip_input_noise(b"J\x1b[<35;10;5m"), b"J");
    assert_eq!(strip_input_noise(b"a\x1b[Mabcb"), b"ab");
    assert_eq!(strip_input_noise(b"\x1b[200~hi\x1b[201~"), b"hi");
}

#[test]
fn strip_input_noise_passes_keys_and_arrows() {
    assert_eq!(strip_input_noise(b"hjkl"), b"hjkl");
    assert_eq!(strip_input_noise(b"\x1b[A\x1b[B"), b"\x1b[A\x1b[B");
}

#[test]
fn exit_grace_opens_on_close_and_counts_down() {
    let mut state = disabled_state();
    state.mode = Mode::Running;
    assert!(!state.in_exit_grace());
    state.tick();
    assert_eq!(state.mode(), Mode::Launcher);
    assert!(state.in_exit_grace());
    for _ in 0..EXIT_GRACE_TICKS {
        assert!(state.in_exit_grace());
        state.tick();
    }
    assert!(!state.in_exit_grace());
}
