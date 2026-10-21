use super::*;

#[test]
fn pair_ws_url_rewrites_scheme() {
    assert_eq!(
        pair_ws_url("https://api.late.sh", "abc").unwrap(),
        "wss://api.late.sh/api/ws/pair?token=abc"
    );
    assert_eq!(
        pair_ws_url("http://localhost:4000", "abc").unwrap(),
        "ws://localhost:4000/api/ws/pair?token=abc"
    );
}

#[test]
fn apply_pair_control_toggles_muted_state() {
    let muted = AtomicBool::new(false);
    let volume_percent = AtomicU8::new(100);

    apply_audio_pair_control(PairControlMessage::ToggleMute, &muted, &volume_percent);
    assert!(muted.load(Ordering::Relaxed));

    apply_audio_pair_control(PairControlMessage::ToggleMute, &muted, &volume_percent);
    assert!(!muted.load(Ordering::Relaxed));
}

#[test]
fn apply_pair_control_adjusts_volume() {
    let muted = AtomicBool::new(false);
    let volume_percent = AtomicU8::new(50);

    apply_audio_pair_control(PairControlMessage::VolumeUp, &muted, &volume_percent);
    assert_eq!(volume_percent.load(Ordering::Relaxed), 55);

    apply_audio_pair_control(PairControlMessage::VolumeDown, &muted, &volume_percent);
    assert_eq!(volume_percent.load(Ordering::Relaxed), 50);
}

#[test]
fn apply_pair_control_sets_absolute_mute() {
    let muted = AtomicBool::new(false);
    let volume_percent = AtomicU8::new(50);

    apply_audio_pair_control(
        PairControlMessage::SetMuted { muted: true },
        &muted,
        &volume_percent,
    );
    assert!(muted.load(Ordering::Relaxed));

    // Absolute, not a toggle: applying the same state again keeps it.
    apply_audio_pair_control(
        PairControlMessage::SetMuted { muted: true },
        &muted,
        &volume_percent,
    );
    assert!(muted.load(Ordering::Relaxed));

    apply_audio_pair_control(
        PairControlMessage::SetMuted { muted: false },
        &muted,
        &volume_percent,
    );
    assert!(!muted.load(Ordering::Relaxed));
    assert_eq!(
        volume_percent.load(Ordering::Relaxed),
        50,
        "mute leaves volume untouched"
    );
}

#[test]
fn apply_pair_control_set_volume_off_zero_unmutes() {
    let muted = AtomicBool::new(true);
    let volume_percent = AtomicU8::new(30);

    apply_audio_pair_control(
        PairControlMessage::SetVolume { volume_percent: 0 },
        &muted,
        &volume_percent,
    );
    assert_eq!(volume_percent.load(Ordering::Relaxed), 0);
    assert!(
        muted.load(Ordering::Relaxed),
        "a zero write is not an unmute"
    );

    apply_audio_pair_control(
        PairControlMessage::SetVolume { volume_percent: 45 },
        &muted,
        &volume_percent,
    );
    assert_eq!(volume_percent.load(Ordering::Relaxed), 45);
    assert!(!muted.load(Ordering::Relaxed), "raising the slider unmutes");
}

