use anyhow::Context;
use late_core::{db::Db, shutdown::CancellationToken};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};
use tracing::warn;
use uuid::Uuid;

use super::data::{CanvasData, DiagramLockMode, PinstarOp, PinstarPeer, ServerMsg};

const DEFAULT_PERSIST_INTERVAL: Duration = Duration::from_secs(5 * 60);
const DEFAULT_IDLE_EVICT_AFTER: Duration = Duration::from_secs(30 * 60);
const MAX_CACHED_SERVERS: usize = 128;

// ── PinstarSnapshot (sent over watch channel) ──────────────────────────────

#[derive(Debug, Clone)]
pub struct PinstarSnapshot {
    pub diagram_id: Uuid,
    pub data: CanvasData,
    pub peers: Vec<PinstarPeer>,
    pub your_role: String,
    pub your_user_id: Option<Uuid>,
    pub last_seq: u64,
    pub title: String,
    pub connect_rejected: Option<String>,
}

impl Default for PinstarSnapshot {
    fn default() -> Self {
        Self {
            diagram_id: Uuid::nil(),
            data: CanvasData {
                nodes: Vec::new(),
                edges: Vec::new(),
                orientation: Default::default(),
                lock_mode: Default::default(),
                locked: false,
            },
            peers: Vec::new(),
            your_role: String::new(),
            your_user_id: None,
            last_seq: 0,
            title: String::new(),
            connect_rejected: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PinstarEvent {
    Ack { client_seq: u64, server_seq: u64 },
    PeerJoined { peer: PinstarPeer },
    PeerLeft { user_id: Uuid },
    ConnectRejected { reason: String },
}

// ── Command (from UI → client thread) ──────────────────────────────────────

enum Command {
    SubmitOp { client_seq: u64, op: PinstarOp },
}

// ── PinstarService (per-session bridge) ────────────────────────────────────

#[derive(Clone)]
pub struct PinstarService {
    diagram_id: Uuid,
    command_tx: mpsc::Sender<Command>,
    snapshot_rx: watch::Receiver<PinstarSnapshot>,
    event_tx: broadcast::Sender<PinstarEvent>,
    next_client_seq: Arc<AtomicU64>,
}

impl PinstarService {
    pub fn diagram_id(&self) -> Uuid {
        self.diagram_id
    }

    pub fn snapshot(&self) -> PinstarSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<PinstarEvent> {
        self.event_tx.subscribe()
    }

    pub fn subscribe_state(&self) -> watch::Receiver<PinstarSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn submit_op(&self, op: PinstarOp) {
        let client_seq = self.next_client_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.command_tx.send(Command::SubmitOp { client_seq, op });
    }

    /// Create a PinstarService connected to a running PinstarServerHandle.
    pub fn new(server: &PinstarServerHandle, user_id: Uuid, username: &str, role: String) -> Self {
        let username = username.to_string();
        let (diagram_id, title, data, peers, last_seq) = {
            let inner = server.inner.lock().unwrap();
            (
                inner.diagram_id,
                inner.title.clone(),
                inner.data.clone(),
                inner.peers_list(),
                inner.seq,
            )
        };
        let initial = PinstarSnapshot {
            diagram_id,
            title,
            data,
            peers,
            your_role: role.clone(),
            your_user_id: Some(user_id),
            last_seq,
            ..Default::default()
        };
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let (event_tx, _) = broadcast::channel(128);
        let (command_tx, command_rx) = mpsc::channel();

        let server_inner = server.inner.clone();
        let thread_event_tx = event_tx.clone();
        let thread_snapshot_tx = snapshot_tx;
        let disconnect_flush = tokio::runtime::Handle::try_current()
            .ok()
            .map(|runtime| (server.clone(), runtime));
        let access_check = tokio::runtime::Handle::try_current()
            .ok()
            .zip(server.db.clone())
            .map(|(runtime, db)| AccessCheck {
                runtime,
                db,
                diagram_id,
            });

        std::thread::Builder::new()
            .name(format!("pinstar-{}", user_id))
            .spawn(move || {
                run_client_loop(ClientLoopArgs {
                    server_inner,
                    user_id,
                    username,
                    role,
                    command_rx,
                    snapshot_tx: thread_snapshot_tx,
                    event_tx: thread_event_tx,
                    disconnect_flush,
                    access_check,
                });
            })
            .expect("failed to spawn pinstar client loop");

        Self {
            diagram_id: server.diagram_id(),
            command_tx,
            snapshot_rx,
            event_tx,
            next_client_seq: Arc::new(AtomicU64::new(1)),
        }
    }
}

struct AccessCheck {
    runtime: tokio::runtime::Handle,
    db: Db,
    diagram_id: Uuid,
}

struct ClientLoopArgs {
    server_inner: std::sync::Arc<std::sync::Mutex<ServerInner>>,
    user_id: Uuid,
    username: String,
    role: String,
    command_rx: mpsc::Receiver<Command>,
    snapshot_tx: watch::Sender<PinstarSnapshot>,
    event_tx: broadcast::Sender<PinstarEvent>,
    disconnect_flush: Option<(PinstarServerHandle, tokio::runtime::Handle)>,
    access_check: Option<AccessCheck>,
}

fn run_client_loop(args: ClientLoopArgs) {
    let ClientLoopArgs {
        server_inner,
        user_id,
        username,
        role,
        command_rx,
        snapshot_tx,
        event_tx,
        disconnect_flush,
        access_check,
    } = args;
    // Send Hello and get initial snapshot
    let (mut broadcast_rx, initial_data, initial_peers, role, diagram_id, title, last_seq) = {
        let mut inner = server_inner.lock().unwrap();
        let broadcast_rx = inner.broadcast_tx.subscribe();
        let (data, peers, role) = inner.add_client(user_id, username.clone(), role);
        // Broadcast PeerJoined
        inner.broadcast(ServerMsg::PeerJoined {
            peer: PinstarPeer {
                user_id,
                username: username.clone(),
            },
        });
        (
            broadcast_rx,
            data,
            peers,
            role,
            inner.diagram_id,
            inner.title.clone(),
            inner.seq,
        )
    };

    // Send Welcome
    let _ = snapshot_tx.send(PinstarSnapshot {
        diagram_id,
        title,
        data: initial_data,
        peers: initial_peers,
        your_role: role,
        your_user_id: Some(user_id),
        last_seq,
        ..Default::default()
    });

    loop {
        match command_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::SubmitOp {
                client_seq: seq,
                op,
            }) => {
                if let Some(role) = refresh_member_role(&access_check, user_id) {
                    let mut inner = server_inner.lock().unwrap();
                    inner.update_client_role(user_id, role);
                } else if access_check.is_some() {
                    let _ = event_tx.send(PinstarEvent::ConnectRejected {
                        reason: "Read-only diagram".to_string(),
                    });
                    continue;
                }

                let server_seq = {
                    let mut inner = server_inner.lock().unwrap();
                    inner.apply_op(user_id, op.clone())
                };
                if let Some(server_seq) = server_seq {
                    let _ = event_tx.send(PinstarEvent::Ack {
                        client_seq: seq,
                        server_seq,
                    });
                    // Update snapshot
                    snapshot_tx.send_modify(|snap| {
                        snap.last_seq = snap.last_seq.max(server_seq);
                        let inner = server_inner.lock().unwrap();
                        snap.data = inner.data.clone();
                    });
                } else {
                    let _ = event_tx.send(PinstarEvent::ConnectRejected {
                        reason: "Read-only diagram".to_string(),
                    });
                }
                drain_broadcasts(
                    &server_inner,
                    &mut broadcast_rx,
                    &snapshot_tx,
                    &event_tx,
                    user_id,
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Drain any broadcasts from other clients
                drain_broadcasts(
                    &server_inner,
                    &mut broadcast_rx,
                    &snapshot_tx,
                    &event_tx,
                    user_id,
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Cleanup: remove client, broadcast PeerLeft
    let should_flush = {
        let mut inner = server_inner.lock().unwrap();
        inner.remove_client(user_id);
        let should_flush = inner.clients.is_empty() && inner.dirty;
        inner.broadcast(ServerMsg::PeerLeft { user_id });
        should_flush
    };
    if should_flush && let Some((handle, runtime)) = disconnect_flush {
        runtime.spawn(async move {
            if let Err(error) = handle.flush().await {
                warn!(error = ?error, "failed to flush pinstar diagram after last client disconnected");
            }
        });
    }
}

fn refresh_member_role(access_check: &Option<AccessCheck>, user_id: Uuid) -> Option<String> {
    let Some(access_check) = access_check else {
        return Some(String::new());
    };

    let result = access_check.runtime.block_on(async {
        let client = access_check.db.get().await?;
        late_core::models::pinstar_diagram::PinstarDiagram::get_with_member_role(
            &client,
            access_check.diagram_id,
            user_id,
        )
        .await
    });

    match result {
        Ok(Some((_, role))) => Some(role),
        Ok(None) => None,
        Err(error) => {
            warn!(error = ?error, "failed to refresh pinstar member role");
            None
        }
    }
}

fn drain_broadcasts(
    server_inner: &std::sync::Arc<std::sync::Mutex<ServerInner>>,
    broadcast_rx: &mut broadcast::Receiver<ServerMsg>,
    snapshot_tx: &watch::Sender<PinstarSnapshot>,
    event_tx: &broadcast::Sender<PinstarEvent>,
    user_id: Uuid,
) {
    loop {
        let msg = match broadcast_rx.try_recv() {
            Ok(msg) => msg,
            Err(broadcast::error::TryRecvError::Empty)
            | Err(broadcast::error::TryRecvError::Closed) => break,
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                warn!(
                    user_id = %user_id,
                    skipped,
                    "pinstar client lagged behind broadcast channel; resyncing snapshot"
                );
                let (data, peers, seq) = {
                    let inner = server_inner.lock().unwrap();
                    (inner.data.clone(), inner.peers_list(), inner.seq)
                };
                snapshot_tx.send_modify(|snap| {
                    snap.data = data;
                    snap.peers = peers;
                    snap.last_seq = snap.last_seq.max(seq);
                });
                continue;
            }
        };

        match msg {
            ServerMsg::OpBroadcast {
                from,
                op,
                server_seq,
            } => {
                if from == user_id {
                    continue;
                }
                snapshot_tx.send_modify(|snap| {
                    op.apply(&mut snap.data);
                    snap.last_seq = snap.last_seq.max(server_seq);
                });
            }
            ServerMsg::PeerJoined { peer } => {
                if peer.user_id == user_id {
                    continue;
                }
                snapshot_tx.send_modify(|snap| {
                    if !snap
                        .peers
                        .iter()
                        .any(|existing| existing.user_id == peer.user_id)
                    {
                        snap.peers.push(peer.clone());
                    }
                });
                let _ = event_tx.send(PinstarEvent::PeerJoined { peer });
            }
            ServerMsg::PeerLeft {
                user_id: left_user_id,
            } => {
                if left_user_id == user_id {
                    continue;
                }
                snapshot_tx.send_modify(|snap| {
                    snap.peers.retain(|p| p.user_id != left_user_id);
                });
                let _ = event_tx.send(PinstarEvent::PeerLeft {
                    user_id: left_user_id,
                });
            }
            _ => {}
        }
    }
}

// ── ServerInner (authoritative state for one diagram) ──────────────────────

struct ClientEntry {
    username: String,
    role: String,
}

struct ServerInner {
    diagram_id: Uuid,
    title: String,
    data: CanvasData,
    db_updated: Option<chrono::DateTime<chrono::Utc>>,
    dirty: bool,
    version: u64,
    seq: u64,
    clients: HashMap<Uuid, ClientEntry>,
    broadcast_tx: broadcast::Sender<ServerMsg>,
    last_accessed: Instant,
}

impl ServerInner {
    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    fn add_client(
        &mut self,
        user_id: Uuid,
        username: String,
        role: String,
    ) -> (CanvasData, Vec<PinstarPeer>, String) {
        self.touch();
        let role = valid_role(&role).unwrap_or("viewer").to_string();
        self.clients.insert(
            user_id,
            ClientEntry {
                username,
                role: role.clone(),
            },
        );
        let peers = self.peers_list();
        (self.data.clone(), peers, role)
    }

    fn remove_client(&mut self, user_id: Uuid) {
        self.touch();
        self.clients.remove(&user_id);
    }

    fn update_client_role(&mut self, user_id: Uuid, role: String) {
        self.touch();
        let Some(role) = valid_role(&role) else {
            return;
        };
        if let Some(entry) = self.clients.get_mut(&user_id) {
            entry.role = role.to_string();
        }
    }

    fn apply_op(&mut self, from: Uuid, op: PinstarOp) -> Option<u64> {
        self.touch();
        let role = self
            .clients
            .get(&from)
            .and_then(|entry| valid_role(&entry.role))?;
        if !self.can_apply(role, &op) {
            return None;
        }
        op.apply(&mut self.data);
        self.dirty = true;
        self.version = self.version.wrapping_add(1);
        self.seq += 1;
        let seq = self.seq;
        self.broadcast(ServerMsg::OpBroadcast {
            from,
            op,
            server_seq: seq,
        });
        Some(seq)
    }

    fn can_apply(&self, role: &str, op: &PinstarOp) -> bool {
        match op {
            PinstarOp::SetLockMode(_) => role == "owner",
            PinstarOp::ReplaceAll(_) => role == "owner" && self.lock_mode() != DiagramLockMode::All,
            _ => match self.lock_mode() {
                DiagramLockMode::Unlocked => matches!(role, "owner" | "editor"),
                DiagramLockMode::All => false,
                DiagramLockMode::EditorOnly => role == "owner",
            },
        }
    }

    fn lock_mode(&self) -> DiagramLockMode {
        if self.data.lock_mode == DiagramLockMode::Unlocked && self.data.locked {
            DiagramLockMode::All
        } else {
            self.data.lock_mode
        }
    }

    fn broadcast(&self, msg: ServerMsg) {
        let _ = self.broadcast_tx.send(msg);
    }

    fn peers_list(&self) -> Vec<PinstarPeer> {
        self.clients
            .iter()
            .map(|(id, entry)| PinstarPeer {
                user_id: *id,
                username: entry.username.clone(),
            })
            .collect()
    }
}

fn valid_role(role: &str) -> Option<&'static str> {
    match role {
        "owner" => Some("owner"),
        "editor" => Some("editor"),
        "viewer" => Some("viewer"),
        _ => None,
    }
}

// ── PinstarServerHandle (shared handle to one diagram server) ──────────────

pub struct PinstarServerHandle {
    diagram_id: Uuid,
    inner: std::sync::Arc<std::sync::Mutex<ServerInner>>,
    flush_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    db: Option<Db>,
}

impl PinstarServerHandle {
    pub fn diagram_id(&self) -> Uuid {
        self.diagram_id
    }

    fn touch(&self) {
        self.inner.lock().unwrap().touch();
    }

    fn idle_info(&self, now: Instant) -> Option<(Uuid, Duration)> {
        let inner = self.inner.lock().unwrap();
        if !inner.clients.is_empty() || inner.dirty {
            return None;
        }
        Some((
            inner.diagram_id,
            now.saturating_duration_since(inner.last_accessed),
        ))
    }

    pub fn title(&self) -> String {
        let inner = self.inner.lock().unwrap();
        inner.title.clone()
    }

    pub fn client_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.clients.len()
    }

    /// Flush dirty state to DB.
    pub async fn flush(&self) -> anyhow::Result<()> {
        let Some(db) = &self.db else { return Ok(()) };
        let _flush_guard = self.flush_lock.lock().await;
        let (id, data, version, expected_updated) = {
            let inner = self.inner.lock().unwrap();
            if !inner.dirty {
                return Ok(());
            }
            (
                inner.diagram_id,
                inner.data.clone(),
                inner.version,
                inner.db_updated,
            )
        };
        let Some(expected_updated) = expected_updated else {
            anyhow::bail!("pinstar diagram missing db updated timestamp");
        };
        let diagram_data = serde_json::to_value(data)?;

        let client = db.get().await.context("db client for pinstar flush")?;
        let updated =
            match late_core::models::pinstar_diagram::PinstarDiagram::update_data_if_updated(
                &client,
                id,
                diagram_data,
                expected_updated,
            )
            .await
            {
                Ok(Some(diagram)) => diagram.updated,
                Ok(None) => {
                    let mut inner = self.inner.lock().unwrap();
                    inner.dirty = true;
                    anyhow::bail!("pinstar diagram changed in database before flush");
                }
                Err(error) => {
                    let mut inner = self.inner.lock().unwrap();
                    inner.dirty = true;
                    return Err(error);
                }
            };

        let mut inner = self.inner.lock().unwrap();
        inner.db_updated = Some(updated);
        inner.dirty = inner.version != version;
        Ok(())
    }
}

// ── PinstarServerRegistry (process-wide) ───────────────────────────────────

#[derive(Clone)]
pub struct PinstarServerRegistry {
    servers: std::sync::Arc<std::sync::Mutex<HashMap<Uuid, PinstarServerHandle>>>,
    db: Option<Db>,
}

struct LoadedDiagram {
    title: String,
    data: CanvasData,
    updated: chrono::DateTime<chrono::Utc>,
}

impl PinstarServerRegistry {
    pub fn new(db: Option<Db>) -> Self {
        Self {
            servers: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            db,
        }
    }

    pub fn db(&self) -> Option<Db> {
        self.db.clone()
    }

    pub async fn run_persist_task(self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(DEFAULT_PERSIST_INTERVAL);
        interval.tick().await; // skip immediate first tick
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    if let Err(error) = self.flush_all().await {
                        warn!(error = ?error, "failed to flush pinstar diagrams during shutdown");
                    }
                    self.evict_idle();
                    break;
                }
                _ = interval.tick() => {
                    if let Err(error) = self.flush_all().await {
                        warn!(error = ?error, "failed to persist pinstar diagrams");
                    }
                    self.evict_idle();
                }
            }
        }
    }

