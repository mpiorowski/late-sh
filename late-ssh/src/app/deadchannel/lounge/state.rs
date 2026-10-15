//! The pure side of the lounge spawns (GAME.md, "The lounge pass"): the
//! day's dice, when a spawn is due, what one is worth, the `/hit`
//! command, and what a session's composer says about it. No I/O, no
//! clock reads: every `now` is handed in.

use chrono::{DateTime, Timelike, Utc};
use late_core::models::deadchannel_spawn::{LoungeDay, LoungeDice, Spawn, SpawnKind, Window};
use rand::Rng;
use uuid::Uuid;

/// What the spawner asks on every tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Due {
    /// Nothing to land this tick.
    Nothing,
    /// The window's flicker: its minute has come, the window is still
    /// open, and a runner is looking.
    Flicker(Window),
    /// The day's howler: its minute has come and two runners are looking.
    Howler,
}

/// Runners looking at #lounge before a flicker lands.
pub(crate) const FLICKER_MIN_RUNNERS: usize = 1;
/// Runners looking at #lounge before a howler lands. Two cannot finish
/// it: the third has to be called in.
pub(crate) const HOWLER_MIN_RUNNERS: usize = 2;
/// The least time between two spawns, whatever the dice say.
pub(crate) const SPAWN_GAP_SECS: i64 = 45 * 60;

impl SpawnKindRules for SpawnKind {
    fn hits_needed(self) -> i32 {
        match self {
            SpawnKind::Flicker => 1,
            SpawnKind::Howler => 3,
        }
    }

    fn lifetime_secs(self) -> i64 {
        match self {
            SpawnKind::Flicker => 90,
            SpawnKind::Howler => 180,
        }
    }

    fn glyph(self) -> char {
        match self {
            SpawnKind::Flicker => '▖',
            SpawnKind::Howler => '╫',
        }
    }
}

/// The lounge's numbers per kind, on the model's enum so the match is
/// exhaustive here and nowhere else.
pub(crate) trait SpawnKindRules {
    /// Hits that put it down, each from a different runner.
    fn hits_needed(self) -> i32;
    /// How long it stays before it slips away.
    fn lifetime_secs(self) -> i64;
    /// Its one cell on the composer.
    fn glyph(self) -> char;
}

/// What a kill pays each hitter: a cache's worth of bits at the hitter's
/// level for a flicker, a crystal for a howler. Never exp: the climb's
/// pace is the rations' (GAME.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pay {
    Bits,
    Crystal,
}

pub(crate) fn pay_for(kind: SpawnKind) -> Pay {
    match kind {
        SpawnKind::Flicker => Pay::Bits,
        SpawnKind::Howler => Pay::Crystal,
    }
}

/// Roll a day's dice: a minute inside each window for its flicker, any
/// minute of the day for the howler.
pub(crate) fn roll_dice<R: Rng>(rng: &mut R) -> LoungeDice {
    let mut flicker_minutes = [0; 3];
    for window in Window::ALL {
        let first = window.first_minute();
        flicker_minutes[window.index() as usize] =
            rng.gen_range(first..first + late_core::models::deadchannel_spawn::WINDOW_MINUTES);
    }
    LoungeDice {
        flicker_minutes,
        howler_minute: rng.gen_range(0..1440),
    }
}

/// The minute of the UTC day `now` falls in.
pub(crate) fn minute_of_day(now: DateTime<Utc>) -> i32 {
    (now.hour() * 60 + now.minute()) as i32
}

/// What is due at `now`, given the day's row, how many runners are
/// looking at #lounge, and whether a spawn is already live. A flicker
/// whose minute passed while nobody was looking lands the moment someone
/// is, as long as its window is still open; the howler the same, any
/// time in the day. The gap is the row's to hold.
pub(crate) fn due(day: &LoungeDay, now: DateTime<Utc>, runners_looking: usize, live: bool) -> Due {
    if live {
        return Due::Nothing;
    }
    let minute = minute_of_day(now);
    let window = Window::of_minute(minute);
    if !day.flicker_spent(window)
        && day.flicker_minute(window) <= minute
        && runners_looking >= FLICKER_MIN_RUNNERS
    {
        return Due::Flicker(window);
    }
    if !day.howler_spent && day.howler_minute <= minute && runners_looking >= HOWLER_MIN_RUNNERS {
        return Due::Howler;
    }
    Due::Nothing
}

/// The composer command. Only `/hit` for now; parsed for runners only,
/// so for everyone else it posts as the words it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoungeCommand {
    Hit,
}

pub(crate) fn parse_lounge_command(body: &str) -> Option<LoungeCommand> {
    match body.trim() {
        "/hit" => Some(LoungeCommand::Hit),
        _ => None,
    }
}

/// How long a spawn that ended stays on the composer, so the room reads
/// how it went.
pub(crate) const AFTERMATH_MS: i64 = 8_000;

/// Whether a session shows `spawn` at `now_ms`: live and not past its
/// time, or ended within the aftermath.
pub(crate) fn shows(spawn: &Spawn, now_ms: i64) -> bool {
    match spawn.ended_at_ms {
        None => now_ms < spawn.expires_at_ms,
        Some(ended) => now_ms < ended + AFTERMATH_MS,
    }
}

/// The names of everyone who hit, joined the way the wire says them:
/// "tom", "tom and dax", "tom, dax and mira".
pub(crate) fn names(spawn: &Spawn) -> String {
    let names: Vec<&str> = spawn.hits.iter().map(|hit| hit.username.as_str()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_string(),
        [all @ .., last] => format!("{} and {last}", all.join(", ")),
    }
}

/// What the composer says about `spawn` to `viewer`, when no word of the
/// viewer's own stands in for it.
pub(crate) fn composer_text(spawn: &Spawn, viewer: Uuid) -> String {
    let kind = spawn.kind.as_str();
    match spawn.outcome {
        Some(late_core::models::deadchannel_spawn::SpawnOutcome::Slain) => {
            let who = names(spawn);
            match pay_for(spawn.kind) {
                Pay::Bits => format!("the {kind} is down. {who} put it down."),
                Pay::Crystal => format!("the {kind} is down. {who} put it down. a crystal each."),
            }
        }
        Some(late_core::models::deadchannel_spawn::SpawnOutcome::Slipped) => {
            format!("the {kind} slipped away.")
        }
        None => {
            let left = spawn.hits_left();
            let hit = if spawn.hit_by(viewer) {
                String::new()
            } else {
                " /hit".to_string()
            };
            match spawn.hits.last() {
                None if left == 1 => format!("a {kind} is in the channel.{hit}"),
                None => format!("a {kind} is in the channel. {left} hits to put it down.{hit}"),
                Some(last) => format!(
                    "a {kind} is in the channel. {} hit it. {left} to go.{hit}",
                    last.username
                ),
            }
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
