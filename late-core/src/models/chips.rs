use std::collections::HashMap;

use anyhow::{Result, bail, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, GenericClient, Transaction};
use uuid::Uuid;

pub const CHIP_FLOOR: i64 = 100;
pub const INITIAL_CHIP_BALANCE: i64 = 1_000;
pub const CHIP_USER_CHANGED_CHANNEL: &str = "chip_user_changed";
/// SQL for the start of the current UTC month, the window every monthly
/// chip figure shares: the Top Chips board, the award snapshot, and the
/// "earned this month" line on a profile.
pub const MONTH_TS_FILTER: &str =
    "date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'";
/// How many ledger rows a profile shows.
pub const PROFILE_LEDGER_ROWS: i64 = 40;

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
    /// Retired: the shared house-table payout reason (`chip_credit`) that
    /// poker and blackjack wrote before each game had its own. Rows with it
    /// still exist, so the variant stays to keep them off the Top Chips
    /// board; [`UserChips::apply`] refuses to write it.
    LegacyTableCredit,
    /// Retired: the shared house-table wager reason (`chip_debit`). Same
    /// story as [`ChipMove::LegacyTableCredit`].
    LegacyTableDebit,
    /// A blackjack wager, including the double-down top-up. May drain the
    /// balance to zero (a losing settlement restores the floor afterwards).
    /// `source_ref` is the round id, shared by every row of that round.
    BlackjackBet,
    /// A blackjack settlement that paid something: the stake back on a push,
    /// or the stake plus winnings. `source_ref` is the round id.
    BlackjackPayout,
    /// A poker commit: blinds, calls, raises. Floor 0 like blackjack.
    /// `source_ref` is the hand id, shared by every row of that hand.
    PokerBet,
    /// A poker pot, or a share of a split pot, reaching a seat.
    /// `source_ref` is the hand id.
    PokerPayout,
    /// The flat bonus for the first watering of the day. `source_ref` is
    /// the UTC date it was paid for, which is also what makes it daily.
    BonsaiWatered,
    /// Post-settlement top-up back to [`CHIP_FLOOR`]. Has its own write path
    /// ([`UserChips::restore_floor`]), never goes through [`UserChips::apply`].
    /// `source_ref` is the round or hand id whose settlement emptied the
    /// balance, so the top-up sits next to the bet that caused it.
    FloorRestore,
    /// Chips handed to another player with `/gift`. `source_ref` is the
    /// recipient's user id.
    GiftSent,
    /// The other side of a gift. `source_ref` is the sender's user id.
    GiftReceived,
    /// The stipend a brand-new chips row starts with, written once by
    /// [`UserChips::ensure`] in the same statement that creates the row, so
    /// a user's ledger always sums to their balance. `source_ref` is the
    /// user id.
    InitialBalance,
    /// Chips paid to gild someone else's chat message. Floor-guarded like a
    /// gift; `source_ref` is the gilded message id.
    GildSent,
    /// Two thirds of a gild reaching the message's author. The other third
    /// has no ledger row at all: that gap is the burn.
    GildReceived,
    /// Chips paid to take the crown. Floor-guarded, and burned whole: there
    /// is no matching credit anywhere, so every take shrinks the supply by
    /// the full price. `source_ref` is the reign id.
    CrownTaken,
    /// Chips paid for pot tickets. Floor-guarded like a gift; `source_ref` is
    /// the pot id, and one row per buy rather than per ticket.
    PotTicket,
    /// The pot's payout to the one ticket that was drawn: 80% of what the
    /// tickets paid in. The other fifth has no ledger row at all, the way a
    /// gild's third has none: that gap is the burn.
    PotWon,
    /// Publishing a News article, from the News composer or from an RSS
    /// entry shared with `s`. One flat credit, minted rather than moved.
    /// `source_ref` is the shared URL, not an article id: the ledger row is
    /// what caps the reward at one per URL per user, and it has to outlive
    /// the article being deleted (see [`crate::models::article::Article::create_shared`]).
    NewsShared,
    /// The Artboard gallery's monthly prize: last month's top three by
    /// their best piece's applause, paid once when the `artboard` profile
    /// award row is written (`profile_award.rs`). Minted rather than moved;
    /// `source_ref` is the award row id, which is what makes a re-run of
    /// the snapshot unable to pay twice.
    ArtboardPrize,
    /// Queueing a YouTube track, from the booth, a pasted URL, or the history
    /// list. One flat credit, minted rather than moved, for the first few
    /// tracks a person queues each UTC day and nothing after that. The track
    /// itself is never a gate: a repeat pays like anything else.
    /// `source_ref` is the video id as provenance, so a ledger row says what
    /// it paid for; nothing reads it back
    /// (see [`crate::models::media_queue_item::MediaQueueItem::insert_youtube`]).
    SongQueued,
    /// Chips paid to buy the house a round: one price per credit the round
    /// actually granted, floor-guarded, and burned whole like the crown.
    /// Nothing is credited to the patrons, who get a
    /// [`crate::models::drink_round::DrinkCredit`] rather than chips.
    /// `source_ref` is the round id.
    RoundPurchase,
    DrinkPurchase,
    ShopPurchase,
    QuestReward,
    DailyQuestStreakReward,
    DailyPuzzleWin,
    AsterionEscape,
    DailyChessWin,
    DailyChess960Win,
    DailyBattleshipWin,
    DailyConnectFourWin,
    DailyReversiWin,
    DailyCheckersWin,
    DailyBackgammonWin,
    DailyBriscolaWin,
    TronWin,
    /// A Super Snake seat that came out ahead, banked when the player stands
    /// up. The arena keeps the running total in memory: one row per visit,
    /// not one per bite. `source_ref` is the visit id, minted when the seat
    /// is taken.
    SsnakeArenaEarned,
    /// The same, for a seat whose crashes outran its food.
    SsnakeArenaLost,
    GreendragonDragonSlain,
    DarkroomEscape,
    DarkroomBeaconEscape,
    NethackAmuletAcquired,
    NethackAscension,
    DcssOrbFound,
    DcssOrbEscape,
    BrogueEscape,
    BrogueMastery,
    LateaniaArchdemonDefeat,
    LateaniaFrontierKingDefeat,
    LateaniaSunderingDeepDefeat,
    LateaniaKaethyrAscendantDefeat,
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
    /// A reason nothing writes any more; [`UserChips::apply`] refuses it.
    Retired,
}

