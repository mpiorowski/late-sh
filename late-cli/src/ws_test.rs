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
