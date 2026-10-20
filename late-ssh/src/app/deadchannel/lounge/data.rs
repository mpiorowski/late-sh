//! The lounge's copy and art: what the wire says when a glyph goes down
//! or slips away, what the scene answers a hit, and the faces the scene
//! draws. Placeholder copy at feed-template standards, design review
//! pending with the rest of the phase 2 copy. Every noun passes the
//! screenshot test.

use late_core::models::deadchannel_spawn::{Spawn, SpawnKind};

use super::state::{mentions, pay_for};
use crate::app::deadchannel::fight::state::Leavings;

/// The wire's line for a spawn put down.
pub(crate) fn slain_line(spawn: &Spawn) -> String {
    let who = mentions(spawn);
    let kind = spawn.kind.as_str();
    match pay_for(spawn.kind) {
        Leavings::Cache => format!("{who} put down a {kind} in the lounge."),
        Leavings::Crystal => {
            format!("{who} put down a {kind} in the lounge. a crystal each.")
        }
    }
}

/// The wire's line for a spawn that slipped away.
pub(crate) fn slipped_line(spawn: &Spawn) -> String {
    let kind = spawn.kind.as_str();
    if spawn.hits.is_empty() {
        return format!("a {kind} slipped past the lounge. nobody was looking.");
    }
    let who = mentions(spawn);
    let needed = match spawn.kind {
        SpawnKind::Flicker => "one",
        SpawnKind::Howler => "three",
    };
    format!("a {kind} slipped past the lounge. {who} hit it; it needed {needed}.")
}

/// The spawn's face, three rows of nine cells: the big one the hitters
/// stand against. The fight's foes of the same name are smaller and
/// theirs.
pub(crate) fn face(kind: SpawnKind) -> [&'static str; 3] {
    match kind {
        SpawnKind::Flicker => [" ▘  ▝  ▘ ", "▗▖ ▚▞ ▗▖ ", " ▖  ▗  ▖ "],
        SpawnKind::Howler => ["▚▞▚▞▚▞▚▞▚", "▐▓▒▓█▓▒▓▌", "▞▚▞▚▞▚▞▚▞"],
    }
}

/// A hitter's seat with nobody in it yet: a hit still owed.
pub(crate) const EMPTY_SEAT: [&str; 3] = [" ┄┄┄ ", "┆   ┆", " ┄┄┄ "];

/// A hitter whose look the directory does not hold (they left the row
/// since): a shape where the portrait would be.
pub(crate) const GHOST_PORTRAIT: [&str; 3] = ["  ▚  ", " ▐█▌ ", "  ▞  "];

/// The strike crossing the lane from a hitter to the face, one frame
/// per cell it has travelled.
pub(crate) const STRIKE_FRAMES: [&str; 5] = ["━╸   ", " ━╸  ", "  ━╸ ", "   ━╸", "    ━"];

/// The scene's answer to a hit that landed with `left` still to go.
pub(crate) fn hit_line(weapon: &str, kind: SpawnKind, left: i32) -> String {
    let kind = kind.as_str();
    match left {
        0 => format!("your {weapon} puts the {kind} down."),
        1 => format!("your {weapon} hits the {kind}. 1 to go."),
        n => format!("your {weapon} hits the {kind}. {n} to go."),
    }
}

pub(crate) const ALREADY_HIT_LINE: &str = "you already hit it. somebody else has to.";
pub(crate) const NOTHING_TO_HIT_LINE: &str = "nothing to hit. the channel is quiet.";
pub(crate) const SIGNAL_DOWN_LINE: &str =
    "your signal is down. nothing in there can see you until tomorrow.";
pub(crate) const NOT_ANSWERING_LINE: &str = "the static is not answering. try again.";
pub(crate) const DECIDING_LINE: &str = "the static is deciding.";