impl ChipMove {
    /// The persisted `chip_ledger.reason` value.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::LegacyTableCredit => "chip_credit",
            Self::LegacyTableDebit => "chip_debit",
            Self::BlackjackBet => "blackjack_bet",
            Self::BlackjackPayout => "blackjack_payout",
            Self::PokerBet => "poker_bet",
            Self::PokerPayout => "poker_payout",
            Self::BonsaiWatered => "bonsai_watered",
            Self::FloorRestore => "floor_restore",
            Self::GiftSent => "chip_gift_sent",
            Self::GiftReceived => "chip_gift_received",
            Self::InitialBalance => "initial_balance",
            Self::GildSent => "chip_gild_sent",
            Self::GildReceived => "chip_gild_received",
            Self::CrownTaken => "chip_crown_taken",
            Self::PotTicket => "pot_ticket",
            Self::PotWon => "pot_won",
            Self::NewsShared => "news_shared",
            Self::ArtboardPrize => "artboard_prize",
            Self::SongQueued => "song_queued",
            Self::RoundPurchase => "round_purchase",
            Self::DrinkPurchase => "drink_purchase",
            Self::ShopPurchase => "shop_purchase",
            Self::QuestReward => "quest_reward",
            Self::DailyQuestStreakReward => "daily_quest_streak_reward",
            Self::DailyPuzzleWin => "daily_puzzle_win",
            Self::AsterionEscape => "asterion_escape",
            Self::DailyChessWin => "daily_chess_win",
            Self::DailyChess960Win => "daily_chess960_win",
            Self::DailyBattleshipWin => "daily_battleship_win",
            Self::DailyConnectFourWin => "daily_connect4_win",
            Self::DailyReversiWin => "daily_reversi_win",
            Self::DailyCheckersWin => "daily_checkers_win",
            Self::DailyBackgammonWin => "daily_backgammon_win",
            Self::DailyBriscolaWin => "daily_briscola_win",
            Self::TronWin => "tron_win",
            Self::SsnakeArenaEarned => "ssnake_arena_earned",
            Self::SsnakeArenaLost => "ssnake_arena_lost",
            Self::GreendragonDragonSlain => "greendragon_dragon_slain",
            Self::DarkroomEscape => "darkroom_escape",
            Self::DarkroomBeaconEscape => "darkroom_beacon_escape",
            Self::NethackAmuletAcquired => "nethack_amulet_acquired",
            Self::NethackAscension => "nethack_ascension",
            Self::DcssOrbFound => "dcss_orb_found",
            Self::DcssOrbEscape => "dcss_orb_escape",
            Self::BrogueEscape => "brogue_escape",
            Self::BrogueMastery => "brogue_mastery",
            Self::LateaniaArchdemonDefeat => "lateania_archdemon_defeat",
            Self::LateaniaFrontierKingDefeat => "lateania_frontier_king_defeat",
            Self::LateaniaSunderingDeepDefeat => "lateania_sundering_deep_defeat",
            Self::LateaniaKaethyrAscendantDefeat => "lateania_kaethyr_ascendant_defeat",
        }
    }

    /// The persisted `chip_ledger.source_kind` value.
    pub const fn source_kind(self) -> &'static str {
        match self {
            Self::LegacyTableCredit | Self::LegacyTableDebit => "user_chips",
            Self::BlackjackBet | Self::BlackjackPayout => "blackjack_rounds",
            Self::PokerBet | Self::PokerPayout => "poker_hands",
            Self::FloorRestore => "house_rounds",
            Self::GiftSent | Self::GiftReceived | Self::InitialBalance => "users",
            Self::SsnakeArenaEarned | Self::SsnakeArenaLost => "ssnake_visits",
            Self::BonsaiWatered => "bonsai_daily_care",
            Self::GildSent | Self::GildReceived => "chat_messages",
            Self::CrownTaken => "crown_reigns",
            Self::PotTicket | Self::PotWon => "pots",
            Self::NewsShared => "articles",
            Self::ArtboardPrize => "profile_awards",
            Self::SongQueued => "media_queue_items",
            Self::RoundPurchase => "drink_rounds",
            Self::DrinkPurchase => "bartender",
            Self::ShopPurchase => "marketplace_item",
            Self::QuestReward => "quest_assignment",
            Self::DailyQuestStreakReward => "daily_quest_streak",
            Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyChess960Win
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::GreendragonDragonSlain
            | Self::DarkroomEscape
            | Self::DarkroomBeaconEscape
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat
            | Self::LateaniaSunderingDeepDefeat
            | Self::LateaniaKaethyrAscendantDefeat => "game_payout_claims",
        }
    }

    pub const fn direction(self) -> ChipDirection {
        match self {
            Self::LegacyTableCredit | Self::LegacyTableDebit => ChipDirection::Retired,
            Self::BlackjackPayout
            | Self::PokerPayout
            | Self::BonsaiWatered
            | Self::GiftReceived
            | Self::InitialBalance
            | Self::GildReceived
            | Self::PotWon
            | Self::NewsShared
            | Self::ArtboardPrize
            | Self::SongQueued
            | Self::QuestReward
            | Self::DailyQuestStreakReward
            | Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyChess960Win
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::SsnakeArenaEarned
            | Self::GreendragonDragonSlain
            | Self::DarkroomEscape
            | Self::DarkroomBeaconEscape
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat
            | Self::LateaniaSunderingDeepDefeat
            | Self::LateaniaKaethyrAscendantDefeat => ChipDirection::Credit,
            Self::BlackjackBet | Self::PokerBet | Self::ShopPurchase | Self::SsnakeArenaLost => {
                ChipDirection::Debit { floor: 0 }
            }
            Self::GiftSent
            | Self::GildSent
            | Self::CrownTaken
            | Self::PotTicket
            | Self::RoundPurchase
            | Self::DrinkPurchase => ChipDirection::Debit { floor: CHIP_FLOOR },
            Self::FloorRestore => ChipDirection::Restore,
        }
    }

    /// Whether the move counts toward the monthly Top Chips board and the
    /// permanent monthly award snapshot.
    ///
    /// The rule is short: everything counts, on both sides of the ledger,
    /// except the two house tables and gifts. Blackjack and poker are out
    /// because a table can fold every hand to one seat and walk it up the
    /// board. Gifts are out because a group can funnel chips into one
    /// player at no cost to the board. Excluding only one side of either is
    /// never right: with the win in and the stake out a table becomes free
    /// upside, and with the sent side out and the received side in the
    /// funnel is back. The floor restore goes with the tables, since only a
    /// losing settlement mints it.
    ///
    /// Gilds stay in: a gild is paid for a message other people rated, the
    /// way the gallery prize is paid for applause. The starting stipend
    /// stays in because everyone gets the same one, so it moves nobody.
    /// Admin grants never reach the ledger at all
    /// ([`UserChips::admin_grant`]), so the board never sees them.
    pub const fn counts_as_earnings(self) -> bool {
        match self {
            Self::LegacyTableCredit
            | Self::LegacyTableDebit
            | Self::BlackjackBet
            | Self::BlackjackPayout
            | Self::PokerBet
            | Self::PokerPayout
            | Self::FloorRestore
            | Self::GiftSent
            | Self::GiftReceived => false,
            Self::GildSent
            | Self::GildReceived
            | Self::InitialBalance
            | Self::BonsaiWatered
            | Self::CrownTaken
            | Self::PotTicket
            | Self::PotWon
            | Self::NewsShared
            | Self::ArtboardPrize
            | Self::SongQueued
            | Self::RoundPurchase
            | Self::DrinkPurchase
            | Self::ShopPurchase
            | Self::QuestReward
            | Self::DailyQuestStreakReward
            | Self::DailyPuzzleWin
            | Self::AsterionEscape
            | Self::DailyChessWin
            | Self::DailyChess960Win
            | Self::DailyBattleshipWin
            | Self::DailyConnectFourWin
            | Self::DailyReversiWin
            | Self::DailyCheckersWin
            | Self::DailyBackgammonWin
            | Self::DailyBriscolaWin
            | Self::TronWin
            | Self::SsnakeArenaEarned
            | Self::SsnakeArenaLost
            | Self::GreendragonDragonSlain
            | Self::DarkroomEscape
            | Self::DarkroomBeaconEscape
            | Self::NethackAmuletAcquired
            | Self::NethackAscension
            | Self::DcssOrbFound
            | Self::DcssOrbEscape
            | Self::BrogueEscape
            | Self::BrogueMastery
            | Self::LateaniaArchdemonDefeat
            | Self::LateaniaFrontierKingDefeat
            | Self::LateaniaSunderingDeepDefeat
            | Self::LateaniaKaethyrAscendantDefeat => true,
        }
    }

    /// The variant behind a persisted `chip_ledger.reason`, or `None` for a
    /// reason nothing in the roster ever wrote (seed data, hand-written
    /// rows). Derived from `ALL`, so a new variant is parseable the moment
    /// it exists.
    pub fn from_reason(reason: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|mv| mv.reason() == reason)
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

