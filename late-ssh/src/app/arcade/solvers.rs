//! Who solved a daily today: the count on the lobby card and the roster
//! behind the `o` modal, computed from the leaderboard snapshot plus this
//! session's own wins. Pure: no I/O, no clock beyond the date passed in.
//!
//! Names are never ranked. A daily runs on the UTC day, so "first" would only
//! say whose timezone woke up earlier; each tier is shuffled once per day
//! from a date seed instead, the same order for every viewer.

use std::collections::HashMap;

use chrono::NaiveDate;
use late_core::models::leaderboard::{DailyCompletionStatus, DailyPuzzle};
use uuid::Uuid;

use crate::usernames::UsernameResolver;

/// The snapshot's view of today plus this session's own marks. The session's
/// wins land in the snapshot within a refresh; until then they are OR'd in
/// here, the same way the card's ✓ marks work.
pub(crate) struct TodaySolvers<'a> {
    pub(crate) statuses: &'a HashMap<Uuid, DailyCompletionStatus>,
    pub(crate) usernames: &'a dyn UsernameResolver,
    pub(crate) own_user_id: Uuid,
    pub(crate) own_username: &'a str,
    pub(crate) own_session: Option<&'a DailyCompletionStatus>,
}

/// One difficulty tier of the roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TierSolvers {
    pub(crate) difficulty_key: &'static str,
    pub(crate) chips: i64,
    /// Usernames in the day's shuffled order.
    pub(crate) names: Vec<String>,
}

/// Everyone who solved one daily today, hardest tier first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Roster {
    /// Distinct solvers across every tier.
    pub(crate) total: usize,
    pub(crate) tiers: Vec<TierSolvers>,
}

impl TodaySolvers<'_> {
    fn solved_tier(&self, user_id: Uuid, game: DailyPuzzle, difficulty_key: &str) -> bool {
        let in_snapshot = self
            .statuses
            .get(&user_id)
            .is_some_and(|s| s.completed_difficulty(game, difficulty_key));
        let in_session = user_id == self.own_user_id
            && self
                .own_session
                .is_some_and(|s| s.completed_difficulty(game, difficulty_key));
        in_snapshot || in_session
    }

    fn solved_any(&self, user_id: Uuid, game: DailyPuzzle) -> bool {
        let in_snapshot = self
            .statuses
            .get(&user_id)
            .is_some_and(|s| s.completed(game));
        let in_session =
            user_id == self.own_user_id && self.own_session.is_some_and(|s| s.completed(game));
        in_snapshot || in_session
    }

    /// Every user id with a mark today, the session's own included.
    fn candidates(&self) -> impl Iterator<Item = Uuid> + '_ {
        let own = if self.statuses.contains_key(&self.own_user_id) {
            None
        } else {
            Some(self.own_user_id)
        };
        self.statuses.keys().copied().chain(own)
    }

    fn name_of(&self, user_id: Uuid) -> Option<String> {
        if user_id == self.own_user_id {
            return Some(self.own_username.to_string());
        }
        self.usernames.username(&user_id).cloned()
    }

    /// How many distinct people solved `game` today, at any tier.
    pub(crate) fn solved_count(&self, game: DailyPuzzle) -> usize {
        self.candidates()
            .filter(|id| self.solved_any(*id, game))
            .count()
    }

    /// The full roster of `game`, one block per tier of `game.tiers()`
    /// (the list the lobby card's marks read, easiest first); the roster
    /// comes back hardest first.
    pub(crate) fn roster(&self, date: NaiveDate, game: DailyPuzzle) -> Roster {
        let tiers = game
            .tiers()
            .list()
            .into_iter()
            .rev()
            .map(|(difficulty_key, tier)| {
                let mut names: Vec<String> = self
                    .candidates()
                    .filter(|id| self.solved_tier(*id, game, difficulty_key))
                    .filter_map(|id| self.name_of(id))
                    .collect();
                // Sort first so the shuffle sees the same input whatever
                // order the maps iterate in.
                names.sort();
                shuffle(&mut names, day_seed(date, game, difficulty_key));
                TierSolvers {
                    difficulty_key,
                    chips: tier.chips(),
                    names,
                }
            })
            .collect();
        Roster {
            total: self.solved_count(game),
            tiers,
        }
    }
}

/// FNV-1a over the date, game and tier: one fixed order per tier per day.
fn day_seed(date: NaiveDate, game: DailyPuzzle, difficulty_key: &str) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325u64;
    for byte in b"late-sh-daily-solvers-v1"
        .iter()
        .copied()
        .chain(date.format("%Y-%m-%d").to_string().bytes())
        .chain(game.key().bytes())
        .chain(difficulty_key.bytes())
    {
        seed ^= u64::from(byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed
}

/// Fisher-Yates over a splitmix64 stream, so the order depends on the seed
/// alone and not on a platform RNG.
fn shuffle<T>(items: &mut [T], mut seed: u64) {
    let mut next = move || {
        seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
#[path = "solvers_test.rs"]
mod solvers_test;
