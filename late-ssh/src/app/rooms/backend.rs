use std::collections::HashMap;

use ratatui::{Frame, layout::Rect};
use serde_json::Value;
use uuid::Uuid;

use super::svc::{GameKind, RoomListItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    Ignored,
    Handled,
    Leave,
}

#[derive(Debug, Clone)]
pub struct DirectoryMeta {
    pub seats: u8,
    pub pace: String,
    pub stakes: String,
}

#[derive(Debug, Clone, Copy)]
pub struct DirectoryHints {
    pub occupied: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
pub struct RoomTitleDetails {
    pub seated: Option<String>,
    pub role: Option<String>,
    pub balance: Option<i64>,
}

pub struct GameDrawCtx<'a> {
    pub usernames: &'a HashMap<Uuid, String>,
}

pub trait ActiveRoomBackend: Send {
    fn room_id(&self) -> Uuid;
    fn tick(&mut self);
    fn touch_activity(&self);
    fn handle_key(&mut self, byte: u8) -> InputAction;
    fn handle_arrow(&mut self, _key: u8) -> bool {
        false
    }
    fn preferred_game_height(&self, area: Rect) -> u16;
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: GameDrawCtx<'_>);
    fn title_details(&self) -> Option<RoomTitleDetails> {
        None
    }
    fn chip_balance(&self) -> Option<i64> {
        None
    }
    fn can_sync_external_chip_balance(&self) -> bool {
        false
    }
    fn sync_external_chip_balance(&mut self, _balance: i64) {}
}

pub trait RoomGameManager: Send + Sync {
    fn kind(&self) -> GameKind;
    fn label(&self) -> &'static str;
    fn slug_prefix(&self) -> &'static str;
    fn default_room_name(&self) -> &'static str;
    fn default_settings(&self) -> Value;
    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta;
    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints>;
    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend>;
}
