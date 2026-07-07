//! Save/load envelope for a Green Dragon character. The character is stored as
//! an opaque JSON blob in `greendragon_characters.data`; this module wraps it
//! with a schema version so the shape can evolve. Every [`Character`] field
//! carries a serde default, so an older blob always deserializes.

use serde_json::{Value, json};

use super::model::Character;

/// Bump when the save shape changes in a way that needs migration logic. Today
/// serde defaults absorb additions, so v1 covers all current changes.
pub const SCHEMA_VERSION: u32 = 1;

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
    blob.get("character")
        .and_then(|c| serde_json::from_value::<Character>(c.clone()).ok())
        .unwrap_or_default()
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
    fn corrupt_blob_falls_back_to_default() {
        let c = from_json(&json!({ "nonsense": true }));
        assert_eq!(c.level, 1);
    }
}
