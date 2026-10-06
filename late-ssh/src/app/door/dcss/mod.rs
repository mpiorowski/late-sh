// Dungeon Crawl Stone Soup - a door game served by late.sh's own DCSS host (the
// `late-dcss` crate). Like nethack, late.sh reaches it over SSH: this module is
// the client that connects to the host, streams the remote terminal through a
// vt100 emulator, and draws it into a ratatui widget below the top bar. The host
// runs the real upstream crawl console binary on a PTY; identity travels as the
// SSH username (the account-derived `-name` playname), authorized by a
// shared-secret key.
//
// crawl: https://crawl.develz.org/
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;
