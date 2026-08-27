use chrono::{DateTime, NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::chips::{ChipMove, UserChips};
use late_core::models::drinks::UserDrinks;
use late_core::models::game_payout::{
    GAME_PAYOUT_PERIOD_COOLDOWN, GamePayout, GamePayoutClaim, GamePayoutKey, GamePayoutMultiGrant,
};
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

// `period_kind = "lifetime"` is gone from every write path: SHOP.md Phase 6
// made every door milestone repeatable, so nothing pays once per account for
// life any more. The rows already banked under it stay as history, and no gate
// reads them.
const PER_EVENT_REWARD_PERIOD_KIND: &str = "event";
/// The lobby's pair-day cap (SHOP.md Phase 7): one paid win per opponent per
/// game per UTC day the match was posted. The key is
/// `<opponent id>:<posting date>`, and the claim row carries the template's
/// `game` like every other claim, so each roster game has its own row.
const PAIR_DAY_REWARD_PERIOD_KIND: &str = "pair_day";

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

/// Result of a successful bartender drink purchase.
#[derive(Debug, Clone, Copy)]
pub struct DrinkPurchase {
    pub balance: i64,
    pub drunk_points: i64,
    pub last_drink_at: DateTime<Utc>,
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
            ChipMove::DailyPuzzleWin,
        )
        .await?;
        Ok(())
    }

    pub async fn debit_bet(&self, user_id: Uuid, amount: i64) -> anyhow::Result<Option<i64>> {
        let client = self.db.get().await?;
        let chips = UserChips::apply(&**client, user_id, ChipMove::Bet, amount, None).await?;
        Ok(chips.map(|c| c.balance))
    }

    /// Charge a bartender drink (floor-guarded) and record the buzz in one
    /// transaction, so a crash can't charge without pouring. Returns None
    /// when the user can't cover the drink and keep the chip floor.
    pub async fn buy_drink(
        &self,
        user_id: Uuid,
        price: i64,
        drink: &str,
    ) -> anyhow::Result<Option<DrinkPurchase>> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(chips) =
            UserChips::apply(&*tx, user_id, ChipMove::DrinkPurchase, price, Some(drink)).await?
        else {
            return Ok(None);
        };
        let drinks = UserDrinks::record_purchase(&tx, user_id, price).await?;
        tx.commit().await?;
        Ok(Some(DrinkPurchase {
            balance: chips.balance,
            drunk_points: drinks.drunk_points,
            last_drink_at: drinks.last_drink_at,
        }))
    }

    /// Comp the newcomer's welcome pour: record the buzz with no chip debit
    /// (it's on the house) and hand back the fresh buzz so the clubhouse glow
    /// can light up immediately. At most once per user ever, guarded by the
    /// `user_drinks` insert; `None` means they have drunk before and the
    /// welcome is spent.
    pub async fn grant_free_drink(
        &self,
        user_id: Uuid,
        points: i64,
    ) -> anyhow::Result<Option<UserDrinks>> {
        let client = self.db.get().await?;
        UserDrinks::record_welcome_pour(&client, user_id, points).await
    }

    pub async fn credit_payout(&self, user_id: Uuid, amount: i64) -> anyhow::Result<i64> {
        let client = self.db.get().await?;
        match UserChips::apply(&**client, user_id, ChipMove::Credit, amount, None).await? {
            Some(chips) => Ok(chips.balance),
            None => anyhow::bail!("chip credit returned no row"),
        }
    }

    /// One ledger move for a named [`ChipMove`], with no reward template and
    /// no cooldown behind it: the perpetual Super Snake arena settles every
    /// food, arena clear, and crash the instant it happens. `None` means a
    /// debit the balance could not cover.
    pub async fn apply_move(
        &self,
        user_id: Uuid,
        chip_move: ChipMove,
        amount: i64,
    ) -> anyhow::Result<Option<i64>> {
        let client = self.db.get().await?;
        let chips = UserChips::apply(&**client, user_id, chip_move, amount, None).await?;
        Ok(chips.map(|chips| chips.balance))
    }

    pub async fn transfer_chips(
        &self,
        sender_id: Uuid,
        recipient_id: Uuid,
        amount: i64,
    ) -> anyhow::Result<(i64, i64)> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some((sender, recipient)) =
            UserChips::transfer_gift(&tx, sender_id, recipient_id, amount).await?
        else {
            anyhow::bail!("insufficient chips");
        };
        tx.commit().await?;
        Ok((sender.balance, recipient.balance))
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
            ChipMove::AsterionEscape,
        )
        .await
    }

    pub async fn credit_daily_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        payout_date: NaiveDate,
        chip_move: ChipMove,
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
            chip_move,
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    pub async fn credit_cooldown_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        chip_move: ChipMove,
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
            chip_move,
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    /// Credit a `per_event` reward once per distinct `event_key` (forever).
    /// Unlike the lifetime grant this pays for each event — e.g. every
    /// distinct daily-match win, keyed on the match id — while staying
    /// idempotent per event, so a re-broadcast or retry never double-pays.
    pub async fn credit_per_event_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        event_key: &str,
        chip_move: ChipMove,
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
                period_kind: PER_EVENT_REWARD_PERIOD_KIND,
                period_key: event_key,
                amount: template.reward_chips,
                chip_move,
            },
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    /// Credit a `per_event` reward that is also capped per counterpart per
    /// posting day: it pays once per distinct `event_key` (a daily match id)
    /// AND once per `pair_day_key` (`<opponent id>:<UTC date the match was
    /// posted>`). Both claims land or neither does.
    ///
    /// Both claims are scoped to the template's `game`, so the cap is per
    /// roster game: chess and battleship against the same opponent on the
    /// same day both pay. Decided in SHOP.md Phase 7 (2026-08-27): honest
    /// friends who play several games together are never touched, and a
    /// colluding pair is bounded at one paid win per game per direction per
    /// day, which is the whole list of eight before it stops.
    ///
    /// This is what closes the lobby's resign loop: two accounts can post,
    /// claim and resign all day, but every match they post today in one game
    /// shares one pair-day key and pays once. Keying on the posting day
    /// rather than the finishing day is what keeps honest play whole: two long
    /// games against the same opponent were posted on different days, so both
    /// pay whichever day they end.
    pub async fn credit_per_event_pair_day_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        event_key: &str,
        pair_day_key: &str,
        chip_move: ChipMove,
    ) -> anyhow::Result<RewardGrant> {
        let mut client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        template.ensure_claim_policy(REWARD_CLAIM_POLICY_PER_EVENT)?;
        let claim = GamePayout::grant_multi(
            &mut client,
            GamePayoutMultiGrant {
                user_id,
                game: template.game()?,
                payout_kind: template.payout_kind()?,
                keys: &[
                    GamePayoutKey::Unique {
                        period_kind: PER_EVENT_REWARD_PERIOD_KIND,
                        period_key: event_key,
                    },
                    GamePayoutKey::Unique {
                        period_kind: PAIR_DAY_REWARD_PERIOD_KIND,
                        period_key: pair_day_key,
                    },
                ],
                amount: template.reward_chips,
                chip_move,
            },
        )
        .await?;
        Ok(reward_grant(template.reward_chips, claim))
    }

    /// Credit a `cooldown` reward that also has to be new: it pays once per
    /// distinct `event_key` (a roguelike run's log line, a Lateania character)
    /// AND at most once per the template's cooldown window per account. Both
    /// claims land or neither does, so a milestone gated by the lockout leaves
    /// no trace and the same event can pay later only if it was never paid.
    ///
    /// This is what makes the door milestones repeatable: the event key
    /// absorbs a log replay, the window spaces the repeats. Claims banked
    /// under the old lifetime gate carry a different `period_kind` and are
    /// invisible to both.
    pub async fn credit_run_cooldown_reward_template(
        &self,
        user_id: Uuid,
        reward_key: &str,
        event_key: &str,
        chip_move: ChipMove,
    ) -> anyhow::Result<RewardGrant> {
        let mut client = self.db.get().await?;
        let template = RewardTemplate::get_active_by_key(&**client, reward_key).await?;
        let cooldown = template.cooldown()?;
        let claim = GamePayout::grant_multi(
            &mut client,
            GamePayoutMultiGrant {
                user_id,
                game: template.game()?,
                payout_kind: template.payout_kind()?,
                keys: &[
                    GamePayoutKey::Unique {
                        period_kind: PER_EVENT_REWARD_PERIOD_KIND,
                        period_key: event_key,
                    },
                    GamePayoutKey::Cooldown {
                        period_kind: GAME_PAYOUT_PERIOD_COOLDOWN,
                        window: cooldown,
                    },
                ],
                amount: template.reward_chips,
                chip_move,
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
        ActivityGame::LeWord => Some(DailyPuzzleRewardGame::LeWord),
        ActivityGame::Minesweeper => Some(DailyPuzzleRewardGame::Minesweeper),
        ActivityGame::Nonogram => Some(DailyPuzzleRewardGame::Nonogram),
        ActivityGame::RubiksCube => Some(DailyPuzzleRewardGame::RubiksCube),
        ActivityGame::Solitaire => Some(DailyPuzzleRewardGame::Solitaire),
        ActivityGame::Sudoku => Some(DailyPuzzleRewardGame::Sudoku),
        ActivityGame::Sshattrick => None,
        _ => None,
    }
}

#[cfg(test)]
#[path = "svc_internal_test.rs"]
mod svc_internal_test;
