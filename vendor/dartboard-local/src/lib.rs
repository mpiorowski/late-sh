//! In-process dartboard server + LocalClient.
//!
//! The server owns the canonical [`Canvas`], assigns globally monotonic
//! sequence numbers, and fans out [`ServerMsg`]s to connected clients. Each
//! [`LocalClient`] is a handle scoped to one session.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use dartboard_core::{
    Canvas, CanvasOp, Client, ClientMsg, ClientOpId, Peer, RgbColor, Seq, ServerMsg, UserId,
};
use rand::seq::SliceRandom;

pub mod store;

pub use store::{CanvasStore, InMemStore};

/// Curated 10-color palette offered to joining users. High-contrast,
/// visually distinct under a dark canvas background. The server randomly
/// assigns a free color on connect and the cap is the palette length —
/// an 11th connection gets a `ConnectRejected`.
const PLAYER_PALETTE: [RgbColor; 10] = [
    RgbColor::new(255, 110, 64),  // salmon
    RgbColor::new(255, 196, 64),  // amber
    RgbColor::new(145, 226, 88),  // lime
    RgbColor::new(72, 220, 170),  // mint
    RgbColor::new(84, 196, 255),  // sky
    RgbColor::new(128, 163, 255), // indigo
    RgbColor::new(192, 132, 255), // violet
    RgbColor::new(255, 124, 196), // rose
    RgbColor::new(240, 240, 200), // cream
    RgbColor::new(180, 200, 120), // olive
];

/// Max concurrent players on a single shared canvas. Equal to the palette
/// length so the cap and the "each user has a unique color" invariant
/// coincide — raising the cap means expanding the palette.
pub const MAX_PLAYERS: usize = PLAYER_PALETTE.len();

fn pick_free_color(used: &[RgbColor]) -> Option<RgbColor> {
    let mut candidates: Vec<RgbColor> = PLAYER_PALETTE
        .iter()
        .copied()
        .filter(|c| !used.contains(c))
        .collect();
    candidates.shuffle(&mut rand::thread_rng());
    candidates.into_iter().next()
}

/// A handle to the running server. Cloneable; every clone references the same
/// canonical canvas and client registry.
#[derive(Clone)]
pub struct ServerHandle {
    inner: Arc<ServerInner>,
}

struct ServerInner {
    state: Mutex<State>,
}

struct State {
    canvas: Canvas,
    seq: Seq,
    next_user_id: UserId,
    clients: Vec<ClientEntry>,
    store: Box<dyn CanvasStore>,
}

struct ClientEntry {
    peer: Peer,
    sender: EntrySender,
}

enum EntrySender {
    Local(mpsc::Sender<ServerMsg>),
}

impl EntrySender {
    fn send(&self, msg: ServerMsg) -> bool {
        match self {
            Self::Local(s) => s.send(msg).is_ok(),
        }
    }
}

/// Introductory payload a client sends before any ops. The server assigns
/// the user's color from its palette; clients no longer propose one.
#[derive(Debug, Clone)]
pub struct Hello {
    pub name: String,
}

/// Outcome of a connect attempt. Rejected connections leave no registered
/// state on the server.
pub enum ConnectOutcome {
    Accepted(LocalClient),
    Rejected(String),
}

