use serde_json::Value;
use uuid::Uuid;

use super::{
    backend::{ActiveRoomBackend, DirectoryHints, DirectoryMeta, RoomGameManager},
    blackjack::manager::BlackjackTableManager,
    svc::{GameKind, RoomListItem},
    tictactoe::manager::TicTacToeTableManager,
};

#[derive(Clone)]
pub struct RoomGameRegistry {
    blackjack: BlackjackTableManager,
    tictactoe: TicTacToeTableManager,
}

impl RoomGameRegistry {
    pub fn new(blackjack: BlackjackTableManager, tictactoe: TicTacToeTableManager) -> Self {
        Self {
            blackjack,
            tictactoe,
        }
    }

    pub fn manager(&self, kind: GameKind) -> &dyn RoomGameManager {
        match kind {
            GameKind::Blackjack => &self.blackjack,
            GameKind::TicTacToe => &self.tictactoe,
        }
    }

    pub fn ordered_kinds(&self) -> &'static [GameKind] {
        &GameKind::ALL
    }

    pub fn label(&self, kind: GameKind) -> &'static str {
        self.manager(kind).label()
    }

    pub fn slug_prefix(&self, kind: GameKind) -> &'static str {
        self.manager(kind).slug_prefix()
    }

    pub fn default_room_name(&self, kind: GameKind) -> &'static str {
        self.manager(kind).default_room_name()
    }

    pub fn default_settings(&self, kind: GameKind) -> Value {
        self.manager(kind).default_settings()
    }

    pub fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        self.manager(room.game_kind).directory_meta(room)
    }

    pub fn directory_hints(&self, room_id: Uuid, kind: GameKind) -> Option<DirectoryHints> {
        self.manager(kind).directory_hints(room_id)
    }

    pub fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        self.manager(room.game_kind)
            .enter(room, user_id, chip_balance)
    }

    pub fn blackjack(&self) -> &BlackjackTableManager {
        &self.blackjack
    }
}
