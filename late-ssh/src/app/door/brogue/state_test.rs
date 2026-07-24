use super::*;

fn test_state(enabled: bool) -> State {
    State::new(StateConfig {
        user_id: uuid::Uuid::nil(),
        host: "127.0.0.1".to_string(),
        port: 2327,
        secret: String::new(),
        term: "xterm".to_string(),
        enabled,
        repaint: None,
        handle_svc: None,
    })
}

fn disabled_state() -> State {
    test_state(false)
}

/// Enabled but with no handle service (headless): the claim prompt is
/// reachable and validation runs, while nothing can spawn tasks.
fn promptable_state() -> State {
    test_state(true)
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
    // case that would cancel the commands menu.
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
    // F1 (both encodings) is consumed: late.sh remaps it to brogue's `?`
    // help, so it must not also be forwarded as the raw escape.
    assert!(state.intercept_input(b"\x1bOP"));
    assert!(state.intercept_input(b"\x1b[11~"));
    // Everything else falls through to be forwarded to brogue verbatim,
    // including a literal `?` (brogue's own help key).
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
fn prompt_consumes_printables_and_builds_the_name() {
    let mut state = promptable_state();
    assert_eq!(state.handle_status(), HandleStatus::Missing { error: None });
    // Valid handle bytes accumulate; every printable is consumed so a
    // stray `q` can't fall through to the global quit mid-word.
    for b in b"Gnoll_Fan" {
        assert!(state.launcher_key(*b));
    }
    assert_eq!(state.entry_input(), "Gnoll_Fan");
    // Rejected chars are still consumed, but don't land in the buffer.
    assert!(state.launcher_key(b'?'));
    assert!(state.launcher_key(b' '));
    assert!(state.launcher_key(b'q'));
    assert_eq!(state.entry_input(), "Gnoll_Fanq");
    // Backspace edits.
    assert!(state.launcher_key(0x7f));
    assert_eq!(state.entry_input(), "Gnoll_Fan");
    // Esc closes the claim modal (usually via the global escape dispatch;
    // the raw byte works too).
    assert!(state.launcher_key(0x1b));
    assert!(!state.name_modal_visible());
}

#[test]
fn modal_dismisses_on_esc_and_reopens_on_enter() {
    let mut state = promptable_state();
    assert!(state.name_modal_visible());
    state.dismiss_name_modal();
    assert!(!state.name_modal_visible());
    // While dismissed, printables are not ours (global keymap keeps them).
    assert!(!state.launcher_key(b'a'));
    assert_eq!(state.entry_input(), "");
    // Enter reopens the modal instead of submitting the hidden buffer.
    assert!(state.launcher_key(b'\r'));
    assert!(state.name_modal_visible());
    // A hub launch attempt reopens it too.
    state.dismiss_name_modal();
    state.connect();
    assert!(state.name_modal_visible());
}

#[test]
fn prompt_caps_the_buffer_at_max_len() {
    let mut state = promptable_state();
    for _ in 0..40 {
        state.launcher_key(b'a');
    }
    assert_eq!(
        state.entry_input().len(),
        late_core::models::arcade_handle::HANDLE_MAX_LEN
    );
}

#[test]
fn submit_surfaces_validation_errors() {
    let mut state = promptable_state();
    // Too short.
    state.launcher_key(b'a');
    state.launcher_key(b'\r');
    let HandleStatus::Missing { error: Some(msg) } = state.handle_status() else {
        panic!("expected a shape error");
    };
    assert!(msg.contains("3-20"));
    // Reserved: the buffer survives so the player can edit it.
    let mut state = promptable_state();
    for b in b"late_abc" {
        state.launcher_key(*b);
    }
    state.launcher_key(b'\n');
    let HandleStatus::Missing { error: Some(msg) } = state.handle_status() else {
        panic!("expected a reserved error");
    };
    assert!(msg.contains("reserved"));
    assert_eq!(state.entry_input(), "late_abc");
}

#[test]
fn launcher_keys_are_inert_when_disabled() {
    let mut state = disabled_state();
    assert!(!state.launcher_key(b'a'));
    assert!(!state.launcher_key(b'\r'));
    assert_eq!(state.entry_input(), "");
}

#[test]
fn is_f1_matches_both_encodings() {
    assert!(is_f1(b"\x1bOP"));
    assert!(is_f1(b"\x1b[11~"));
    assert!(!is_f1(b"\x1b[A"));
    assert!(!is_f1(b"?"));
}
