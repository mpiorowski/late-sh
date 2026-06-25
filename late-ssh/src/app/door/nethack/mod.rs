// NetHack - a door game served by late.sh's own NetHack host (the `late-nethack`
// crate). Like rebels, late.sh reaches it over SSH: this module is the client
// that connects to the host, streams the remote terminal through a vt100
// emulator, and draws it into a ratatui widget below the top bar. The host runs
// the real upstream NetHack binary on a PTY; identity travels as the SSH
// username (the account-derived `-u` playname), authorized by a shared-secret
// key.
//
// nethack: https://www.nethack.org/
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;
