use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    rooms::{
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
            RoomGameManager,
        },
        svc::{GameKind, RoomListItem},
        tictactoe::{
            create_modal::TicTacToeCreateModal,
            state::{State, Winner},
            svc::TicTacToeService,
        },
    },
};

#[derive(Clone)]
pub struct TicTacToeTableManager {
    activity: ActivityPublisher,
    tables: Arc<Mutex<HashMap<Uuid, TicTacToeService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl TicTacToeTableManager {
    pub fn new(activity: ActivityPublisher) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            activity,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> TicTacToeService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room.id)
            .or_insert_with(|| {
                TicTacToeService::new_with_events(
                    room.id,
                    self.activity.clone(),
                    room.display_name.clone(),
                    self.event_tx.clone(),
                )
            })
            .clone()
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

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(TicTacToeCreateModal::new(self.default_room_name()))
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

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        _chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        Box::new(State::new(self.get_or_create(room), user_id))
    }
}

impl ActiveRoomBackend for State {
    fn room_id(&self) -> Uuid {
        self.room_id()
    }

    fn tick(&mut self) {
        State::tick(self);
    }

    fn touch_activity(&self) {
        State::touch_activity(self);
    }

    fn handle_key(&mut self, byte: u8) -> crate::app::rooms::backend::InputAction {
        crate::app::rooms::tictactoe::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::tictactoe::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        let scaled = area.height.saturating_mul(9) / 20;
        scaled.min(19)
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
