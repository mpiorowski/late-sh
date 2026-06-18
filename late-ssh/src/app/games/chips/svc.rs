use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::asterion::ASTERION_ESCAPE_LEDGER_REASON;
use late_core::models::chips::UserChips;
use late_core::models::game_payout::{GamePayout, GamePayoutClaim};
use late_core::models::reward::{
    ASTERION_DAILY_ESCAPE_REWARD_KEY, DailyPuzzleRewardGame, REWARD_CLAIM_POLICY_PER_EVENT,
    REWARD_CLAIM_POLICY_UTC_DAY, RewardTemplate, daily_puzzle_reward_key,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::{
    channel::ActivitySender,
    event::{ActivityEvent, ActivityGame, ActivityKind},
};

const LIFETIME_REWARD_PERIOD_KIND: &str = "lifetime";
const LIFETIME_REWARD_PERIOD_KEY: &str = "once";

#[derive(Clone)]
pub struct ChipService {
    db: Db,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardGrant {
    pub credited: bool,
    pub balance: i64,
    pub amount: i64,
}

impl ChipService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Ensure a chips row exists for the user. Called on SSH login.
    pub async fn ensure_chips(&self, user_id: Uuid) -> anyhow::Result<UserChips> {
        let client = self.db.get().await?;
        UserChips::ensure(&client, user_id).await
    }

    pub fn start_activity_reward_task(
        &self,
        activity_tx: ActivitySender,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        let mut rx = activity_tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Err(error) = svc.apply_activity_reward(event).await {
                            tracing::warn!(error = ?error, "failed to apply chip activity reward");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "chip activity reward receiver lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }

    async fn apply_activity_reward(&self, event: ActivityEvent) -> anyhow::Result<()> {
        let Some(user_id) = event.user_id else {
            return Ok(());
        };
        let ActivityKind::GameWon { game, detail, .. } = event.kind else {
            return Ok(());
        };
        let Some(game) = daily_puzzle_reward_game(game) else {
            return Ok(());
        };
        let Some(difficulty_key) = detail else {
            return Ok(());
        };

        let reward_key = daily_puzzle_reward_key(game, &difficulty_key);
        self.credit_daily_reward_template(
            user_id,
            &reward_key,
            event.occurred_at.date_naive(),
            "daily_puzzle_win",
        )
        .await?;
        Ok(())
    }

    pub async fn debit_bet(&self, user_id: Uuid, amount: i64) -> anyhow::Result<Option<i64>> {
        let client = self.db.get().await?;
        let chips = UserChips::deduct(&client, user_id, amount).await?;
        Ok(chips.map(|c| c.balance))
    }

    pub async fn credit_payout(&self, user_id: Uuid, amount: i64) -> anyhow::Result<i64> {
        let client = self.db.get().await?;
        let chips = UserChips::add_bonus(&client, user_id, amount).await?;
        Ok(chips.balance)
    }

    pub async fn has_asterion_daily_escape(
        &self,
        user_id: Uuid,
        escape_date: NaiveDate,
    ) -> anyhow::Result<bool> {
        self.has_daily_reward_claim(user_id, ASTERION_DAILY_ESCAPE_REWARD_KEY, escape_date)
            .await
    }

    pub async fn has_daily_reward_claim(
        &self,
        user_id: Uuid,
        reward_key: &str,
        payout_date: NaiveDate,
    ) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        template.ensure_claim_policy(REWARD_CLAIM_POLICY_UTC_DAY)?;
        GamePayout::has_claimed_daily(
            &client,
            user_id,
            template.game()?,
            template.payout_kind()?,
            payout_date,
        )
        .await
    }

    pub async fn credit_asterion_daily_escape(
        &self,
        user_id: Uuid,
        escape_date: NaiveDate,
    ) -> anyhow::Result<RewardGrant> {
        self.credit_daily_reward_template(
            user_id,
            ASTERION_DAILY_ESCAPE_REWARD_KEY,
            escape_date,
            ASTERION_ESCAPE_LEDGER_REASON,
        )
        .await
    }

    pub async fn credit_daily_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        payout_date: NaiveDate,
        ledger_reason: &str,
    ) -> anyhow::Result<RewardGrant> {
        let client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        template.ensure_claim_policy(REWARD_CLAIM_POLICY_UTC_DAY)?;
        let claim = GamePayout::grant_daily(
            &client,
            user_id,
            template.game()?,
            template.payout_kind()?,
            payout_date,
            template.reward_chips,
            ledger_reason,
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    pub async fn credit_cooldown_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        ledger_reason: &str,
    ) -> anyhow::Result<RewardGrant> {
        let mut client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        let cooldown = template.cooldown()?;
        let claim = GamePayout::grant_cooldown(
            &mut client,
            user_id,
            template.game()?,
            template.payout_kind()?,
            cooldown,
            template.reward_chips,
            ledger_reason,
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    pub async fn credit_lifetime_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        ledger_reason: &str,
    ) -> anyhow::Result<RewardGrant> {
        let client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        template.ensure_claim_policy(REWARD_CLAIM_POLICY_PER_EVENT)?;
        let claim = GamePayout::grant_period(
            &client,
            late_core::models::game_payout::GamePayoutPeriodGrant {
                user_id,
                game: template.game()?,
                payout_kind: template.payout_kind()?,
                period_kind: LIFETIME_REWARD_PERIOD_KIND,
                period_key: LIFETIME_REWARD_PERIOD_KEY,
                amount: template.reward_chips,
                ledger_reason,
            },
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    pub async fn restore_floor(&self, user_id: Uuid) -> anyhow::Result<i64> {
        let client = self.db.get().await?;
        let chips = UserChips::restore_floor(&client, user_id).await?;
        Ok(chips.balance)
    }
}

const fn reward_grant(amount: i64, claim: GamePayoutClaim) -> RewardGrant {
    RewardGrant {
        credited: claim.credited,
        balance: claim.balance,
        amount,
    }
}

const fn daily_puzzle_reward_game(game: ActivityGame) -> Option<DailyPuzzleRewardGame> {
    match game {
        ActivityGame::Minesweeper => Some(DailyPuzzleRewardGame::Minesweeper),
        ActivityGame::Nonogram => Some(DailyPuzzleRewardGame::Nonogram),
        ActivityGame::Solitaire => Some(DailyPuzzleRewardGame::Solitaire),
        ActivityGame::Sudoku => Some(DailyPuzzleRewardGame::Sudoku),
        ActivityGame::Sshattrick => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_puzzle_reward_game_accepts_only_daily_puzzle_games() {
        assert_eq!(
            daily_puzzle_reward_game(ActivityGame::Minesweeper),
            Some(DailyPuzzleRewardGame::Minesweeper)
        );
        assert_eq!(
            daily_puzzle_reward_game(ActivityGame::Sudoku),
            Some(DailyPuzzleRewardGame::Sudoku)
        );
        assert_eq!(daily_puzzle_reward_game(ActivityGame::Lateris), None);
        assert_eq!(daily_puzzle_reward_game(ActivityGame::Blackjack), None);
    }
}
