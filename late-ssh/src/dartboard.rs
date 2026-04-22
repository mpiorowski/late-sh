use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use dartboard_core::Canvas;
use dartboard_local::{CanvasStore, ServerHandle};
use late_core::{MutexRecover, db::Db, models::artboard::Snapshot};

use crate::app::artboard::provenance::{
    ArtboardProvenance, SharedArtboardProvenance, clone_shared_provenance,
};

pub const CANVAS_WIDTH: usize = 384;
pub const CANVAS_HEIGHT: usize = 192;
const DEFAULT_PERSIST_INTERVAL: Duration = Duration::from_secs(5 * 60);

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
    save_canvas_snapshot(db, &canvas, &provenance).await
}

pub fn spawn_server() -> ServerHandle {
    ServerHandle::spawn_local(LateShCanvasStore)
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
    ServerHandle::spawn_local(PostgresCanvasStore::new(
        db,
        initial_canvas,
        shared_provenance,
        persist_interval,
    ))
}

#[derive(Debug, Clone)]
pub struct PersistedArtboard {
    pub canvas: Canvas,
    pub provenance: ArtboardProvenance,
}

fn blank_canvas() -> Canvas {
    Canvas::with_size(CANVAS_WIDTH, CANVAS_HEIGHT)
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
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard save")?;
    Snapshot::upsert(&client, Snapshot::MAIN_BOARD_KEY, canvas, provenance)
        .await
        .context("failed to upsert artboard snapshot")?;
    Ok(())
}
