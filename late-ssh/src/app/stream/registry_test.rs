use uuid::Uuid;

use super::{PublisherReport, StreamRegistry};

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

    let first = registry.report_publisher(&handles.publish_token, true, false);
    let second = registry.report_publisher(&handles.publish_token, true, false);

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
        registry.report_publisher("nope", true, false),
        PublisherReport::Gone
    );
}

#[test]
fn publisher_stop_keeps_the_stream_in_grace() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false);

    let outcome = registry.report_publisher(&handles.publish_token, false, false);

    assert_eq!(outcome, PublisherReport::Stopped);
    // Grace still counts as live so the room row does not flicker out on a
    // page refresh; a re-report resumes without a second announcement.
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(
        registry.report_publisher(&handles.publish_token, true, false),
        PublisherReport::Live { went_live: false }
    );
}

#[test]
fn watch_heartbeats_drive_the_watching_count() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false);

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

    registry.report_publisher(&handles.publish_token, true, true);
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.mic_on_air);

    registry.report_publisher(&handles.publish_token, true, false);
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(!view.mic_on_air);
}

#[test]
fn end_for_user_kills_the_watch_url() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false);

    assert!(registry.end_for_user(user));
    assert!(!registry.end_for_user(user));

    assert!(registry.watch_view(&handles.stream_id).is_none());
    assert!(registry.publisher_info(&handles.publish_token).is_none());
    assert!(registry.snapshot().streams.is_empty());
}

#[test]
fn stream_lookup_by_username_is_case_insensitive() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = registry.begin(user, "Mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, false);

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
    registry.report_publisher(&handles.publish_token, true, false);
    registry.watch_heartbeat(&handles.stream_id, "viewer-a");

    registry.sweep();

    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(view.watching, 1);
}