/// Wire contract with the server's fan-out: the CLI parses the same event
/// names its own `set_muted`/`set_volume` requests carry upstream.
#[test]
fn set_controls_deserialize_from_wire_names() {
    let muted = serde_json::from_str::<PairControlMessage>(r#"{"event":"set_muted","muted":true}"#)
        .unwrap();
    assert!(matches!(
        muted,
        PairControlMessage::SetMuted { muted: true }
    ));

    let volume =
        serde_json::from_str::<PairControlMessage>(r#"{"event":"set_volume","volume_percent":45}"#)
            .unwrap();
    assert!(matches!(
        volume,
        PairControlMessage::SetVolume { volume_percent: 45 }
    ));
}

#[test]
fn queue_update_deserializes_track_details() {
    let message = serde_json::from_str::<PairControlMessage>(
        r#"{
            "event": "queue_update",
            "current": {
                "id": "item-1",
                "video_id": "video-1",
                "title": "A track",
                "channel": "A channel",
                "duration_ms": 123000,
                "started_at_ms": 10000
            },
            "queue": [],
            "sequence": 4
        }"#,
    )
    .unwrap();

    let PairControlMessage::QueueUpdate {
        current: Some(current),
    } = message
    else {
        panic!("expected queue update with a current track");
    };
    assert_eq!(current.id, "item-1");
    assert_eq!(current.video_id, "video-1");
    assert_eq!(current.title.as_deref(), Some("A track"));
    assert_eq!(current.channel.as_deref(), Some("A channel"));
    assert_eq!(current.duration_ms, Some(123_000));
    assert_eq!(current.started_at_ms, Some(10_000));
}

/// A session the server never saw has no way to learn the stored device
/// mute, so the boot mute is released, and released only once.
#[test]
fn never_paired_session_releases_startup_mute_once() {
    let mut policy = PairRetryPolicy::new();
    for attempt in 1..=MAX_CONSECUTIVE_FAILURES {
        assert_eq!(
            policy.note_attempt(PairAttempt::NotEstablished),
            ReconnectPlan::Soon,
            "attempt {attempt} is still inside the retry budget"
        );
    }
    assert_eq!(
        policy.note_attempt(PairAttempt::NotEstablished),
        ReconnectPlan::ReleaseStartupMuteThenSlow
    );
    assert_eq!(
        policy.note_attempt(PairAttempt::NotEstablished),
        ReconnectPlan::Slow,
        "the boot mute is released once, not again on every later failure"
    );
}

/// The regression: a session the server has already seen is running the
/// user's stored mute, so losing the socket must never unmute it. This is
/// what used to start the music mid-session once reconnects piled up.
#[test]
fn paired_session_never_releases_the_mute() {
    let mut policy = PairRetryPolicy::new();
    assert_eq!(
        policy.note_attempt(PairAttempt::Ended {
            lived: Duration::from_secs(1)
        }),
        ReconnectPlan::Soon
    );

    let plans: Vec<ReconnectPlan> = (0..12)
        .map(|_| policy.note_attempt(PairAttempt::NotEstablished))
        .collect();

    let mut expected = vec![ReconnectPlan::Soon; 9];
    expected.extend([ReconnectPlan::Slow; 3]);
    assert_eq!(
        plans, expected,
        "the session keeps retrying, and keeps the mute it was given"
    );
}

/// One long session's worth of rare drops must never accumulate into a
/// give-up: a connection that held past `STABLE_CONNECTION` clears the count.
#[test]
fn a_stable_connection_clears_the_failure_count() {
    let mut policy = PairRetryPolicy::new();
    for round in 0..30 {
        assert_eq!(
            policy.note_attempt(PairAttempt::Ended {
                lived: STABLE_CONNECTION
            }),
            ReconnectPlan::Soon,
            "round {round}: an hours-long session dropping is one failure, not a spent budget"
        );
    }
}

/// The reset only applies to a connection that actually held: a socket that
/// dies straight away, over and over, still backs off to the slow retry.
#[test]
fn flapping_connections_still_reach_the_slow_retry() {
    let mut policy = PairRetryPolicy::new();
    let brief = PairAttempt::Ended {
        lived: STABLE_CONNECTION - Duration::from_secs(1),
    };
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(policy.note_attempt(brief), ReconnectPlan::Soon);
    }
    assert_eq!(
        policy.note_attempt(brief),
        ReconnectPlan::Slow,
        "a flapping paired session slows down without touching the mute"
    );
}

