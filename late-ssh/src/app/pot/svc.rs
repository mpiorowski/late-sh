//! The pot: orchestration for the weekly raffle.
//!
//! Two commands (`/pot`, `/pot buy N`), one sweeper, one story a week. This
//! module owns every refusal, every log line, the #lounge lines, and the
//! process-shared snapshot the status HUD badge reads;
//! `late_core::models::pot` owns the tables and the money math underneath it.
//!
//! Distribution is the crown's shape: a `watch` for the state every session
//! reads once a second, a `broadcast` for the answers that belong to one
//! session each, and a `pot_changed` Postgres notify so a buy or a draw on
//! one replica reaches every other one. The draw itself is guarded by a
//! status transition (`UPDATE ... WHERE status = 'open' RETURNING *`), so
//! exactly one replica pays however many are sweeping.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::{
        chips::{ChipMove, UserChips},
        pot::{
            POT_CHANGED_CHANNEL, POT_MAX_TICKETS_PER_DAY, POT_TICKET_PRICE, Pot, PotChange,
            PotDraw, PotTicket, PotTicketHolder, draw_from_seed, listen_for_pot_changes,
            next_draw_at,
        },
    },
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::{
    app::activity::publisher::ActivityPublisher, app::common::primitives::thousands, metrics,
};

use super::state::short_duration;

/// Command answers are per-session and short-lived; a session that falls this
/// far behind has bigger problems than a stale pot banner.
const POT_EVENT_CAP: usize = 32;

/// How often the sweeper wakes: to open the first pot, to draw a due one, and
/// to re-read the snapshot as a backstop for a missed notify.
const POT_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// One player's place in the pot, as the snapshot hands it to that player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PotHolding {
    /// The whole holding in this pot.
    pub tickets: i64,
    /// The part bought today (UTC), against the daily cap.
    pub bought_today: i64,
}

impl PotHolding {
    /// How many more tickets today's cap allows.
    pub fn room_today(self) -> i64 {
        (POT_MAX_TICKETS_PER_DAY - self.bought_today).max(0)
    }
}

/// The pot as every session reads it. Public numbers only, plus a private
/// index of who holds what: [`Self::holding_for`] is the only way out of it,
/// so a session can look up its own holding and nothing wider.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PotSnapshot {
    pub pot_id: Option<Uuid>,
    pub ticket_price: i64,
    pub ticket_count: i64,
    /// `None` until the first refresh finds an open pot.
    pub draws_at: Option<DateTime<Utc>>,
    holders: HashMap<Uuid, PotHolding>,
}

impl PotSnapshot {
    fn from_pot(pot: &Pot, holders: &[PotTicketHolder]) -> Self {
        Self {
            pot_id: Some(pot.id),
            ticket_price: pot.ticket_price,
            ticket_count: holders.iter().map(|holder| holder.tickets).sum(),
            draws_at: Some(pot.draws_at),
            holders: holders
                .iter()
                .map(|holder| {
                    (
                        holder.user_id,
                        PotHolding {
                            tickets: holder.tickets,
                            bought_today: holder.bought_today,
                        },
                    )
                })
                .collect(),
        }
    }

    /// What the tickets have paid in. Derived, never stored: the tickets are
    /// the only running total the pot has.
    pub fn size(&self) -> i64 {
        self.ticket_count.saturating_mul(self.ticket_price)
    }

    /// One player's holding. Callers pass their own id; nothing here hands
    /// out the map. A player with no tickets has the default holding: none
    /// held, none bought today, the whole cap left.
    pub fn holding_for(&self, user_id: Uuid) -> PotHolding {
        self.holders.get(&user_id).copied().unwrap_or_default()
    }
}

/// Why a buy did not happen. Every arm is a rule the caller can act on, and
/// every arm costs nothing: a refused buy never touches the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PotRefusal {
    /// The pot drew between the command and the transaction. Vanishingly
    /// rare (one 60-second sweep a week), and the next pot is already open.
    Closed,
    /// This buy would take the player past today's cap.
    CapReached { bought_today: i64 },
    /// The price would take the buyer below the chip floor.
    InsufficientChips { price: i64 },
}

