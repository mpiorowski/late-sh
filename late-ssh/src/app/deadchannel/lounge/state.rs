//! The pure side of the lounge spawns (GAME.md, "The lounge pass"): the
//! day's dice, when a spawn is due, what one is worth, the `/hit`
//! command, and what a session's composer says about it. No I/O, no
//! clock reads: every `now` is handed in.

use chrono::{DateTime, Timelike, Utc};
use late_core::models::deadchannel_spawn::{
    LoungeDay, LoungeDice, Spawn, SpawnKind, SpawnOutcome, Window,
};
use rand::Rng;
use uuid::Uuid;

use crate::app::deadchannel::fight::state::Leavings;

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

/// Hits that put a spawn of `kind` down, each from a different runner.
pub(crate) fn hits_needed(kind: SpawnKind) -> i32 {
    match kind {
        SpawnKind::Flicker => 1,
        SpawnKind::Howler => 3,
    }
}

/// How long a spawn of `kind` stays before it slips away.
pub(crate) fn lifetime_secs(kind: SpawnKind) -> i64 {
    match kind {
        SpawnKind::Flicker => 90,
        SpawnKind::Howler => 180,
    }
}

/// What a kill pays each hitter, in the sheet's own closed set
/// (`fight::state::Leavings`): a cache's worth of bits at the hitter's
/// level for a flicker, a crystal for a howler. Never exp: the climb's
/// pace is the rations' (GAME.md).
pub(crate) fn pay_for(kind: SpawnKind) -> Leavings {
    match kind {
        SpawnKind::Flicker => Leavings::Cache,
        SpawnKind::Howler => Leavings::Crystal,
    }
}

/// Whether `incoming`, heard on the channel, is older news of `held`
/// than what the process already holds: the same spawn, and the copy
/// held has ended while this one has not, or has more hits. Two hits
/// landing together can notify in the other order from their rows; the
/// older payload is dropped so a slain spawn never reads live again.
pub(crate) fn is_stale(held: Option<&Spawn>, incoming: &Spawn) -> bool {
    match held {
        Some(held) if held.id == incoming.id => {
            (held.ended_at_ms.is_some() && incoming.ended_at_ms.is_none())
                || held.hits.len() > incoming.hits.len()
        }
        Some(_) | None => false,
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

/// The composer command. Only `/hit` for now; parsed while the session
/// is a runner looking at #lounge, so anywhere else it is an unknown
/// command like any other slash word.
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

/// How long the scene plays a hit: the hitter's strike crosses to the
/// glyph for this long after their hit landed.
pub(crate) const STRIKE_MS: i64 = 5_000;

/// Whether the scene has frames to play at `now_ms`: a hit within
/// `STRIKE_MS`, or the glyph dissolving through the aftermath.
pub(crate) fn animates(spawn: &Spawn, now_ms: i64) -> bool {
    let striking = spawn
        .hits
        .last()
        .is_some_and(|hit| now_ms < hit.at_ms + STRIKE_MS);
    striking || spawn.ended_at_ms.is_some()
}

/// Whole seconds a live spawn has left at `now_ms`; `None` once it ended.
pub(crate) fn left_secs(spawn: &Spawn, now_ms: i64) -> Option<i64> {
    match spawn.ended_at_ms {
        Some(_) => None,
        None => Some((spawn.expires_at_ms - now_ms).max(0) / 1_000),
    }
}

/// What the scene says about `spawn`: the headline, the detail under
/// it, and whether the `/hit` key goes with them: lit while the spawn is
/// live and the viewer has not hit it. The ui styles the key off the
/// flag, never off the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneText {
    pub headline: String,
    pub detail: String,
    pub hit_key: bool,
}

/// What the scene says about `spawn` to `viewer`, when no word of the
/// viewer's own stands in for the detail.
pub(crate) fn scene_text(spawn: &Spawn, viewer: Uuid) -> SceneText {
    let kind = spawn.kind.as_str();
    match spawn.outcome {
        Some(SpawnOutcome::Slain) => {
            let who = names(spawn);
            let detail = match pay_for(spawn.kind) {
                Leavings::Cache => format!("{who} put it down."),
                Leavings::Crystal => format!("{who} put it down. a crystal each."),
            };
            SceneText {
                headline: format!("the {kind} is down."),
                detail,
                hit_key: false,
            }
        }
        Some(SpawnOutcome::Slipped) => SceneText {
            headline: format!("the {kind} slipped away."),
            detail: match spawn.hits.is_empty() {
                true => "nobody put it down.".to_string(),
                false => format!("{} hit it. not enough.", names(spawn)),
            },
            hit_key: false,
        },
        None => {
            let left = spawn.hits_left();
            let detail = match spawn.hits.last() {
                None if left == 1 => "one hit puts it down.".to_string(),
                None => format!("{left} hits put it down."),
                Some(last) => format!("{} hit it. {left} to go.", last.username),
            };
            SceneText {
                headline: format!("a {kind} is in the channel."),
                detail,
                hit_key: !spawn.hit_by(viewer),
            }
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
