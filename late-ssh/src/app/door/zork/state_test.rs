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
        handle_svc: None,
    });
    state.enabled = true;
    state.force_handle_for_test("tester");
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
/// Every game is played under an arcade name, as on the roguelikes: without
/// one the claim prompt is over the menus, and choosing a game reopens it
/// instead of starting a session.
#[test]
fn without_a_handle_a_launch_opens_the_claim_prompt() {
    let mut state = State::new(StateConfig {
        user_id: uuid::Uuid::from_u128(1),
        host: "127.0.0.1".into(),
        port: 1,
        secret: "test".into(),
        term: "xterm".into(),
        enabled: false,
        repaint: None,
        handle_svc: None,
    });
    state.enabled = true;
    assert!(state.name_modal_visible());
    state.dismiss_name_modal();
    assert!(!state.name_modal_visible());

    state.menu_key(b'\r');
    state.step(true);
    state.step(true);
    state.menu_key(b'\r');
    assert_eq!(state.mode(), Mode::Confirm(Action::New));
    state.menu_key(b'y');
    assert!(state.name_modal_visible(), "the prompt is back");
    assert!(state.proxy.is_none(), "no session without a handle");
    assert_eq!(state.mode(), Mode::Actions);
}
/// A game chosen while the handle lookup is in flight starts the edition it
/// was chosen on, even when the player has moved to another edition by the
/// time the handle lands: a New confirmed for Zork I never replaces Zork II's
/// autosave without Zork II's own confirmation.
#[tokio::test]
async fn a_launch_waiting_on_the_handle_starts_the_edition_it_was_chosen_on() {
    use crate::app::door::arcade::ArcadeHandleService;
    use crate::test_helpers::{new_test_db, wait_until};

    let test_db = new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "zork-deferred").await;
    let handles = ArcadeHandleService::new(test_db.db.clone());
    handles
        .claim(user.id, "deferred")
        .await
        .expect("claim the handle");
    let mut state = State::new(StateConfig {
        user_id: user.id,
        host: "127.0.0.1".into(),
        port: 1,
        secret: "test".into(),
        term: "xterm".into(),
        enabled: true,
        repaint: None,
        handle_svc: Some(handles),
    });
    // Nothing has awaited, so the lookup is still in flight.
    assert_eq!(state.handle_status(), HandleStatus::Loading);

    // New on Zork I, confirmed, waits on the handle.
    state.menu_key(b'\r');
    state.step(true);
    state.step(true);
    state.menu_key(b'\r');
    state.menu_key(b'y');
    assert!(state.proxy.is_none(), "no session before the handle lands");

    // The player moves on to Zork II's actions.
    assert!(!state.back());
    state.step(true);
    state.menu_key(b'\r');
    assert_eq!(state.edition(), Edition::Zork2);

    let lookup = &state;
    wait_until(
        || async move { lookup.handle_status() != HandleStatus::Loading },
        "the handle lookup landed",
    )
    .await;
    state.tick();
    assert_eq!(state.live(), Some(Edition::Zork1));
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
