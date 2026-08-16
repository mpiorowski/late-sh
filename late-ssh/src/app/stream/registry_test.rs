use std::time::Instant;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;

use super::{
    BeginObsOutcome, BeginOutcome, EndReason, ObsIngress, PENDING_TTL, PUBLISHER_GRACE,
    PUBLISHER_TTL, PublisherReport, StreamHandles, StreamPhase, StreamRegistry, WATCHER_TTL,
    WATCHERS_MAX,
};

fn ids() -> (Uuid, Uuid, Uuid) {
    (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7())
}

fn begin_ok(
    registry: &StreamRegistry,
    user: Uuid,
    username: &str,
    title: &str,
    room: Uuid,
    channel: Uuid,
) -> StreamHandles {
    match registry.begin(user, username, title, room, channel) {
        BeginOutcome::Ready(handles) => handles,
        BeginOutcome::PublisherConflict => panic!("unexpected publisher conflict"),
    }
}

fn example_ingress(id: &str) -> ObsIngress {
    ObsIngress {
        ingress_id: id.to_string(),
        whip_url: format!("https://whip.example/w/{id}"),
        stream_key: format!("key-{id}"),
    }
}

/// The whole access model for a stream is that its URL is unguessable, so the
/// switch from 32-char hex to 22-char base64url must not have cost any of the
/// 122 bits a v4 UUID carries. The alphabet also has to stay inside what
/// `late-web`'s `valid_capability_id` accepts, or the shortened link 404s.
#[test]
fn capability_ids_are_full_entropy_base64url() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "title", room, channel);

    for id in [&handles.stream_id, &handles.publish_token] {
        assert_eq!(id.len(), 22, "16 bytes, base64url, unpadded: {id}");
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "outside the url-safe alphabet: {id}"
        );
        let bytes = URL_SAFE_NO_PAD
            .decode(id)
            .unwrap_or_else(|err| panic!("{id} does not decode: {err}"));
        assert_eq!(bytes.len(), 16, "128 bits round-trip: {id}");
    }
    assert_ne!(handles.stream_id, handles.publish_token);
}

#[test]
fn begin_is_one_stream_per_user_and_updates_title() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();

    let first = begin_ok(&registry, user, "mat", "first title", room, channel);
    let second = begin_ok(&registry, user, "mat", "second title", room, channel);

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

    let handles = begin_ok(&registry, user, "mat", "quiet", room, channel);

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
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);

    let first = registry.report_publisher(&handles.publish_token, true, None);
    let second = registry.report_publisher(&handles.publish_token, true, None);

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
        registry.report_publisher("nope", true, None),
        PublisherReport::Gone
    );
}

#[test]
fn publisher_stop_keeps_the_stream_in_grace() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

    let outcome = registry.report_publisher(&handles.publish_token, false, None);

    assert_eq!(outcome, PublisherReport::Stopped);
    // Grace still counts as live so the room row does not flicker out on a
    // page refresh; a re-report resumes without a second announcement.
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);
    assert_eq!(
        registry.report_publisher(&handles.publish_token, true, None),
        PublisherReport::Live { went_live: false }
    );
}

#[test]
fn watch_heartbeats_drive_the_watching_count() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-a"));
    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-b"));
    assert!(registry.watch_heartbeat(&handles.stream_id, "viewer-a"));

    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert_eq!(view.watching, 2);
    assert!(!registry.watch_heartbeat("unknown-stream", "viewer-a"));
}

