use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use tokio_postgres::GenericClient;
use uuid::Uuid;

pub const REWARD_CLAIM_POLICY_ASSIGNMENT: &str = "assignment";
pub const REWARD_CLAIM_POLICY_COOLDOWN: &str = "cooldown";
pub const REWARD_CLAIM_POLICY_PER_EVENT: &str = "per_event";
pub const REWARD_CLAIM_POLICY_UTC_DAY: &str = "utc_day";

pub const ASTERION_DAILY_ESCAPE_REWARD_KEY: &str = "asterion_daily_escape";
pub const CHESS_WIN_REWARD_KEY: &str = "chess_win_payout";
pub const LATEANIA_ARCHDEMON_REWARD_KEY: &str = "lateania_archdemon_defeat";
pub const LATEANIA_FRONTIER_KING_REWARD_KEY: &str = "lateania_frontier_king_defeat";
pub const NETHACK_AMULET_REWARD_KEY: &str = "nethack_amulet";
pub const NETHACK_ASCENSION_REWARD_KEY: &str = "nethack_ascension";
pub const SSHATTRICK_WIN_REWARD_KEY: &str = "sshattrick_win_payout";
pub const TRON_WIN_2P_REWARD_KEY: &str = "tron_win_2p";
pub const TRON_WIN_3P_REWARD_KEY: &str = "tron_win_3p";
pub const TRON_WIN_4P_REWARD_KEY: &str = "tron_win_4p";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DailyPuzzleRewardGame {
    LeWord,
    Minesweeper,
    Nonogram,
    RubiksCube,
    Solitaire,
    Sudoku,
}

impl DailyPuzzleRewardGame {
    pub fn key(self) -> &'static str {
        match self {
            Self::LeWord => "le_word",
            Self::Minesweeper => "minesweeper",
            Self::Nonogram => "nonogram",
            Self::RubiksCube => "rubiks_cube",
            Self::Solitaire => "solitaire",
            Self::Sudoku => "sudoku",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RewardTemplate {
    pub id: Uuid,
    pub key: String,
    pub kind: String,
    pub params: Value,
    pub reward_chips: i64,
    pub claim_policy: String,
    pub cooldown_seconds: Option<i32>,
}

impl From<tokio_postgres::Row> for RewardTemplate {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            key: row.get("key"),
            kind: row.get("kind"),
            params: row.get("params"),
            reward_chips: row.get("reward_chips"),
            claim_policy: row.get("claim_policy"),
            cooldown_seconds: row.get("cooldown_seconds"),
        }
    }
}

impl RewardTemplate {
    pub async fn get_active_by_key(client: &impl GenericClient, key: &str) -> Result<Self> {
        let row = client
            .query_opt(
                "SELECT id, key, kind, params, reward_chips, claim_policy, cooldown_seconds
                 FROM reward_templates
                 WHERE key = $1 AND active = true",
                &[&key],
            )
            .await?;
        row.map(Self::from)
            .with_context(|| format!("active reward template {key:?} not found"))
    }

    pub fn ensure_claim_policy(&self, expected: &str) -> Result<()> {
        ensure!(
            self.claim_policy == expected,
            "reward template {} uses claim policy {}, expected {}",
            self.key,
            self.claim_policy,
            expected
        );
        Ok(())
    }

    pub fn cooldown(&self) -> Result<Duration> {
        self.ensure_claim_policy(REWARD_CLAIM_POLICY_COOLDOWN)?;
        let seconds = self
            .cooldown_seconds
            .context("cooldown reward template missing cooldown_seconds")?;
        ensure!(
            seconds > 0,
            "cooldown reward template {} has non-positive cooldown",
            self.key
        );
        Ok(Duration::from_secs(seconds as u64))
    }

    pub fn game(&self) -> Result<&str> {
        self.required_param_str("game")
    }

    pub fn payout_kind(&self) -> Result<&str> {
        self.required_param_str("payout_kind")
    }

    fn required_param_str(&self, name: &str) -> Result<&str> {
        self.params
            .get(name)
            .and_then(Value::as_str)
            .with_context(|| format!("reward template {} missing string param {name}", self.key))
    }
}

pub fn daily_puzzle_reward_key(game: DailyPuzzleRewardGame, difficulty_key: &str) -> String {
    format!(
        "{}_daily_{}_win",
        game.key(),
        difficulty_key.replace('-', "_")
    )
}

pub fn tron_win_reward_key(round_rider_count: usize) -> Option<&'static str> {
    match round_rider_count {
        2 => Some(TRON_WIN_2P_REWARD_KEY),
        3 => Some(TRON_WIN_3P_REWARD_KEY),
        count if count >= 4 => Some(TRON_WIN_4P_REWARD_KEY),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_puzzle_reward_key_uses_typed_game_and_normalized_difficulty() {
        assert_eq!(
            daily_puzzle_reward_key(DailyPuzzleRewardGame::Solitaire, "draw-3"),
            "solitaire_daily_draw_3_win"
        );
        assert_eq!(
            daily_puzzle_reward_key(DailyPuzzleRewardGame::LeWord, "daily"),
            "le_word_daily_daily_win"
        );
        assert_eq!(
            daily_puzzle_reward_key(DailyPuzzleRewardGame::RubiksCube, "daily"),
            "rubiks_cube_daily_daily_win"
        );
    }
}
