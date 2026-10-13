//! The board's orchestration: every refusal, every log line, every metric
//! and both #lounge lines are decided here. `late_core::models::calendar`
//! owns the tables and the rules underneath; `state.rs` only reads replies.
//!
//! Private data (a viewer's own events, who they said they are in for)
//! returns on the requesting session's channel. The board's live posts are
//! one process-shared `watch` every session reads, re-read on
//! `calendar_changed` and once a minute as the clock moves posts into and
//! out of the live window. The start announcement is a sweeper on every
//! replica claiming rows (`CalendarStore::claim_started`), so one replica
//! posts each.
use crate::{
    app::activity::publisher::ActivityPublisher,
    metrics,
    pg_listener::{Refresh, Signal, read_until_ok},
};
use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;
use late_core::{
    db::Db,
    models::calendar::{
        CalendarError, CalendarEvent, CalendarRefusal, CalendarRefusalKind, CalendarSource,
        CalendarStore, EventDraft,
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

/// How often every replica looks for board events that just started.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// How often the shared board snapshot is re-read without a notify, so a
/// post crossing into or out of the live window shows on the strip and the
/// Live panel. The re-read invalidates no session.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);

/// A settled write, for the metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarWrite {
    BoardPosted,
    PersonalPosted,
    Edited,
    Deleted,
    /// Staff deleted somebody else's post.
    StaffDeleted,
    RsvpIn,
    RsvpOut,
}

/// A write that did not settle because the database failed, for the metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarOp {
    Load,
    Open,
    Save,
    Delete,
    Rsvp,
    Announce,
}

/// A #lounge line the board shipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarAnnouncement {
    Posted,
    Starting,
}

#[derive(Clone)]
pub struct CalendarService {
    pub store: CalendarStore,
    /// The #lounge feed publisher. `None` in tests and in any process that
    /// runs without the activity broadcast; the board works, it just tells
    /// nobody.
    activity: Option<ActivityPublisher>,
    epoch: Arc<AtomicU64>,
    changed: watch::Sender<u64>,
    board: watch::Sender<Vec<CalendarEvent>>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub events: Vec<CalendarEvent>,
    /// The board events the viewer said they are in.
    pub rsvps: Vec<Uuid>,
    pub staff: bool,
}

/// A save that did not land, worded for the viewer. A conflict is the one
/// the editor recovers from in place: a reload adopts the newer revision
/// and keeps the draft.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    Conflict(String),
    Other(String),
}

#[derive(Debug)]
pub enum Reply {
    Loaded {
        generation: u64,
        result: Result<Snapshot, String>,
    },
    Saved(Result<CalendarEvent, SaveError>),
    Deleted(Result<(), String>),
    Rsvp(Result<CalendarEvent, String>),
    Opened {
        generation: u64,
        result: Result<CalendarEvent, String>,
    },
}

#[derive(Clone, Copy)]
pub struct Query {
    pub viewer: Uuid,
    pub board: bool,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub tz: Tz,
    pub generation: u64,
}

/// The one place a `CalendarError` becomes a metric and a message. A
/// refusal is the user's own doing and is told to them; a failure is ours,
/// logged with the error and told to them in general terms.
fn settle<T>(op: CalendarOp, result: Result<T, CalendarError>) -> Result<T, String> {
    match result {
        Ok(value) => Ok(value),
        Err(CalendarError::Refused(refusal)) => {
            metrics::record_calendar_refusal(refusal.kind());
            Err(refusal.message())
        }
        Err(CalendarError::Failed(error)) => {
            metrics::record_calendar_failure(op);
            late_core::error_span!(
                "calendar_failed",
                error = ?error,
                op = ?op,
                "calendar operation failed"
            );
            Err("Something went wrong; try again".to_string())
        }
    }
}

