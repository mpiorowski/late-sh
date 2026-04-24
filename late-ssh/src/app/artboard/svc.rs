use std::{sync::mpsc, thread, time::Duration};

use anyhow::Context;
use dartboard_core::{
    Canvas, CanvasOp, Client, ClientOpId, Peer, RgbColor, Seq, ServerMsg, UserId,
};
use dartboard_local::{ConnectOutcome, Hello, LocalClient, ServerHandle};
use late_core::{db::Db, models::artboard::Snapshot};
use tokio::sync::{broadcast, mpsc as tokio_mpsc, watch};
use uuid::Uuid;

use super::provenance::{
    ArtboardProvenance, SharedArtboardProvenance, apply_shared_op, clone_shared_provenance,
};

#[derive(Debug, Clone, Default)]
pub struct DartboardSnapshot {
    pub canvas: Canvas,
    pub provenance: ArtboardProvenance,
    pub peers: Vec<Peer>,
    pub your_name: String,
    pub your_user_id: Option<UserId>,
    pub your_color: Option<RgbColor>,
    pub last_seq: Seq,
    /// Set when the server rejected the connect. Takes the place of a
    /// `Welcome` — the session cannot paint or observe peers. Stored on the
    /// snapshot (rather than emitted as a broadcast event) because the
    /// rejection fires during `new()` before any caller can subscribe.
    pub connect_rejected: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DartboardEvent {
    Ack {
        client_op_id: ClientOpId,
        seq: Seq,
    },
    Reject {
        client_op_id: ClientOpId,
        reason: String,
    },
    PeerJoined {
        peer: Peer,
    },
    PeerLeft {
        user_id: UserId,
    },
    ConnectRejected {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ArtboardSnapshotKind {
    Daily,
    Monthly,
}

impl ArtboardSnapshotKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Monthly => "monthly",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArtboardArchiveSnapshot {
    pub board_key: String,
    pub kind: ArtboardSnapshotKind,
    pub label: String,
    pub canvas: Canvas,
    pub provenance: ArtboardProvenance,
}

#[derive(Debug)]
pub enum ArtboardArchiveResult {
    Loaded(Vec<ArtboardArchiveSnapshot>),
    Failed(String),
}

pub struct ArtboardArchiveLoader {
    service: ArtboardSnapshotService,
    tx: tokio_mpsc::UnboundedSender<ArtboardArchiveResult>,
    rx: tokio_mpsc::UnboundedReceiver<ArtboardArchiveResult>,
}

impl ArtboardArchiveLoader {
    pub fn new(service: ArtboardSnapshotService) -> Self {
        let (tx, rx) = tokio_mpsc::unbounded_channel();
        Self { service, tx, rx }
    }

    pub fn request_list(&self) {
        self.service.list_archives_task(self.tx.clone());
    }

    pub fn try_recv(&mut self) -> Option<ArtboardArchiveResult> {
        self.rx.try_recv().ok()
    }
}

#[derive(Clone)]
pub struct ArtboardSnapshotService {
    db: Option<Db>,
}

impl ArtboardSnapshotService {
    pub fn new(db: Db) -> Self {
        Self { db: Some(db) }
    }

    pub fn disabled() -> Self {
        Self { db: None }
    }

    fn list_archives_task(&self, tx: tokio_mpsc::UnboundedSender<ArtboardArchiveResult>) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(ArtboardArchiveResult::Loaded(Vec::new()));
            return;
        };
        tokio::spawn(async move {
            let result = list_archive_snapshots(&db).await;
            let msg = match result {
                Ok(snapshots) => ArtboardArchiveResult::Loaded(snapshots),
                Err(error) => ArtboardArchiveResult::Failed(format!("{error:#}")),
            };
            let _ = tx.send(msg);
        });
    }
}

#[derive(Clone)]
pub struct DartboardService {
    command_tx: mpsc::Sender<Command>,
    snapshot_rx: watch::Receiver<DartboardSnapshot>,
    event_tx: broadcast::Sender<DartboardEvent>,
}

async fn list_archive_snapshots(db: &Db) -> anyhow::Result<Vec<ArtboardArchiveSnapshot>> {
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard snapshot list")?;
    let mut snapshots = Vec::new();
    for (prefix, kind) in [
        ("daily:", ArtboardSnapshotKind::Daily),
        ("monthly:", ArtboardSnapshotKind::Monthly),
    ] {
        let rows = Snapshot::list_by_board_key_prefix(&client, prefix)
            .await
            .with_context(|| format!("failed to list {prefix} artboard snapshots"))?;
        for row in rows {
            snapshots.push(decode_archive_snapshot(row, kind)?);
        }
    }
    Ok(snapshots)
}

fn decode_archive_snapshot(
    snapshot: Snapshot,
    kind: ArtboardSnapshotKind,
) -> anyhow::Result<ArtboardArchiveSnapshot> {
    let label = snapshot
        .board_key
        .split_once(':')
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| snapshot.board_key.clone());
    let canvas =
        serde_json::from_value(snapshot.canvas).context("failed to decode artboard canvas")?;
    let provenance = serde_json::from_value(snapshot.provenance)
        .context("failed to decode artboard provenance")?;
    Ok(ArtboardArchiveSnapshot {
        board_key: snapshot.board_key,
        kind,
        label,
        canvas,
        provenance,
    })
}

