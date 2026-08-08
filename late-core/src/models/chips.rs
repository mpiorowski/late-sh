use std::collections::HashMap;

use anyhow::{Result, bail, ensure};
use chrono::NaiveDate;
use tokio_postgres::{Client, GenericClient, Transaction};
use uuid::Uuid;

pub const CHIP_FLOOR: i64 = 100;
pub const INITIAL_CHIP_BALANCE: i64 = 1_000;
pub const CHIP_USER_CHANGED_CHANNEL: &str = "chip_user_changed";

pub async fn listen_for_chip_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {CHIP_USER_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// The three daily-puzzle difficulty tiers. One enum owns both reward
/// scales: the chip bonus a daily win pays (mirrored in seeded
/// `reward_templates` rows) and the Arcade Wins leaderboard points, so the
/// two can never drift apart in string-matched copies. Solitaire's draw
/// modes are not tiers: draw-1 pays [`Difficulty::Medium`] chips but scores
/// [`Difficulty::Easy`] points, so its mapping lives at each consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: &'static [Self] = &[Self::Easy, Self::Medium, Self::Hard];

    /// The persisted `difficulty_key` value.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Hard => "hard",
        }
    }

    /// Chip bonus for a daily win at this tier.
    pub const fn chips(self) -> i64 {
        match self {
            Self::Easy => 100,
            Self::Medium => 250,
            Self::Hard => 500,
        }
    }

    /// Monthly Arcade Wins leaderboard points for a daily win at this tier.
    pub const fn points(self) -> i64 {
        match self {
            Self::Easy => 1,
            Self::Medium => 3,
            Self::Hard => 5,
        }
    }
}