impl CalendarService {
    pub fn new(db: Db) -> Self {
        let (changed, _) = watch::channel(0);
        let (board, _) = watch::channel(Vec::new());
        Self {
            store: CalendarStore::new(db),
            activity: None,
            epoch: Arc::new(AtomicU64::new(0)),
            changed,
            board,
        }
    }
    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }
    /// Bumps on every `calendar_changed` and every reconnect: sessions drop
    /// what they loaded and read again.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
    /// The board's live posts, shared by every session on this replica.
    pub fn board_live(&self) -> watch::Receiver<Vec<CalendarEvent>> {
        self.board.subscribe()
    }
    /// The replica's board worker. A notify, or the listener coming back
    /// after missing some, bumps the epoch so every session drops what it
    /// loaded and reads again. The minute tick only moves posts into and out
    /// of the live window, so it re-reads the board alone. Either way the
    /// board already published stays up until the fresh read replaces it.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut minute = tokio::time::interval(SNAPSHOT_INTERVAL);
            loop {
                tokio::select! {
                    signal = signals.recv() => {
                        if signal.is_none() {
                            break;
                        }
                        // A burst collapses into one invalidation and one read.
                        while signals.try_recv().is_ok() {}
                        let epoch = svc.epoch.fetch_add(1, Ordering::Relaxed) + 1;
                        svc.changed.send_replace(epoch);
                    }
                    _ = minute.tick() => {}
                }
                read_until_ok(Refresh::CalendarNotices, || async {
                    let events = svc.store.live_board(Utc::now()).await?;
                    svc.publish_board(events);
                    Ok(())
                })
                .await;
            }
        })
    }
    /// Hand every session on this replica the board's live posts.
    pub fn publish_board(&self, events: Vec<CalendarEvent>) {
        self.board.send_replace(events);
    }
    /// The start announcement: every replica sweeps, the claim decides who
    /// posts. This is the orchestration layer for it; nothing below logs.
    pub fn start_sweeper_task(&self) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                svc.sweep().await;
                tokio::time::sleep(SWEEP_INTERVAL).await;
            }
        })
    }
    /// One sweep, for tests that want the claim without the clock.
    #[cfg(test)]
    pub(super) async fn sweep_for_test(&self) {
        self.sweep().await;
    }
    async fn sweep(&self) {
        match self.store.claim_started(Utc::now()).await {
            Ok(started) => {
                for event in started {
                    metrics::record_calendar_announcement(CalendarAnnouncement::Starting);
                    if let Some(activity) = &self.activity {
                        activity.event_starting(event.id, event.title.clone(), event.going);
                    }
                }
            }
            Err(error) => {
                metrics::record_calendar_failure(CalendarOp::Announce);
                late_core::error_span!(
                    "calendar_announce_failed",
                    error = ?error,
                    "failed to claim started board events"
                );
            }
        }
    }
    pub fn load(&self, q: Query, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        let span = info_span!("calendar.load", viewer = %q.viewer, board = q.board);
        tokio::spawn(
            async move {
                let result = async {
                    Ok::<_, CalendarError>(Snapshot {
                        events: svc
                            .store
                            .visible(q.viewer, q.board, q.from, q.to, q.tz)
                            .await?,
                        rsvps: svc.store.rsvps(q.viewer).await?,
                        staff: svc.store.staff(q.viewer).await?,
                    })
                }
                .await;
                let _ = tx.send(Reply::Loaded {
                    generation: q.generation,
                    result: settle(CalendarOp::Load, result),
                });
            }
            .instrument(span),
        );
    }
    pub fn save(
        &self,
        viewer: Uuid,
        source: CalendarSource,
        existing: Option<(Uuid, i64)>,
        draft: EventDraft,
        tx: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span =
            info_span!("calendar.save", viewer = %viewer, ?source, editing = existing.is_some());
        tokio::spawn(
            async move {
                let result = svc.store.save(viewer, source, existing, &draft).await;
                let conflict = matches!(
                    result,
                    Err(CalendarError::Refused(CalendarRefusal::Revision))
                );
                let reply = match settle(CalendarOp::Save, result) {
                    Ok(saved) => {
                        let write = match (saved.created, source) {
                            (true, CalendarSource::Board) => CalendarWrite::BoardPosted,
                            (true, CalendarSource::Personal) => CalendarWrite::PersonalPosted,
                            (false, _) => CalendarWrite::Edited,
                        };
                        metrics::record_calendar_write(write);
                        if write == CalendarWrite::BoardPosted {
                            metrics::record_calendar_announcement(CalendarAnnouncement::Posted);
                            if let Some(activity) = &svc.activity {
                                activity.event_posted_task(
                                    viewer,
                                    saved.event.id,
                                    saved.event.title.clone(),
                                    saved.event.starts_at,
                                );
                            }
                        }
                        Ok(saved.event)
                    }
                    Err(message) if conflict => Err(SaveError::Conflict(message)),
                    Err(message) => Err(SaveError::Other(message)),
                };
                let _ = tx.send(Reply::Saved(reply));
            }
            .instrument(span),
        );
    }
    pub fn delete(&self, viewer: Uuid, id: Uuid, revision: i64, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        let span = info_span!("calendar.delete", viewer = %viewer, event = %id);
        tokio::spawn(
            async move {
                let result = svc.store.delete(viewer, id, revision).await;
                let reply = match settle(CalendarOp::Delete, result) {
                    Ok(deleted) => {
                        metrics::record_calendar_write(if deleted.by_staff {
                            CalendarWrite::StaffDeleted
                        } else {
                            CalendarWrite::Deleted
                        });
                        Ok(())
                    }
                    Err(message) => Err(message),
                };
                let _ = tx.send(Reply::Deleted(reply));
            }
            .instrument(span),
        );
    }
    pub fn rsvp(&self, viewer: Uuid, id: Uuid, going: bool, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        let span = info_span!("calendar.rsvp", viewer = %viewer, event = %id, going);
        tokio::spawn(
            async move {
                let result = svc.store.set_rsvp(viewer, id, going).await;
                let reply = match settle(CalendarOp::Rsvp, result) {
                    Ok(event) => {
                        metrics::record_calendar_write(if going {
                            CalendarWrite::RsvpIn
                        } else {
                            CalendarWrite::RsvpOut
                        });
                        Ok(event)
                    }
                    Err(message) => Err(message),
                };
                let _ = tx.send(Reply::Rsvp(reply));
            }
            .instrument(span),
        );
    }
    pub fn open(&self, viewer: Uuid, id: Uuid, generation: u64, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        let span = info_span!("calendar.open", viewer = %viewer, event = %id);
        tokio::spawn(
            async move {
                let result = svc.store.event(viewer, id).await;
                let _ = tx.send(Reply::Opened {
                    generation,
                    result: settle(CalendarOp::Open, result),
                });
            }
            .instrument(span),
        );
    }
}

/// The refusal kinds, named for the metric label.
pub fn refusal_label(kind: CalendarRefusalKind) -> &'static str {
    match kind {
        CalendarRefusalKind::Banned => "banned",
        CalendarRefusalKind::DailyCap => "daily_cap",
        CalendarRefusalKind::ReadOnly => "read_only",
        CalendarRefusalKind::Revision => "revision",
        CalendarRefusalKind::Gone => "gone",
        CalendarRefusalKind::NoRsvp => "no_rsvp",
        CalendarRefusalKind::Invalid => "invalid",
    }
}
