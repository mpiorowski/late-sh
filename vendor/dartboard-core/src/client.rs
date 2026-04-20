use crate::ops::CanvasOp;
use crate::wire::{ClientOpId, ServerMsg};

/// A Client is a handle to a dartboard session. It submits ops to the server
/// and drains server events. Implementations: LocalClient (in-proc, in
/// dartboard-server) and WebsocketClient (cross-process, in dartboard-client-ws).
pub trait Client {
    /// Submit an op to the server. Returns the client_op_id the op was tagged
    /// with (monotonic per client), which will be echoed back in the matching
    /// Ack or Reject.
    fn submit_op(&mut self, op: CanvasOp) -> ClientOpId;

    /// Non-blocking event drain. Returns None if no events are pending.
    fn try_recv(&mut self) -> Option<ServerMsg>;
}