/// Defines [`ChipMove`] and its `ALL` roster from one variant list: a
/// variant cannot exist without an `ALL` entry, so roster-derived lists
/// (like the earnings exclusions) can never silently skip one.
macro_rules! chip_moves {
    ($($(#[$doc:meta])* $variant:ident),+ $(,)?) => {
        /// Every way chips move. Adding a variant forces a decision in each
        /// match below: the persisted ledger reason, the direction and floor
        /// guard, the source kind, and whether the move counts toward the
        /// monthly chip-earner leaderboard. Call sites name their move
        /// instead of passing raw strings, and the `user_chips` triggers
        /// (migration 128) own the `chip_user_changed` notify, so no move
        /// can forget it.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum ChipMove {
            $($(#[$doc])* $variant,)+
        }

        impl ChipMove {
            /// Every variant in declaration order, generated from the same
            /// list as the enum itself.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
    };
}

chip_moves!(
    /// Generic game credit: house-table payouts, bonsai watering bonus.
    Credit,
    /// Generic wager debit: house-table bets, may drain the balance to zero
    /// (losing settlements restore the floor afterwards).
    Bet,
    /// Post-settlement top-up back to [`CHIP_FLOOR`]. Has its own write path
    /// ([`UserChips::restore_floor`]), never goes through [`UserChips::apply`].
    FloorRestore,
    GiftSent,
    GiftReceived,
    DrinkPurchase,
    ShopPurchase,
    QuestReward,
    DailyQuestStreakReward,
    DailyPuzzleWin,
    AsterionEscape,
    DailyChessWin,
    DailyBattleshipWin,
    DailyConnectFourWin,
    DailyReversiWin,
    DailyCheckersWin,
    DailyBackgammonWin,
    DailyBriscolaWin,
    TronWin,
    SsnakeWin,
    GreendragonDragonSlain,
    NethackAmuletAcquired,
    NethackAscension,
    DcssOrbFound,
    DcssOrbEscape,
    BrogueEscape,
    BrogueMastery,
    LateaniaArchdemonDefeat,
    LateaniaFrontierKingDefeat,
);

/// Which way a move touches the balance, and under what guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipDirection {
    Credit,
    /// The debit only succeeds while `balance - amount >= floor`.
    Debit {
        floor: i64,
    },
    /// Not a delta at all; handled by [`UserChips::restore_floor`].
    Restore,
}

impl ChipMove {
    /// The persisted `chip_ledger.reason` value.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Credit => "chip_credit",
            Self::Bet => "chip_debit",
            Self::FloorRestore => "floor_restore",
            Self::GiftSent => "chip_gift_sent",
            Self::GiftReceived => "chip_gift_received",
            Self::DrinkPurchase => "drink_purchase",
            Self::ShopPurchase => "shop_purchase",
            Self::QuestReward => "quest_reward",
            Self::DailyQuestStreakReward => "daily_quest_streak_reward",
            Self::DailyPuzzleWin => "daily_puzzle_win",
            Self::AsterionEscape => "asterion_escape",
            Self::DailyChessWin => "daily_chess_win",
            Self::DailyBattleshipWin => "daily_battleship_win",
            Self::DailyConnectFourWin => "daily_connect4_win",
            Self::DailyReversiWin => "daily_reversi_win",
            Self::DailyCheckersWin => "daily_checkers_win",
            Self::DailyBackgammonWin => "daily_backgammon_win",
            Self::DailyBriscolaWin => "daily_briscola_win",
            Self::TronWin => "tron_win",
            Self::SsnakeWin => "ssnake_win",
            Self::GreendragonDragonSlain => "greendragon_dragon_slain",
            Self::NethackAmuletAcquired => "nethack_amulet_acquired",
            Self::NethackAscension => "nethack_ascension",
            Self::DcssOrbFound => "dcss_orb_found",
            Self::DcssOrbEscape => "dcss_orb_escape",
            Self::BrogueEscape => "brogue_escape",
            Self::BrogueMastery => "brogue_mastery",
            Self::LateaniaArchdemonDefeat => "lateania_archdemon_defeat",
            Self::LateaniaFrontierKingDefeat => "lateania_frontier_king_defeat",
        }
    }

    /// The persisted `chip_ledger.source_kind` value.
    pub const fn source_kind(self) -> &'static str {
        match self {
            Self::Credit | Self::Bet | Self::FloorRestore | Self::GiftSent | Self::GiftReceived => {
                "user_chips"
            }
            Self::DrinkPurchase => "bartender",
            Self::ShopPurchase => "marketplace_item",
            Self::QuestReward => "quest_assignment",
            Self::DailyQuestStreakReward => "daily_quest_streak",
            Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::SsnakeWin
            | Self::GreendragonDragonSlain
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat => "game_payout_claims",
        }
    }

    pub const fn direction(self) -> ChipDirection {
        match self {
            Self::Credit
            | Self::GiftReceived
            | Self::QuestReward
            | Self::DailyQuestStreakReward
            | Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::SsnakeWin
            | Self::GreendragonDragonSlain
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat => ChipDirection::Credit,
            Self::Bet | Self::ShopPurchase => ChipDirection::Debit { floor: 0 },
            Self::GiftSent | Self::DrinkPurchase => ChipDirection::Debit { floor: CHIP_FLOOR },
            Self::FloorRestore => ChipDirection::Restore,
        }
    }

    /// Whether the move counts toward the monthly chip-earner leaderboard
    /// and the permanent monthly award snapshot.
    pub const fn counts_as_earnings(self) -> bool {
        match self {
            Self::FloorRestore | Self::ShopPurchase => false,
            Self::Credit
            | Self::Bet
            | Self::GiftSent
            | Self::GiftReceived
            | Self::DrinkPurchase
            | Self::QuestReward
            | Self::DailyQuestStreakReward
            | Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::SsnakeWin
            | Self::GreendragonDragonSlain
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat => true,
        }
    }

    /// The `chip_ledger.reason` values excluded from earnings queries.
    /// Both consumers (monthly leaderboard, monthly award snapshot) build
    /// their exclusion list here, so they can never drift apart.
    pub fn excluded_earning_reasons() -> Vec<&'static str> {
        Self::ALL
            .iter()
            .filter(|mv| !mv.counts_as_earnings())
            .map(|mv| mv.reason())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct UserChips {
    pub user_id: Uuid,
    pub balance: i64,
    pub last_stipend_date: Option<NaiveDate>,
}

impl From<tokio_postgres::Row> for UserChips {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            user_id: row.get("user_id"),
            balance: row.get("balance"),
            last_stipend_date: row.get("last_stipend_date"),
        }
    }
}

