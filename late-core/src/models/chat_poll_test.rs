use super::*;

#[test]
fn normalize_duration_accepts_configured_options() {
    for duration_secs in POLL_DURATION_OPTIONS_SECS {
        assert_eq!(
            normalize_duration_secs(duration_secs).unwrap(),
            duration_secs
        );
    }
}

#[test]
fn normalize_duration_rejects_unconfigured_values() {
    assert!(normalize_duration_secs(5 * 60).is_err());
    assert!(normalize_duration_secs(40 * 60).is_err());
}
