use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use late_core::MutexRecover;
use late_core::db::Db;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::rooms::{
    backend::{
        ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, GameDrawCtx,
        InputAction, RoomGameEvent, RoomGameManager, RoomTitleDetails,
    },
    sshattrick::{
        create_modal::SshattrickCreateModal,
        state::State,
        svc::{SEATS_PER_ROOM, SshattrickService, SshattrickServiceInit},
    },
    svc::{GameKind, RoomListItem, RoomsService},
};
use sshattrick_core::GameSide;

const STOPPED_SERVICE_PRUNE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct SshattrickRoomManager {
    rooms_service: RoomsService,
    db: Db,
    tables: Arc<Mutex<HashMap<Uuid, SshattrickService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl SshattrickRoomManager {
    pub fn new(rooms_service: RoomsService, db: Db) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        let manager = Self {
            rooms_service,
            db,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        };
        manager.spawn_stopped_service_pruner();
        manager
    }

    fn get_or_create_for_session(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        session_id: Uuid,
    ) -> (SshattrickService, Uuid) {
        let mut tables = self.tables.lock_recover();
        tables.retain(|_, svc| !svc.is_stopped());
        if let Some(existing) = tables.get(&room.id).cloned() {
            existing.register_session(user_id, session_id);
            if !existing.is_stopped() {
                return (existing, session_id);
            }
            existing.unregister_session(user_id, session_id);
            tables.remove(&room.id);
        }
        let svc = SshattrickService::new_with_events(SshattrickServiceInit {
            room_id: room.id,
            rooms_service: self.rooms_service.clone(),
            db: self.db.clone(),
            room_event_tx: self.event_tx.clone(),
        });
        svc.register_session(user_id, session_id);
        tables.insert(room.id, svc.clone());
        (svc, session_id)
    }

    fn prune_stopped(&self) {
        self.tables
            .lock_recover()
            .retain(|_, svc| !svc.is_stopped());
    }

    fn spawn_stopped_service_pruner(&self) {
        let manager = self.clone();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let mut interval = tokio::time::interval(STOPPED_SERVICE_PRUNE_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                manager.prune_stopped();
            }
        });
    }
}

impl RoomGameManager for SshattrickRoomManager {
    fn kind(&self) -> GameKind {
        GameKind::Sshattrick
    }

    fn label(&self) -> &'static str {
        "ssHattrick"
    }

    fn slug_prefix(&self) -> &'static str {
        "sshattrick"
    }

    fn default_room_name(&self) -> &'static str {
        "SsHattrick"
    }

    fn default_settings(&self) -> Value {
        Value::Object(serde_json::Map::new())
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(SshattrickCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        DirectoryMeta {
            seats: SEATS_PER_ROOM as u8,
            pace: "real-time".to_string(),
            stakes: "casual".to_string(),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        self.prune_stopped();
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_public();
        let occupied = snapshot.red.is_some() as usize + snapshot.blue.is_some() as usize;
        Some(DirectoryHints {
            occupied,
            total: SEATS_PER_ROOM,
        })
    }

    fn is_user_seated(&self, room_id: Uuid, user_id: Uuid) -> bool {
        let Some(svc) = self.tables.lock_recover().get(&room_id).cloned() else {
            return false;
        };
        let (red, blue) = svc.seated_user_ids();
        red == Some(user_id) || blue == Some(user_id)
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭───╮", "│ ● │", "╰───╯"]
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        _chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        let (svc, session_id) = self.get_or_create_for_session(room, user_id, Uuid::now_v7());
        Box::new(State::new(svc, user_id, session_id))
    }
}

impl ActiveRoomBackend for State {
    fn room_id(&self) -> Uuid {
        State::room_id(self)
    }

    fn tick(&mut self) {
        State::tick(self);
    }

    fn touch_activity(&self) {
        State::touch_activity(self);
    }

    fn drop_on_leave(&self) -> bool {
        true
    }

    fn handle_key(&mut self, byte: u8) -> InputAction {
        super::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        super::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        super::ui::preferred_height(area)
    }

    fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect, ctx: GameDrawCtx<'_>) {
        super::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<RoomTitleDetails> {
        let public = self.public();
        let private = self.private();
        let seated_count = public.red.is_some() as u8 + public.blue.is_some() as u8;
        let role = match private.seated_as {
            Some(GameSide::Red) => "red",
            Some(GameSide::Blue) => "blue",
            None if seated_count as usize >= SEATS_PER_ROOM => "watching",
            None => "joining",
        };
        Some(RoomTitleDetails {
            seated: Some(format!("{seated_count}/{SEATS_PER_ROOM} seats")),
            role: Some(role.to_string()),
            balance: None,
        })
    }
}
