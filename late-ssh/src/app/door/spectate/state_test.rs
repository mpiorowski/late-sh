use super::*;

#[test]
fn a_watch_ends_off_the_hub_or_with_its_stream() {
    assert_eq!(
        end_reason(false, WatchStatus::Watching, "alice"),
        Some(WatchEnd::LeftHub)
    );
    assert_eq!(
        end_reason(true, WatchStatus::Ended, "alice"),
        Some(WatchEnd::GameEnded("alice".to_string()))
    );
    assert_eq!(end_reason(true, WatchStatus::Connecting, "alice"), None);
    assert_eq!(end_reason(true, WatchStatus::Watching, "alice"), None);
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