/// The server accepts the upgrade before it checks the per-IP pair limit and
/// the per-token capacity, so a rejected socket still takes our
/// `client_state` write and then dies unread. That session never registered
/// and never got its alignment: it must count as not established, so the
/// boot mute is still released once the budget is spent.
#[test]
fn a_socket_the_server_dropped_unread_still_releases_the_boot_mute() {
    let dropped_unread = PairSessionEnd {
        server_frame_received: false,
        result: Err(anyhow::anyhow!(
            "connection reset without closing handshake"
        )),
    };
    assert_eq!(
        dropped_unread.attempt(Duration::from_secs(1)),
        PairAttempt::NotEstablished
    );

    let mut policy = PairRetryPolicy::new();
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(
            policy.note_attempt(dropped_unread.attempt(Duration::from_secs(1))),
            ReconnectPlan::Soon
        );
    }
    assert_eq!(
        policy.note_attempt(dropped_unread.attempt(Duration::from_secs(1))),
        ReconnectPlan::ReleaseStartupMuteThenSlow,
        "a session the server never registered is still on its boot mute"
    );
}

/// The other side of the line: one frame from the server proves the
/// registration happened, whatever ended the session afterwards.
#[test]
fn a_session_that_heard_the_server_is_established() {
    let registered_then_failed = PairSessionEnd {
        server_frame_received: true,
        result: Err(anyhow::anyhow!("broken pipe")),
    };
    assert_eq!(
        registered_then_failed.attempt(Duration::from_secs(5)),
        PairAttempt::Ended {
            lived: Duration::from_secs(5)
        }
    );
}

fn client_info(video_blockers: Vec<VideoBlocker>) -> PairClientInfo {
    PairClientInfo {
        ssh_mode: "native",
        platform: "linux",
        video_blockers,
    }
}

/// A CLI that can play advertises `video` and sends no blockers.
#[test]
fn client_state_advertises_video_when_nothing_blocks_it() {
    let payload = client_state_payload(&client_info(vec![]), false, 30);
    let capabilities = payload["capabilities"].as_array().unwrap();
    assert!(capabilities.contains(&json!("video")));
    assert!(payload.get("video_blockers").is_none());
}

/// A CLI that cannot play sends every blocker by its wire name instead.
#[test]
fn client_state_sends_blockers_instead_of_video() {
    let payload = client_state_payload(
        &client_info(vec![VideoBlocker::NotKitty, VideoBlocker::NoMpv]),
        false,
        30,
    );
    let capabilities = payload["capabilities"].as_array().unwrap();
    assert!(!capabilities.contains(&json!("video")));
    assert_eq!(payload["video_blockers"], json!(["not_kitty", "no_mpv"]));
}

/// A `video_play` without `start_s` is from a server that predates
/// resumes, and starts from the top.
#[test]
fn video_controls_deserialize_from_wire_names() {
    for (wire, expected_start) in [
        (
            r#"{"event":"video_play","play_id":7,"url":"https://www.youtube.com/watch?v=abc","left":10,"top":3,"cols":100,"rows":28}"#,
            0,
        ),
        (
            r#"{"event":"video_play","play_id":7,"url":"https://www.youtube.com/watch?v=abc","left":10,"top":3,"cols":100,"rows":28,"start_s":1391}"#,
            1391,
        ),
    ] {
        let play = serde_json::from_str::<PairControlMessage>(wire).unwrap();
        let PairControlMessage::VideoPlay {
            play_id,
            url,
            left,
            top,
            cols,
            rows,
            start_s,
        } = play
        else {
            panic!("expected video_play");
        };
        assert_eq!(
            (play_id, url.as_str(), left, top, cols, rows, start_s),
            (
                7,
                "https://www.youtube.com/watch?v=abc",
                10,
                3,
                100,
                28,
                expected_start
            )
        );
    }

    let control = serde_json::from_str::<PairControlMessage>(
        r#"{"event":"video_control","play_id":7,"control":"seek_back"}"#,
    )
    .unwrap();
    assert!(matches!(
        control,
        PairControlMessage::VideoControl {
            play_id: 7,
            control: VideoControl::SeekBack
        }
    ));

    let stop = serde_json::from_str::<PairControlMessage>(r#"{"event":"video_stop","play_id":7}"#)
        .unwrap();
    assert!(matches!(stop, PairControlMessage::VideoStop { play_id: 7 }));
}
