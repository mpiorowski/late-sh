// BashQuest - a door game served by late.sh's own bashquest host (the
// `late-bashquest` crate). Like dopewars/DCSS, late.sh reaches it over SSH:
// this module is the client that connects to the host, streams the remote
// terminal through a vt100 emulator, and draws it into a ratatui widget below
// the top bar. The host runs bashquest.sh (a native late.sh original, not a
// foreign upstream binary) on a PTY, authorized by a shared-secret-derived
// key, with identity carried by the account's arcade handle.
//
// bashquest.sh: https://github.com/hardlygospel/bashquest
pub mod graduate;
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;
