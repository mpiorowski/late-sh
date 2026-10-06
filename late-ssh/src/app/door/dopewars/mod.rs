// dopewars - a door game served by late.sh's own dopewars host (the
// `late-dopewars` crate). Like nethack, late.sh reaches it over SSH: this module
// is the client that connects to the host, streams the remote terminal through a
// vt100 emulator, and draws it into a ratatui widget below the top bar. The host
// runs the real upstream dopewars curses client on a PTY against one shared,
// PVC-backed high-score file, authorized by a shared-secret-derived key.
// dopewars has no mid-game save, so a dropped connection simply ends the run.
//
// dopewars: https://dopewars.sourceforge.io/
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;

#[cfg(test)]
mod identity_test;

#[cfg(test)]
mod proxy_test;
