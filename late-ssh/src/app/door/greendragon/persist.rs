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


