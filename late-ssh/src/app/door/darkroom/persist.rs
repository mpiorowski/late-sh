//! Save/load envelope for a Dark Room game. The game is stored as an opaque
//! JSON blob in `darkroom_saves.data`; this module wraps it with a schema
//! version so the shape can evolve. Every [`Game`] field carries a serde
//! default, so an older blob always deserializes.

use serde_json::{Value, json};

use super::model::Game;

/// Bump when the save shape changes in a way that needs migration logic.
/// Plain field additions are absorbed by serde defaults.
pub const SCHEMA_VERSION: u32 = 1;

/// Serialize a game into the stored blob shape.
pub fn to_json(game: &Game) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "game": game,
    })
}

/// Deserialize a stored blob back into a game. A missing or corrupt blob falls
/// back to a fresh game rather than failing the session: the worst case is a
/// player who lost a save gets a dark room, which is where everyone starts.
///
/// That fallback room is never a veteran's, whatever the account has done: the
/// only reader that knows an account's history is `svc::load_game`, and it
/// only reaches for it when there was no row to read at all. A save that
/// failed to parse is a fault, not a fresh start, and quietly handing it the
/// battleship would hide that.
pub fn from_json(blob: &Value) -> Game {
    match blob.get("game") {
        Some(game) => match serde_json::from_value::<Game>(game.clone()) {
            Ok(game) => game,
            Err(_) => Game::new(false),
        },
        None => Game::new(false),
    }
}
