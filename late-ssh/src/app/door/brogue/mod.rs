// Brogue CE - a door game served by late.sh's own Brogue host (the
// `late-brogue` crate). Like dcss and nethack, late.sh reaches it over SSH:
// this module is the client that connects to the host, streams the remote
// terminal through a vt100 emulator, and draws it into a ratatui widget below
// the top bar. The host runs the real upstream brogue curses binary on a PTY;
// identity travels as the SSH username (the account's arcade handle, which
// keys the per-player save directory), authorized by a shared-secret key.
//
// Brogue CE: https://github.com/tmewett/BrogueCE
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;
