use anyhow::Context;
use late_core::db::Db;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::warn;
use uuid::Uuid;

use super::data::{CanvasData, PinstarOp, PinstarPeer, ServerMsg};

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

    pub fn submit_op(&self, client_seq: u64, op: PinstarOp) {
        let _ = self.command_tx.send(Command::SubmitOp { client_seq, op });
    }

    /// Create a PinstarService connected to a running PinstarServerHandle.
    pub fn new(server: &PinstarServerHandle, user_id: Uuid, username: &str, role: String) -> Self {
        let username = username.to_string();
        let initial = PinstarSnapshot {
            diagram_id: server.diagram_id(),
            title: server.title().to_string(),
            your_role: role,
            your_user_id: Some(user_id),
            ..Default::default()
        };
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let (event_tx, _) = broadcast::channel(128);
        let (command_tx, command_rx) = mpsc::channel();

        let server_inner = server.inner.clone();
        let thread_event_tx = event_tx.clone();
        let thread_snapshot_tx = snapshot_tx;

        std::thread::Builder::new()
            .name(format!("pinstar-{}", user_id))
            .spawn(move || {
                run_client_loop(
                    server_inner,
                    user_id,
                    username,
                    command_rx,
                    thread_snapshot_tx,
                    thread_event_tx,
                );
            })
            .expect("failed to spawn pinstar client loop");

        Self {
            diagram_id: server.diagram_id(),
            command_tx,
            snapshot_rx,
            event_tx,
        }
    }

    #[cfg(test)]
    pub(crate) fn disconnected_for_tests(initial_snapshot: PinstarSnapshot) -> Self {
        let diagram_id = initial_snapshot.diagram_id;
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        let (event_tx, _) = broadcast::channel(128);
        let (command_tx, command_rx) = mpsc::channel();
        drop(snapshot_tx);
        drop(command_rx);
        Self {
            diagram_id,
            command_tx,
            snapshot_rx,
            event_tx,
        }
    }
}

