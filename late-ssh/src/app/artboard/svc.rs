use std::{sync::mpsc, thread, time::Duration};

use anyhow::Context;
use chrono::{DateTime, Utc};
use dartboard_core::{
    Canvas, CanvasOp, Client, ClientOpId, Peer, RgbColor, Seq, ServerMsg, UserId,
};
use dartboard_local::{ConnectOutcome, Hello, LocalClient, ServerHandle};
use late_core::{db::Db, models::artboard::Snapshot};
use tokio::sync::{broadcast, mpsc as tokio_mpsc, watch};
use tracing::{info, warn};
use uuid::Uuid;

const PAINT_REGION_LOG_THRESHOLD: usize = 50;

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
    Curated,
}

impl ArtboardSnapshotKind {
    /// Rail order: the three archive rows, top to bottom.
    pub const ALL: [Self; 3] = [Self::Daily, Self::Monthly, Self::Curated];

    pub fn label(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Monthly => "monthly",
            Self::Curated => "curated",
        }
    }

    /// The rail row's label.
    pub fn title(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Monthly => "Monthly",
            Self::Curated => "Curated",
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::Daily => Snapshot::DAILY_PREFIX,
            Self::Monthly => Snapshot::MONTHLY_PREFIX,
            Self::Curated => Snapshot::CURATED_PREFIX,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Daily => 0,
            Self::Monthly => 1,
            Self::Curated => 2,
        }
    }
}

/// One row of an archive list: the key and when it was saved, nothing
/// heavier. The canvas comes later, one snapshot at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtboardArchiveEntry {
    pub board_key: String,
    pub kind: ArtboardSnapshotKind,
    /// The key without its prefix: `2026-04-23`, `2026-04`, a curated name.
    pub label: String,
    pub updated: DateTime<Utc>,
}