    /// Get or create a server handle for a diagram. Loads from DB if needed.
    pub async fn create_new_diagram(&self, owner_id: Uuid, title: String) -> anyhow::Result<Uuid> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no db configured"))?;
        let client = db.get().await.context("db client for create diagram")?;
        let data = CanvasData::default();
        let title = if title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            title
        };
        let diagram = late_core::models::pinstar_diagram::PinstarDiagram::create(
            &client,
            late_core::models::pinstar_diagram::PinstarDiagramParams {
                owner_id,
                title,
                diagram_data: serde_json::to_value(data)?,
                format: "canvas".to_string(),
            },
        )
        .await?;

        Ok(diagram.id)
    }

    pub async fn import_diagram(
        &self,
        owner_id: Uuid,
        title: String,
        data: CanvasData,
    ) -> anyhow::Result<Uuid> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no db configured"))?;
        let client = db.get().await.context("db client for import diagram")?;
        let diagram = late_core::models::pinstar_diagram::PinstarDiagram::create(
            &client,
            late_core::models::pinstar_diagram::PinstarDiagramParams {
                owner_id,
                title,
                diagram_data: serde_json::to_value(data)?,
                format: "canvas".to_string(),
            },
        )
        .await?;

        Ok(diagram.id)
    }

    pub async fn get_or_create(&self, diagram_id: Uuid) -> anyhow::Result<PinstarServerHandle> {
        // Fast path: already in memory
        {
            let servers = self.servers.lock().unwrap();
            if let Some(handle) = servers.get(&diagram_id) {
                handle.touch();
                return Ok(handle.clone());
            }
        }

        // Slow path: load from DB
        let loaded = self.load_diagram(diagram_id).await?;
        let handle =
            self.create_server(diagram_id, loaded.title, loaded.data, Some(loaded.updated));

        let mut servers = self.servers.lock().unwrap();
        // Another thread may have inserted first
        if let Some(existing) = servers.get(&diagram_id) {
            existing.touch();
            return Ok(existing.clone());
        }
        servers.insert(diagram_id, handle.clone());
        Ok(handle)
    }

    /// Create a new blank diagram in DB and return a server handle.
    pub async fn create_diagram(
        &self,
        owner_id: Uuid,
        title: String,
    ) -> anyhow::Result<PinstarServerHandle> {
        let Some(db) = &self.db else {
            anyhow::bail!("database not available");
        };
        let client = db.get().await.context("db client for create diagram")?;
        let data = CanvasData {
            nodes: Vec::new(),
            edges: Vec::new(),
            orientation: Default::default(),
            lock_mode: Default::default(),
            locked: false,
        };
        let diagram_data = serde_json::to_value(&data)?;
        let diagram = late_core::models::pinstar_diagram::PinstarDiagram::create(
            &client,
            late_core::models::pinstar_diagram::PinstarDiagramParams {
                owner_id,
                title: title.clone(),
                diagram_data,
                format: "canvas".to_string(),
            },
        )
        .await?;

        let handle = self.create_server(diagram.id, title, data, Some(diagram.updated));
        let mut servers = self.servers.lock().unwrap();
        servers.insert(diagram.id, handle.clone());
        Ok(handle)
    }

    fn create_server(
        &self,
        diagram_id: Uuid,
        title: String,
        data: CanvasData,
        db_updated: Option<chrono::DateTime<chrono::Utc>>,
    ) -> PinstarServerHandle {
        let (broadcast_tx, _) = broadcast::channel(256);
        let inner = ServerInner {
            diagram_id,
            title,
            data,
            db_updated,
            dirty: false,
            version: 0,
            seq: 0,
            clients: HashMap::new(),
            broadcast_tx,
            last_accessed: Instant::now(),
        };
        PinstarServerHandle {
            diagram_id,
            inner: std::sync::Arc::new(std::sync::Mutex::new(inner)),
            flush_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            db: self.db.clone(),
        }
    }

    async fn load_diagram(&self, diagram_id: Uuid) -> anyhow::Result<LoadedDiagram> {
        let Some(db) = &self.db else {
            anyhow::bail!("database not available");
        };
        let client = db.get().await.context("db client for load diagram")?;
        let diagram = late_core::models::pinstar_diagram::PinstarDiagram::get(&client, diagram_id)
            .await?
            .context("diagram not found")?;
        let data: CanvasData = serde_json::from_value(diagram.diagram_data)?;
        Ok(LoadedDiagram {
            title: diagram.title,
            data,
            updated: diagram.updated,
        })
    }

    /// Flush all dirty servers to DB.
    pub async fn flush_all(&self) -> anyhow::Result<()> {
        let handles: Vec<PinstarServerHandle> = {
            let servers = self.servers.lock().unwrap();
            servers.values().cloned().collect()
        };
        let mut first_error = None;
        for handle in handles {
            if let Err(e) = handle.flush().await {
                warn!(diagram_id = %handle.diagram_id(), "failed to flush pinstar diagram: {e:#}");
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    pub fn evict(&self, diagram_id: Uuid) {
        self.servers.lock().unwrap().remove(&diagram_id);
    }

    pub fn evict_idle(&self) {
        let now = Instant::now();
        let mut idle: Vec<(Uuid, Duration)> = {
            let servers = self.servers.lock().unwrap();
            servers
                .values()
                .filter_map(|handle| handle.idle_info(now))
                .collect()
        };
        idle.sort_by(|(_, a), (_, b)| b.cmp(a));

        let mut servers = self.servers.lock().unwrap();
        for (diagram_id, idle_for) in &idle {
            let still_idle = servers
                .get(diagram_id)
                .and_then(|handle| handle.idle_info(now))
                .is_some_and(|(_, current_idle)| current_idle >= DEFAULT_IDLE_EVICT_AFTER);
            if *idle_for >= DEFAULT_IDLE_EVICT_AFTER && still_idle {
                servers.remove(diagram_id);
            }
        }

        if servers.len() <= MAX_CACHED_SERVERS {
            return;
        }
        for (diagram_id, _) in idle {
            if servers.len() <= MAX_CACHED_SERVERS {
                break;
            }
            let still_idle = servers
                .get(&diagram_id)
                .and_then(|handle| handle.idle_info(now))
                .is_some();
            if still_idle {
                servers.remove(&diagram_id);
            }
        }
    }

    pub fn server_count(&self) -> usize {
        self.servers.lock().unwrap().len()
    }
}

// PinstarServerHandle needs Clone for the registry
impl Clone for PinstarServerHandle {
    fn clone(&self) -> Self {
        Self {
            diagram_id: self.diagram_id,
            inner: self.inner.clone(),
            flush_lock: self.flush_lock.clone(),
            db: self.db.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pinstar::data::{CanvasNode, TextNode};
    use std::time::Instant;

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    fn text_node(id: &str) -> CanvasNode {
        CanvasNode::Text(TextNode {
            id: id.to_string(),
            x: 10.0,
            y: 20.0,
            width: 120.0,
            height: 60.0,
            text: "node".to_string(),
            color: None,
        })
    }

    #[test]
    fn broadcasts_ops_to_every_connected_client() {
        let registry = PinstarServerRegistry::new(None);
        let server = registry.create_server(
            Uuid::now_v7(),
            "test".to_string(),
            CanvasData::default(),
            None,
        );

        let alice_id = Uuid::now_v7();
        let bob_id = Uuid::now_v7();
        let cara_id = Uuid::now_v7();

        let alice = PinstarService::new(&server, alice_id, "alice", "editor".to_string());
        let bob = PinstarService::new(&server, bob_id, "bob", "editor".to_string());
        let cara = PinstarService::new(&server, cara_id, "cara", "editor".to_string());

        assert!(wait_until(|| {
            alice.snapshot().your_user_id == Some(alice_id)
                && bob.snapshot().your_user_id == Some(bob_id)
                && cara.snapshot().your_user_id == Some(cara_id)
        }));

        alice.submit_op(PinstarOp::AddNode(text_node("shared-node")));

        assert!(wait_until(|| {
            bob.snapshot()
                .data
                .nodes
                .iter()
                .any(|node| node.id() == "shared-node")
                && cara
                    .snapshot()
                    .data
                    .nodes
                    .iter()
                    .any(|node| node.id() == "shared-node")
        }));
    }
}
