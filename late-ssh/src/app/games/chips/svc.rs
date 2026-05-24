use std::time::Duration;

use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::asterion::{
    ASTERION_DAILY_ESCAPE_PAYOUT, ASTERION_ESCAPE_LEDGER_REASON, ASTERION_ESCAPE_PAYOUT_KIND,
    ASTERION_GAME_KEY,
};
use late_core::models::chips::{UserChips, difficulty_bonus};
use late_core::models::game_payout::{GamePayout, GamePayoutClaim};
use uuid::Uuid;

#[derive(Clone)]
pub struct ChipService {
    db: Db,
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

    pub fn grant_daily_bonus_task(&self, user_id: Uuid, difficulty_key: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            let bonus = difficulty_bonus(&difficulty_key);
            if let Err(e) = svc.grant_bonus(user_id, bonus).await {
                tracing::error!(error = ?e, "failed to grant chip bonus");
            }
        });
    }

    async fn grant_bonus(&self, user_id: Uuid, amount: i64) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        UserChips::add_bonus(&client, user_id, amount).await?;
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
        self.has_daily_game_payout(
            user_id,
            ASTERION_GAME_KEY,
            ASTERION_ESCAPE_PAYOUT_KIND,
            escape_date,
        )
        .await
    }

    pub async fn has_daily_game_payout(
        &self,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        payout_date: NaiveDate,
    ) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        GamePayout::has_claimed_daily(&client, user_id, game, payout_kind, payout_date).await
    }

    pub async fn credit_asterion_daily_escape(
        &self,
        user_id: Uuid,
        escape_date: NaiveDate,
    ) -> anyhow::Result<GamePayoutClaim> {
        self.credit_daily_game_payout(
            user_id,
            ASTERION_GAME_KEY,
            ASTERION_ESCAPE_PAYOUT_KIND,
            escape_date,
            ASTERION_DAILY_ESCAPE_PAYOUT,
            ASTERION_ESCAPE_LEDGER_REASON,
        )
        .await
    }

    pub async fn credit_daily_game_payout(
        &self,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        payout_date: NaiveDate,
        amount: i64,
        ledger_reason: &str,
    ) -> anyhow::Result<GamePayoutClaim> {
        let client = self.db.get().await?;
        GamePayout::grant_daily(
            &client,
            user_id,
            game,
            payout_kind,
            payout_date,
            amount,
            ledger_reason,
        )
        .await
    }

    pub async fn credit_cooldown_game_payout(
        &self,
        user_id: Uuid,
        game: &str,
        payout_kind: &str,
        cooldown: Duration,
        amount: i64,
        ledger_reason: &str,
    ) -> anyhow::Result<GamePayoutClaim> {
        let client = self.db.get().await?;
        GamePayout::grant_cooldown(
            &client,
            user_id,
            game,
            payout_kind,
            cooldown,
            amount,
            ledger_reason,
        )
        .await
    }

    pub async fn restore_floor(&self, user_id: Uuid) -> anyhow::Result<i64> {
        let client = self.db.get().await?;
        let chips = UserChips::restore_floor(&client, user_id).await?;
        Ok(chips.balance)
    }
}
