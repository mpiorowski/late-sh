//! Gilds: chips paid to mark someone else's chat message, permanently.
//!
//! A gild is a purchase, not a reaction. It never comes back, the buyer may
//! hold at most one of each tier on a message, and two thirds of the price
//! reaches the author while the last third is never re-minted (the gap
//! between [`crate::models::chips::ChipMove::GildSent`] and
//! [`crate::models::chips::ChipMove::GildReceived`] in the ledger).
//!
//! This module owns every read and write of `chat_message_gilds`. The chips
//! themselves move through `chips.rs`; the transaction that does both belongs
//! to the caller.

use std::cmp::Ordering;
use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;

/// Cross-process repaint channel. A gild lands on whichever replica the
/// buyer is connected to; every other replica learns about it here. The
/// payload is `<message id>:<room id>`, so a listener repaints without first
/// having to look the message up.
pub const CHAT_MESSAGE_GILDED_CHANNEL: &str = "chat_message_gilded";

pub async fn listen_for_gild_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {CHAT_MESSAGE_GILDED_CHANNEL};"))
        .await?;
    Ok(())
}

/// The gilded message and the room it is in, as parsed from a
/// [`CHAT_MESSAGE_GILDED_CHANNEL`] payload. `None` for anything that is not
/// the two ids this module writes.
pub fn parse_gilded_payload(payload: &str) -> Option<(Uuid, Uuid)> {
    let (message_id, room_id) = payload.split_once(':')?;
    Some((message_id.parse().ok()?, room_id.parse().ok()?))
}

/// The gild a message must reach before #lounge hears about it, and it says
/// so exactly once. One gild is a nod between two people; three is a room
/// agreeing, which is the only version worth a feed line.
pub const GILD_FEED_THRESHOLD: i64 = 3;

/// The three prices a gild can be bought at. Closed on purpose: the tier is
/// the whole product (a whale spends 100x on the same visible act), and a
/// fourth price would have to answer for its marker, its color, and its split
/// right here rather than in a config row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GildTier {
    Bronze,
    Silver,
    Gold,
}

impl GildTier {
    /// Cheapest first. Declaration order is also `Ord` order, which is what
    /// "the highest tier a message holds" reads off.
    pub const ALL: &'static [Self] = &[Self::Bronze, Self::Silver, Self::Gold];

    /// The persisted `tier` value.
    pub const fn rank(self) -> i16 {
        match self {
            Self::Bronze => 1,
            Self::Silver => 2,
            Self::Gold => 3,
        }
    }

    /// `None` for anything outside the roster, so a row written by an older
    /// or newer schema is a loud failure rather than a silent Bronze.
    pub const fn from_rank(rank: i16) -> Option<Self> {
        match rank {
            1 => Some(Self::Bronze),
            2 => Some(Self::Silver),
            3 => Some(Self::Gold),
            _ => None,
        }
    }

    /// What the buyer pays. Decided in SHOP.md's fixed-numbers table: an
    /// hour, a day, a week of completionist arcade play (~2,000 a day), in
    /// the 4x-5x steps the rest of the Shop ladder uses.
    pub const fn price(self) -> i64 {
        match self {
            Self::Bronze => 500,
            Self::Silver => 2_000,
            Self::Gold => 10_000,
        }
    }

    /// Two thirds of the price, floored, credited to the author.
    pub const fn author_share(self) -> i64 {
        self.price() * 2 / 3
    }

    /// The remaining third. It has no ledger row at all: the burn is the gap
    /// between the debit and the credit.
    pub const fn burn(self) -> i64 {
        self.price() - self.author_share()
    }

    /// Picker copy.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bronze => "Bronze",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
        }
    }

    /// What the message row shows once the gild lands: one glyph per tier
    /// rank, painted in the tier color by the renderer. Glyphs rather than a
    /// word so the marker reads at a glance in any width.
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Bronze => "◆",
            Self::Silver => "◆◆",
            Self::Gold => "◆◆◆",
        }
    }
}

