//! The crown: orchestration for the one slot everyone can see.
//!
//! Two commands (`/crown`, `/crown take`), one transaction, one glyph. This
//! module owns every refusal, every log line, the #lounge story, and the
//! process-shared holder that renderers read; `late_core::models::crown`
//! owns the table and the price ladder underneath it.
//!
//! Distribution is the `StreamService` shape: a `watch` for the state every
//! session reads once a second (the holder), and a `broadcast` for the
//! answers to commands, which belong to one session each. A take lands on
//! whichever replica the buyer is connected to and reaches every other one
//! over the `crown_changed` Postgres notify, so there is exactly one code
//! path that moves the glyph.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::{
        chips::{ChipMove, UserChips},
        crown::{
            CROWN_CHANGED_CHANNEL, CrownChange, CrownReign, crown_month, listen_for_crown_changes,
            next_price,
        },
        profile::fetch_username,
        user::User,
    },
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::{
    app::activity::publisher::ActivityPublisher, app::common::primitives::thousands, metrics,
};

/// Command answers are per-session and short-lived; a session that falls this
/// far behind has bigger problems than a stale crown banner.
const CROWN_EVENT_CAP: usize = 32;

/// Who wears the glyph, and for which UTC month. The month travels with the
/// holder because expiry is read-time: a reign left open across the rollover
/// stops counting the moment the month does, with no sweeper and no notify
/// to wake anyone up (see [`CrownHolder::if_current`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrownHolder {
    pub user_id: Uuid,
    pub month: NaiveDate,
}

impl CrownHolder {
    /// The user wearing the glyph right now, `None` once the month has
    /// rolled over.
    pub fn if_current(self, now: DateTime<Utc>) -> Option<Uuid> {
        (self.month == crown_month(now)).then_some(self.user_id)
    }
}

/// Why a take did not happen. Every arm is a rule the caller can act on, and
/// every arm costs nothing: a refused take never touches the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrownRefusal {
    /// You already hold it. Paying yourself 1.5x for a glyph you are already
    /// wearing is not a purchase, it is a mistake.
    AlreadyYours,
    /// The price would take the caller below the chip floor.
    InsufficientChips { price: i64 },
}

impl CrownRefusal {
    /// Sentence-case banner copy, the one place a refusal is worded.
    pub fn message(self) -> String {
        match self {
            Self::AlreadyYours => "You already wear the crown".to_string(),
            Self::InsufficientChips { price } => {
                format!("Taking the crown costs {} chips", thousands(price))
            }
        }
    }
}

