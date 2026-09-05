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
    /// House-table payout: a poker pot or a blackjack settlement. Wagered
    /// money, not earned money: it stays off the Top Chips board with
    /// [`ChipMove::Bet`], since netting the two still lets a poker table
    /// fold every hand to one seat and walk that seat up the board.
    Credit,
    /// House-table wager: poker and blackjack bets, may drain the balance to
    /// zero (losing settlements restore the floor afterwards).
    Bet,
    /// The flat bonus for watering your bonsai. A daily-habit drip rather
    /// than something achieved, so it stays off the Top Chips board; it has
    /// its own reason so the ledger never has to guess whether a generic
    /// credit was a poker pot or a watering can.
    BonsaiWatered,
    /// Post-settlement top-up back to [`CHIP_FLOOR`]. Has its own write path
    /// ([`UserChips::restore_floor`]), never goes through [`UserChips::apply`].
    FloorRestore,
    GiftSent,
    GiftReceived,
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
    /// not one per bite.
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
}

impl ChipMove {
    /// The persisted `chip_ledger.reason` value.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Credit => "chip_credit",
            Self::Bet => "chip_debit",
            Self::BonsaiWatered => "bonsai_watered",
            Self::FloorRestore => "floor_restore",
            Self::GiftSent => "chip_gift_sent",
            Self::GiftReceived => "chip_gift_received",
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
            Self::Credit
            | Self::Bet
            | Self::FloorRestore
            | Self::GiftSent
            | Self::GiftReceived
            | Self::SsnakeArenaEarned
            | Self::SsnakeArenaLost => "user_chips",
            Self::BonsaiWatered => "bonsai_trees",
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
            Self::Credit
            | Self::BonsaiWatered
            | Self::GiftReceived
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
            Self::Bet | Self::ShopPurchase | Self::SsnakeArenaLost => {
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
    /// One rule decides it: a move counts only if the house minted it for
    /// something the player did. Wagers (the tables), transfers between
    /// players (gifts, gilds), and spending (drinks, rounds, the crown, the
    /// pot, the Shop) are out on both sides of the ledger. Excluding only
    /// one side is never right: with the win in and the stake out, a table
    /// or a lottery becomes free upside; with the stake in and the win out,
    /// playing is a pure negative on a board the player cannot climb; with
    /// the sent side out and the received side in, a group can funnel chips
    /// into one player at no cost. So a transfer is out entirely, and a
    /// spend that has no credit side is out because buying a beer must not
    /// cost anyone their place.
    ///
    /// The bonsai water bonus is minted but not for an achievement: it is a
    /// daily-habit drip, so it sits out too.
    /// Super Snake is the one paired earned/lost move that stays in: the
    /// house mints every food, nothing moves between seats, and the payout
    /// scaling already caps the grind.
    /// The gallery prize counts, unlike the pot: it is paid for work other
    /// people chose to applaud, the way a door milestone is paid for a run,
    /// not drawn from a hat.
    pub const fn counts_as_earnings(self) -> bool {
        match self {
            Self::Credit
            | Self::Bet
            | Self::BonsaiWatered
            | Self::FloorRestore
            | Self::GiftSent
            | Self::GiftReceived
            | Self::GildSent
            | Self::GildReceived
            | Self::CrownTaken
            | Self::PotTicket
            | Self::PotWon
            | Self::RoundPurchase
            | Self::DrinkPurchase
            | Self::ShopPurchase => false,
            Self::NewsShared
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
        tx.execute(
            "INSERT INTO user_chips (user_id, balance)
             VALUES ($1, $3), ($2, $3)
             ON CONFLICT (user_id) DO NOTHING",
            &[&sender_id, &author_id, &INITIAL_CHIP_BALANCE],
        )
        .await?;

        let source_ref = message_id.to_string();
        let Some(sender) =
            Self::apply(tx, sender_id, ChipMove::GildSent, price, Some(&source_ref)).await?
        else {
            return Ok(None);
        };
        let Some(author) = Self::apply(
            tx,
            author_id,
            ChipMove::GildReceived,
            author_share,
            Some(&source_ref),
        )
        .await?
        else {
            bail!("gild credit returned no row");
        };
        Ok(Some((sender, author)))
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
