//! The Lobby: the single front door for multiplayer play (`Ctrl+Q`). Fronts
//! two game domains that stay separate services: async daily correspondence
//! matches (`daily/`) and live fixed house tables (`house/`).

pub mod daily;
pub mod house;
pub mod modal_input;
pub mod modal_ui;
pub mod state;
pub mod workspace;
