use crate::audio_config::*;

#[test]
fn default_config_is_valid() {
    let cfg = AnalyzerConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn default_values() {
    let cfg = AnalyzerConfig::default();
    assert_eq!(cfg.fft_size, 1024);
    assert_eq!(cfg.band_count, 8);
    assert_eq!(cfg.target_hz, 15);
}

#[test]
fn rejects_zero_target_hz() {
    let cfg = AnalyzerConfig {
        target_hz: 0,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_non_power_of_two_fft() {
    let cfg = AnalyzerConfig {
        fft_size: 100,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_wrong_band_count() {
    let cfg = AnalyzerConfig {
        band_count: 16,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn rejects_zero_fft_size() {
    let cfg = AnalyzerConfig {
        fft_size: 0,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn accepts_valid_powers_of_two() {
    for size in [256, 512, 1024, 2048, 4096] {
        let cfg = AnalyzerConfig {
            fft_size: size,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok(), "fft_size={size} should be valid");
    }
}
