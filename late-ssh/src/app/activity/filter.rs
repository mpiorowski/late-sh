use super::event::{ActivityCategory, ActivityEvent, ActivityGame, ActivityKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityFilter {
    categories: &'static [ActivityCategory],
}

impl ActivityFilter {
    pub const fn dashboard() -> Self {
        Self {
            categories: &[
                ActivityCategory::Session,
                ActivityCategory::Game,
                ActivityCategory::Bonsai,
            ],
        }
    }

    pub fn includes(&self, event: &ActivityEvent) -> bool {
        self.categories.contains(&event.category())
    }
}

/// THE routing decision for #lounge system lines: invitations and stories in,
/// grind out. Every kind and every game is matched explicitly — when a new
/// event or game is added, the compiler drags you here to decide whether it
/// ships a story into the lounge.
pub fn lounge_includes(event: &ActivityEvent) -> bool {
    match &event.kind {
        // Presence story: someone showed up.
        ActivityKind::UserJoined => true,
        // Invitations: an open seat someone can still claim.
        ActivityKind::SatDown { .. } => true,
        // Door-game stories: entering a world, felling its bosses.
        ActivityKind::GameStarted { .. } | ActivityKind::BossSlain { .. } => true,
        ActivityKind::GameEvent { game, .. } => match game {
            // Door games: their moments are curated at the source
            // (start/descend/die/milestones), so they read as stories.
            ActivityGame::Mud | ActivityGame::Nethack | ActivityGame::GreenDragon => true,
            ActivityGame::Asterion
            | ActivityGame::Blackjack
            | ActivityGame::Chess
            | ActivityGame::LeWord
            | ActivityGame::Minesweeper
            | ActivityGame::Nonogram
            | ActivityGame::Poker
            | ActivityGame::RubiksCube
            | ActivityGame::Sshattrick
            | ActivityGame::Solitaire
            | ActivityGame::Sudoku
            | ActivityGame::TicTacToe
            | ActivityGame::Lateris
            | ActivityGame::TwentyFortyEight
            | ActivityGame::Tron
            | ActivityGame::Snake
            | ActivityGame::Traffic => false,
        },
        ActivityKind::GameWon { game, .. } => match game {
            // Human-vs-human matches are rare enough to be stories.
            ActivityGame::Asterion
            | ActivityGame::Chess
            | ActivityGame::Sshattrick
            | ActivityGame::TicTacToe
            | ActivityGame::Tron => true,
            // Door-game wins are milestone-gated at the source (dragon
            // kills, NetHack amulet/ascension) — stories.
            ActivityGame::GreenDragon | ActivityGame::Nethack => true,
            // Lateania fires a win per mob kill; boss kills arrive as
            // `BossSlain` instead.
            ActivityGame::Mud => false,
            // Per-hand gambling wins are pure noise; the sit is the story.
            ActivityGame::Blackjack | ActivityGame::Poker => false,
            // Solo arcade solves: high volume, no second player to invite.
            ActivityGame::LeWord
            | ActivityGame::Minesweeper
            | ActivityGame::Nonogram
            | ActivityGame::RubiksCube
            | ActivityGame::Solitaire
            | ActivityGame::Sudoku
            | ActivityGame::Lateris
            | ActivityGame::TwentyFortyEight
            | ActivityGame::Snake
            | ActivityGame::Traffic => false,
        },
        // Finished daily correspondence matches: one line per match (win/loss
        // or draw). Rare and human-vs-human, so a genuine story.
        ActivityKind::DailyResult { .. } => true,
        // Quest-only grind signals, never surfaced anywhere public.
        ActivityKind::GamePlayed { .. } | ActivityKind::GameScored { .. } => false,
        // The bonsai is a private ritual: neither the daily watering nor the
        // death after N dry days belongs in the public feed.
        ActivityKind::BonsaiWatered => false,
        ActivityKind::BonsaiLost { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::app::activity::event::ActivityEvent;

    #[test]
    fn dashboard_filter_includes_public_activity() {
        let event = ActivityEvent::joined(Uuid::nil(), "user");

        assert!(ActivityFilter::dashboard().includes(&event));
    }
}
