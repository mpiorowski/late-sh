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
        asterion::{
            create_modal::AsterionCreateModal, state::State, svc::AsterionService,
        },
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, GameDrawCtx,
            InputAction, RoomGameEvent, RoomGameManager, RoomTitleDetails,
        },
        svc::{GameKind, RoomListItem},
    },
};

pub const MAX_HEROES_PER_ROOM: usize = 12;

#[derive(Clone)]
pub struct AsterionRoomManager {
    activity: ActivityPublisher,
    tables: Arc<Mutex<HashMap<Uuid, AsterionService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl AsterionRoomManager {
    pub fn new(activity: ActivityPublisher) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            activity,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> Option<AsterionService> {
        let mut tables = self.tables.lock_recover();
        if let Some(existing) = tables.get(&room.id) {
            return Some(existing.clone());
        }
        match AsterionService::new_with_events(
            room.id,
            self.activity.clone(),
            room.display_name.clone(),
            String::new(),
            self.event_tx.clone(),
        ) {
            Ok(svc) => {
                tables.insert(room.id, svc.clone());
                Some(svc)
            }
            Err(err) => {
                tracing::error!(error = ?err, room_id = %room.id, "failed to spawn asterion service");
                None
            }
        }
    }
}

impl RoomGameManager for AsterionRoomManager {
    fn kind(&self) -> GameKind {
        GameKind::Asterion
    }

    fn label(&self) -> &'static str {
        "Asterion"
    }

    fn slug_prefix(&self) -> &'static str {
        "ast"
    }

    fn default_room_name(&self) -> &'static str {
        "Asterion Maze"
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(AsterionCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        DirectoryMeta {
            seats: MAX_HEROES_PER_ROOM as u8,
            pace: "real-time".to_string(),
            stakes: "no stakes".to_string(),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        Some(DirectoryHints {
            occupied: snapshot.heroes.len(),
            total: MAX_HEROES_PER_ROOM,
        })
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭───╮", "│ ▓ │", "╰─◊─╯"]
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        _chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        let svc = match self.get_or_create(room) {
            Some(svc) => svc,
            None => {
                return Box::new(MessageState {
                    room_id: room.id,
                    message: "Asterion failed to start. Press Esc to leave.",
                });
            }
        };
        let snapshot = svc.current_snapshot();
        let already_in = snapshot.heroes.iter().any(|h| h.player_id == user_id);
        if !already_in && snapshot.heroes.len() >= MAX_HEROES_PER_ROOM {
            return Box::new(MessageState {
                room_id: room.id,
                message: "Room is full. Press Esc to leave.",
            });
        }
        let name = short_player_name(user_id);
        Box::new(State::new(svc, user_id, name))
    }
}

fn short_player_name(user_id: Uuid) -> String {
    let s = user_id.simple().to_string();
    format!("u-{}", &s[..s.len().min(8)])
}

impl ActiveRoomBackend for State {
    fn room_id(&self) -> Uuid {
        State::room_id(self)
    }

    fn tick(&mut self) {
        State::tick(self);
    }

    fn touch_activity(&self) {}

    fn handle_key(&mut self, byte: u8) -> InputAction {
        super::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        super::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        let scaled = area.height.saturating_mul(3) / 4;
        scaled.min(28)
    }

    fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect, ctx: GameDrawCtx<'_>) {
        super::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<RoomTitleDetails> {
        let snapshot = self.snapshot();
        Some(RoomTitleDetails {
            seated: Some(format!("{} heroes", snapshot.heroes.len())),
            role: None,
            balance: None,
        })
    }
}

struct MessageState {
    room_id: Uuid,
    message: &'static str,
}

impl ActiveRoomBackend for MessageState {
    fn room_id(&self) -> Uuid {
        self.room_id
    }
    fn tick(&mut self) {}
    fn touch_activity(&self) {}
    fn handle_key(&mut self, byte: u8) -> InputAction {
        match byte {
            0x1B | b'q' | b'Q' => InputAction::Leave,
            _ => InputAction::Ignored,
        }
    }
    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        area.height.min(6)
    }
    fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect, _ctx: GameDrawCtx<'_>) {
        use ratatui::widgets::Paragraph;
        frame.render_widget(Paragraph::new(self.message), area);
    }
}
