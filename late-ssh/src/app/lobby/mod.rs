//! The Lobby: the single front door for multiplayer play (`Ctrl+G`). Fronts
//! two game domains that stay separate services: async daily correspondence
//! matches (`daily/`) and live fixed house tables (`house/`).

pub mod daily;
pub mod house;
pub mod modal_input;
pub mod modal_ui;
mod modal_widgets;
pub mod realm;
pub mod state;
