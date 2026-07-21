use super::*;

#[test]
fn env_var_missing_or_blank_treats_missing_and_blank_as_missing() {
    let key = "LATE_TEST_AUDIO_HINT_ENV";

    unsafe { env::remove_var(key) };
    assert!(env_var_missing_or_blank(key));

    unsafe { env::set_var(key, "   ") };
    assert!(env_var_missing_or_blank(key));

    unsafe { env::set_var(key, "set") };
    assert!(!env_var_missing_or_blank(key));

    unsafe { env::remove_var(key) };
}

#[test]
fn disabled_runtime_uses_zeroed_playback_state() {
    let runtime = AudioRuntime::disabled();

    assert!(!runtime.enabled);
    assert_eq!(runtime.sample_rate, 1);
    assert_eq!(
        runtime
            .played_samples
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        runtime
            .volume_percent
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(!runtime.muted.load(std::sync::atomic::Ordering::Relaxed));
    assert!(
        !runtime
            .icecast_output_available
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}
