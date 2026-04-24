use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use chrono::{Datelike, NaiveDate, Utc};
use dartboard_core::{Canvas, CanvasOp};
use dartboard_local::{CanvasStore, ColorSelectionMode, ServerHandle};
use late_core::{MutexRecover, db::Db, models::artboard::Snapshot};
use tokio::time::{MissedTickBehavior, interval};

use crate::app::artboard::provenance::{
    ArtboardProvenance, SharedArtboardProvenance, clone_shared_provenance,
};

pub const CANVAS_WIDTH: usize = 384;
pub const CANVAS_HEIGHT: usize = 192;
const DEFAULT_PERSIST_INTERVAL: Duration = Duration::from_secs(5 * 60);
const DAILY_SNAPSHOT_PREFIX: &str = "daily:";
const MONTHLY_SNAPSHOT_PREFIX: &str = "monthly:";
const MAX_DAILY_SNAPSHOTS: usize = 7;
const SYSTEM_ROLLOVER_USER_ID: u64 = 0;
const SYSTEM_ROLLOVER_CLIENT_OP_ID: u64 = 0;

#[derive(Default)]
struct LateShCanvasStore;

impl CanvasStore for LateShCanvasStore {
    fn load(&self) -> Option<Canvas> {
        Some(blank_canvas())
    }

    fn save(&mut self, _canvas: &Canvas) {}
}

#[derive(Default)]
struct PersistState {
    latest_canvas: Option<Canvas>,
    dirty: bool,
}

struct PostgresCanvasStore {
    initial_canvas: Canvas,
    persist_state: Arc<Mutex<PersistState>>,
    persist_notify_tx: mpsc::Sender<()>,
}

impl PostgresCanvasStore {
    fn new(
        db: Db,
        initial_canvas: Option<Canvas>,
        shared_provenance: SharedArtboardProvenance,
        persist_interval: Duration,
    ) -> Self {
        let initial_canvas = initial_canvas.unwrap_or_else(blank_canvas);
        let persist_state = Arc::new(Mutex::new(PersistState::default()));
        let (persist_notify_tx, persist_notify_rx) = mpsc::channel();

        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let thread_state = persist_state.clone();
                let thread_provenance = shared_provenance.clone();
                thread::Builder::new()
                    .name("dartboard-persist".to_string())
                    .spawn(move || {
                        run_persist_loop(
                            db,
                            thread_state,
                            thread_provenance,
                            persist_notify_rx,
                            runtime,
                            persist_interval,
                        )
                    })
                    .expect("failed to spawn dartboard persist loop");
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "dartboard persistence disabled: no tokio runtime available"
                );
            }
        }

        Self {
            initial_canvas,
            persist_state,
            persist_notify_tx,
        }
    }
}

impl CanvasStore for PostgresCanvasStore {
    fn load(&self) -> Option<Canvas> {
        Some(self.initial_canvas.clone())
    }

    fn save(&mut self, canvas: &Canvas) {
        let mut state = self.persist_state.lock_recover();
        state.latest_canvas = Some(canvas.clone());
        if state.dirty {
            return;
        }
        state.dirty = true;
        drop(state);
        let _ = self.persist_notify_tx.send(());
    }
}

pub async fn load_persisted_canvas(db: &Db) -> anyhow::Result<Option<Canvas>> {
    Ok(load_persisted_artboard(db)
        .await?
        .map(|snapshot| snapshot.canvas))
}

pub async fn load_persisted_provenance(db: &Db) -> anyhow::Result<Option<ArtboardProvenance>> {
    Ok(load_persisted_artboard(db)
        .await?
        .map(|snapshot| snapshot.provenance))
}

pub async fn load_persisted_artboard(db: &Db) -> anyhow::Result<Option<PersistedArtboard>> {
    let client = db.get().await.context("failed to get db client")?;
    let Some(snapshot) = Snapshot::find_by_board_key(&client, Snapshot::MAIN_BOARD_KEY)
        .await
        .context("failed to load artboard snapshot row")?
    else {
        return Ok(None);
    };
    let canvas =
        serde_json::from_value(snapshot.canvas).context("failed to decode artboard snapshot")?;
    let provenance = serde_json::from_value(snapshot.provenance)
        .context("failed to decode artboard provenance")?;
    Ok(Some(PersistedArtboard { canvas, provenance }))
}

pub async fn flush_server_snapshot(
    db: &Db,
    server: &ServerHandle,
    shared_provenance: &SharedArtboardProvenance,
) -> anyhow::Result<()> {
    let canvas = server.canvas_snapshot();
    let provenance = clone_shared_provenance(shared_provenance);
    save_canvas_snapshot_for_key(db, Snapshot::MAIN_BOARD_KEY, &canvas, &provenance).await
}