impl PotRefusal {
    /// Sentence-case banner copy, the one place a refusal is worded.
    pub fn message(self) -> String {
        match self {
            Self::Closed => "The pot just drew, the next one is open".to_string(),
            Self::CapReached { bought_today } => format!(
                "You bought {bought_today} of the {POT_MAX_TICKETS_PER_DAY} tickets one player may buy a day"
            ),
            Self::InsufficientChips { price } => {
                format!("That many tickets costs {} chips", thousands(price))
            }
        }
    }
}

/// A buy that did not pay: a rule said no, or the database did. The two are
/// separated because only one of them is the caller's business.
#[derive(Debug)]
pub enum PotError {
    Refused(PotRefusal),
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for PotError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

/// What `/pot` answers with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotStatus {
    pub size: i64,
    pub ticket_count: i64,
    pub my_tickets: i64,
    /// What the viewer may still buy today under the daily cap.
    pub room_today: i64,
    pub ticket_price: i64,
    /// Seconds to the draw, or `None` when no pot is open yet (a fresh
    /// database between the first boot and the first sweep).
    pub draws_in_secs: Option<i64>,
}

impl PotStatus {
    /// The single banner line `/pot` prints. One line rather than an
    /// overlay: the pot is four facts, and the command reads like `/crown`.
    pub fn line(&self) -> String {
        let Some(secs) = self.draws_in_secs else {
            return "The pot has not opened yet.".to_string();
        };
        // The holding and what it cost, then today's room under the cap: the
        // one thing a player cannot work out from the HUD badge.
        let held = match self.my_tickets {
            0 => "you hold none".to_string(),
            held => format!(
                "you hold {} ({} chips)",
                thousands(held),
                thousands(held.saturating_mul(self.ticket_price))
            ),
        };
        let today = match self.room_today {
            0 => "none more today".to_string(),
            room => format!("{room} more today"),
        };
        format!(
            "Pot {} on {} tickets, {held}, {today}, draws in {}. /pot buy N at {} each.",
            thousands(self.size),
            thousands(self.ticket_count),
            short_duration(secs),
            thousands(self.ticket_price)
        )
    }
}

/// A settled buy: what the buyer is told.
#[derive(Clone, Copy, Debug)]
pub struct PotBuyOutcome {
    pub pot_id: Uuid,
    /// Tickets bought by this buy, not the holding.
    pub tickets: i64,
    pub held: i64,
    pub price: i64,
    pub balance: i64,
    /// The pot's size after the buy, for the buyer's banner.
    pub size: i64,
}

/// How a due pot settled. The sweeper announces off this; nothing else reads
/// it.
#[derive(Clone, Copy, Debug)]
pub enum PotSettlement {
    Drawn {
        pot_id: Uuid,
        draw: PotDraw,
    },
    /// Nobody bought in. No payout, no #lounge line: an empty pot rolling is
    /// not a story.
    Rolled {
        pot_id: Uuid,
    },
}

/// The answer to one session's command, or the news that this session won.
/// Broadcast to every session in this process; each one picks out what is
/// addressed to it.
#[derive(Clone, Debug)]
pub enum PotEvent {
    /// `/pot buy N` settled: the buyer's receipt. Sent by the replica that
    /// ran the buy, which is the one the buyer is connected to.
    Bought {
        user_id: Uuid,
        tickets: i64,
        held: i64,
        price: i64,
        balance: i64,
    },
    /// This session's user won the pot. Raised from the `pot_changed`
    /// notify rather than from the draw, so it reaches the winner whichever
    /// replica they are on, and never on the one that happened to sweep.
    Won {
        user_id: Uuid,
        payout: i64,
        winner_tickets: i64,
        total_tickets: i64,
    },
    /// `/pot buy N` was refused or failed. Nothing was charged either way.
    Failed { user_id: Uuid, message: String },
}

#[derive(Clone)]
pub struct PotService {
    db: Db,
    /// The #lounge feed publisher. `None` in tests and in any process that
    /// runs without the activity broadcast; the draw still settles, it just
    /// tells nobody.
    activity: Option<ActivityPublisher>,
    snapshot_tx: watch::Sender<Arc<PotSnapshot>>,
    evt_tx: broadcast::Sender<PotEvent>,
}

