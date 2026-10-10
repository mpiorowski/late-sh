//! Async work only. Private data returns on the requesting session's channel.
use super::ical::{self, Candidate};
use crate::pg_listener::{Refresh, Signal, read_until_ok};
use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;
use late_core::{
    db::Db,
    models::calendar::{
        CalendarEvent, CalendarPreferences, CalendarSource, CalendarStore, CreationTier,
        EventDraft, PublicCalendar,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;
#[derive(Clone)]
pub struct CalendarService {
    pub store: CalendarStore,
    epoch: Arc<AtomicU64>,
    changed: watch::Sender<u64>,
    server: watch::Sender<Vec<CalendarEvent>>,
}
#[derive(Debug)]
pub struct Snapshot {
    pub events: Vec<CalendarEvent>,
    pub event_error: Option<String>,
    pub personal_notices: Vec<CalendarEvent>,
    pub public: Vec<PublicCalendar>,
    pub preferences: CalendarPreferences,
    pub role: CreationTier,
}
#[derive(Debug)]
pub enum Reply {
    Loaded {
        generation: u64,
        result: Result<Snapshot, String>,
    },
    Saved(Result<CalendarEvent, String>),
    Deleted(Result<(), String>),
    Preferences(Result<CalendarPreferences, String>),
    Opened {
        generation: u64,
        result: Result<CalendarEvent, String>,
    },
    Imported {
        generation: u64,
        result: Result<Vec<Candidate>, String>,
    },
}
#[derive(Clone, Copy)]
pub struct Query {
    pub viewer: Uuid,
    pub source: CalendarSource,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub tz: Tz,
    pub generation: u64,
}
impl CalendarService {
    pub(super) fn import(
        &self,
        input: String,
        tz: Tz,
        generation: u64,
        tx: mpsc::UnboundedSender<Reply>,
    ) {
        tokio::spawn(async move {
            let result = async {
                let content = if let Some(url) = ical::import_url(&input)? {
                    let timeout = std::time::Duration::from_secs(20);
                    let bytes = tokio::time::timeout(timeout,
                        crate::app::files::image_upload::download_url_bytes_following_redirects(
                            &url, timeout, ical::MAX_BYTES, 5))
                        .await.map_err(|_| anyhow::anyhow!("iCalendar download timed out"))?
                        .map_err(|_| anyhow::anyhow!("Could not fetch iCalendar: URL must be public, reachable and at most 1 MiB"))?;
                    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("iCalendar must be UTF-8 text"))?
                } else { input };
                ical::parse(&content, tz)
            }.await.map_err(|e| e.to_string());
            let _ = tx.send(Reply::Imported { generation, result });
        });
    }
    pub fn new(db: Db) -> Self {
        let (changed, _) = watch::channel(0);
        let (server, _) = watch::channel(Vec::new());
        Self {
            store: CalendarStore::new(db),
            epoch: Arc::new(AtomicU64::new(0)),
            changed,
            server,
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
    pub fn server_notices(&self) -> watch::Receiver<Vec<CalendarEvent>> {
        self.server.subscribe()
    }
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut minute = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {signal=signals.recv()=>{if signal.is_none(){break;}while signals.try_recv().is_ok(){}},_=minute.tick()=>{}}
                // Publish invalidation first. Sessions erase shared data before
                // rechecking access, including on listener reconnect.
                let epoch = svc.epoch.fetch_add(1, Ordering::Relaxed) + 1;
                svc.server.send_replace(Vec::new());
                svc.changed.send_replace(epoch);
                read_until_ok(Refresh::CalendarNotices, || async {
                    let events = svc.store.upcoming_server(Utc::now()).await?;
                    svc.server.send_replace(events);
                    Ok(())
                })
                .await;
            }
        })
    }
    pub fn load(&self, q: Query, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = async {
                let preferences = svc.store.preferences(q.viewer).await?;
                let visible = svc
                    .store
                    .visible(
                        q.viewer,
                        q.source,
                        preferences.server_overlay,
                        q.from,
                        q.to,
                        q.tz,
                    )
                    .await;
                let (events, event_error) = match visible {
                    Ok(events) => (events, None),
                    Err(error) => (Vec::new(), Some(error.to_string())),
                };
                Ok::<_, anyhow::Error>(Snapshot {
                    preferences,
                    events,
                    event_error,
                    role: svc.store.role(q.viewer).await?,
                    public: svc.store.public_calendars(q.viewer).await?,
                    personal_notices: svc.store.upcoming_personal(q.viewer, Utc::now()).await?,
                })
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(Reply::Loaded {
                generation: q.generation,
                result,
            });
        });
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
        tokio::spawn(async move {
            let _ = tx.send(Reply::Saved(
                svc.store
                    .save(viewer, source, existing, &draft)
                    .await
                    .map_err(|e| e.to_string()),
            ));
        });
    }
    pub fn delete(&self, viewer: Uuid, id: Uuid, revision: i64, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _ = tx.send(Reply::Deleted(
                svc.store
                    .delete(viewer, id, revision)
                    .await
                    .map_err(|e| e.to_string()),
            ));
        });
    }
    pub fn preferences(
        &self,
        viewer: Uuid,
        p: CalendarPreferences,
        tx: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _ = tx.send(Reply::Preferences(
                svc.store
                    .save_preferences(viewer, &p)
                    .await
                    .map_err(|e| e.to_string()),
            ));
        });
    }
    pub fn open(&self, viewer: Uuid, id: Uuid, generation: u64, tx: mpsc::UnboundedSender<Reply>) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _ = tx.send(Reply::Opened {
                generation,
                result: svc.store.event(viewer, id).await.map_err(|e| e.to_string()),
            });
        });
    }
}
