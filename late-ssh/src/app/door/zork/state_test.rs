use super::super::protocol::Slot;
use super::*;

fn state() -> State {
    let mut state = State::new(StateConfig {
        user_id: uuid::Uuid::from_u128(1),
        host: "127.0.0.1".into(),
        port: 1,
        secret: "test".into(),
        term: "xterm".into(),
        enabled: false,
        repaint: None,
    });
    state.enabled = true;
    state.catalogue = Some(Catalogue {
        editions: Edition::ALL
            .into_iter()
            .map(|edition| EditionSlots {
                edition,
                autosave: Slot::missing(),
                manual: Slot::missing(),
            })
            .collect(),
        active: None,
    });
    state
}
#[test]
fn menus_do_not_create_sessions_and_default_confirmations_cancel() {
    let mut state = state();
    state.menu_key(b'\r');
    assert_eq!(state.mode(), Mode::Actions);
    assert!(!state.available(Action::Auto));
    assert!(!state.available(Action::Manual));
    state.menu_key(b'\r');
    assert!(state.proxy.is_none());
    state.step(true);
    state.step(true);
    state.menu_key(b'\r');
    assert_eq!(state.mode(), Mode::Confirm(Action::New));
    state.menu_key(b'\r');
    assert_eq!(state.mode(), Mode::Actions);
    assert!(state.proxy.is_none());
    assert!(!state.back());
    assert!(state.back());
}
#[test]
fn invalid_saves_are_disabled_and_live_continue_wins_over_missing_auto() {
    let mut state = state();
    state.catalogue.as_mut().unwrap().editions[0].manual.status = Availability::Invalid;
    assert!(!state.available(Action::Manual));
    state.live = Some(Edition::Zork1);
    assert!(state.available(Action::Auto));
    state.edition = Edition::Zork2;
    assert!(!state.available(Action::Auto));
}
#[test]
fn mouse_and_paste_wrappers_are_removed_but_game_controls_remain() {
    assert_eq!(
        InputFilter::default().feed(b"\x1b[<35;8;9M\x1b[200~look\x1b[201~\r\x13\x1b[A"),
        b"look\r\x13\x1b[A"
    );
}
#[test]
fn filtering_is_independent_of_ssh_packet_boundaries_and_escape_still_cancels() {
    let input = b"\x1b[<35;8;9M\x1b[Mabc\x1b[200~look\x1b[201~\r\x13\x1b[A\x1bOB";
    for split in 0..input.len() {
        let mut filter = InputFilter::default();
        let mut out = filter.feed(&input[..split]);
        out.extend(filter.feed(&input[split..]));
        assert_eq!(out, b"look\r\x13\x1b[A\x1bOB", "split {split}");
        assert!(filter.pending.is_empty());
    }
    let mut filter = InputFilter::default();
    assert!(filter.feed(b"\x1b").is_empty());
    assert_eq!(filter.flush(), b"\x1b");
    assert!(filter.feed(b"\x1b[<35;").is_empty());
    assert!(filter.flush().is_empty());
}
#[tokio::test]
async fn failed_return_keeps_the_live_game_and_idle_timeout_reaps_it() {
    let mut state = state();
    state.enabled = false; // No metadata I/O for the synchronous timer check.
    state.start(Edition::Zork1, Action::New);
    state.pending = Some((
        Edition::Zork2,
        Action::Auto,
        Instant::now() - RETURN_TIMEOUT,
    ));
    state.tick();
    assert_eq!(state.live(), Some(Edition::Zork1));
    assert!(state.proxy().is_some());
    assert_eq!(state.mode(), Mode::Actions);
    assert!(state.message().contains("still open"));
    state.last_input = Instant::now() - IDLE_SHUTDOWN;
    state.tick();
    assert!(!state.is_running());
    assert!(state.proxy().is_none());
    assert!(state.message().contains("20 minutes"));
}
