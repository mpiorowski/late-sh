//! Native trilogy menus and an SSH/PTY client for the independent Zork host.
pub mod identity;
#[cfg(test)]
mod identity_test;
pub mod protocol;
pub mod proxy;
#[cfg(test)]
mod proxy_test;
pub mod render;
pub mod state;
