//! late-bastion — long-lived SSH frontend for late.sh.
//!
//! Terminates user SSH connections and (eventually) tunnels the shell byte
//! stream to `late-ssh` over a WebSocket, transparently reconnecting across
//! backend deploys.
//!
//! See `BASTION.md` and the workspace doc `PERSISTENT-CONNECTION-GATEWAY.md`
//! for the full design.

pub mod config;
pub mod handshake;
pub mod ssh;
