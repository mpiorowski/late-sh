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
