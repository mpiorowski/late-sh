//! Plain data model for the World Cup HUD.
//!
//! These types are the distilled, render-ready shape the UI consumes. The
//! FotMob page JSON is converted into a [`WorldCupSnapshot`] in
//! [`super::fotmob`]; nothing here knows about JSON or the network.

use chrono::{DateTime, Utc};

/// A full tournament snapshot, published by the service and read by the UI.
///
/// `stale` means the most recent fetch failed and we are showing the last
/// good data; the UI surfaces that without going blank. An all-empty
/// snapshot (the `Default`) means we have nothing yet.
#[derive(Debug, Clone, Default)]
pub struct WorldCupSnapshot {
    pub season: String,
    pub groups: Vec<Group>,
    /// All tournament matches in chronological order.
    pub matches: Vec<Match>,
    /// Knockout rounds, ordered Round of 32 → Final (+ third place last).
    pub bracket: Vec<BracketRound>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

impl WorldCupSnapshot {
    /// True when we have no data at all to show.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.matches.is_empty() && self.bracket.is_empty()
    }

    /// Matches currently in progress.
    pub fn live(&self) -> impl Iterator<Item = &Match> {
        self.matches
            .iter()
            .filter(|m| m.status == MatchStatus::Live)
    }

    /// The next not-yet-started matches, soonest first. The source list is
    /// already chronological, so this is a filtered view.
    pub fn upcoming(&self) -> impl Iterator<Item = &Match> {
        self.matches
            .iter()
            .filter(|m| m.status == MatchStatus::Upcoming)
    }

    /// Most recently finished matches, newest first.
    pub fn recent_finished(&self) -> impl Iterator<Item = &Match> {
        self.matches
            .iter()
            .rev()
            .filter(|m| m.status == MatchStatus::Finished)
    }
}

/// One World Cup group with its current standings.
#[derive(Debug, Clone, Default)]
pub struct Group {
    /// Single-letter group id, "A".."L".
    pub letter: String,
    pub rows: Vec<TeamRow>,
}

/// A single standings row.
#[derive(Debug, Clone, Default)]
pub struct TeamRow {
    pub name: String,
    pub played: u32,
    pub goal_diff: i32,
    pub points: u32,
    pub qual: Qual,
}

/// Qualification status derived from FotMob's `qualColor`. Green = a direct
/// advancing slot, amber = a contended/third-place slot, none = out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Qual {
    #[default]
    None,
    Direct,
    Playoff,
}

/// A single match in the overview list.
#[derive(Debug, Clone, Default)]
pub struct Match {
    pub home: String,
    pub away: String,
    /// Present once the match is live or finished.
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub kickoff: Option<DateTime<Utc>>,
    pub status: MatchStatus,
    /// Short status reason for finished games, e.g. "FT", "AET", "Pen".
    pub reason_short: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchStatus {
    #[default]
    Upcoming,
    Live,
    Finished,
    Cancelled,
}

/// One knockout round (a column in the bracket).
#[derive(Debug, Clone, Default)]
pub struct BracketRound {
    /// Human label, e.g. "Round of 32", "Quarter-finals", "Final".
    pub label: String,
    pub matchups: Vec<Matchup>,
}

/// A single knockout tie.
#[derive(Debug, Clone, Default)]
pub struct Matchup {
    /// Full team name (used for the flag lookup).
    pub home_name: String,
    pub away_name: String,
    /// Short code shown as the label, e.g. "GER".
    pub home_short: String,
    pub away_short: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub winner: Winner,
    /// At least one side is not yet decided.
    pub tbd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Winner {
    #[default]
    None,
    Home,
    Away,
}