fn run_client_loop(
    server_inner: std::sync::Arc<std::sync::Mutex<ServerInner>>,
    user_id: Uuid,
    username: String,
    command_rx: mpsc::Receiver<Command>,
    snapshot_tx: watch::Sender<PinstarSnapshot>,
    event_tx: broadcast::Sender<PinstarEvent>,
) {
    // Send Hello and get initial snapshot
    let (mut broadcast_rx, initial_data, initial_peers, role, diagram_id, title) = {
        let mut inner = server_inner.lock().unwrap();
        let broadcast_rx = inner.broadcast_tx.subscribe();
        let (data, peers, role) = inner.add_client(user_id, username.clone());
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
        ..Default::default()
    });

    let _client_seq: u64 = 0;

    loop {
        match command_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::SubmitOp {
                client_seq: seq,
                op,
            }) => {
                let server_seq = {
                    let mut inner = server_inner.lock().unwrap();
                    inner.apply_op(user_id, op.clone())
                };
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
    {
        let mut inner = server_inner.lock().unwrap();
        inner.remove_client(user_id);
        inner.broadcast(ServerMsg::PeerLeft { user_id });
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
                let _ = snapshot_tx.send_modify(|snap| {
                    op.apply(&mut snap.data);
                    snap.last_seq = snap.last_seq.max(server_seq);
                });
            }
            ServerMsg::PeerJoined { peer } => {
                if peer.user_id == user_id {
                    continue;
                }
                let _ = snapshot_tx.send_modify(|snap| {
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
                let _ = snapshot_tx.send_modify(|snap| {
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
}

struct ServerInner {
    diagram_id: Uuid,
    title: String,
    data: CanvasData,
    dirty: bool,
    seq: u64,
    clients: HashMap<Uuid, ClientEntry>,
    broadcast_tx: broadcast::Sender<ServerMsg>,
}

impl ServerInner {
    fn add_client(
        &mut self,
        user_id: Uuid,
        username: String,
    ) -> (CanvasData, Vec<PinstarPeer>, String) {
        let role = "editor".to_string(); // TODO: look up actual role
        self.clients.insert(user_id, ClientEntry { username });
        let peers = self.peers_list();
        (self.data.clone(), peers, role)
    }

    fn remove_client(&mut self, user_id: Uuid) {
        self.clients.remove(&user_id);
    }

    fn apply_op(&mut self, from: Uuid, op: PinstarOp) -> u64 {
        op.apply(&mut self.data);
        self.dirty = true;
        self.seq += 1;
        let seq = self.seq;
        self.broadcast(ServerMsg::OpBroadcast {
            from,
            op,
            server_seq: seq,
        });
        seq
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

// ── PinstarServerHandle (shared handle to one diagram server) ──────────────

pub struct PinstarServerHandle {
    diagram_id: Uuid,
    inner: std::sync::Arc<std::sync::Mutex<ServerInner>>,
    db: Option<Db>,
}

impl PinstarServerHandle {
    pub fn diagram_id(&self) -> Uuid {
        self.diagram_id
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
        let inner = self.inner.lock().unwrap();
        if !inner.dirty {
            return Ok(());
        }
        let diagram_data = serde_json::to_value(&inner.data)?;
        let id = inner.diagram_id;
        drop(inner); // release lock before DB call

        let client = db.get().await.context("db client for pinstar flush")?;
        late_core::models::pinstar_diagram::PinstarDiagram::update_data(&client, id, diagram_data)
            .await?;

        let mut inner = self.inner.lock().unwrap();
        inner.dirty = false;
        Ok(())
    }
}

impl Drop for PinstarServerHandle {
    fn drop(&mut self) {
        // Try to flush on drop (best-effort, synchronous context)
        if let Some(db) = self.db.take() {
            let inner = self.inner.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
                let data = {
                    let inner = inner.lock().unwrap();
                    if !inner.dirty {
                        return Some(());
                    }
                    serde_json::to_value(&inner.data).ok()?
                };
                let id = {
                    let inner = inner.lock().unwrap();
                    inner.diagram_id
                };
                rt.block_on(async {
                    if let Ok(client) = db.get().await {
                        let _ = late_core::models::pinstar_diagram::PinstarDiagram::update_data(
                            &client, id, data,
                        )
                        .await;
                    }
                });
                Some(())
            });
        }
    }
}

// ── PinstarServerRegistry (process-wide) ───────────────────────────────────

#[derive(Clone)]
pub struct PinstarServerRegistry {
    servers: std::sync::Arc<std::sync::Mutex<HashMap<Uuid, PinstarServerHandle>>>,
    db: Option<Db>,
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

    /// Get or create a server handle for a diagram. Loads from DB if needed.
    pub async fn create_new_diagram(&self, owner_id: Uuid, title: String) -> anyhow::Result<Uuid> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No DB configured"))?;
        let client = db.get().await?;

        let diagram_id = Uuid::new_v4();
        let diagram_data = serde_json::to_value(crate::app::pinstar::data::CanvasData::default())?;

        client.execute(
            "INSERT INTO pinstar_diagrams (id, owner_id, title, diagram_data, format) VALUES ($1, $2, $3, $4, $5)",
            &[&diagram_id, &owner_id, &title, &diagram_data, &"canvas"]
        ).await?;

        Ok(diagram_id)
    }

    pub async fn get_or_create(&self, diagram_id: Uuid) -> anyhow::Result<PinstarServerHandle> {
        // Fast path: already in memory
        {
            let servers = self.servers.lock().unwrap();
            if let Some(handle) = servers.get(&diagram_id) {
                return Ok(handle.clone());
            }
        }

        // Slow path: load from DB
        let data = self.load_diagram(diagram_id).await?;
        let handle = self.create_server(diagram_id, data);

        let mut servers = self.servers.lock().unwrap();
        // Another thread may have inserted first
        if let Some(existing) = servers.get(&diagram_id) {
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
            anyhow::bail!("Database not available");
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

        let handle = self.create_server(diagram.id, data);
        let mut servers = self.servers.lock().unwrap();
        servers.insert(diagram.id, handle.clone());
        Ok(handle)
    }

    fn create_server(&self, diagram_id: Uuid, data: CanvasData) -> PinstarServerHandle {
        let (broadcast_tx, _) = broadcast::channel(256);
        let inner = ServerInner {
            diagram_id,
            title: String::new(),
            data,
            dirty: false,
            seq: 0,
            clients: HashMap::new(),
            broadcast_tx,
        };
        PinstarServerHandle {
            diagram_id,
            inner: std::sync::Arc::new(std::sync::Mutex::new(inner)),
            db: self.db.clone(),
        }
    }

    async fn load_diagram(&self, diagram_id: Uuid) -> anyhow::Result<CanvasData> {
        let Some(db) = &self.db else {
            anyhow::bail!("Database not available");
        };
        let client = db.get().await.context("db client for load diagram")?;
        let diagram = late_core::models::pinstar_diagram::PinstarDiagram::get(&client, diagram_id)
            .await?
            .context("Diagram not found")?;
        let data: CanvasData = serde_json::from_value(diagram.diagram_data)?;
        Ok(data)
    }

    /// Flush all dirty servers to DB.
    pub async fn flush_all(&self) {
        let handles: Vec<PinstarServerHandle> = {
            let servers = self.servers.lock().unwrap();
            servers.values().cloned().collect()
        };
        for handle in handles {
            if let Err(e) = handle.flush().await {
                warn!(diagram_id = %handle.diagram_id(), "failed to flush pinstar diagram: {e:#}");
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
        let server = registry.create_server(Uuid::new_v4(), CanvasData::default());

        let alice_id = Uuid::new_v4();
        let bob_id = Uuid::new_v4();
        let cara_id = Uuid::new_v4();

        let alice = PinstarService::new(&server, alice_id, "alice", "editor".to_string());
        let bob = PinstarService::new(&server, bob_id, "bob", "editor".to_string());
        let cara = PinstarService::new(&server, cara_id, "cara", "editor".to_string());

        assert!(wait_until(|| {
            alice.snapshot().your_user_id == Some(alice_id)
                && bob.snapshot().your_user_id == Some(bob_id)
                && cara.snapshot().your_user_id == Some(cara_id)
        }));

        alice.submit_op(1, PinstarOp::AddNode(text_node("shared-node")));

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
