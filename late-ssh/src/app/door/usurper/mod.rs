// Usurper - the classic LORD-era BBS door game (Jakob Dangarden, 1993; GPL'd
// and ported to 64-bit by Rick Parrish), served by late.sh's own host (the
// `late-usurper` crate). Like nethack/dcss, late.sh reaches it over SSH: this
// module is the client that connects to the host, streams the remote terminal
// through a vt100 emulator, and draws it into a ratatui widget below the top
// bar. The host runs the real upstream USURPER.EXE on a PTY with a per-session
// DOOR32.SYS dropfile; identity travels as the SSH username (the account's
// arcade handle), authorized by a shared-secret key. The host transcodes the
// game's CP437 output to UTF-8 before it reaches this side.
//
// usurper: https://www.usurper.info/
pub mod identity;
#[cfg(test)]
mod identity_test;
pub mod proxy;
pub mod render;
pub mod state;
#[cfg(test)]
mod state_test;
