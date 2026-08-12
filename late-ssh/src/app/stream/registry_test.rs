use std::time::Instant;

use uuid::Uuid;

use super::{
    PENDING_TTL, PUBLISHER_GRACE, PUBLISHER_TTL, PublisherReport, StreamRegistry, WATCHER_TTL,
    WATCHERS_MAX,
};

fn ids() -> (Uuid, Uuid, Uuid) {
    (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7())
}

#[test]
fn begin_is_one_stream_per_user_and_updates_title() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();

    let first = registry.begin(user, "mat", "first title", room, channel);
    let second = registry.begin(user, "mat", "second title", room, channel);

    assert_eq!(first.stream_id, second.stream_id);
    assert_eq!(first.publish_token, second.publish_token);
    let info = registry
        .publisher_info(&first.publish_token)
        .expect("publisher info");
    assert_eq!(info.title, "second title");
    assert_eq!(info.username, "mat");
}

#[test]
fn pending_streams_appear_in_the_snapshot_as_not_live() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();

    let handles = registry.begin(user, "mat", "quiet", room, channel);

    // The rail's "stream" section lists a stream from /golive on; the
    // announcement and LIVE tag key off `live`, which waits for media.
    let snapshot = registry.snapshot();
    assert_eq!(snapshot.streams.len(), 1);
    assert!(!snapshot.streams[0].live);
    // The watch URL already resolves (the page shows "not live yet").
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(!view.live);
}

#[test]
fn first_media_report_goes_live_exactly_once() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);

    let first = registry.report_publisher(&handles.publish_token, true, false, None);
    let second = registry.report_publisher(&handles.publish_token, true, false, None);

    assert_eq!(first, PublisherReport::Live { went_live: true });
    assert_eq!(second, PublisherReport::Live { went_live: false });
    let snapshot = registry.snapshot();
    assert_eq!(snapshot.streams.len(), 1);
    assert!(snapshot.streams[0].live);
    assert_eq!(snapshot.streams[0].title, "show");
}

#[test]
fn unknown_publish_token_reports_gone() {
    let registry = StreamRegistry::new();
    assert_eq!(
        registry.report_publisher("nope", true, false, None),
        PublisherReport::Gone
    );
}

#[test]
fn publisher_stop_keeps_the_stream_in_grace() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    let outcome = registry.report_publisher(&handles.publish_token, false, false, None);

    assert_eq!(outcome, PublisherReport::Stopped);
    // Grace still counts as live so the room row does not flicker out on a
    // page refresh; a re-report resumes without a second announcement.
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(
        registry.report_publisher(&handles.publish_token, true, false, None),
        PublisherReport::Live { went_live: false }
    );
}

#[test]
fn watch_heartbeats_drive_the_watching_count() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-a"));
    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-b"));
    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-a"));

    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert_eq!(view.watching, 2);
    assert!(!registry.watch_heartbeat("unknown-stream", "viewer-a"));
}

#[test]
fn mic_state_reaches_the_view() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);

    registry.report_publisher(&handles.publish_token, true, true, None);
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.mic_on_air);

    registry.report_publisher(&handles.publish_token, true, false, None);
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(!view.mic_on_air);
}

#[test]
fn end_for_user_kills_the_watch_url() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    // The ended stream carries the voice channel so the caller can
    // force-disconnect the publisher's LiveKit session.
    let ended = registry.end_for_user(user).expect("ended stream");
    assert_eq!(ended.user_id, user);
    assert_eq!(ended.voice_channel_id, channel);
    assert!(registry.end_for_user(user).is_none());

    assert!(registry.watch_view(&handles.stream_id).is_none());
    assert!(registry.publisher_info(&handles.publish_token).is_none());
    assert!(registry.snapshot().streams.is_empty());
}

#[test]
fn stream_lookup_by_username_is_case_insensitive() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "Mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    let view = registry
        .stream_for_username("mat")
        .expect("stream for username");
    assert_eq!(view.user_id, user);
    assert!(registry.stream_for_username("someone-else").is_none());
}

