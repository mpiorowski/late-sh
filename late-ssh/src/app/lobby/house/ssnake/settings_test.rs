use crate::app::lobby::house::ssnake::settings::*;
use serde_json::json;

#[test]
fn settings_round_trip() {
    let settings = SsnakeTableSettings {
        speed: SsnakeSpeed::Swift,
        seats: 4,
    };
    assert_eq!(
        SsnakeTableSettings::from_json(&settings.to_json()),
        settings
    );
    let relaxed = SsnakeTableSettings {
        speed: SsnakeSpeed::Classic,
        seats: 2,
    };
    assert_eq!(SsnakeTableSettings::from_json(&relaxed.to_json()), relaxed);
}

#[test]
fn seats_clamp_to_table_bounds() {
    assert_eq!(SsnakeTableSettings::from_json(&json!({})).seats, 2);
    assert_eq!(
        SsnakeTableSettings::from_json(&json!({ "seats": 1 })).seats,
        2
    );
    assert_eq!(
        SsnakeTableSettings::from_json(&json!({ "seats": 9 })).seats,
        5
    );
    assert_eq!(
        SsnakeTableSettings::from_json(&json!({ "seats": 3 })).seats,
        3
    );
}

#[test]
fn unknown_speed_falls_back_to_classic() {
    let settings = SsnakeTableSettings::from_json(&json!({ "speed": "ludicrous" }));
    assert_eq!(settings.speed, SsnakeSpeed::Classic);
}

#[test]
fn speed_scales_level_tick() {
    assert_eq!(SsnakeSpeed::Relaxed.scale_tick(180), 240);
    assert_eq!(SsnakeSpeed::Classic.scale_tick(180), 180);
    assert_eq!(SsnakeSpeed::Swift.scale_tick(180), 135);
}
