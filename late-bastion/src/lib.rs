//! late-bastion — long-lived SSH frontend for late.sh.
//!
//! Terminates user SSH connections and (eventually) tunnels the shell byte
//! stream to `late-ssh` over a WebSocket, transparently reconnecting across
//! backend deploys.
//!
//! See `BASTION.md` and `devdocs/LATE-CONNECTION-BASTION.md`
//! for the full design.

mod admission;
#[cfg(test)]
mod admission_test;
pub mod config;
pub mod handshake;
pub mod proxy;
#[cfg(test)]
mod proxy_test;
pub mod ssh;