impl PotService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, _) = watch::channel(Arc::new(PotSnapshot::default()));
        let (evt_tx, _) = broadcast::channel(POT_EVENT_CAP);
        Self {
            db,
            activity: None,
            snapshot_tx,
            evt_tx,
        }
    }

    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }

    /// The process-shared snapshot, read once a second by every session's
    /// tick so no render ever queries for the pot.
    pub fn subscribe_snapshot(&self) -> watch::Receiver<Arc<PotSnapshot>> {
        let mut rx = self.snapshot_tx.subscribe();
        // `subscribe` marks the current value as seen, which would leave a
        // session connecting mid-pot with an empty HUD badge until the next buy.
        rx.mark_changed();
        rx
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<PotEvent> {
        self.evt_tx.subscribe()
    }

    /// Re-read the open pot and publish it. The startup seed, the notify
    /// handler, and the sweeper are the same call, so a replica that
    /// reconnects its listener cannot be left showing a stale size.
    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let snapshot = match Pot::find_open(&**client).await? {
            None => PotSnapshot::default(),
            Some(pot) => {
                let holders = PotTicket::holders(&**client, pot.id).await?;
                PotSnapshot::from_pot(&pot, &holders)
            }
        };
        self.snapshot_tx.send_replace(Arc::new(snapshot));
        Ok(())
    }

    /// The pot's one background loop: settle a due pot (or open the first
    /// one), then republish the snapshot. Every replica runs it; the status
    /// transition in [`Self::settle_due`] is what makes exactly one of them
    /// pay.
    pub fn start_sweeper_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                service.sweep().await;
                tokio::time::sleep(POT_SWEEP_INTERVAL).await;
            }
        })
    }

    /// One sweep. This is the orchestration layer for the draw: every
    /// failure is logged here, and nothing below it logs.
    async fn sweep(&self) {
        match self.settle_due().await {
            Ok(None) => {}
            Ok(Some(settlement)) => self.announce_settlement(settlement),
            Err(error) => {
                late_core::error_span!(
                    "pot_draw_failed",
                    error = ?error,
                    "failed to settle the pot"
                );
            }
        }
        if let Err(error) = self.refresh().await {
            tracing::warn!(error = ?error, "failed to refresh the pot");
        }
    }

    /// Keep every replica's pot in step. One long-lived Postgres connection
    /// LISTENs on [`POT_CHANGED_CHANNEL`]; a dropped connection reconnects
    /// after five seconds and re-seeds, so a buy committed during the gap is
    /// not lost. Same shape as `CrownService::start_listener_task`.
    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = service.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "pot postgres listener stopped");
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }

    async fn listen_once(&self, db_config: &DbConfig) -> Result<()> {
        let mut config = tokio_postgres::Config::new();
        config.host(&db_config.host);
        config.port(db_config.port);
        config.user(&db_config.user);
        config.password(&db_config.password);
        config.dbname(&db_config.dbname);

        let (client, mut connection) = config.connect(tokio_postgres::NoTls).await?;
        let listen = listen_for_pot_changes(&client);
        tokio::pin!(listen);
        loop {
            tokio::select! {
                result = &mut listen => {
                    result?;
                    break;
                }
                message = std::future::poll_fn(|cx| connection.poll_message(cx)) => {
                    let Some(message) = message else {
                        return Ok(());
                    };
                    self.handle_notification(message?).await;
                }
            }
        }

        // Seeded after the LISTEN is live, so a buy committed between the two
        // is caught by this read rather than dropped.
        self.refresh().await?;

        loop {
            let Some(message) = std::future::poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };
            self.handle_notification(message?).await;
        }
    }

    async fn handle_notification(&self, message: tokio_postgres::AsyncMessage) {
        let tokio_postgres::AsyncMessage::Notification(notification) = message else {
            return;
        };
        if notification.channel() != POT_CHANGED_CHANNEL {
            return;
        }
        self.apply_change(notification.payload()).await;
    }

    /// One `pot_changed` notify, on every replica including the one that
    /// sent it: re-read the pot for the badge, then tell the winner if they
    /// are connected here.
    ///
    /// A failed re-read is this replica's badge lagging until the next
    /// notify, not a reason to drop the LISTEN connection: propagating it
    /// would lose every buy committed during the reconnect window. A payload
    /// that does not parse is logged for the same reason; the re-read does
    /// not depend on it.
    pub(super) async fn apply_change(&self, payload: &str) {
        if let Err(error) = self.refresh().await {
            tracing::warn!(error = ?error, "failed to refresh the pot");
        }
        let change = match PotChange::parse(payload) {
            Ok(change) => change,
            Err(error) => {
                tracing::warn!(error = ?error, payload, "unreadable pot_changed payload");
                return;
            }
        };
        match change {
            PotChange::Bought | PotChange::Rolled => {}
            PotChange::Drawn {
                winner_user_id,
                payout_chips,
                winner_tickets,
                total_tickets,
            } => {
                let _ = self.evt_tx.send(PotEvent::Won {
                    user_id: winner_user_id,
                    payout: payout_chips,
                    winner_tickets,
                    total_tickets,
                });
            }
        }
    }

    /// Answer `/pot` for one session, from the shared snapshot: the badge
    /// already has every number, so the command costs no query at all.
    pub fn status_for(&self, user_id: Uuid) -> PotStatus {
        let snapshot = self.snapshot_tx.borrow().clone();
        let holding = snapshot.holding_for(user_id);
        PotStatus {
            size: snapshot.size(),
            ticket_count: snapshot.ticket_count,
            my_tickets: holding.tickets,
            room_today: holding.room_today(),
            ticket_price: snapshot.ticket_price,
            draws_in_secs: snapshot
                .draws_at
                .map(|draws_at| (draws_at - Utc::now()).num_seconds()),
        }
    }

    /// Buy tickets. Fire-and-forget because the caller is at a composer, not
    /// in a request/response: the line clears on Enter and the banner arrives
    /// with the answer.
    ///
    /// This is the orchestration layer: every refusal, every failure and the
    /// metrics are decided here, and [`Self::buy`] below does nothing but
    /// the transaction.
    pub fn buy_task(&self, user_id: Uuid, count: i64) {
        let service = self.clone();
        let span = info_span!("pot.buy_task", user_id = %user_id, count);
        tokio::spawn(
            async move {
                match service.buy(user_id, count).await {
                    Ok(outcome) => {
                        metrics::record_pot_tickets_bought(outcome.tickets, outcome.price);
                        let _ = service.evt_tx.send(PotEvent::Bought {
                            user_id,
                            tickets: outcome.tickets,
                            held: outcome.held,
                            price: outcome.price,
                            balance: outcome.balance,
                        });
                    }
                    Err(PotError::Refused(refusal)) => {
                        metrics::record_pot_buy_refused(refusal);
                        let _ = service.evt_tx.send(PotEvent::Failed {
                            user_id,
                            message: refusal.message(),
                        });
                    }
                    Err(PotError::Failed(error)) => {
                        late_core::error_span!(
                            "pot_buy_failed",
                            error = ?error,
                            "failed to buy pot tickets"
                        );
                        let _ = service.evt_tx.send(PotEvent::Failed {
                            user_id,
                            message: "Buying tickets failed, nothing was charged".to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    /// The buy transaction: lock the open pot, insert under the cap, debit,
    /// notify. Every early return drops the transaction, which rolls it back,
    /// so a refusal here is uncharged too.
    ///
    /// The row lock is what makes the daily cap exact: two buys by the same
    /// player at the same instant serialize on it instead of both reading the
    /// same sum, and a buy can never land in a pot the sweeper is drawing.
    pub(super) async fn buy(&self, user_id: Uuid, count: i64) -> Result<PotBuyOutcome, PotError> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await.map_err(anyhow::Error::from)?;
        let Some(pot) = Pot::lock_open_for_buy(&tx).await? else {
            return Err(PotError::Refused(PotRefusal::Closed));
        };
        let Some(held) =
            PotTicket::buy_in_tx(&tx, pot.id, user_id, count, POT_MAX_TICKETS_PER_DAY).await?
        else {
            let bought_today = PotTicket::user_total_today(&*tx, pot.id, user_id).await?;
            return Err(PotError::Refused(PotRefusal::CapReached { bought_today }));
        };
        let price = count.saturating_mul(pot.ticket_price);
        let Some(chips) = UserChips::apply(
            &*tx,
            user_id,
            ChipMove::PotTicket,
            price,
            Some(&pot.id.to_string()),
        )
        .await?
        else {
            return Err(PotError::Refused(PotRefusal::InsufficientChips { price }));
        };
        // The badge crosses processes over Postgres, not over this process's
        // broadcast, so every replica learns about the buy the same way.
        Pot::notify_changed(&tx, &PotChange::Bought).await?;
        let holders = PotTicket::holders(&*tx, pot.id).await?;
        let ticket_count: i64 = holders.iter().map(|holder| holder.tickets).sum();
        tx.commit().await.map_err(anyhow::Error::from)?;

        Ok(PotBuyOutcome {
            pot_id: pot.id,
            tickets: count,
            held,
            price,
            balance: chips.balance,
            size: ticket_count.saturating_mul(pot.ticket_price),
        })
    }

    /// Draw the open pot if its hour has come, and make sure a pot is open
    /// either way. One transaction, serialized against every other replica's
    /// by the advisory lock in `Pot::lock_open`, and settled by a status
    /// transition so only one of them can pay.
    pub(super) async fn settle_due(&self) -> Result<Option<PotSettlement>> {
        let now = Utc::now();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(pot) = Pot::lock_open(&tx).await? else {
            // First boot, or a database whose last pot was settled without a
            // successor. Opening one is the whole job of this pass.
            Pot::open_in_tx(&tx, next_draw_at(now), POT_TICKET_PRICE).await?;
            tx.commit().await?;
            return Ok(None);
        };
        if pot.draws_at > now {
            return Ok(None);
        }

        let holders = PotTicket::holders(&*tx, pot.id).await?;
        let settlement = match draw_from_seed(&holders, pot.ticket_price, draw_seed()) {
            // Nobody bought in: roll it, open the next, tell nobody.
            None => {
                if Pot::settle_rolled_in_tx(&tx, pot.id).await?.is_none() {
                    return Ok(None);
                }
                Pot::notify_changed(&tx, &PotChange::Rolled).await?;
                PotSettlement::Rolled { pot_id: pot.id }
            }
            Some(draw) => {
                if Pot::settle_drawn_in_tx(&tx, pot.id, &draw).await?.is_none() {
                    // Another replica got the row: it pays and announces, and
                    // this transaction rolls back with nothing in it.
                    return Ok(None);
                }
                UserChips::apply(
                    &*tx,
                    draw.winner_user_id,
                    ChipMove::PotWon,
                    draw.payout_chips,
                    Some(&pot.id.to_string()),
                )
                .await?;
                Pot::notify_changed(
                    &tx,
                    &PotChange::Drawn {
                        winner_user_id: draw.winner_user_id,
                        payout_chips: draw.payout_chips,
                        winner_tickets: draw.winner_tickets,
                        total_tickets: draw.total_tickets,
                    },
                )
                .await?;
                PotSettlement::Drawn {
                    pot_id: pot.id,
                    draw,
                }
            }
        };
        // Always exactly one open pot: the successor opens in the same
        // transaction that closed its predecessor, so `/pot` never has to
        // answer "there isn't one".
        Pot::open_in_tx(&tx, next_draw_at(now), POT_TICKET_PRICE).await?;
        tx.commit().await?;
        Ok(Some(settlement))
    }

    /// Everything a settled draw has to tell. The winner's own banner is not
    /// here: it rides the `pot_changed` notify (see [`Self::apply_change`]),
    /// including in this process, so it reaches them on whichever replica
    /// they are connected to.
    fn announce_settlement(&self, settlement: PotSettlement) {
        let PotSettlement::Drawn { pot_id, draw } = settlement else {
            // A pot nobody entered is not a story, and there is nobody to pay.
            return;
        };
        metrics::record_pot_drawn(draw.payout_chips, draw.total_tickets);
        if let Some(activity) = &self.activity {
            activity.pot_drawn_task(
                draw.winner_user_id,
                pot_id,
                draw.payout_chips,
                draw.winner_tickets,
                draw.total_tickets,
            );
        }
    }
}

/// The draw's seed. The wall clock in nanoseconds, mixed once inside
/// `draw_from_seed`, so the draw is reproducible from a seed in tests and
/// unpredictable in production.
fn draw_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}