impl ServerHandle {
    pub fn spawn_local<S: CanvasStore + 'static>(store: S) -> Self {
        let canvas = store.load().unwrap_or_default();
        let inner = Arc::new(ServerInner {
            state: Mutex::new(State {
                canvas,
                seq: 0,
                next_user_id: 1,
                clients: Vec::new(),
                store: Box::new(store),
            }),
        });
        Self { inner }
    }

    /// Connect a new local client. Returns a `LocalClient` on success or a
    /// rejection reason if the server is full. The existing
    /// `connect_local` keeps the ergonomic "unwrap or panic on full"
    /// behavior so tests and single-session callers aren't forced to branch.
    pub fn try_connect_local(&self, hello: Hello) -> ConnectOutcome {
        let (tx, rx) = mpsc::channel();
        match self.register(hello, EntrySender::Local(tx)) {
            Ok(user_id) => ConnectOutcome::Accepted(LocalClient {
                server: self.clone(),
                user_id,
                rx,
                next_client_op_id: 1,
            }),
            Err(reason) => ConnectOutcome::Rejected(reason),
        }
    }

    /// Convenience form that panics on rejection. Kept for the existing
    /// test-suite and for callers who have already enforced the player cap
    /// elsewhere.
    pub fn connect_local(&self, hello: Hello) -> LocalClient {
        match self.try_connect_local(hello) {
            ConnectOutcome::Accepted(client) => client,
            ConnectOutcome::Rejected(reason) => {
                panic!("connect_local rejected: {reason}")
            }
        }
    }

    /// Register a new client with an already-constructed sender. Returns
    /// `Err` with a human-readable reason when the server can't accept
    /// another client; the sender is dropped and no `Welcome` is sent.
    /// On rejection the caller is expected to surface `ConnectRejected` to
    /// the client if it has a transport.
    pub(crate) fn register(&self, hello: Hello, sender: EntrySender) -> Result<UserId, String> {
        let mut state = self.inner.state.lock().unwrap();

        let used_colors: Vec<RgbColor> = state.clients.iter().map(|c| c.peer.color).collect();
        let Some(color) = pick_free_color(&used_colors) else {
            let reason = format!(
                "dartboard is full ({} / {} players)",
                state.clients.len(),
                MAX_PLAYERS
            );
            // Best-effort notify the client before dropping the sender.
            let _ = sender.send(ServerMsg::ConnectRejected {
                reason: reason.clone(),
            });
            return Err(reason);
        };

        let user_id = state.next_user_id;
        state.next_user_id += 1;

        let peer = Peer {
            user_id,
            name: hello.name,
            color,
        };

        sender.send(ServerMsg::Welcome {
            your_user_id: user_id,
            your_color: color,
            peers: state.clients.iter().map(|c| c.peer.clone()).collect(),
            snapshot: state.canvas.clone(),
        });

        for entry in &state.clients {
            entry
                .sender
                .send(ServerMsg::PeerJoined { peer: peer.clone() });
        }

        state.clients.push(ClientEntry { peer, sender });
        Ok(user_id)
    }

    pub fn peer_count(&self) -> usize {
        self.inner.state.lock().unwrap().clients.len()
    }

    pub fn canvas_snapshot(&self) -> Canvas {
        self.inner.state.lock().unwrap().canvas.clone()
    }

    pub(crate) fn submit_op(&self, user_id: UserId, client_op_id: ClientOpId, op: CanvasOp) {
        let mut state = self.inner.state.lock().unwrap();

        let State {
            canvas,
            seq,
            clients,
            store,
            ..
        } = &mut *state;

        canvas.apply(&op);
        *seq += 1;
        let seq = *seq;
        store.save(canvas);

        for entry in clients.iter() {
            if entry.peer.user_id == user_id {
                entry.sender.send(ServerMsg::Ack { client_op_id, seq });
            }
            entry.sender.send(ServerMsg::OpBroadcast {
                from: user_id,
                op: op.clone(),
                seq,
            });
        }
    }

    pub(crate) fn disconnect(&self, user_id: UserId) {
        let mut state = self.inner.state.lock().unwrap();
        state.clients.retain(|c| c.peer.user_id != user_id);
        for entry in &state.clients {
            entry.sender.send(ServerMsg::PeerLeft { user_id });
        }
    }
}

/// In-process client handle. Sends ops directly into the server under the
/// shared state lock; receives events over a std mpsc channel.
pub struct LocalClient {
    server: ServerHandle,
    user_id: UserId,
    rx: mpsc::Receiver<ServerMsg>,
    next_client_op_id: ClientOpId,
}

