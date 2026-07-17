//! Save/load envelope for a Green Dragon character. The character is stored as
//! an opaque JSON blob in `greendragon_characters.data`; this module wraps it
//! with a schema version so the shape can evolve. Every [`Character`] field
//! carries a serde default, so an older blob always deserializes.

use serde_json::{Value, json};

use super::model::Character;

/// Bump when the save shape changes in a way that needs migration logic.
/// Plain field additions are absorbed by serde defaults; v2 marks the switch
/// from auto-applied dragon-kill boons to chooseable dragon points; v3 marks
/// the address style becoming a real one-time choice (phase-2 saves carried a
/// stamped `First` nobody ever picked, so the chooser re-arms for them).
pub const SCHEMA_VERSION: u32 = 3;

/// Serialize a character into the stored blob shape.
pub fn to_json(character: &Character) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "character": character,
    })
}

/// Deserialize a stored blob back into a character. Falls back to a default
/// character if the blob is missing/corrupt (the caller sets the name).
pub fn from_json(blob: &Value) -> Character {
    let version = blob
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let mut c = blob
        .get("character")
        .and_then(|c| serde_json::from_value::<Character>(c.clone()).ok())
        .unwrap_or_default();
    if version < 2 {
        migrate_v1_dragon_boons(&mut c);
    }
    if version < 3 {
        // Pre-phase-3 saves never chose an address style — the field was a
        // placeholder stamp. Re-arm the one-time chooser for them.
        c.style = super::model::AddressStyle::Unchosen;
    }
    c
}

/// v1 saves auto-applied +1 atk / +1 def / +5 HP per dragon kill *and* granted
/// an implicit +1 daily forest fight per kill (capped at 10). v2 makes dragon
/// points a one-per-kill player choice. Legacy characters keep their (over-
/// granted) boons and have the implicit ff turned into spent ff points, so
/// nothing they had regresses; they simply hold no unspent points.
fn migrate_v1_dragon_boons(c: &mut Character) {
    if c.dragon_kills > 0 && c.dragon_ff_bonus == 0 && c.dragon_points_unspent == 0 {
        c.dragon_ff_bonus = c.dragon_kills.min(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(c.gold, super::super::model::START_GOLD); // defaulted
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
        use super::super::model::{AddressStyle, Race};
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
        use super::super::model::AddressStyle;
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
}