/// A take that did not pay: a rule said no, or the database did. The two are
/// separated because only one of them is the caller's business.
#[derive(Debug)]
pub enum CrownError {
    Refused(CrownRefusal),
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for CrownError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

/// What `/crown` answers with: who holds it, how long they have, and what
/// unseating them costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrownStatus {
    /// The live holder, absent when the crown is vacant.
    pub holder: Option<CrownStatusHolder>,
    /// What taking it costs right now.
    pub price: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrownStatusHolder {
    pub username: String,
    /// Whether the caller is the one wearing it.
    pub is_you: bool,
    pub held_for_secs: i64,
}

impl CrownStatus {
    /// The single banner line `/crown` prints. One line rather than an
    /// overlay: the crown is three facts, and a command that answers inline
    /// is the same shape as `/gift`.
    pub fn line(&self) -> String {
        let price = thousands(self.price);
        let Some(holder) = &self.holder else {
            return format!("The crown is vacant. /crown take claims it for {price} chips.");
        };
        let who = match holder.is_you {
            true => "You have worn the crown".to_string(),
            false => format!("{} has worn the crown", holder.username),
        };
        let held = short_duration(holder.held_for_secs);
        format!("{who} for {held}. /crown take costs {price} chips.")
    }
}

/// A settled take: what the buyer is told, what the deposed holder is told,
/// and what #lounge hears.
#[derive(Clone, Debug)]
pub struct CrownTakeOutcome {
    pub reign_id: Uuid,
    pub price: i64,
    pub taker_balance: i64,
    /// The deposed holder's username, absent when the crown was vacant.
    pub from: Option<String>,
}

/// The answer to one session's command, or the news that someone else took
/// the crown. Broadcast to every session in this process; each one picks
/// out what is addressed to it.
#[derive(Clone, Debug)]
pub enum CrownEvent {
    /// `/crown`, answered for the session that asked.
    Status { user_id: Uuid, line: String },
    /// A take landed: the taker's receipt. Sent by the replica that ran the
    /// take, which is the one the taker is connected to.
    Taken {
        taker_id: Uuid,
        taker_balance: i64,
        price: i64,
        /// The deposed holder's username, absent when the crown was vacant.
        from: Option<String>,
    },
    /// Someone took the crown off this user. Raised from the
    /// `crown_changed` notify rather than from the take itself, so it
    /// reaches the deposed holder whichever replica they are on: a glyph
    /// vanishing off your own name with no explanation reads as a bug.
    Deposed {
        user_id: Uuid,
        taker_username: String,
        price: i64,
    },
    /// `/crown take` was refused or failed. Nothing was charged either way.
    Failed { user_id: Uuid, message: String },
}

/// `3h12m`, `12m`, `45s`: the one duration format the crown's copy uses.
fn short_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let (hours, minutes) = (secs / 3_600, (secs % 3_600) / 60);
    match (hours, minutes) {
        (0, 0) => format!("{secs}s"),
        (0, minutes) => format!("{minutes}m"),
        (hours, minutes) => format!("{hours}h{minutes:02}m"),
    }
}

#[derive(Clone)]
pub struct CrownService {
    db: Db,
    /// The #lounge feed publisher. `None` in tests and in any process that
    /// runs without the activity broadcast; the take still lands, it just
    /// tells nobody.
    activity: Option<ActivityPublisher>,
    holder_tx: watch::Sender<Option<CrownHolder>>,
    evt_tx: broadcast::Sender<CrownEvent>,
}

