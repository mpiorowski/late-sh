// NetHack - a door game that runs the real upstream NetHack binary locally on a
// PTY. Unlike rebels (which proxies a remote SSH server), late.sh owns the
// process: it spawns nethack, streams the child terminal through a vt100
// emulator, and draws it into a ratatui widget below the top bar.
//
// nethack: https://www.nethack.org/
pub mod proxy;
pub mod render;
pub mod state;