pub async fn run_daily_snapshot_rollover_task(
    db: Db,
    server: ServerHandle,
    shared_provenance: SharedArtboardProvenance,
    shutdown: late_core::shutdown::CancellationToken,
) {
    let mut ticker = interval(Duration::from_secs(30));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut current_day = Utc::now().date_naive();
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = ticker.tick() => {
                let now = Utc::now();
                let today = now.date_naive();
                if today == current_day {
                    continue;
                }
                current_day = today;
                if let Err(error) = rollover_daily_snapshot(&db, &server, &shared_provenance, now).await {
                    tracing::error!(error = ?error, at = %now, "failed to roll over artboard snapshots");
                }
            }
        }
    }
}

pub fn spawn_server() -> ServerHandle {
    ServerHandle::spawn_local_with_color_selection_mode(
        LateShCanvasStore,
        ColorSelectionMode::RandomUnique,
    )
}

pub fn spawn_persistent_server(
    db: Db,
    initial_canvas: Option<Canvas>,
    shared_provenance: SharedArtboardProvenance,
) -> ServerHandle {
    spawn_persistent_server_with_interval(
        db,
        initial_canvas,
        shared_provenance,
        DEFAULT_PERSIST_INTERVAL,
    )
}

pub fn spawn_persistent_server_with_interval(
    db: Db,
    initial_canvas: Option<Canvas>,
    shared_provenance: SharedArtboardProvenance,
    persist_interval: Duration,
) -> ServerHandle {
    ServerHandle::spawn_local_with_color_selection_mode(
        PostgresCanvasStore::new(db, initial_canvas, shared_provenance, persist_interval),
        ColorSelectionMode::RandomUnique,
    )
}

#[derive(Debug, Clone)]
pub struct PersistedArtboard {
    pub canvas: Canvas,
    pub provenance: ArtboardProvenance,
}

fn blank_canvas() -> Canvas {
    Canvas::with_size(CANVAS_WIDTH, CANVAS_HEIGHT)
}

fn daily_board_key(date: NaiveDate) -> String {
    format!("{DAILY_SNAPSHOT_PREFIX}{date}")
}

fn monthly_board_key(date: NaiveDate) -> String {
    format!(
        "{MONTHLY_SNAPSHOT_PREFIX}{:04}-{:02}",
        date.year(),
        date.month()
    )
}

async fn rollover_daily_snapshot(
    db: &Db,
    server: &ServerHandle,
    shared_provenance: &SharedArtboardProvenance,
    now: chrono::DateTime<Utc>,
) -> anyhow::Result<()> {
    let archived_day = now
        .date_naive()
        .pred_opt()
        .context("failed to compute previous UTC date for artboard rollover")?;
    let daily_key = daily_board_key(archived_day);

    {
        let client = db.get().await.context("failed to get db client")?;
        if Snapshot::find_by_board_key(&client, &daily_key)
            .await
            .context("failed to check existing daily artboard snapshot")?
            .is_some()
        {
            return Ok(());
        }
    }

    let canvas = server.canvas_snapshot();
    let provenance = clone_shared_provenance(shared_provenance);
    save_canvas_snapshot_for_key(db, &daily_key, &canvas, &provenance).await?;
    prune_daily_snapshots(db, MAX_DAILY_SNAPSHOTS).await?;
    tracing::info!(board_key = %daily_key, "saved daily artboard snapshot");

    if now.day() == 1 {
        let monthly_key = monthly_board_key(archived_day);
        save_canvas_snapshot_for_key(db, &monthly_key, &canvas, &provenance).await?;
        tracing::info!(board_key = %monthly_key, "saved monthly artboard snapshot");

        let blank = blank_canvas();
        let blank_provenance = ArtboardProvenance::default();
        {
            let mut shared = shared_provenance.lock_recover();
            *shared = blank_provenance.clone();
        }
        server.submit_op_for(
            SYSTEM_ROLLOVER_USER_ID,
            SYSTEM_ROLLOVER_CLIENT_OP_ID,
            CanvasOp::Replace {
                canvas: blank.clone(),
            },
        );
        save_canvas_snapshot_for_key(db, Snapshot::MAIN_BOARD_KEY, &blank, &blank_provenance)
            .await?;
        tracing::info!("blanked live artboard for UTC month rollover");
    }

    Ok(())
}