/// One archive, decoded and ready to put on the board.
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
    Listed {
        kind: ArtboardSnapshotKind,
        entries: Vec<ArtboardArchiveEntry>,
    },
    ListFailed {
        kind: ArtboardSnapshotKind,
        error: String,
    },
    Loaded(ArtboardArchiveSnapshot),
    LoadFailed {
        board_key: String,
        error: String,
    },
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

    pub fn request_list(&self, kind: ArtboardSnapshotKind) {
        self.service.list_task(kind, self.tx.clone());
    }

    pub fn request_load(&self, kind: ArtboardSnapshotKind, board_key: String) {
        self.service.load_task(kind, board_key, self.tx.clone());
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

    /// The keys of one archive kind, newest first. Without a database the
    /// list is empty, not an error.
    fn list_task(
        &self,
        kind: ArtboardSnapshotKind,
        tx: tokio_mpsc::UnboundedSender<ArtboardArchiveResult>,
    ) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(ArtboardArchiveResult::Listed {
                kind,
                entries: Vec::new(),
            });
            return;
        };
        tokio::spawn(async move {
            let result = list_archive_entries(&db, kind).await;
            let msg = match result {
                Ok(entries) => ArtboardArchiveResult::Listed { kind, entries },
                Err(error) => {
                    warn!(error = ?error, kind = kind.label(), "artboard archive listing failed");
                    ArtboardArchiveResult::ListFailed {
                        kind,
                        error: format!("{error:#}"),
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// One archive, decoded off the render path.
    fn load_task(
        &self,
        kind: ArtboardSnapshotKind,
        board_key: String,
        tx: tokio_mpsc::UnboundedSender<ArtboardArchiveResult>,
    ) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(ArtboardArchiveResult::LoadFailed {
                board_key,
                error: "archives are unavailable".to_string(),
            });
            return;
        };
        tokio::spawn(async move {
            let result = load_archive_snapshot(&db, kind, &board_key).await;
            let msg = match result {
                Ok(snapshot) => ArtboardArchiveResult::Loaded(snapshot),
                Err(error) => {
                    warn!(error = ?error, %board_key, "artboard archive load failed");
                    ArtboardArchiveResult::LoadFailed {
                        board_key,
                        error: format!("{error:#}"),
                    }
                }
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

async fn list_archive_entries(
    db: &Db,
    kind: ArtboardSnapshotKind,
) -> anyhow::Result<Vec<ArtboardArchiveEntry>> {
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard archive list")?;
    let rows = Snapshot::list_summaries_by_board_key_prefix(&client, kind.prefix())
        .await
        .with_context(|| format!("failed to list {} artboard archives", kind.label()))?;
    Ok(rows
        .into_iter()
        .map(|row| ArtboardArchiveEntry {
            label: archive_label(&row.board_key),
            board_key: row.board_key,
            kind,
            updated: row.updated,
        })
        .collect())
}

async fn load_archive_snapshot(
    db: &Db,
    kind: ArtboardSnapshotKind,
    board_key: &str,
) -> anyhow::Result<ArtboardArchiveSnapshot> {
    let client = db
        .get()
        .await
        .context("failed to get db client for artboard archive load")?;
    let row = Snapshot::find_by_board_key(&client, board_key)
        .await
        .with_context(|| format!("failed to load artboard archive {board_key}"))?;
    let Some(row) = row else {
        anyhow::bail!("archive {board_key} is gone");
    };
    let canvas: Canvas =
        serde_json::from_value(row.canvas).context("failed to decode archive canvas")?;
    let provenance: ArtboardProvenance =
        serde_json::from_value(row.provenance).context("failed to decode archive provenance")?;
    Ok(ArtboardArchiveSnapshot {
        label: archive_label(&row.board_key),
        board_key: row.board_key,
        kind,
        canvas,
        provenance,
    })
}

fn archive_label(board_key: &str) -> String {
    match board_key.split_once(':') {
        Some((_, label)) => label.to_string(),
        None => board_key.to_string(),
    }
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
            info!(
                user = username,
                cells = snapshot.iter().count(),
                peers = peers.len(),
                "artboard welcome"
            );
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
            snapshot_tx.send_modify(|snapshot| {
                snapshot.last_seq = snapshot.last_seq.max(seq);
            });
            let _ = event_tx.send(DartboardEvent::Ack { client_op_id, seq });
        }
        ServerMsg::OpBroadcast { from, op, seq } => {
            log_op(&op, username);
            let needs_before = matches!(
                op,
                CanvasOp::ShiftRow { .. } | CanvasOp::ShiftCol { .. } | CanvasOp::Replace { .. }
            );
            snapshot_tx.send_modify(|snapshot| {
                let actor = actor_name(snapshot, from, username);
                let before = needs_before.then(|| snapshot.canvas.clone());
                let before_ref = before.as_ref().unwrap_or(&snapshot.canvas);
                if let Some(actor_str) = actor.as_deref() {
                    snapshot.provenance.apply_op(before_ref, &op, actor_str);
                    apply_shared_op(shared_provenance, before_ref, &op, actor_str);
                } else if matches!(op, CanvasOp::Replace { .. }) {
                    snapshot.provenance = clone_shared_provenance(shared_provenance);
                }
                snapshot.canvas.apply(&op);
                snapshot.last_seq = snapshot.last_seq.max(seq);
            });
        }
        ServerMsg::PeerJoined { peer } => {
            snapshot_tx.send_modify(|snapshot| {
                if !snapshot
                    .peers
                    .iter()
                    .any(|existing| existing.user_id == peer.user_id)
                {
                    snapshot.peers.push(peer.clone());
                    snapshot.peers.sort_by_key(|existing| existing.user_id);
                }
            });
            let _ = event_tx.send(DartboardEvent::PeerJoined { peer });
        }
        ServerMsg::PeerLeft { user_id } => {
            snapshot_tx.send_modify(|snapshot| {
                snapshot.peers.retain(|peer| peer.user_id != user_id);
            });
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
            warn!(user = username, reason = %reason, "artboard connect rejected");
            let _ = event_tx.send(DartboardEvent::ConnectRejected { reason });
        }
    }
}

fn log_op(op: &CanvasOp, username: &str) {
    match op {
        CanvasOp::Replace { canvas } => {
            info!(
                user = username,
                cells = canvas.iter().count(),
                "artboard replace op"
            );
        }
        CanvasOp::PaintRegion { cells } if cells.len() > PAINT_REGION_LOG_THRESHOLD => {
            info!(
                user = username,
                cells = cells.len(),
                "artboard paint region"
            );
        }
        _ => {}
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