#[test]
fn end_for_user_kills_the_watch_url() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

    // The ended stream carries the voice channel so the caller can
    // force-disconnect the publisher's LiveKit session, plus the teardown
    // story the orchestration layer logs.
    let ended = registry
        .end_for_user(user, EndReason::Command)
        .expect("ended stream");
    assert_eq!(ended.user_id, user);
    assert_eq!(ended.username, "mat");
    assert_eq!(ended.voice_channel_id, channel);
    // A console stream has no ingress to delete.
    assert_eq!(ended.ingress_id, None);
    assert_eq!(ended.reason, EndReason::Command);
    assert_eq!(ended.phase, StreamPhase::Live);
    assert!(ended.announced);
    assert!(registry.end_for_user(user, EndReason::Command).is_none());

    assert!(registry.watch_view(&handles.stream_id).is_none());
    assert!(registry.publisher_info(&handles.publish_token).is_none());
    assert!(registry.snapshot().streams.is_empty());
}

#[test]
fn stream_lookup_by_username_is_case_insensitive() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "Mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

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
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);
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
    let handles = begin_ok(&registry, user, "mat", "never shown", room, channel);

    assert!(registry.sweep_at(Instant::now()).is_empty());
    let ended = registry.sweep_at(Instant::now() + PENDING_TTL);

    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].user_id, user);
    assert_eq!(ended[0].voice_channel_id, channel);
    // A stream that never went live is the one teardown a streamer cannot
    // see coming, so the reason has to say so.
    assert_eq!(ended[0].reason, EndReason::PendingExpired);
    assert_eq!(ended[0].phase, StreamPhase::Pending);
    assert!(!ended[0].announced);
    assert!(registry.watch_view(&handles.stream_id).is_none());
    assert!(registry.snapshot().streams.is_empty());
}

#[test]
fn sweep_moves_a_stale_publisher_into_grace_then_tears_down() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

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
    assert_eq!(ended[0].reason, EndReason::GraceExpired);
    assert_eq!(ended[0].phase, StreamPhase::Grace);
    assert!(ended[0].announced);
    // The age of the console's last report is what tells a silent page apart
    // from one that reported a stop, so it is measured from the report, not
    // from the start of grace.
    assert!(ended[0].since_publisher_report >= PUBLISHER_TTL + PUBLISHER_GRACE);
    assert!(registry.watch_view(&handles.stream_id).is_none());
}

#[test]
fn sweep_prunes_stale_watchers() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);
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
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);

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
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);

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
        registry.report_publisher(&handles.publish_token, false, None),
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
        registry.report_publisher(&handles.publish_token, true, Some(&secret)),
        PublisherReport::Live { went_live: true }
    );

    // A fresh stream after a stop starts unclaimed with new ids.
    registry
        .end_for_user(user, EndReason::Command)
        .expect("ended stream");
    let fresh = begin_ok(&registry, user, "mat", "next show", room, channel);
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

#[test]
fn obs_begin_stores_the_ingress_and_reuses_it_on_a_rerun() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();

    let first = match registry.begin_obs(
        user,
        "mat",
        "obs show",
        room,
        channel,
        example_ingress("in-1"),
    ) {
        BeginObsOutcome::Ready { handles, ingress } => {
            assert_eq!(ingress, example_ingress("in-1"));
            handles
        }
        other => panic!("expected ready, got {other:?}"),
    };

    // A re-run hands back the stored ingress, never the freshly minted
    // duplicate, so the caller knows to delete its own.
    match registry.begin_obs(
        user,
        "mat",
        "new title",
        room,
        channel,
        example_ingress("in-2"),
    ) {
        BeginObsOutcome::Ready { handles, ingress } => {
            assert_eq!(handles.stream_id, first.stream_id);
            assert_eq!(ingress, example_ingress("in-1"));
        }
        other => panic!("expected ready, got {other:?}"),
    }
    assert_eq!(registry.obs_ingress(user), Some(example_ingress("in-1")));
    let info = registry
        .publisher_info(&first.publish_token)
        .expect("publisher info");
    assert_eq!(info.title, "new title");
}

