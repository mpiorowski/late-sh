//! The replica's street task: the one writer of this process's view of
//! who stands where on the night city street, and its voice on the wire.
//!
//! Sessions never await it. A step, a look away, or a logout is an event
//! on an unbounded queue (`StreetSession`), and the task folds it into
//! `Street`. Every [`FLUSH`] it hands what changed to the publisher, a
//! second task that sends one `pg_notify` per batch in order; every
//! [`HEARTBEAT`] the batch is the whole roster instead. Batching per
//! replica is what keeps the wire cheap: a notify rate of replicas times
//! flushes, however many runners are walking. The publisher's queue holds
//! one batch; while it is busy the changes wait in `Street` and go out
//! together on a later flush, so a slow database delays the street and
//! never grows a queue.
//!
//! What other replicas say arrives through the process listener
//! (`crate::pg_listener`, `deadchannel_street`) into the same loop. After a
//! LISTEN reconnect there is nothing to re-read: every replica's next
//! heartbeat refills the street, and a leaver missed meanwhile ages out.
//! The merged view goes out on a `watch` that sessions copy on their tick.

use std::sync::Arc;
use std::time::Duration;

use late_core::db::Db;
use late_core::models::deadchannel_street::{StreetBatch, notify_street, parse_street_payload};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::state::{Street, StreetView};
use crate::metrics::{self, StreetWire};
use crate::pg_listener::{Channel, Signal};

/// How often changes go out: the worst extra latency another replica
/// sees on a step.
const FLUSH: Duration = Duration::from_millis(200);
/// How often the whole roster goes out: keeps this replica's runners alive
/// elsewhere (`state::HEARD_TTL_MS`) and fills a replica that just came up.
const HEARTBEAT: Duration = Duration::from_secs(3);

/// What a session tells the street.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreetEvent {
    Stand {
        session_id: Uuid,
        user_id: Uuid,
        x: u16,
        y: u16,
        present: bool,
    },
    Leave {
        session_id: Uuid,
    },
}

/// What sessions share: user id to runner, by `Arc` so a tick copy is a
/// pointer bump.
pub type SharedStreetView = Arc<StreetView>;

#[derive(Clone)]
pub struct StreetService {
    events_tx: mpsc::UnboundedSender<StreetEvent>,
    view_rx: watch::Receiver<SharedStreetView>,
}

impl StreetService {
    /// What the task subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::DeadchannelStreet];

    /// Start this replica's street: the loop and its publisher. The
    /// replica id is fresh per process, so a restarted pod is never taken
    /// for the runners of its previous life.
    pub fn start(db: Db, signals: mpsc::UnboundedReceiver<Signal>) -> Self {
        let replica_id = Uuid::now_v7();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let (view_tx, view_rx) = watch::channel(Arc::new(StreetView::new()));
        let (publish_tx, publish_rx) = mpsc::channel(1);
        tokio::spawn(publish_loop(db, publish_rx));
        tokio::spawn(street_loop(
            Street::new(replica_id),
            events_rx,
            signals,
            view_tx,
            publish_tx,
        ));
        tracing::info!(replica_id = %replica_id, "deadchannel street started");
        Self { events_tx, view_rx }
    }

    /// A street with no task behind it, for test apps and headless paths:
    /// events go nowhere and the view is whatever it was built with.
    pub fn detached(view: StreetView) -> Self {
        let (events_tx, _) = mpsc::unbounded_channel();
        let (_, view_rx) = watch::channel(Arc::new(view));
        Self { events_tx, view_rx }
    }

    pub(super) fn events_tx(&self) -> mpsc::UnboundedSender<StreetEvent> {
        self.events_tx.clone()
    }

    pub(super) fn view_rx(&self) -> watch::Receiver<SharedStreetView> {
        self.view_rx.clone()
    }
}

async fn street_loop(
    mut street: Street,
    mut events: mpsc::UnboundedReceiver<StreetEvent>,
    mut signals: mpsc::UnboundedReceiver<Signal>,
    view_tx: watch::Sender<SharedStreetView>,
    publish_tx: mpsc::Sender<StreetBatch>,
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
                let now = now_ms();
                let mut changed = apply(&mut street, event, now);
                while let Ok(event) = events.try_recv() {
                    changed |= apply(&mut street, event, now);
                }
                changed
            }
            signal = signals.recv(), if signals_open => match signal {
                Some(Signal::Resync) => {
                    tracing::debug!("deadchannel street resync");
                    false
                }
                Some(Signal::Notify { payload, .. }) => hear(&mut street, &payload),
                None => {
                    signals_open = false;
                    false
                }
            },
            _ = flush.tick() => {
                let expired = street.expire(now_ms());
                for _ in 0..expired {
                    metrics::record_deadchannel_street(StreetWire::Expired);
                }
                if let Ok(permit) = publish_tx.try_reserve() {
                    let batch = if last_heartbeat.elapsed() >= HEARTBEAT {
                        last_heartbeat = tokio::time::Instant::now();
                        street.take_heartbeat()
                    } else {
                        street.take_changes()
                    };
                    if !batch.is_empty() {
                        permit.send(batch);
                    }
                }
                expired > 0
            }
        };
        if changed {
            view_tx.send_replace(Arc::new(street.view()));
        }
    }
}

fn apply(street: &mut Street, event: StreetEvent, now_ms: i64) -> bool {
    match event {
        StreetEvent::Stand {
            session_id,
            user_id,
            x,
            y,
            present,
        } => street.stand(session_id, user_id, x, y, present, now_ms),
        StreetEvent::Leave { session_id } => street.leave(session_id),
    }
}

fn hear(street: &mut Street, payload: &str) -> bool {
    match parse_street_payload(payload) {
        Some(batch) => {
            metrics::record_deadchannel_street(StreetWire::Heard);
            street.hear(batch, now_ms())
        }
        None => {
            metrics::record_deadchannel_street(StreetWire::Rejected);
            tracing::warn!(payload = %payload, "deadchannel street payload failed to parse");
            false
        }
    }
}

/// Send each batch, split to fit a notify, in the order it was handed
/// over. A failed send is logged and counted here and not retried: the
/// next heartbeat carries every stand again.
async fn publish_loop(db: Db, mut batches: mpsc::Receiver<StreetBatch>) {
    while let Some(batch) = batches.recv().await {
        for notify in batch.into_notifies() {
            match publish(&db, &notify).await {
                Ok(()) => metrics::record_deadchannel_street(StreetWire::Published),
                Err(error) => {
                    metrics::record_deadchannel_street(StreetWire::PublishFailed);
                    tracing::warn!(error = ?error, "failed to publish deadchannel street");
                }
            }
        }
    }
}

async fn publish(db: &Db, batch: &StreetBatch) -> anyhow::Result<()> {
    let client = db.get().await?;
    notify_street(&client, batch).await
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