impl CrownService {
    pub fn new(db: Db) -> Self {
        let (holder_tx, _) = watch::channel(None);
        let (evt_tx, _) = broadcast::channel(CROWN_EVENT_CAP);
        Self {
            db,
            activity: None,
            holder_tx,
            evt_tx,
        }
    }

    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }

    /// The process-shared holder, read once a second by every session's tick
    /// so no render ever queries for the glyph.
    pub fn subscribe_holder(&self) -> watch::Receiver<Option<CrownHolder>> {
        let mut rx = self.holder_tx.subscribe();
        // `subscribe` marks the current value as seen, which would leave a
        // session connecting mid-reign with no glyph until the next take.
        rx.mark_changed();
        rx
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<CrownEvent> {
        self.evt_tx.subscribe()
    }

    /// Re-read the open reign and publish it. The startup seed and the
    /// notify handler are the same call, so a replica that reconnects its
    /// listener cannot be left holding a stale glyph.
    pub async fn refresh_holder(&self) -> Result<()> {
        let client = self.db.get().await?;
        let holder = CrownReign::find_open(&client)
            .await?
            .map(|reign| CrownHolder {
                user_id: reign.holder_user_id,
                month: reign.month,
            });
        self.holder_tx.send_replace(holder);
        Ok(())
    }

    /// Keep every replica's glyph in step. One long-lived Postgres
    /// connection LISTENs on [`CROWN_CHANGED_CHANNEL`]; a dropped connection
    /// reconnects after five seconds and re-seeds, so a take committed
    /// during the gap is not lost. Same shape as
    /// `ChatService::start_gild_listener_task`.
    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = service.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "crown postgres listener stopped");
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
        let listen = listen_for_crown_changes(&client);
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

        // Seeded after the LISTEN is live, so a take committed between the
        // two is caught by this read rather than dropped.
        self.refresh_holder().await?;

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
        if notification.channel() != CROWN_CHANGED_CHANNEL {
            return;
        }
        self.apply_change(notification.payload()).await;
    }

    /// One `crown_changed` notify, on every replica including the one that
    /// sent it: re-read the holder for the glyph, then tell the deposed
    /// holder who took it if they are connected here.
    ///
    /// A failed re-read is this replica's glyph lagging until the next
    /// take, not a reason to drop the LISTEN connection: propagating it
    /// would lose every take committed during the reconnect window. A
    /// payload that does not parse is logged for the same reason; the
    /// re-read does not depend on it.
    pub(super) async fn apply_change(&self, payload: &str) {
        if let Err(error) = self.refresh_holder().await {
            tracing::warn!(error = ?error, "failed to refresh the crown holder");
        }
        let change = match CrownChange::parse(payload) {
            Ok(change) => change,
            Err(error) => {
                tracing::warn!(error = ?error, payload, "unreadable crown_changed payload");
                return;
            }
        };
        if let Some(user_id) = change.deposed_user_id {
            let _ = self.evt_tx.send(CrownEvent::Deposed {
                user_id,
                taker_username: change.taker_username,
                price: change.price,
            });
        }
    }

    /// Answer `/crown` for one session.
    pub fn status_task(&self, user_id: Uuid) {
        let service = self.clone();
        let span = info_span!("crown.status_task", user_id = %user_id);
        tokio::spawn(
            async move {
                let event = match service.status(user_id).await {
                    Ok(status) => CrownEvent::Status {
                        user_id,
                        line: status.line(),
                    },
                    Err(error) => {
                        late_core::error_span!(
                            "crown_status_failed",
                            error = ?error,
                            "failed to read the crown"
                        );
                        CrownEvent::Failed {
                            user_id,
                            message: "Could not read the crown".to_string(),
                        }
                    }
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(span),
        );
    }

    /// Buy the crown. Fire-and-forget because the caller is at a composer,
    /// not in a request/response: the line clears on Enter and the banner
    /// arrives with the answer.
    ///
    /// This is the orchestration layer: every refusal, every failure, the
    /// metrics and the #lounge line are decided here, and [`Self::take`]
    /// below does nothing but the transaction.
    pub fn take_task(&self, user_id: Uuid, username: String) {
        let service = self.clone();
        let span = info_span!("crown.take_task", user_id = %user_id);
        tokio::spawn(
            async move {
                match service.take(user_id, &username).await {
                    Ok(outcome) => service.announce(user_id, outcome),
                    Err(CrownError::Refused(refusal)) => {
                        metrics::record_crown_take_refused(refusal);
                        let _ = service.evt_tx.send(CrownEvent::Failed {
                            user_id,
                            message: refusal.message(),
                        });
                    }
                    Err(CrownError::Failed(error)) => {
                        late_core::error_span!(
                            "crown_take_failed",
                            error = ?error,
                            "failed to take the crown"
                        );
                        let _ = service.evt_tx.send(CrownEvent::Failed {
                            user_id,
                            message: "Taking the crown failed, nothing was charged".to_string(),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }

    /// Everything a settled take has to tell: the buyer and #lounge. The
    /// glyph and the deposed holder's banner both move off the Postgres
    /// notify (see `take` and `apply_change`), including in this process,
    /// so what is sent here is only what the buyer is told.
    fn announce(&self, user_id: Uuid, outcome: CrownTakeOutcome) {
        metrics::record_crown_taken(outcome.price);
        let _ = self.evt_tx.send(CrownEvent::Taken {
            taker_id: user_id,
            taker_balance: outcome.taker_balance,
            price: outcome.price,
            from: outcome.from.clone(),
        });
        if let Some(activity) = &self.activity {
            activity.crown_taken_task(
                user_id,
                outcome.reign_id,
                outcome.price,
                next_price(Some(outcome.price)),
                outcome.from,
            );
        }
    }

    /// Who holds the crown and what it costs, for one caller. A plain read:
    /// nothing here locks, and a stale reign resolves to vacant exactly the
    /// way the take path resolves it.
    pub(super) async fn status(&self, user_id: Uuid) -> Result<CrownStatus> {
        let now = Utc::now();
        let client = self.db.get().await?;
        let open = CrownReign::find_open(&client).await?;
        let current = open.filter(|reign| reign.is_current(now));
        let Some(current) = current else {
            return Ok(CrownStatus {
                holder: None,
                price: next_price(None),
            });
        };
        let username = match User::get(&client, current.holder_user_id).await? {
            Some(user) => user.username,
            // The holder deleted their account between the reign and this
            // read. The row is gone by cascade the moment that commits, so
            // this is a race with the delete, not a corrupt reign.
            None => {
                return Ok(CrownStatus {
                    holder: None,
                    price: next_price(None),
                });
            }
        };
        Ok(CrownStatus {
            holder: Some(CrownStatusHolder {
                username,
                is_you: current.holder_user_id == user_id,
                held_for_secs: (now - current.taken_at).num_seconds().max(0),
            }),
            price: next_price(Some(current.paid_chips)),
        })
    }

    /// The deposed holder's name for the banner and the #lounge line. Runs
    /// after the take has committed, so it never fails: a connection this
    /// cannot get costs the line a name, not the buyer their chips.
    async fn deposed_username(&self, user_id: Uuid) -> String {
        match self.db.get().await {
            Ok(client) => fetch_username(&client, user_id).await,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    user_id = %user_id,
                    "failed to name the deposed crown holder"
                );
                "someone".to_string()
            }
        }
    }

    /// The one transaction: serialize every take, read the reign, close it,
    /// open the new one, burn the chips. Every early return drops the
    /// transaction, which rolls it back, so a refusal here is uncharged too.
    pub(super) async fn take(
        &self,
        user_id: Uuid,
        taker_username: &str,
    ) -> Result<CrownTakeOutcome, CrownError> {
        let now = Utc::now();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await.map_err(anyhow::Error::from)?;
        let open = CrownReign::lock_open(&tx).await?;
        // A reign from a previous month is stale rather than current: the
        // crown reads as vacant at the minimum price, and the row below is
        // closed on the way past.
        let current = open.as_ref().filter(|reign| reign.is_current(now));
        // No hold: a reign is takeable the moment it exists, at the next
        // rung. Two takes racing for one crown both land, the second at 1.5x
        // the first, which is the auction working as designed; the price
        // ladder, not a timer, is what throttles a war.
        if let Some(current) = current
            && current.holder_user_id == user_id
        {
            return Err(CrownError::Refused(CrownRefusal::AlreadyYours));
        }
        let price = next_price(current.map(|reign| reign.paid_chips));
        let deposed = current.map(|reign| reign.holder_user_id);
        if let Some(open) = &open {
            CrownReign::close_in_tx(&tx, open.id).await?;
        }
        let reign = CrownReign::open_in_tx(&tx, user_id, price).await?;
        let Some(chips) = UserChips::apply(
            &*tx,
            user_id,
            ChipMove::CrownTaken,
            price,
            Some(&reign.id.to_string()),
        )
        .await?
        else {
            return Err(CrownError::Refused(CrownRefusal::InsufficientChips {
                price,
            }));
        };
        // The glyph and the deposed holder's banner move over Postgres, not
        // over this process's broadcast, so every replica learns about them
        // the same way and there is exactly one code path for each.
        CrownReign::notify_changed(
            &tx,
            &CrownChange {
                taker_username: taker_username.to_string(),
                price,
                deposed_user_id: deposed,
            },
        )
        .await?;
        tx.commit().await.map_err(anyhow::Error::from)?;
        drop(client);

        // Past the commit, nothing may turn a settled take back into an
        // error: the chips are gone and the reign is live, so a name this
        // read cannot produce degrades to `fetch_username`'s "someone"
        // rather than becoming a "nothing was charged" the buyer would read
        // as the truth.
        let from = match deposed {
            None => None,
            Some(deposed) => Some(self.deposed_username(deposed).await),
        };
        Ok(CrownTakeOutcome {
            reign_id: reign.id,
            price,
            taker_balance: chips.balance,
            from,
        })
    }
}