impl LocalClient {
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    pub fn send(&mut self, msg: ClientMsg) -> Option<ClientOpId> {
        match msg {
            ClientMsg::Hello { .. } => None,
            ClientMsg::Op { op, .. } => Some(self.submit_op(op)),
        }
    }
}

impl Client for LocalClient {
    fn submit_op(&mut self, op: CanvasOp) -> ClientOpId {
        let id = self.next_client_op_id;
        self.next_client_op_id += 1;
        self.server.submit_op(self.user_id, id, op);
        id
    }

    fn try_recv(&mut self) -> Option<ServerMsg> {
        self.rx.try_recv().ok()
    }
}

impl Drop for LocalClient {
    fn drop(&mut self) {
        self.server.disconnect(self.user_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartboard_core::{ops::RowShift, Pos};

    fn red() -> RgbColor {
        RgbColor::new(255, 0, 0)
    }

    fn drain_events(client: &mut LocalClient) -> Vec<ServerMsg> {
        let mut events = Vec::new();
        while let Some(msg) = client.try_recv() {
            events.push(msg);
        }
        events
    }

    #[test]
    fn welcome_contains_snapshot_and_existing_peers() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut alice = server.connect_local(Hello {
            name: "alice".into(),
        });
        let mut bob = server.connect_local(Hello { name: "bob".into() });

        let alice_events = drain_events(&mut alice);
        let bob_events = drain_events(&mut bob);

        match &alice_events[0] {
            ServerMsg::Welcome { peers, .. } => assert!(peers.is_empty()),
            other => panic!("expected Welcome, got {:?}", other),
        }
        match &bob_events[0] {
            ServerMsg::Welcome { peers, .. } => {
                assert_eq!(peers.len(), 1);
                assert_eq!(peers[0].name, "alice");
            }
            other => panic!("expected Welcome, got {:?}", other),
        }
        assert!(alice_events
            .iter()
            .any(|m| matches!(m, ServerMsg::PeerJoined { .. })));
    }

    #[test]
    fn submit_op_broadcasts_and_acks() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut alice = server.connect_local(Hello {
            name: "alice".into(),
        });
        let mut bob = server.connect_local(Hello { name: "bob".into() });
        let _ = drain_events(&mut alice);
        let _ = drain_events(&mut bob);

        alice.submit_op(CanvasOp::PaintCell {
            pos: Pos { x: 2, y: 1 },
            ch: 'A',
            fg: red(),
        });

        let alice_events = drain_events(&mut alice);
        let bob_events = drain_events(&mut bob);

        assert!(alice_events
            .iter()
            .any(|m| matches!(m, ServerMsg::Ack { .. })));
        assert!(alice_events
            .iter()
            .any(|m| matches!(m, ServerMsg::OpBroadcast { .. })));
        assert!(bob_events
            .iter()
            .any(|m| matches!(m, ServerMsg::OpBroadcast { .. })));

        let snap = server.canvas_snapshot();
        assert_eq!(snap.get(Pos { x: 2, y: 1 }), 'A');
    }

    #[test]
    fn sequence_numbers_are_monotonic() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut client = server.connect_local(Hello {
            name: "solo".into(),
        });
        let _ = drain_events(&mut client);

        client.submit_op(CanvasOp::PaintCell {
            pos: Pos { x: 0, y: 0 },
            ch: 'A',
            fg: red(),
        });
        client.submit_op(CanvasOp::PaintCell {
            pos: Pos { x: 1, y: 0 },
            ch: 'B',
            fg: red(),
        });

