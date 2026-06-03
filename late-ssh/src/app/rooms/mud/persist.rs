// Character persistence for Lateania.
//
// A `SavedCharacter` is the durable slice of a player: class, progression, gold,
// vitals, and gear. It serializes to the JSON blob stored in the mud_characters
// table (see late_core::models::mud_character). Transient combat state (current
// target, active effects, cooldowns, respawn timers) is deliberately NOT saved -
// a character reloads at full readiness in a safe room.
//
// The struct is versioned. Unknown/missing fields fall back to defaults via
// serde, so adding fields later never breaks an old save.

use serde::{Deserialize, Serialize};

use super::classes::Class;
use super::world::RoomId;

const SCHEMA_VERSION: u32 = 1;

pub struct SavedCharacterInit {
    pub class: Option<Class>,
    pub xp: i64,
    pub level: i32,
    pub gold: i64,
    pub hp: i32,
    pub room: RoomId,
    pub inventory: Vec<u32>,
    pub equipped: Vec<(String, u32)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedCharacter {
    #[serde(default)]
    pub version: u32,
    /// Stable class key (see Class::as_key); None means "not yet chosen".
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub xp: i64,
    #[serde(default = "one")]
    pub level: i32,
    #[serde(default)]
    pub gold: i64,
    /// Saved current HP (clamped to max on load).
    #[serde(default)]
    pub hp: i32,
    /// Room the character logged out in; reloaded here if it still exists.
    #[serde(default = "start_room")]
    pub room: RoomId,
    #[serde(default)]
    pub inventory: Vec<u32>,
    /// Equipped items as (slot-key, item-id) pairs.
    #[serde(default)]
    pub equipped: Vec<(String, u32)>,
}

fn one() -> i32 {
    1
}

fn start_room() -> RoomId {
    1
}

impl SavedCharacter {
    pub fn new_for(init: SavedCharacterInit) -> Self {
        Self {
            version: SCHEMA_VERSION,
            class: init.class.map(|c| c.as_key().to_string()),
            xp: init.xp,
            level: init.level,
            gold: init.gold,
            hp: init.hp,
            room: init.room,
            inventory: init.inventory,
            equipped: init.equipped,
        }
    }

    pub fn class(&self) -> Option<Class> {
        self.class.as_deref().and_then(Class::from_key)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }

    /// Parse a stored blob; returns None if it is empty or unreadable, so a
    /// corrupt save degrades to "fresh character" instead of crashing.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        if value.is_null() || value == &serde_json::json!({}) {
            return None;
        }
        serde_json::from_value(value.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let c = SavedCharacter::new_for(SavedCharacterInit {
            class: Some(Class::Rogue),
            xp: 1234,
            level: 7,
            gold: 560,
            hp: 42,
            room: 18,
            inventory: vec![1300, 1301],
            equipped: vec![("weapon".to_string(), 1004)],
        });
        let json = c.to_json();
        let back = SavedCharacter::from_json(&json).expect("parses");
        assert_eq!(back.class(), Some(Class::Rogue));
        assert_eq!(back.xp, 1234);
        assert_eq!(back.level, 7);
        assert_eq!(back.gold, 560);
        assert_eq!(back.inventory, vec![1300, 1301]);
        assert_eq!(back.equipped, vec![("weapon".to_string(), 1004)]);
    }

    #[test]
    fn empty_blob_is_treated_as_no_save() {
        assert!(SavedCharacter::from_json(&serde_json::json!({})).is_none());
        assert!(SavedCharacter::from_json(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // A minimal/old blob with only a class should still load.
        let json = serde_json::json!({ "class": "mage" });
        let c = SavedCharacter::from_json(&json).expect("parses partial");
        assert_eq!(c.class(), Some(Class::Mage));
        assert_eq!(c.level, 1);
        assert_eq!(c.room, 1);
        assert!(c.inventory.is_empty());
    }
}