/// One `chip_ledger` row as a profile shows it. `reason` stays the persisted
/// string so a row nothing in the roster wrote still renders, as "other".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipLedgerEntry {
    pub delta: i64,
    pub reason: String,
    pub source_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ChipLedgerEntry {
    pub fn chip_move(&self) -> Option<ChipMove> {
        ChipMove::from_reason(&self.reason)
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

    /// Ensure a chips row exists for the user. Called on SSH login; see
    /// [`Self::ensure_in`] for the transactional twin.
    pub async fn ensure(client: &Client, user_id: Uuid) -> Result<Self> {
        Self::ensure_in(client, user_id).await
    }

    /// [`Self::ensure`] on any client, so the paths that may touch a user
    /// who has never logged in (gifts, gilds, the Shop, an admin grant) can
    /// run it inside their own transaction. The first call writes the
    /// [`ChipMove::InitialBalance`] ledger row in the same statement as the
    /// row itself; later calls only read.
    pub async fn ensure_in(client: &impl GenericClient, user_id: Uuid) -> Result<Self> {
        let row = client
            .query_one(
                "WITH inserted AS (
                    INSERT INTO user_chips (user_id, balance)
                    VALUES ($1, $2)
                    ON CONFLICT (user_id) DO NOTHING
                    RETURNING *
                 ),
                 ledger AS (
                    INSERT INTO chip_ledger
                      (user_id, delta, reason, source_kind, source_ref)
                    SELECT user_id, $2, $3, $4, $5
                    FROM inserted
                 )
                 SELECT * FROM inserted
                 UNION ALL
                 SELECT * FROM user_chips
                 WHERE user_id = $1 AND NOT EXISTS (SELECT 1 FROM inserted)",
                &[
                    &user_id,
                    &INITIAL_CHIP_BALANCE,
                    &ChipMove::InitialBalance.reason(),
                    &ChipMove::InitialBalance.source_kind(),
                    &user_id.to_string(),
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// The single write path for delta chip moves: one guarded balance
    /// update plus its ledger row, in one statement. Credits upsert (a
    /// missing row starts at the credited amount); debits enforce the floor
    /// from [`ChipMove::direction`] and return `None` when the balance
    /// cannot cover the move. The `chip_user_changed` notify comes from the
    /// `user_chips` triggers, never from here.
    ///
    /// `source_ref` is required: every ledger row says what it paid for
    /// (see each [`ChipMove`] variant for what its ref is).
    pub async fn apply(
        client: &impl GenericClient,
        user_id: Uuid,
        mv: ChipMove,
        amount: i64,
        source_ref: &str,
    ) -> Result<Option<Self>> {
        ensure!(amount > 0, "chip move amount must be positive");
        ensure!(
            !source_ref.is_empty(),
            "chip move source_ref must name its source"
        );
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
            ChipDirection::Retired => {
                bail!(
                    "chip move {} is retired and can no longer be written",
                    mv.reason()
                )
            }
        }
    }

    /// An admin handing chips to a player with `/grant`. By decision
    /// (2026-09-06) this is the one balance change with no ledger row: the
    /// ledger records what players did, and a grant is the house's doing.
    /// The row is ensured first so a player who has never logged in lands
    /// on the stipend plus the grant, and the stipend's own row is written
    /// as usual. The `chip_user_changed` notify still fires from the
    /// `user_chips` trigger.
    pub async fn admin_grant(
        client: &impl GenericClient,
        user_id: Uuid,
        amount: i64,
    ) -> Result<Self> {
        ensure!(amount > 0, "admin grant amount must be positive");
        Self::ensure_in(client, user_id).await?;
        let row = client
            .query_one(
                "UPDATE user_chips
                 SET balance = balance + $2, updated = current_timestamp
                 WHERE user_id = $1
                 RETURNING *",
                &[&user_id, &amount],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Top the balance back up to [`CHIP_FLOOR`] after a losing house-table
    /// settlement. `source_ref` is the round or hand id that emptied it.
    pub async fn restore_floor(client: &Client, user_id: Uuid, source_ref: &str) -> Result<Self> {
        ensure!(
            !source_ref.is_empty(),
            "floor restore source_ref must name its round"
        );
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
                    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                    SELECT $1, delta, $3, $4, $5
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
                    &source_ref,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Move chips from sender to recipient: a [`ChipMove::GiftSent`] debit
    /// (floor-guarded) and a [`ChipMove::GiftReceived`] credit, each carrying
    /// the other party's id as `source_ref`. The debit and credit are
    /// separate statements, so this takes the transaction that makes them
    /// atomic. Returns `None` when the sender cannot cover the gift and keep
    /// the floor.
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
        Self::ensure_in(tx, sender_id).await?;
        Self::ensure_in(tx, recipient_id).await?;

        let Some(sender) = Self::apply(
            tx,
            sender_id,
            ChipMove::GiftSent,
            amount,
            &recipient_id.to_string(),
        )
        .await?
        else {
            return Ok(None);
        };
        let Some(recipient) = Self::apply(
            tx,
            recipient_id,
            ChipMove::GiftReceived,
            amount,
            &sender_id.to_string(),
        )
        .await?
        else {
            bail!("gift credit returned no row");
        };
        Ok(Some((sender, recipient)))
    }

    /// A gild's chip movement: the buyer pays `price` under the floor guard,
    /// the message's author is credited `author_share`, and the difference is
    /// simply never minted. Same shape as [`Self::transfer_gift`] (both
    /// statements, so the caller owns the transaction), with the split.
    /// `message_id` is the `source_ref` on both ledger rows, so the pair is
    /// auditable from either side. `None` when the buyer cannot pay and keep
    /// the floor.
    pub async fn transfer_gild(
        tx: &Transaction<'_>,
        sender_id: Uuid,
        author_id: Uuid,
        price: i64,
        author_share: i64,
        message_id: Uuid,
    ) -> Result<Option<(Self, Self)>> {
        ensure!(price > 0, "gild price must be positive");
        ensure!(
            author_share > 0 && author_share < price,
            "gild author share must burn something and pay something"
        );
        ensure!(sender_id != author_id, "cannot gild yourself");

        // Both chip rows must exist first, for the same reason gifting needs
        // it: an author with no row yet would otherwise fail the credit.
        Self::ensure_in(tx, sender_id).await?;
        Self::ensure_in(tx, author_id).await?;

        let source_ref = message_id.to_string();
        let Some(sender) =
            Self::apply(tx, sender_id, ChipMove::GildSent, price, &source_ref).await?
        else {
            return Ok(None);
        };
        let Some(author) = Self::apply(
            tx,
            author_id,
            ChipMove::GildReceived,
            author_share,
            &source_ref,
        )
        .await?
        else {
            bail!("gild credit returned no row");
        };
        Ok(Some((sender, author)))
    }

    /// A user's newest ledger rows, newest first. Owner-scoped in the query.
    pub async fn recent_ledger(
        client: &Client,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ChipLedgerEntry>> {
        let rows = client
            .query(
                "SELECT delta, reason, source_ref, created_at
                 FROM chip_ledger
                 WHERE user_id = $1
                 ORDER BY created_at DESC, id DESC
                 LIMIT $2",
                &[&user_id, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ChipLedgerEntry {
                delta: row.get("delta"),
                reason: row.get("reason"),
                source_ref: row.get("source_ref"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// What a user has earned this UTC month by the Top Chips rule
    /// ([`ChipMove::counts_as_earnings`]): the same sum the board ranks, so
    /// the profile figure and the board never disagree.
    pub async fn earned_this_month(client: &Client, user_id: Uuid) -> Result<i64> {
        let excluded = ChipMove::excluded_earning_reasons();
        let row = client
            .query_one(
                &format!(
                    "SELECT COALESCE(SUM(delta), 0)::bigint AS earned
                     FROM chip_ledger
                     WHERE user_id = $1
                       AND reason <> ALL($2)
                       AND created_at >= {MONTH_TS_FILTER}"
                ),
                &[&user_id, &excluded],
            )
            .await?;
        Ok(row.get("earned"))
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