fn run_persist_loop(
    db: Db,
    persist_state: Arc<Mutex<PersistState>>,
    shared_provenance: SharedArtboardProvenance,
    persist_notify_rx: mpsc::Receiver<()>,
    runtime: tokio::runtime::Handle,
    persist_interval: Duration,
) {
    loop {
        match persist_notify_rx.recv() {
            Ok(()) => {}
            Err(_) => {
                flush_dirty_canvas(&db, &persist_state, &shared_provenance, &runtime);
                return;
            }
        }

        loop {
            let deadline = Instant::now() + persist_interval;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                let timeout = deadline.saturating_duration_since(now);
                match persist_notify_rx.recv_timeout(timeout) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        flush_dirty_canvas(&db, &persist_state, &shared_provenance, &runtime);
                        return;
                    }
                }
            }

            if !flush_dirty_canvas(&db, &persist_state, &shared_provenance, &runtime) {
                break;
            }
        }
    }
}

fn flush_dirty_canvas(
    db: &Db,
    persist_state: &Arc<Mutex<PersistState>>,
    shared_provenance: &SharedArtboardProvenance,
    runtime: &tokio::runtime::Handle,
) -> bool {
    let canvas = {
        let mut state = persist_state.lock_recover();
        if !state.dirty {
            return false;
        }
        state.dirty = false;
        state.latest_canvas.clone()
    };

    let Some(canvas) = canvas else {
        return false;
    };

    let provenance = clone_shared_provenance(shared_provenance);
    if let Err(error) = persist_canvas(runtime, db, &canvas, &provenance) {
        tracing::error!(error = ?error, "failed to persist artboard snapshot");
        let mut state = persist_state.lock_recover();
        state.latest_canvas = Some(canvas);
        state.dirty = true;
        return true;
    }

    tracing::debug!("persisted artboard snapshot");
    persist_state.lock_recover().dirty
}

fn persist_canvas(
    runtime: &tokio::runtime::Handle,
    db: &Db,
    canvas: &Canvas,
    provenance: &ArtboardProvenance,
) -> anyhow::Result<()> {
    runtime.block_on(save_canvas_snapshot(db, canvas, provenance))
}

async fn save_canvas_snapshot(
    db: &Db,
    canvas: &Canvas,
    provenance: &ArtboardProvenance,
) -> anyhow::Result<()> {
    let canvas = serde_json::to_value(canvas).context("failed to serialize artboard canvas")?;
    let provenance =
        serde_json::to_value(provenance).context("failed to serialize artboard provenance")?;
    save_canvas_snapshot_value(db, Snapshot::MAIN_BOARD_KEY, canvas, provenance).await
}

async fn save_canvas_snapshot_for_key(
    db: &Db,
    board_key: &str,
    canvas: &Canvas,
    provenance: &ArtboardProvenance,
) -> anyhow::Result<()> {
    let canvas = serde_json::to_value(canvas).context("failed to serialize artboard canvas")?;
    let provenance =
        serde_json::to_value(provenance).context("failed to serialize artboard provenance")?;
    save_canvas_snapshot_value(db, board_key, canvas, provenance).await
}

async fn save_canvas_snapshot_value(
    db: &Db,
    board_key: &str,
    canvas: serde_json::Value,
    provenance: serde_json::Value,
) -> anyhow::Result<()> {
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard save")?;
    Snapshot::upsert(&client, board_key, canvas, provenance)
        .await
        .context("failed to upsert artboard snapshot")?;
    Ok(())
}

async fn prune_daily_snapshots(db: &Db, keep: usize) -> anyhow::Result<()> {
    let client = db
        .get()
        .await
        .context("failed to get db client for daily artboard prune")?;
    let snapshots = Snapshot::list_by_board_key_prefix(&client, DAILY_SNAPSHOT_PREFIX)
        .await
        .context("failed to list daily artboard snapshots")?;
    for snapshot in snapshots.into_iter().skip(keep) {
        Snapshot::delete_by_board_key(&client, &snapshot.board_key)
            .await
            .with_context(|| {
                format!(
                    "failed to delete old artboard snapshot {}",
                    snapshot.board_key
                )
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{daily_board_key, monthly_board_key};

    #[test]
    fn daily_board_key_uses_iso_date() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 30).expect("valid date");
        assert_eq!(daily_board_key(date), "daily:2026-04-30");
    }

    #[test]
    fn monthly_board_key_uses_year_month() {
        let date = NaiveDate::from_ymd_opt(2026, 4, 30).expect("valid date");
        assert_eq!(monthly_board_key(date), "monthly:2026-04");
    }
}