enum Command {
    SubmitOp(CanvasOp),
}

impl DartboardService {
    pub fn new(
        server: ServerHandle,
        user_id: Uuid,
        username: &str,
        shared_provenance: SharedArtboardProvenance,
    ) -> Self {
        let username = username.to_string();
        let hello = Hello {
            name: username.clone(),
            color: requested_user_color_hint(user_id),
        };
        let initial_snapshot = DartboardSnapshot {
            your_name: username.clone(),
            provenance: clone_shared_provenance(&shared_provenance),
            ..Default::default()
        };
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        let (event_tx, _) = broadcast::channel(128);
        let (command_tx, command_rx) = mpsc::channel();

        match server.try_connect_local(hello) {
            ConnectOutcome::Accepted(client) => {
                let thread_snapshot_tx = snapshot_tx.clone();
                let thread_event_tx = event_tx.clone();
                let thread_shared_provenance = shared_provenance.clone();
                let thread_username = username.clone();
                thread::Builder::new()
                    .name(format!("dartboard-{}", user_id))
                    .spawn(move || {
                        run_client_loop(
                            client,
                            command_rx,
                            thread_snapshot_tx,
                            thread_event_tx,
                            thread_shared_provenance,
                            thread_username,
                        )
                    })
                    .expect("failed to spawn dartboard client loop");
            }
            ConnectOutcome::Rejected(reason) => {
                let rejected_snapshot = DartboardSnapshot {
                    your_name: username,
                    provenance: clone_shared_provenance(&shared_provenance),
                    connect_rejected: Some(reason),
                    ..Default::default()
                };
                let _ = snapshot_tx.send(rejected_snapshot);
                // No client loop; dropping the receiver here means subsequent
                // `submit_op` calls through `command_tx` are silently ignored.
                drop(command_rx);
            }
        }

        Self {
            command_tx,
            snapshot_rx,
            event_tx,
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<DartboardSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<DartboardEvent> {
        self.event_tx.subscribe()
    }

    pub fn submit_op(&self, op: CanvasOp) {
        let _ = self.command_tx.send(Command::SubmitOp(op));
    }

    #[cfg(test)]
    pub(crate) fn disconnected_for_tests(initial_snapshot: DartboardSnapshot) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        let (event_tx, _) = broadcast::channel(128);
        let (command_tx, command_rx) = mpsc::channel();
        drop(snapshot_tx);
        drop(command_rx);
        Self {
            command_tx,
            snapshot_rx,
            event_tx,
        }
    }
}

fn requested_user_color_hint(user_id: Uuid) -> RgbColor {
    const PALETTE: [RgbColor; 8] = [
        RgbColor::new(255, 110, 64),
        RgbColor::new(255, 236, 96),
        RgbColor::new(145, 226, 88),
        RgbColor::new(72, 220, 170),
        RgbColor::new(84, 196, 255),
        RgbColor::new(128, 163, 255),
        RgbColor::new(192, 132, 255),
        RgbColor::new(255, 124, 196),
    ];

    let idx = user_id.as_bytes()[0] as usize % PALETTE.len();
    PALETTE[idx]
}

fn run_client_loop(
    mut client: LocalClient,
    command_rx: mpsc::Receiver<Command>,
    snapshot_tx: watch::Sender<DartboardSnapshot>,
    event_tx: broadcast::Sender<DartboardEvent>,
    shared_provenance: SharedArtboardProvenance,
    username: String,
) {
    loop {
        match command_rx.recv_timeout(Duration::from_millis(16)) {
            Ok(Command::SubmitOp(op)) => {
                client.submit_op(op);
                drain_server_messages(
                    &mut client,
                    &snapshot_tx,
                    &event_tx,
                    &shared_provenance,
                    &username,
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                drain_server_messages(
                    &mut client,
                    &snapshot_tx,
                    &event_tx,
                    &shared_provenance,
                    &username,
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                drain_server_messages(
                    &mut client,
                    &snapshot_tx,
                    &event_tx,
                    &shared_provenance,
                    &username,
                );
                break;
            }
        }
    }
}

fn drain_server_messages(
    client: &mut LocalClient,
    snapshot_tx: &watch::Sender<DartboardSnapshot>,
    event_tx: &broadcast::Sender<DartboardEvent>,
    shared_provenance: &SharedArtboardProvenance,
    username: &str,
) {
    while let Some(msg) = client.try_recv() {
        handle_server_msg(msg, snapshot_tx, event_tx, shared_provenance, username);
    }
}

fn handle_server_msg(
    msg: ServerMsg,
    snapshot_tx: &watch::Sender<DartboardSnapshot>,
    event_tx: &broadcast::Sender<DartboardEvent>,
    shared_provenance: &SharedArtboardProvenance,
    username: &str,
) {
    match msg {
        ServerMsg::Welcome {
            your_user_id,
            your_color,
            peers,
            snapshot,
        } => {
            let _ = snapshot_tx.send(DartboardSnapshot {
                canvas: snapshot,
                provenance: clone_shared_provenance(shared_provenance),
                peers,
                your_name: username.to_string(),
                your_user_id: Some(your_user_id),
                your_color: Some(your_color),
                last_seq: 0,
                connect_rejected: None,
            });
        }
        ServerMsg::Ack { client_op_id, seq } => {
            let mut snapshot = snapshot_tx.borrow().clone();
            snapshot.last_seq = snapshot.last_seq.max(seq);
            let _ = snapshot_tx.send(snapshot);
            let _ = event_tx.send(DartboardEvent::Ack { client_op_id, seq });
        }
        ServerMsg::OpBroadcast { from, op, seq } => {
            let mut snapshot = snapshot_tx.borrow().clone();
            let before = snapshot.canvas.clone();
            snapshot.canvas.apply(&op);
            if let Some(actor) = actor_name(&snapshot, from, username) {
                snapshot.provenance.apply_op(&before, &op, &actor);
                apply_shared_op(shared_provenance, &before, &op, &actor);
            } else if matches!(op, CanvasOp::Replace { .. }) {
                snapshot.provenance = clone_shared_provenance(shared_provenance);
            }
            snapshot.last_seq = snapshot.last_seq.max(seq);
            let _ = snapshot_tx.send(snapshot);
        }
        ServerMsg::PeerJoined { peer } => {
            let mut snapshot = snapshot_tx.borrow().clone();
            if !snapshot
                .peers
                .iter()
                .any(|existing| existing.user_id == peer.user_id)
            {
                snapshot.peers.push(peer.clone());
                snapshot.peers.sort_by_key(|existing| existing.user_id);
            }
            let _ = snapshot_tx.send(snapshot);
            let _ = event_tx.send(DartboardEvent::PeerJoined { peer });
        }
        ServerMsg::PeerLeft { user_id } => {
            let mut snapshot = snapshot_tx.borrow().clone();
            snapshot.peers.retain(|peer| peer.user_id != user_id);
            let _ = snapshot_tx.send(snapshot);
            let _ = event_tx.send(DartboardEvent::PeerLeft { user_id });
        }
        ServerMsg::Reject {
            client_op_id,
            reason,
        } => {
            let _ = event_tx.send(DartboardEvent::Reject {
                client_op_id,
                reason,
            });
        }
        ServerMsg::ConnectRejected { reason } => {
            let _ = event_tx.send(DartboardEvent::ConnectRejected { reason });
        }
    }
}

fn actor_name(snapshot: &DartboardSnapshot, from: UserId, username: &str) -> Option<String> {
    if snapshot.your_user_id == Some(from) {
        return Some(username.to_string());
    }
    snapshot
        .peers
        .iter()
        .find(|peer| peer.user_id == from)
        .map(|peer| peer.name.clone())
}
