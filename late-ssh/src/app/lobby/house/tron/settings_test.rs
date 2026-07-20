use serde_json::json;
use crate::app::lobby::house::tron::settings::*;

#[test]
fn settings_round_trip_speed() {
    let settings = TronTableSettings {
        speed: TronSpeed::Quick,
        mode: TronMode::Gaps,
    };
    assert_eq!(TronTableSettings::from_json(&settings.to_json()), settings);
}

#[test]
fn unknown_values_fall_back_to_safe_defaults() {
    let settings =
        TronTableSettings::from_json(&json!({ "speed": "warp", "mode": "overdrive" }));
    assert_eq!(settings.speed, TronSpeed::Standard);
    assert_eq!(settings.mode, TronMode::Classic);
}

#[test]
fn missing_mode_preserves_legacy_classic_rooms() {
    let settings = TronTableSettings::from_json(&json!({ "speed": "quick" }));
    assert_eq!(settings.speed, TronSpeed::Quick);
    assert_eq!(settings.mode, TronMode::Classic);
}
