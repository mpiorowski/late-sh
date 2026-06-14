// Rebels in the Sky - a door game that proxies to the standalone rebels SSH
// server at frittura.org:3788. Unlike Lateania (native, in-process), the remote
// renders the frames; late.sh runs the stream through a vt100 terminal emulator
// and draws it into a ratatui widget below the top bar.
//
// rebels: https://github.com/ricott1/rebels-in-the-sky
pub mod identity;
pub mod proxy;
pub mod render;
pub mod state;
