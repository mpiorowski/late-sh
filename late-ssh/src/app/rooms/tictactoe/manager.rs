use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use uuid::Uuid;

use crate::app::rooms::{
    backend::{ActiveRoomBackend, DirectoryHints, DirectoryMeta, RoomGameManager},
    svc::{GameKind, RoomListItem},
    tictactoe::{
        state::{State, Winner},
        svc::TicTacToeService,
    },
};

#[derive(Clone)]
pub struct TicTacToeTableManager {
    tables: Arc<Mutex<HashMap<Uuid, TicTacToeService>>>,
}

impl TicTacToeTableManager {
    pub fn new() -> Self {
        Self {
            tables: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_or_create(&self, room_id: Uuid) -> TicTacToeService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room_id)
            .or_insert_with(|| TicTacToeService::new(room_id))
            .clone()
    }
}

impl Default for TicTacToeTableManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomGameManager for TicTacToeTableManager {
    fn kind(&self) -> GameKind {
        GameKind::TicTacToe
    }

    fn label(&self) -> &'static str {
        "Tic-Tac-Toe"
    }

    fn slug_prefix(&self) -> &'static str {
        "ttt"
    }

    fn default_room_name(&self) -> &'static str {
        "Tic-Tac-Toe Board"
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        DirectoryMeta {
            seats: 2,
            pace: "turn-based".to_string(),
            stakes: "no stakes".to_string(),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        Some(DirectoryHints { occupied, total: 2 })
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        _chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        Box::new(State::new(self.get_or_create(room.id), user_id))
    }
}

impl ActiveRoomBackend for State {
    fn room_id(&self) -> Uuid {
        self.room_id()
    }

    fn tick(&mut self) {
        State::tick(self);
    }

    fn touch_activity(&self) {}

    fn handle_key(&mut self, byte: u8) -> crate::app::rooms::backend::InputAction {
        crate::app::rooms::tictactoe::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::tictactoe::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        area.height.saturating_mul(3) / 5
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::tictactoe::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        let role = self
            .user_mark()
            .map(|mark| mark.label().to_string())
            .unwrap_or_else(|| "viewer".to_string());
        let state = match snapshot.winner {
            Some(Winner::Mark(mark)) => format!("{} won", mark.label()),
            Some(Winner::Draw) => "draw".to_string(),
            None => format!("{} turn", snapshot.turn.label()),
        };
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{occupied}/2 seated")),
            role: Some(format!("{role} · {state}")),
            balance: None,
        })
    }
}
