//! late-bastion — long-lived SSH frontend for late.sh.
//!
//! Terminates user SSH connections and tunnels the shell byte
//! stream to `late-ssh` over a WebSocket, transparently reconnecting across
//! backend deploys.
//!
//! See `devdocs/BASTION-FEATURE.md` for the current design.

mod admission;
#[cfg(test)]
mod admission_test;
pub mod config;
pub mod handshake;
pub mod proxy;
#[cfg(test)]
mod proxy_test;
pub mod ssh;
