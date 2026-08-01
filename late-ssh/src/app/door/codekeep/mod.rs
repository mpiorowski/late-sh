// CodeKeep: The Pale, a terminal-native deck-building roguelike served by
// late.sh's dedicated `late-codekeep` SSH host. This module is the client: it
// streams the remote PTY through a vt100 parser and embeds it in the Games hub.
//
// Upstream: https://github.com/tooyipjee/codekeep
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;

#[cfg(test)]
mod identity_test;

#[cfg(test)]
mod proxy_test;
