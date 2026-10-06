use super::{AsterionSessions, SharedState, fallback_name, sanitize_username};
use asterion_core::Game;
use uuid::Uuid;

#[test]
fn sanitize_strips_control_chars_and_trims() {
    assert_eq!(
        sanitize_username("  alice\nbob\t  "),
        Some("alicebob".to_string())
    );
}

#[test]
fn sanitize_returns_none_for_blank_after_strip() {
    assert_eq!(sanitize_username("   \r\n\t  "), None);
}

#[test]
fn sanitize_keeps_unicode_graphemes() {
    assert_eq!(sanitize_username("björn"), Some("björn".to_string()));
}

#[test]
fn fallback_name_is_prefixed_and_eight_hex_chars() {
    let id = Uuid::nil();
    let name = fallback_name(id);
    assert_eq!(name, "u-00000000");
}

#[test]
fn sessions_only_remove_player_after_last_session_leaves() {
    let sessions = AsterionSessions::default();
    let user_id = Uuid::now_v7();
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();

    sessions.add(user_id, first);
    sessions.add(user_id, second);

    assert!(sessions.contains(user_id, first));
    assert!(!sessions.remove(user_id, first));
    assert!(sessions.contains(user_id, second));
    assert!(sessions.remove(user_id, second));
    assert!(!sessions.contains(user_id, second));
}

#[test]
fn drain_new_wins_clears_announced_state_after_reset() {
    let user_id = Uuid::now_v7();
    let mut state = SharedState::new(Uuid::now_v7(), Game::new().expect("game builds"));
    state.add_player(user_id, "alice", false);
    state.wins_announced.insert(user_id);

    assert!(state.drain_new_wins().is_empty());
    assert!(!state.wins_announced.contains(&user_id));
}
