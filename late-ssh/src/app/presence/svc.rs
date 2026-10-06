//! The replica's presence task: the one writer of this process's view of
//! who is where (the tavern, the Nightcap stools, the night city street),
//! and its voice on the wire.
//!
//! Sessions never await it. A changed record or a logout is an event on an
//! unbounded queue (`PresenceSession`), and the task folds it into
//! `Presence`. Every [`FLUSH`] it hands what changed to the publisher, a
//! second task that sends one `pg_notify` per batch in order; every
//! [`HEARTBEAT`] the batch is every local record instead. Batching per
//! replica is what keeps the wire cheap: a notify rate of replicas times
//! flushes, however many people are walking. The publisher's queue holds
//! one batch; while it is busy the changes wait in `Presence` and go out
//! together on a later flush, so a slow database delays presence and never
//! grows a queue.
//!
//! What other replicas say arrives through the process listener
//! (`crate::pg_listener`, `presence`) into the same loop. After a LISTEN
//! reconnect there is nothing to re-read: every replica's next heartbeat
//! refills presence, and a leaver missed meanwhile ages out. Every live
//! record goes out on a `watch` that sessions copy on their tick.

use std::sync::Arc;
use std::time::Duration;

use late_core::db::Db;
use late_core::models::presence::{
    PresenceBatch, PresenceRecord, notify_presence, parse_presence_payload,
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::state::Presence;
use crate::metrics::{self, PresenceScope, PresenceWire};
use crate::pg_listener::{Channel, Signal};

/// How often changes go out: the worst extra latency another replica sees
/// on a step.
const FLUSH: Duration = Duration::from_millis(200);
/// How often every local record goes out: keeps this replica's sessions
/// alive elsewhere (`state::HEARD_TTL_MS`) and fills a replica that just
/// came up.
const HEARTBEAT: Duration = Duration::from_secs(3);

/// What a session tells the task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresenceEvent {
    Put(Box<PresenceRecord>),
    Leave { session_id: Uuid },
}

/// What sessions share: every live record, in session order, by `Arc` so a
/// tick copy is a pointer bump.
pub type Records = Arc<Vec<PresenceRecord>>;

#[derive(Clone)]
pub struct PresenceService {
    events_tx: mpsc::UnboundedSender<PresenceEvent>,
    records_rx: watch::Receiver<Records>,
}

impl PresenceService {
    /// What the task subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::Presence];

    /// Start this replica's presence: the loop and its publisher. The
    /// replica id is fresh per process, so a restarted pod is never taken
    /// for the sessions of its previous life.
    pub fn start(db: Db, signals: mpsc::UnboundedReceiver<Signal>) -> Self {
        let replica_id = Uuid::now_v7();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (records_tx, records_rx) = watch::channel(Arc::new(Vec::new()));
        let (publish_tx, publish_rx) = mpsc::channel(1);
        tokio::spawn(publish_loop(db, publish_rx));
        tokio::spawn(presence_loop(
            Presence::new(replica_id),
            events_rx,
            signals,
            records_tx,
            publish_tx,
        ));
        tracing::info!(replica_id = %replica_id, "presence started");
        Self {
            events_tx,
            records_rx,
        }
    }

    /// Presence with no task behind it, for test apps and headless paths:
    /// events go nowhere and the records are whatever it was built with.
    pub fn detached(records: Vec<PresenceRecord>) -> Self {
        let (events_tx, _) = mpsc::unbounded_channel();
        let (_, records_rx) = watch::channel(Arc::new(records));
        Self {
            events_tx,
            records_rx,
        }
    }

    /// Every live record right now: what a new session seats itself from.
    pub fn records(&self) -> Records {
        self.records_rx.borrow().clone()
    }

    pub(super) fn events_tx(&self) -> mpsc::UnboundedSender<PresenceEvent> {
        self.events_tx.clone()
    }

    pub(super) fn records_rx(&self) -> watch::Receiver<Records> {
        self.records_rx.clone()
    }
}

async fn presence_loop(
    mut presence: Presence,
    mut events: mpsc::UnboundedReceiver<PresenceEvent>,
    mut signals: mpsc::UnboundedReceiver<Signal>,
    records_tx: watch::Sender<Records>,
    publish_tx: mpsc::Sender<PresenceBatch>,
) {
    let mut flush = tokio::time::interval(FLUSH);
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_heartbeat = tokio::time::Instant::now();
    let mut signals_open = true;
    loop {
        let changed = tokio::select! {
            event = events.recv() => {
                let Some(event) = event else {
                    // Every sender is gone: the service itself was dropped.
                    return;
                };
                let mut changed = apply(&mut presence, event);
                while let Ok(event) = events.try_recv() {
                    changed |= apply(&mut presence, event);
                }
                changed
            }
            signal = signals.recv(), if signals_open => match signal {
                Some(Signal::Resync) => {
                    tracing::debug!("presence resync");
                    false
                }
                Some(Signal::Notify { payload, .. }) => hear(&mut presence, &payload),
                None => {
                    signals_open = false;
                    false
                }
            },
            _ = flush.tick() => {
                let expired = presence.expire(now_ms());
                for _ in 0..expired {
                    metrics::record_presence(PresenceWire::Expired);
                }
                if let Ok(permit) = publish_tx.try_reserve() {
                    let batch = if last_heartbeat.elapsed() >= HEARTBEAT {
                        last_heartbeat = tokio::time::Instant::now();
                        presence.take_heartbeat()
                    } else {
                        presence.take_changes()
                    };
                    if !batch.is_empty() {
                        permit.send(batch);
                    }
                }
                expired > 0
            }
        };
        if changed {
            let records = presence.records();
            metrics::record_presence_records(PresenceScope::Local, presence.local_count());
            metrics::record_presence_records(PresenceScope::All, records.len());
            records_tx.send_replace(Arc::new(records));
        }
    }
}

fn apply(presence: &mut Presence, event: PresenceEvent) -> bool {
    match event {
        PresenceEvent::Put(record) => presence.put(*record),
        PresenceEvent::Leave { session_id } => presence.leave(session_id),
    }
}

fn hear(presence: &mut Presence, payload: &str) -> bool {
    match parse_presence_payload(payload) {
        Some(batch) => {
            metrics::record_presence(PresenceWire::Heard);
            presence.hear(batch, now_ms())
        }
        None => {
            metrics::record_presence(PresenceWire::Rejected);
            tracing::warn!(payload = %payload, "presence payload failed to parse");
            false
        }
    }
}

/// Send each batch, split to fit a notify, in the order it was handed
/// over. A failed send is logged and counted here and not retried: the
/// next heartbeat carries every record again.
async fn publish_loop(db: Db, mut batches: mpsc::Receiver<PresenceBatch>) {
    while let Some(batch) = batches.recv().await {
        for notify in batch.into_notifies() {
            match publish(&db, &notify).await {
                Ok(()) => metrics::record_presence(PresenceWire::Published),
                Err(error) => {
                    metrics::record_presence(PresenceWire::PublishFailed);
                    tracing::warn!(error = ?error, "failed to publish presence");
                }
            }
        }
    }
}

async fn publish(db: &Db, batch: &PresenceBatch) -> anyhow::Result<()> {
    let client = db.get().await?;
    notify_presence(&client, batch).await
}

/// Wall clock in unix ms: the one clock every presence stamp is on, so
/// stamps from different replicas compare.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