/// Where a buy landed on the buyer's slot. Closed so the service lists every
/// outcome, and the two refusals are data rather than a `None` to guess at.
#[derive(Debug)]
pub enum GildPlacement {
    /// First gild from this buyer on this message.
    Placed(ChatMessageGild),
    /// The buyer's gild raised from `from` to the row's new tier.
    Upgraded {
        from: GildTier,
        gild: ChatMessageGild,
    },
    /// The buyer already holds exactly this tier.
    SameTier,
    /// The buyer holds this higher tier; a gild never goes down.
    HeldHigher(GildTier),
}

/// One gild row: a buyer's single slot on a message. The only write after
/// the insert is a tier raise (see [`ChatMessageGild::place_in_tx`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessageGild {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub message_id: Uuid,
    pub author_user_id: Uuid,
    pub user_id: Uuid,
    pub tier: GildTier,
    pub chips: i64,
}

impl TryFrom<Row> for ChatMessageGild {
    type Error = anyhow::Error;

    fn try_from(row: Row) -> Result<Self> {
        let rank: i16 = row.get("tier");
        let Some(tier) = GildTier::from_rank(rank) else {
            anyhow::bail!("unknown gild tier {rank}");
        };
        Ok(Self {
            id: row.get("id"),
            created: row.get("created"),
            message_id: row.get("message_id"),
            author_user_id: row.get("author_user_id"),
            user_id: row.get("user_id"),
            tier,
            chips: row.get("chips"),
        })
    }
}

/// What a message row paints: the best tier anyone bought on it and how many
/// gilds it holds in total. Every viewer sees the same summary; who paid is
/// not part of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatMessageGildSummary {
    pub top_tier: GildTier,
    pub count: i64,
}

/// Gilds received, per tier, for one author. Fixed shape rather than a map,
/// so the profile renders three rows without deciding what a missing key
/// means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GildCounts {
    pub bronze: i64,
    pub silver: i64,
    pub gold: i64,
}

impl GildCounts {
    pub const fn get(self, tier: GildTier) -> i64 {
        match tier {
            GildTier::Bronze => self.bronze,
            GildTier::Silver => self.silver,
            GildTier::Gold => self.gold,
        }
    }

    pub const fn total(self) -> i64 {
        self.bronze + self.silver + self.gold
    }

    pub const fn is_empty(self) -> bool {
        self.total() == 0
    }
}

impl ChatMessageGild {
    /// Take the row lock on the gilded message. Every gild on one message
    /// serializes behind this, which is what makes both the placement's
    /// read-then-write and the "third buyer" count exact under concurrency. `None` means
    /// the message is gone (hard-deleted while the buyer was choosing a
    /// tier).
    pub async fn lock_message_author(
        tx: &Transaction<'_>,
        message_id: Uuid,
    ) -> Result<Option<Uuid>> {
        let row = tx
            .query_opt(
                "SELECT user_id FROM chat_messages WHERE id = $1 FOR UPDATE",
                &[&message_id],
            )
            .await?;
        Ok(row.map(|row| row.get("user_id")))
    }

    /// Land a buy on the buyer's single slot on this message. A gild only
    /// ever goes up: a first buy inserts, a higher tier raises the row in
    /// place (the buyer pays the new tier's full price; the ledger keeps
    /// both payments), and the same or a lower tier is a refusal, not an
    /// error. Call under the lock from [`Self::lock_message_author`], which
    /// is what makes the read-then-write here exact.
    pub async fn place_in_tx(
        tx: &Transaction<'_>,
        message_id: Uuid,
        author_user_id: Uuid,
        user_id: Uuid,
        tier: GildTier,
    ) -> Result<GildPlacement> {
        let held = tx
            .query_opt(
                "SELECT * FROM chat_message_gilds
                 WHERE message_id = $1 AND user_id = $2",
                &[&message_id, &user_id],
            )
            .await?
            .map(Self::try_from)
            .transpose()?;
        let Some(held) = held else {
            let row = tx
                .query_one(
                    "INSERT INTO chat_message_gilds
                       (message_id, author_user_id, user_id, tier, chips)
                     VALUES ($1, $2, $3, $4, $5)
                     RETURNING *",
                    &[
                        &message_id,
                        &author_user_id,
                        &user_id,
                        &tier.rank(),
                        &tier.price(),
                    ],
                )
                .await?;
            return Ok(GildPlacement::Placed(Self::try_from(row)?));
        };
        match held.tier.cmp(&tier) {
            Ordering::Equal => Ok(GildPlacement::SameTier),
            Ordering::Greater => Ok(GildPlacement::HeldHigher(held.tier)),
            Ordering::Less => {
                let row = tx
                    .query_one(
                        "UPDATE chat_message_gilds
                         SET tier = $2, chips = $3
                         WHERE id = $1
                         RETURNING *",
                        &[&held.id, &tier.rank(), &tier.price()],
                    )
                    .await?;
                Ok(GildPlacement::Upgraded {
                    from: held.tier,
                    gild: Self::try_from(row)?,
                })
            }
        }
    }

