//! Shared runtime types for the house-table games. These used to live in
//! `rooms/backend.rs`; they moved here with the surviving runtimes so the
//! whole `app/rooms/` folder can be deleted in the demolition phase.
//! `rooms/backend.rs` re-exports them until then.

use uuid::Uuid;

use crate::app::files::terminal_image::{TerminalImageFrame, TerminalImageProtocol};
use crate::usernames::UsernameLookup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

#[derive(Debug, Clone)]
pub struct RoomTitleDetails {
    pub seated: Option<String>,
    pub role: Option<String>,
    pub balance: Option<i64>,
}

pub struct GameDrawCtx<'a> {
    pub usernames: &'a UsernameLookup<'a>,
    pub image_protocol: Option<TerminalImageProtocol>,
    pub terminal_images: &'a mut TerminalImageFrame,
}

#[derive(Debug, Clone)]
pub enum RoomGameEvent {
    SeatJoined { room_id: Uuid, user_id: Uuid },
}