#[test]
fn publisher_kinds_conflict_instead_of_rewiring_a_stream() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();

    begin_ok(&registry, user, "mat", "console show", room, channel);
    assert_eq!(
        registry.begin_obs(user, "mat", "obs", room, channel, example_ingress("in-1")),
        BeginObsOutcome::PublisherConflict
    );
    // The console stream is untouched by the refused OBS begin.
    assert_eq!(registry.obs_ingress(user), None);

    registry
        .end_for_user(user, EndReason::Command)
        .expect("ended stream");
    registry.begin_obs(user, "mat", "obs", room, channel, example_ingress("in-1"));
    assert_eq!(
        registry.begin(user, "mat", "console", room, channel),
        BeginOutcome::PublisherConflict
    );
}

#[test]
fn obs_reports_drive_the_phase_machine() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = match registry.begin_obs(
        user,
        "mat",
        "obs show",
        room,
        channel,
        example_ingress("in-1"),
    ) {
        BeginObsOutcome::Ready { handles, .. } => handles,
        other => panic!("expected ready, got {other:?}"),
    };

    let polls = registry.obs_streams();
    assert_eq!(polls.len(), 1);
    assert_eq!(polls[0].user_id, user);
    assert_eq!(polls[0].ingress_id, "in-1");

    // First publishing poll goes live exactly once.
    assert_eq!(
        registry.report_obs(user, true),
        PublisherReport::Live { went_live: true }
    );
    assert_eq!(
        registry.report_obs(user, true),
        PublisherReport::Live { went_live: false }
    );
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);

    // A non-publishing poll starts grace; the stream still shows live.
    assert_eq!(registry.report_obs(user, false), PublisherReport::Stopped);
    let view = registry.watch_view(&handles.stream_id).expect("watch view");
    assert!(view.live);

    // Unknown or non-OBS users report gone.
    assert_eq!(
        registry.report_obs(Uuid::now_v7(), true),
        PublisherReport::Gone
    );

    // Ending carries the ingress id so teardown can delete it.
    let ended = registry
        .end_for_user(user, EndReason::Command)
        .expect("ended stream");
    assert_eq!(ended.ingress_id, Some("in-1".to_string()));
}

#[test]
fn obs_sweep_carries_the_ingress_id() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    registry.begin_obs(
        user,
        "mat",
        "never started",
        room,
        channel,
        example_ingress("in-1"),
    );

    let ended = registry.sweep_at(Instant::now() + PENDING_TTL);

    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].ingress_id, Some("in-1".to_string()));
}

#[test]
fn note_viewer_announces_each_named_viewer_once_per_stream() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);
    let (alice, bob, _) = ids();

    // First arrival names the streamer for the feed line; coming back to the
    // same stream stays quiet, so a room reopen cannot spam #lounge.
    assert_eq!(registry.note_viewer(user, alice), Some("mat".to_string()));
    assert_eq!(registry.note_viewer(user, alice), None);
    assert_eq!(registry.note_viewer(user, bob), Some("mat".to_string()));

    // The streamer walking into their own room is not an audience, and a
    // stream that ended has nobody to announce to.
    assert_eq!(registry.note_viewer(user, user), None);
    assert_eq!(registry.note_viewer(Uuid::now_v7(), alice), None);

    // The set is per stream: tomorrow's broadcast announces the regular again.
    registry.end_for_user(user, EndReason::Command);
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    registry.report_publisher(&handles.publish_token, true, None);
    assert_eq!(registry.note_viewer(user, alice), Some("mat".to_string()));
}

#[test]
fn note_viewer_stays_quiet_while_the_stream_is_pending() {
    let registry = StreamRegistry::new();
    let (user, room, channel) = ids();
    let handles = begin_ok(&registry, user, "mat", "show", room, channel);
    let (alice, _, _) = ids();

    // No line ever points at a black screen, and the pending visit must not
    // burn the announcement either.
    assert_eq!(registry.note_viewer(user, alice), None);

    registry.report_publisher(&handles.publish_token, true, None);
    assert_eq!(registry.note_viewer(user, alice), Some("mat".to_string()));
}
