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
use uuid::Uuid;

use super::classes::Class;
use super::stats::AbilityScores;
use super::world::RoomId;

const SCHEMA_VERSION: u32 = 3;
const WORLD_SCHEMA_VERSION: u32 = 1;

pub struct SavedCharacterInit {
    pub class: Option<Class>,
    pub xp: i64,
    pub level: i32,
    pub gold: i64,
    pub hp: i32,
    pub room: RoomId,
    pub visited: Vec<RoomId>,
    pub inventory: Vec<u32>,
    pub equipped: Vec<(String, u32)>,
    pub scores: AbilityScores,
    pub titles: Vec<String>,
    pub title_levels: Vec<i32>,
    pub active_title: Option<usize>,
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
    /// Rooms the character has visited, for the overhead map. Empty for pre-v3
    /// saves, which simply start the map from wherever they reload.
    #[serde(default)]
    pub visited: Vec<RoomId>,
    #[serde(default)]
    pub inventory: Vec<u32>,
    /// Equipped items as (slot-key, item-id) pairs.
    #[serde(default)]
    pub equipped: Vec<(String, u32)>,
    /// Rolled D&D ability scores; default (all 10s) for pre-v2 saves.
    #[serde(default)]
    pub scores: AbilityScores,
    /// Titles earned by slaying notable foes (most recent last).
    #[serde(default)]
    pub titles: Vec<String>,
    /// Level for each title (parallel to `titles`); empty/short for pre-v4 saves,
    /// padded on load.
    #[serde(default)]
    pub title_levels: Vec<i32>,
    /// Index into `titles` of the displayed title, if the player has chosen one.
    #[serde(default)]
    pub active_title: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedWorld {
    #[serde(default = "world_schema_version")]
    pub version: u32,
    #[serde(default)]
    pub mobs: Vec<SavedMob>,
    #[serde(default)]
    pub mob_stuns: Vec<SavedMobStun>,
    #[serde(default)]
    pub mob_dots: Vec<SavedMobDot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMob {
    pub id: u32,
    pub hp: i32,
    pub alive: bool,
    #[serde(default)]
    pub respawn_remaining_secs: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMobStun {
    pub mob_id: u32,
    pub remaining_ticks: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMobDot {
    pub mob_id: u32,
    pub owner: Uuid,
    pub damage: i32,
    pub remaining_ticks: u8,
}

fn one() -> i32 {
    1
}

fn world_schema_version() -> u32 {
    WORLD_SCHEMA_VERSION
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
            visited: init.visited,
            inventory: init.inventory,
            equipped: init.equipped,
            scores: init.scores,
            titles: init.titles,
            title_levels: init.title_levels,
            active_title: init.active_title,
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

impl SavedWorld {
    pub fn new(
        mobs: Vec<SavedMob>,
        mob_stuns: Vec<SavedMobStun>,
        mob_dots: Vec<SavedMobDot>,
    ) -> Self {
        Self {
            version: WORLD_SCHEMA_VERSION,
            mobs,
            mob_stuns,
            mob_dots,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }

    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        if value.is_null() || value == &serde_json::json!({}) {
            return None;
        }
        let saved: Self = serde_json::from_value(value.clone()).ok()?;
        (saved.version == WORLD_SCHEMA_VERSION).then_some(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let scores = AbilityScores {
            dexterity: 16,
            ..Default::default()
        };
        let c = SavedCharacter::new_for(SavedCharacterInit {
            class: Some(Class::Rogue),
            xp: 1234,
            level: 7,
            gold: 560,
            hp: 42,
            room: 18,
            visited: vec![1, 5, 18],
            inventory: vec![1300, 1301],
            equipped: vec![("weapon".to_string(), 1004)],
            scores,
            titles: vec!["Wyrmbane".to_string()],
            title_levels: vec![12],
            active_title: Some(0),
        });
        let json = c.to_json();
        let back = SavedCharacter::from_json(&json).expect("parses");
        assert_eq!(back.class(), Some(Class::Rogue));
        assert_eq!(back.xp, 1234);
        assert_eq!(back.level, 7);
        assert_eq!(back.gold, 560);
        assert_eq!(back.visited, vec![1, 5, 18]);
        assert_eq!(back.inventory, vec![1300, 1301]);
        assert_eq!(back.equipped, vec![("weapon".to_string(), 1004)]);
        assert_eq!(back.scores.dexterity, 16);
        assert_eq!(back.titles, vec!["Wyrmbane".to_string()]);
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
        assert!(c.visited.is_empty());
        assert!(c.inventory.is_empty());
    }

    #[test]
    fn world_state_round_trips_through_json() {
        let owner = Uuid::nil();
        let world = SavedWorld::new(
            vec![SavedMob {
                id: 42,
                hp: 3,
                alive: false,
                respawn_remaining_secs: Some(17),
            }],
            vec![SavedMobStun {
                mob_id: 42,
                remaining_ticks: 2,
            }],
            vec![SavedMobDot {
                mob_id: 42,
                owner,
                damage: 5,
                remaining_ticks: 3,
            }],
        );
        let json = world.to_json();
        let back = SavedWorld::from_json(&json).expect("parses");
        assert_eq!(back.mobs[0].id, 42);
        assert_eq!(back.mobs[0].respawn_remaining_secs, Some(17));
        assert_eq!(back.mob_stuns[0].remaining_ticks, 2);
        assert_eq!(back.mob_dots[0].owner, owner);
    }
}
