//! The keypress-return push: one line in the empty composer for a session
//! that comes back after hours away, saying where the ladder stands. The
//! composer is where the return keypress lands, so it is the one row a
//! returner reads for sure; the AFK divider in the chat sits where they
//! stopped reading, which after a busy night is far above the fold. The
//! line is private by nature (it is your own input box), gone the moment
//! you type, and the usual hint is back once [`PUSH_FOR`] has passed.

use std::time::{Duration, Instant};

use late_core::models::daily_rung::{Rung, STAR_GLYPH};

use super::svc::QuestSnapshot;

/// Quiet at least this long before the keypress that brings the session
/// back earns the push. A coffee break or a long lunch does not.
pub(crate) const PUSH_AFTER: Duration = Duration::from_secs(2 * 60 * 60);

/// How long the push holds the empty composer after the return, so a
/// session that came back and only scrolled is not nagged all evening.
pub(crate) const PUSH_FOR: Duration = Duration::from_secs(10 * 60);

/// Whether today's rung is still to be won, for the line and its metric.
/// `pub` (not `pub(crate)`) because `metrics.rs` exposes it as a `pub` label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RungStanding {
    Open,
    Done,
}

/// The push's clock: when the session last came back from a long quiet,
/// and how long that quiet was.
#[derive(Debug, Default)]
pub(crate) struct ReturnPush {
    armed: Option<Armed>,
}

#[derive(Clone, Copy, Debug)]
struct Armed {
    at: Instant,
    away_for: Duration,
}

impl ReturnPush {
    /// A person's input landed after `away_for` quiet. Arms the push when
    /// that was long enough, and says so, for the caller to count.
    pub(crate) fn note_return(&mut self, now: Instant, away_for: Duration) -> bool {
        if away_for < PUSH_AFTER {
            return false;
        }
        self.armed = Some(Armed { at: now, away_for });
        true
    }

    /// The line for the empty composer while the push holds, read fresh
    /// each frame so a rung won inside the window reads as done.
    pub(crate) fn line(
        &self,
        now: Instant,
        snapshot: &QuestSnapshot,
        rung: Rung,
        standing: RungStanding,
    ) -> Option<String> {
        let armed = self.armed?;
        if now.duration_since(armed.at) >= PUSH_FOR {
            return None;
        }
        Some(push_line(
            armed.away_for,
            snapshot.daily_streak.consecutive_days,
            rung,
            standing,
        ))
    }
}

/// `back after 9 h · streak day 4 · the rung: medium Sudoku, open`. The
/// streak is left out at zero; the rung is always named, since the first
/// star is the whole point for someone without one.
fn push_line(away_for: Duration, streak_days: i32, rung: Rung, standing: RungStanding) -> String {
    let mut line = format!("back after {}", away_label(away_for));
    if streak_days > 0 {
        line.push_str(&format!(" · streak day {streak_days}"));
    }
    let standing = match standing {
        RungStanding::Open => "open".to_string(),
        RungStanding::Done => format!("done {STAR_GLYPH}"),
    };
    line.push_str(&format!(" · the rung: {}, {standing}", rung.label()));
    line
}

/// Whole hours up to two days, whole days past that: `9 h`, `3 d`.
fn away_label(away_for: Duration) -> String {
    let hours = away_for.as_secs() / 3600;
    if hours >= 48 {
        format!("{} d", hours / 24)
    } else {
        format!("{hours} h")
    }
}

#[cfg(test)]
#[path = "return_push_test.rs"]
mod return_push_test;
