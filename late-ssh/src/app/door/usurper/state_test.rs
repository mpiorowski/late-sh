use super::state::{HandleStatus, Mode, State, strip_input_noise};

fn disabled_state() -> State {
    State::new(
        uuid::Uuid::nil(),
        "127.0.0.1".to_string(),
        2326,
        String::new(),
        "xterm".to_string(),
        false,
        None,
        None,
    )
}

/// Enabled but with no handle service (headless): the claim prompt is
/// reachable and validation runs, while nothing can spawn tasks.
fn promptable_state() -> State {
    State::new(
        uuid::Uuid::nil(),
        "127.0.0.1".to_string(),
        2326,
        String::new(),
        "xterm".to_string(),
        true,
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
    state.forward_input(b"q\r");
}

#[test]
fn strip_input_noise_drops_mouse_and_paste_keeps_keys() {
    assert_eq!(strip_input_noise(b"\x1b[<35;10;5M?"), b"?");
    assert_eq!(strip_input_noise(b"a\x1b[Mabcb"), b"ab");
    assert_eq!(strip_input_noise(b"\x1b[200~hi\x1b[201~"), b"hi");
    assert_eq!(strip_input_noise(b"PQ\r"), b"PQ\r");
}

#[test]
fn strip_input_noise_drops_sysop_function_keys() {
    // SS3 F1-F4: in local mode these are DDPlus sysop keys, never the player's.
    assert_eq!(strip_input_noise(b"\x1bOP"), b"");
    assert_eq!(strip_input_noise(b"\x1bOS"), b"");
    // CSI forms: F1 (11~), F2 (12~, sysop chat), F7/F8 (18~/19~, time adjust),
    // F10 (21~, eject), F12 (24~).
    assert_eq!(strip_input_noise(b"\x1b[11~"), b"");
    assert_eq!(strip_input_noise(b"\x1b[12~"), b"");
    assert_eq!(strip_input_noise(b"\x1b[18~x\x1b[19~"), b"x");
    assert_eq!(strip_input_noise(b"\x1b[21~"), b"");
    assert_eq!(strip_input_noise(b"\x1b[24~"), b"");
    // Linux-console F1: ESC [ [ A
    assert_eq!(strip_input_noise(b"\x1b[[A"), b"");
}

#[test]
fn strip_input_noise_keeps_arrows_and_nav_keys() {
    // Arrows must survive (menus use them).
    assert_eq!(strip_input_noise(b"\x1b[A\x1b[B"), b"\x1b[A\x1b[B");
    // Home/Del/PgUp-style CSI codes are not F-keys and pass through.
    assert_eq!(strip_input_noise(b"\x1b[3~"), b"\x1b[3~");
    // A truncated escape falls through unchanged.
    assert_eq!(strip_input_noise(b"\x1b[1"), b"\x1b[1");
}

#[test]
fn exit_grace_opens_on_close_and_counts_down() {
    let mut state = disabled_state();
    // Simulate a game that has exited: in Running with no proxy, the next
    // tick returns to the Launcher and opens the input grace.
    state.force_running_for_test();
    assert!(!state.in_exit_grace());
    state.tick();
    assert_eq!(state.mode(), Mode::Launcher);
    assert!(state.in_exit_grace());
    // The grace counts down once per tick and eventually clears, so the
    // launcher does not swallow input forever.
    while state.in_exit_grace() {
        state.tick();
    }
    assert!(!state.in_exit_grace());
}

#[test]
fn prompt_consumes_printables_and_builds_the_name() {
    let mut state = promptable_state();
    assert_eq!(state.handle_status(), HandleStatus::Missing { error: None });
    for b in b"Gnoll_Fan" {
        assert!(state.launcher_key(*b));
    }
    assert_eq!(state.entry_input(), "Gnoll_Fan");
    // Backspace edits; Esc closes the claim modal.
    assert!(state.launcher_key(0x7f));
    assert_eq!(state.entry_input(), "Gnoll_Fa");
    assert!(state.launcher_key(0x1b));
    assert!(!state.name_modal_visible());
}

#[test]
fn launcher_keys_are_inert_when_disabled() {
    let mut state = disabled_state();
    assert!(!state.launcher_key(b'a'));
    assert!(!state.launcher_key(b'\r'));
    assert_eq!(state.entry_input(), "");
}