    /// Tell every replica to repaint this message. Sent inside the gild
    /// transaction, so Postgres delivers it on commit and never for a
    /// rolled-back gild.
    pub async fn notify_gilded(
        tx: &Transaction<'_>,
        message_id: Uuid,
        room_id: Uuid,
    ) -> Result<()> {
        let payload = format!("{message_id}:{room_id}");
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&CHAT_MESSAGE_GILDED_CHANNEL, &payload],
        )
        .await?;
        Ok(())
    }

    /// How many buyers have gilded the message (one row each). Read under
    /// the same lock as the placement, so the count that decides the #lounge
    /// line is exact.
    pub async fn count_for_message(tx: &Transaction<'_>, message_id: Uuid) -> Result<i64> {
        let row = tx
            .query_one(
                "SELECT COUNT(*)::bigint AS count
                 FROM chat_message_gilds
                 WHERE message_id = $1",
                &[&message_id],
            )
            .await?;
        Ok(row.get("count"))
    }

    /// Every gild on one message, best tier first and earliest buyer first
    /// within a tier: the "who gilded this" list behind `ff`.
    pub async fn list_for_message(
        client: &impl GenericClient,
        message_id: Uuid,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM chat_message_gilds
                 WHERE message_id = $1
                 ORDER BY tier DESC, created, id",
                &[&message_id],
            )
            .await?;
        rows.into_iter().map(Self::try_from).collect()
    }

    /// Markers for one page of messages: one query, never one per row (the
    /// shape of `ChatMessageReaction::list_summaries_for_messages`). Messages
    /// with no gilds are simply absent.
    pub async fn list_summaries_for_messages(
        client: &impl GenericClient,
        message_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, ChatMessageGildSummary>> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT message_id,
                        MAX(tier)::smallint AS top_tier,
                        COUNT(*)::bigint AS count
                 FROM chat_message_gilds
                 WHERE message_id = ANY($1)
                 GROUP BY message_id",
                &[&message_ids],
            )
            .await?;

        let mut summaries = HashMap::with_capacity(rows.len());
        for row in rows {
            let rank: i16 = row.get("top_tier");
            let Some(top_tier) = GildTier::from_rank(rank) else {
                anyhow::bail!("unknown gild tier {rank}");
            };
            summaries.insert(
                row.get("message_id"),
                ChatMessageGildSummary {
                    top_tier,
                    count: row.get("count"),
                },
            );
        }
        Ok(summaries)
    }

    /// The marker for one message, after it just changed.
    pub async fn summary_for_message(
        client: &impl GenericClient,
        message_id: Uuid,
    ) -> Result<Option<ChatMessageGildSummary>> {
        Ok(Self::list_summaries_for_messages(client, &[message_id])
            .await?
            .remove(&message_id))
    }

    /// Gilds this author has received, per tier. Owner-scoped in the query.
    pub async fn counts_for_author(
        client: &impl GenericClient,
        author_user_id: Uuid,
    ) -> Result<GildCounts> {
        let rows = client
            .query(
                "SELECT tier, COUNT(*)::bigint AS count
                 FROM chat_message_gilds
                 WHERE author_user_id = $1
                 GROUP BY tier",
                &[&author_user_id],
            )
            .await?;

        let mut counts = GildCounts::default();
        for row in rows {
            let rank: i16 = row.get("tier");
            let count: i64 = row.get("count");
            match GildTier::from_rank(rank) {
                Some(GildTier::Bronze) => counts.bronze = count,
                Some(GildTier::Silver) => counts.silver = count,
                Some(GildTier::Gold) => counts.gold = count,
                None => anyhow::bail!("unknown gild tier {rank}"),
            }
        }
        Ok(counts)
    }
}