impl UserChips {
    /// Load the user's chips row without creating one, `None` if they have no
    /// chip account yet. Chip rows are created lazily on the first chip
    /// operation, not on profile access, so callers verifying that invariant
    /// need a read that never inserts.
    pub async fn find(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM user_chips WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Ensure a chips row exists for the user. Called on SSH login.
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO user_chips (user_id, balance)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO NOTHING
                 RETURNING *",
                &[&user_id, &INITIAL_CHIP_BALANCE],
            )
            .await;
        match row {
            Ok(row) => Ok(Self::from(row)),
            Err(_) => {
                // Row already existed, fetch it
                let row = client
                    .query_one("SELECT * FROM user_chips WHERE user_id = $1", &[&user_id])
                    .await?;
                Ok(Self::from(row))
            }
        }
    }

    /// The single write path for delta chip moves: one guarded balance
    /// update plus its ledger row, in one statement. Credits upsert (a
    /// missing row starts at the credited amount); debits enforce the floor
    /// from [`ChipMove::direction`] and return `None` when the balance
    /// cannot cover the move. The `chip_user_changed` notify comes from the
    /// `user_chips` triggers, never from here.
    pub async fn apply(
        client: &impl GenericClient,
        user_id: Uuid,
        mv: ChipMove,
        amount: i64,
        source_ref: Option<&str>,
    ) -> Result<Option<Self>> {
        ensure!(amount > 0, "chip move amount must be positive");
        match mv.direction() {
            ChipDirection::Credit => {
                let row = client
                    .query_one(
                        "WITH upserted AS (
                            INSERT INTO user_chips (user_id, balance)
                            VALUES ($1, $2)
                            ON CONFLICT (user_id) DO UPDATE SET
                              balance = user_chips.balance + $2,
                              updated = current_timestamp
                            RETURNING *
                         ),
                         ledger AS (
                            INSERT INTO chip_ledger
                              (user_id, delta, reason, source_kind, source_ref)
                            SELECT user_id, $2, $3, $4, $5
                            FROM upserted
                         )
                         SELECT * FROM upserted",
                        &[
                            &user_id,
                            &amount,
                            &mv.reason(),
                            &mv.source_kind(),
                            &source_ref,
                        ],
                    )
                    .await?;
                Ok(Some(Self::from(row)))
            }
            ChipDirection::Debit { floor } => {
                let row = client
                    .query_opt(
                        "WITH updated AS (
                            UPDATE user_chips
                            SET balance = balance - $2, updated = current_timestamp
                            WHERE user_id = $1 AND balance - $2 >= $3
                            RETURNING *
                         ),
                         ledger AS (
                            INSERT INTO chip_ledger
                              (user_id, delta, reason, source_kind, source_ref)
                            SELECT user_id, -$2, $4, $5, $6
                            FROM updated
                         )
                         SELECT * FROM updated",
                        &[
                            &user_id,
                            &amount,
                            &floor,
                            &mv.reason(),
                            &mv.source_kind(),
                            &source_ref,
                        ],
                    )
                    .await?;
                Ok(row.map(Self::from))
            }
            ChipDirection::Restore => {
                bail!("floor restore has a dedicated write path, use restore_floor")
            }
        }
    }

    pub async fn restore_floor(client: &Client, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "WITH prior AS (
                    SELECT balance
                    FROM user_chips
                    WHERE user_id = $1
                    FOR UPDATE
                 ),
                 upserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id) DO UPDATE SET
                      balance = GREATEST(user_chips.balance, $2),
                      updated = current_timestamp
                    RETURNING *
                 ),
                 restored AS (
                    SELECT GREATEST($2 - COALESCE((SELECT balance FROM prior), $2), 0)::bigint AS delta
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind)
                    SELECT $1, delta, $3, $4
                    FROM restored
                    WHERE delta > 0
                 )
                 SELECT upserted.*
                 FROM upserted",
                &[
                    &user_id,
                    &CHIP_FLOOR,
                    &ChipMove::FloorRestore.reason(),
                    &ChipMove::FloorRestore.source_kind(),
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Move chips from sender to recipient: a [`ChipMove::GiftSent`] debit
    /// (floor-guarded) and a [`ChipMove::GiftReceived`] credit. The debit and
    /// credit are separate statements, so this takes the transaction that
    /// makes them atomic. Returns `None` when the sender cannot cover the
    /// gift and keep the floor.
    pub async fn transfer_gift(
        tx: &Transaction<'_>,
        sender_id: Uuid,
        recipient_id: Uuid,
        amount: i64,
    ) -> Result<Option<(Self, Self)>> {
        ensure!(amount > 0, "gift amount must be positive");
        ensure!(sender_id != recipient_id, "cannot gift yourself");

        // Ensure both chip rows exist first, so gifting to a user without a
        // pre-existing row credits on top of the initial balance instead of
        // spuriously failing.
        tx.execute(
            "INSERT INTO user_chips (user_id, balance)
             VALUES ($1, $3), ($2, $3)
             ON CONFLICT (user_id) DO NOTHING",
            &[&sender_id, &recipient_id, &INITIAL_CHIP_BALANCE],
        )
        .await?;

        let Some(sender) = Self::apply(tx, sender_id, ChipMove::GiftSent, amount, None).await?
        else {
            return Ok(None);
        };
        let Some(recipient) =
            Self::apply(tx, recipient_id, ChipMove::GiftReceived, amount, None).await?
        else {
            bail!("gift credit returned no row");
        };
        Ok(Some((sender, recipient)))
    }

    /// All user chip balances (for per-user lookup in leaderboard refresh).
    pub async fn all_balances(client: &Client) -> Result<HashMap<Uuid, i64>> {
        let rows = client
            .query("SELECT user_id, balance FROM user_chips", &[])
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("user_id"), row.get("balance")))
            .collect())
    }
}