        let mut seqs = Vec::new();
        for msg in drain_events(&mut client) {
            if let ServerMsg::OpBroadcast { seq, .. } = msg {
                seqs.push(seq);
            }
        }
        assert_eq!(seqs, vec![1, 2]);
    }

    #[test]
    fn shift_row_op_is_applied_server_side() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut client = server.connect_local(Hello {
            name: "solo".into(),
        });
        let _ = drain_events(&mut client);

        client.submit_op(CanvasOp::PaintCell {
            pos: Pos { x: 0, y: 0 },
            ch: 'A',
            fg: red(),
        });
        client.submit_op(CanvasOp::PaintCell {
            pos: Pos { x: 1, y: 0 },
            ch: 'B',
            fg: red(),
        });
        client.submit_op(CanvasOp::ShiftRow {
            y: 0,
            kind: RowShift::PushLeft { to_x: 1 },
        });

        let snap = server.canvas_snapshot();
        assert_eq!(snap.get(Pos { x: 0, y: 0 }), 'B');
        assert_eq!(snap.get(Pos { x: 1, y: 0 }), ' ');
    }

    #[test]
    fn assigned_colors_are_unique_across_peers() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut alice = server.connect_local(Hello {
            name: "alice".into(),
        });
        let mut bob = server.connect_local(Hello { name: "bob".into() });

        let alice_color = match drain_events(&mut alice).into_iter().next() {
            Some(ServerMsg::Welcome { your_color, .. }) => your_color,
            other => panic!("expected Welcome, got {:?}", other),
        };
        let bob_color = match drain_events(&mut bob).into_iter().next() {
            Some(ServerMsg::Welcome { your_color, .. }) => your_color,
            other => panic!("expected Welcome, got {:?}", other),
        };

        assert!(PLAYER_PALETTE.contains(&alice_color));
        assert!(PLAYER_PALETTE.contains(&bob_color));
        assert_ne!(alice_color, bob_color);
    }

    #[test]
    fn dropping_client_frees_color_for_reuse() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut alice = server.connect_local(Hello {
            name: "alice".into(),
        });
        let alice_color = match drain_events(&mut alice).into_iter().next() {
            Some(ServerMsg::Welcome { your_color, .. }) => your_color,
            other => panic!("expected Welcome, got {:?}", other),
        };
        drop(alice);

        // Fill all remaining 9 slots after the drop, confirming the pool
        // exactly repopulates the palette (including alice's freed color).
        let mut clients = Vec::new();
        let mut seen_colors = Vec::new();
        for i in 0..MAX_PLAYERS {
            let mut c = server.connect_local(Hello {
                name: format!("peer{i}"),
            });
            if let Some(ServerMsg::Welcome { your_color, .. }) =
                drain_events(&mut c).into_iter().next()
            {
                seen_colors.push(your_color);
            }
            clients.push(c);
        }
        assert_eq!(seen_colors.len(), MAX_PLAYERS);
        assert!(seen_colors.contains(&alice_color));
    }

    #[test]
    fn eleventh_connect_is_rejected() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut clients = Vec::new();
        for i in 0..MAX_PLAYERS {
            clients.push(server.connect_local(Hello {
                name: format!("peer{i}"),
            }));
        }

        match server.try_connect_local(Hello {
            name: "overflow".into(),
        }) {
            ConnectOutcome::Rejected(reason) => {
                assert!(reason.to_lowercase().contains("full"), "reason: {reason}");
            }
            ConnectOutcome::Accepted(_) => panic!("server should be full"),
        }
        assert_eq!(server.peer_count(), MAX_PLAYERS);
    }

    #[test]
    fn dropping_client_broadcasts_peer_left() {
        let server = ServerHandle::spawn_local(InMemStore);
        let mut alice = server.connect_local(Hello {
            name: "alice".into(),
        });
        let alice_id;
        {
            let bob = server.connect_local(Hello { name: "bob".into() });
            alice_id = alice.user_id();
            drop(bob);
        }
        let events = drain_events(&mut alice);
        assert!(
            events
                .iter()
                .any(|m| matches!(m, ServerMsg::PeerLeft { .. })),
            "expected PeerLeft in {:?}",
            events
        );
        assert_eq!(server.peer_count(), 1);
        let _ = alice_id;
    }
}