#[test]
fn sweep_keeps_fresh_streams() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);
    registry.watch_heartbeat(&handles.stream_id, "viewer-a");

    assert!(registry.sweep().is_empty());

    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(view.watching, 1);
}

#[test]
fn sweep_expires_a_pending_stream_after_the_ttl() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "never shown", room, channel);

    assert!(registry.sweep_at(Instant::now()).is_empty());
    let ended = registry.sweep_at(Instant::now() + PENDING_TTL);

    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].user_id, user);
    assert_eq!(ended[0].voice_channel_id, channel);
    assert!(registry.watch_view(&handles.stream_id).is_none());
    assert!(registry.snapshot().streams.is_empty());
}

#[test]
fn sweep_moves_a_stale_publisher_into_grace_then_tears_down() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    // Publisher silent past its TTL: grace, still shown as live (the room
    // row must not flicker out on a page refresh).
    let stale_at = Instant::now() + PUBLISHER_TTL;
    assert!(registry.sweep_at(stale_at).is_empty());
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);

    // Grace runs out: torn down, capability URLs dead.
    let ended = registry.sweep_at(stale_at + PUBLISHER_GRACE);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].user_id, user);
    assert!(registry.watch_view(&handles.stream_id).is_none());
}

#[test]
fn sweep_prunes_stale_watchers() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);
    registry.watch_heartbeat(&handles.stream_id, "viewer-a");

    let ended = registry.sweep_at(Instant::now() + WATCHER_TTL);

    // WATCHER_TTL > PUBLISHER_TTL, so the same sweep also moves the silent
    // publisher into grace; the stream survives this pass (still shown as
    // live), the stale watcher does not.
    assert!(ended.is_empty());
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(view.watching, 0);
}

#[test]
fn watcher_cap_bounds_the_watching_count() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false, None);

    for i in 0..(WATCHERS_MAX + 25) {
        assert!(registry.watch_heartbeat(&handles.stream_id, &format!("viewer-{i}")));
    }

    // New ids past the cap are dropped; known ids still refresh.
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert_eq!(view.watching, WATCHERS_MAX);
    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-0"));
}

#[test]
fn publisher_claim_locks_the_token_to_the_first_caller() {
    use super::PublisherAccess;
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);

    // First grant fetch claims the token and mints the secret.
    let secret = match registry.access_publisher(&handles.publish_token, None) {
        PublisherAccess::Granted {
            new_claim: Some(secret),
            info,
        } => {
            assert_eq!(info.user_id, user);
            secret
        }
        other => panic!("expected a claiming grant, got {other:?}"),
    };

    // A bare replay of the leaked URL is refused, grant and report alike.
    assert_eq!(
        registry.access_publisher(&handles.publish_token, None),
        PublisherAccess::Denied
    );
    assert_eq!(
        registry.access_publisher(&handles.publish_token, Some("wrong")),
        PublisherAccess::Denied
    );
    assert_eq!(
        registry.report_publisher(&handles.publish_token, false, false, None),
        PublisherReport::Denied
    );

    // The claiming console keeps working: refetches and reports pass.
    assert!(matches!(
        registry.access_publisher(&handles.publish_token, Some(&secret)),
        PublisherAccess::Granted {
            new_claim: None,
            ..
        }
    ));
    assert_eq!(
        registry.report_publisher(&handles.publish_token, true, false, Some(&secret)),
        PublisherReport::Live { went_live: true }
    );

    // A fresh stream after a stop starts unclaimed with new ids.
    registry.end_for_user(user).expect("ended stream");
    let fresh = registry.begin(user, "mat", "next show", room, channel);
    assert_ne!(fresh.publish_token, handles.publish_token);
    assert!(matches!(
        registry.access_publisher(&fresh.publish_token, None),
        PublisherAccess::Granted {
            new_claim: Some(_),
            ..
        }
    ));
    assert_eq!(
        registry.access_publisher(&handles.publish_token, None),
        PublisherAccess::Gone
    );
}
