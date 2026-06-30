// dopewars - a door game run as a local PTY child of late-ssh. Unlike rebels
// and nethack (reached over SSH), late.sh spawns the real upstream dopewars
// curses client directly on a pty, streams its terminal through a vt100 emulator,
// and blits the grid into a ratatui widget below the top bar. dopewars has no
// savegame and no save-lock, so there is no host crate and no graceful-save
// dance: a dropped run simply ends.
//
// dopewars: https://dopewars.sourceforge.io/
pub mod proxy;
pub mod render;
pub mod state;
