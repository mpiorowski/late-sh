use super::*;
use std::time::Duration;

#[test]
fn a_preview_ends_off_the_hub_or_with_its_stream() {
    let preview = WatchMode::Preview;
    assert_eq!(
        end_reason(
            preview,
            false,
            Duration::ZERO,
            WatchStatus::Watching,
            "alice"
        ),
        Some(WatchEnd::LeftHub)
    );
    assert_eq!(
        end_reason(preview, true, Duration::ZERO, WatchStatus::Ended, "alice"),
        Some(WatchEnd::GameEnded("alice".to_string()))
    );
    assert_eq!(
        end_reason(
            preview,
            true,
            Duration::ZERO,
            WatchStatus::Connecting,
            "alice"
        ),
        None
    );
    assert_eq!(
        end_reason(
            preview,
            true,
            Duration::ZERO,
            WatchStatus::Watching,
            "alice"
        ),
        None
    );
}

#[test]
fn an_open_watch_outlives_a_hop_away_but_not_its_stream() {
    let open = WatchMode::Open;
    assert_eq!(
        end_reason(
            open,
            false,
            Duration::from_secs(60),
            WatchStatus::Watching,
            "alice"
        ),
        None
    );
    assert_eq!(
        end_reason(open, true, Duration::ZERO, WatchStatus::Watching, "alice"),
        None
    );
    assert_eq!(
        end_reason(
            open,
            false,
            Duration::from_secs(60),
            WatchStatus::Ended,
            "alice"
        ),
        Some(WatchEnd::GameEnded("alice".to_string()))
    );
}

#[test]
fn an_open_watch_off_screen_ends_after_the_away_window() {
    let open = WatchMode::Open;
    let just_short = AWAY_WINDOW - Duration::from_secs(1);
    assert_eq!(
        end_reason(open, false, just_short, WatchStatus::Watching, "alice"),
        None
    );
    assert_eq!(
        end_reason(open, false, AWAY_WINDOW, WatchStatus::Watching, "alice"),
        Some(WatchEnd::WentAway)
    );
}

#[test]
fn a_live_game_key_holds_a_handle_and_refuses_anything_else() {
    let key = LiveGameKey::new(SpectateGame::Dcss, "Mat_01").expect("a handle");
    assert_eq!(key.playname(), "Mat_01");
    assert_eq!(key.game(), SpectateGame::Dcss);
    let longest = "a".repeat(HANDLE_MAX_LEN);
    assert_eq!(
        LiveGameKey::new(SpectateGame::Dcss, &longest).map(|key| key.playname().to_string()),
        Some(longest.clone())
    );
    assert_eq!(
        LiveGameKey::new(SpectateGame::Dcss, &format!("{longest}a")),
        None
    );
    assert_eq!(LiveGameKey::new(SpectateGame::Dcss, "late_watch\t"), None);
}

#[test]
fn a_chat_link_resolves_once_then_joins_once() {
    let room = Uuid::now_v7();
    let mut link = ChatLink::new(SpectateGame::Dcss, "alice".to_string());
    assert_eq!(link.step(None), ChatLinkStep::Resolve);
    // Still in flight: nothing is asked for twice, and there is no room yet.
    assert_eq!(link.step(None), ChatLinkStep::Idle);
    assert_eq!(link.room_id(), None);
    assert_eq!(link.step(Some(room)), ChatLinkStep::Join(room));
    assert_eq!(link.room_id(), Some(room));
    assert_eq!(link.step(Some(room)), ChatLinkStep::Idle);
}

#[test]
fn a_chat_link_to_a_known_room_skips_the_resolve() {
    let room = Uuid::now_v7();
    let mut link = ChatLink::new(SpectateGame::Dcss, "alice".to_string());
    assert_eq!(link.step(Some(room)), ChatLinkStep::Join(room));
    assert_eq!(link.step(Some(room)), ChatLinkStep::Idle);
}
