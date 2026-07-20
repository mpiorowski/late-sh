use super::model::Character;
use serde_json::{Value, json};
use crate::app::door::greendragon::persist::*;

#[test]
fn round_trips_a_character() {
    let mut c = Character::new("hero", 42);
    c.level = 7;
    c.weapon_tier = 9;
    c.gold = 1234;
    c.dragon_kills = 2;
    let blob = to_json(&c);
    assert_eq!(blob["schema_version"], SCHEMA_VERSION);
    let back = from_json(&blob);
    assert_eq!(back.level, 7);
    assert_eq!(back.weapon_tier, 9);
    assert_eq!(back.gold, 1234);
    assert_eq!(back.dragon_kills, 2);
    assert_eq!(back.name, "hero");
}

#[test]
fn missing_fields_use_defaults() {
    let blob = json!({ "schema_version": 1, "character": { "name": "old", "level": 3 } });
    let c = from_json(&blob);
    assert_eq!(c.name, "old");
    assert_eq!(c.level, 3);
    assert_eq!(c.gold, crate::app::door::greendragon::model::START_GOLD); // defaulted
    assert!(c.alive);
}

#[test]
fn v1_blobs_grandfather_the_implicit_ff_bonus() {
    // A v1 save with kills gets its old implicit daily-turn bonus turned
    // into spent ff points (capped at 10), with no unspent points.
    let blob = json!({
        "schema_version": 1,
        "character": { "name": "vet", "dragon_kills": 14, "dragon_attack_bonus": 14 }
    });
    let c = from_json(&blob);
    assert_eq!(c.dragon_ff_bonus, 10);
    assert_eq!(c.dragon_points_unspent, 0);
    assert_eq!(c.dragon_attack_bonus, 14); // boons kept

    // A v2 save is taken at face value: a zero ff bonus stays zero.
    let blob = json!({
        "schema_version": 2,
        "character": { "name": "new", "dragon_kills": 3, "dragon_points_unspent": 1 }
    });
    let c = from_json(&blob);
    assert_eq!(c.dragon_ff_bonus, 0);
    assert_eq!(c.dragon_points_unspent, 1);
}

#[test]
fn pre_race_blobs_arm_the_race_gate() {
    use crate::app::door::greendragon::model::{AddressStyle, Race};
    // Saves from before phase 2 have no race/title/style: plain serde
    // defaults, no migration needed. An unset race arms the choice gate
    // on load; an empty title is stamped off the ladder there too.
    let blob = json!({
        "schema_version": 2,
        "character": { "name": "vet", "level": 9, "dragon_kills": 3 }
    });
    let c = from_json(&blob);
    assert_eq!(c.race, Race::None);
    assert_eq!(c.title, "");
    assert_eq!(c.style, AddressStyle::Unchosen);
}

#[test]
fn pre_v3_blobs_rearm_the_style_chooser() {
    use crate::app::door::greendragon::model::AddressStyle;
    // A v2 save carries a stamped "First" nobody chose: the v3 migration
    // clears it so the one-time chooser fires. A v3 save keeps its pick.
    let blob = json!({
        "schema_version": 2,
        "character": { "name": "vet", "style": "First" }
    });
    assert_eq!(from_json(&blob).style, AddressStyle::Unchosen);
    let blob = json!({
        "schema_version": 3,
        "character": { "name": "new", "style": "Second" }
    });
    assert_eq!(from_json(&blob).style, AddressStyle::Second);
}

#[test]
fn corrupt_blob_falls_back_to_default() {
    let c = from_json(&json!({ "nonsense": true }));
    assert_eq!(c.level, 1);
}
