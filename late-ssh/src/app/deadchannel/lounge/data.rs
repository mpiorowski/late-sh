//! The lounge's copy: what the wire says when a glyph goes down or slips
//! away, and what the composer answers a hit. Placeholder copy at
//! feed-template standards, design review pending with the rest of the
//! phase 2 copy. Every noun passes the screenshot test.

use late_core::models::deadchannel_spawn::{Spawn, SpawnKind};

use super::state::{Pay, names, pay_for};

/// The wire's line for a spawn put down.
pub(crate) fn slain_line(spawn: &Spawn) -> String {
    let who = names(spawn);
    let kind = spawn.kind.as_str();
    match pay_for(spawn.kind) {
        Pay::Bits => format!("{who} put down a {kind} in the lounge."),
        Pay::Crystal => format!("{who} put down a {kind} in the lounge. a crystal each."),
    }
}

/// The wire's line for a spawn that slipped away.
pub(crate) fn slipped_line(spawn: &Spawn) -> String {
    let kind = spawn.kind.as_str();
    if spawn.hits.is_empty() {
        return format!("a {kind} slipped past the lounge. nobody was looking.");
    }
    let who = names(spawn);
    let needed = match spawn.kind {
        SpawnKind::Flicker => "one",
        SpawnKind::Howler => "three",
    };
    format!("a {kind} slipped past the lounge. {who} hit it; it needed {needed}.")
}

/// The composer's answer to a hit that landed with `left` still to go.
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
