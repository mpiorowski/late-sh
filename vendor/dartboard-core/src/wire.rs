use serde::{Deserialize, Serialize};

use crate::canvas::Canvas;
use crate::color::RgbColor;
use crate::ops::CanvasOp;

pub type UserId = u64;
pub type ClientOpId = u64;
pub type Seq = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub user_id: UserId,
    pub name: String,
    pub color: RgbColor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Join the session. The server assigns `your_color` from its palette.
    /// Clients no longer propose a color — future post-handshake color-change
    /// requests are the planned path for user-driven preferences.
    Hello { name: String },
    Op {
        client_op_id: ClientOpId,
        op: CanvasOp,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        your_user_id: UserId,
        your_color: RgbColor,
        peers: Vec<Peer>,
        snapshot: Canvas,
    },
    /// Sent in place of `Welcome` when the server cannot accept the client
    /// (e.g. the color palette / seat cap is full). The client is not
    /// registered and should treat the connection as closed after receiving
    /// this message.
    ConnectRejected {
        reason: String,
    },
    Ack {
        client_op_id: ClientOpId,
        seq: Seq,
    },
    OpBroadcast {
        from: UserId,
        op: CanvasOp,
        seq: Seq,
    },
    PeerJoined {
        peer: Peer,
    },
    PeerLeft {
        user_id: UserId,
    },
    Reject {
        client_op_id: ClientOpId,
        reason: String,
    },
}
