use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::asterion::ASTERION_DAILY_ESCAPE_PAYOUT;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    games::chips::svc::ChipService,
    house::asterion::{
        state::State,
        svc::{AsterionService, AsterionServiceInit, MAX_HEROES_PER_ROOM},
    },
    rooms::{
        asterion::create_modal::AsterionCreateModal,
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, GameDrawCtx,
            InputAction, RoomGameEvent, RoomGameManager, RoomTitleDetails,
        },
        svc::{GameKind, RoomListItem, RoomsService},
    },
};

const STOPPED_SERVICE_PRUNE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AsterionRoomManager {
    chip_svc: ChipService,
    db: Db,
    rooms_service: RoomsService,
    tables: Arc<Mutex<HashMap<Uuid, AsterionService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl AsterionRoomManager {
    pub fn new(chip_svc: ChipService, rooms_service: RoomsService, db: Db) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        let manager = Self {
            chip_svc,
            db,
            rooms_service,
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
    ) -> Option<(AsterionService, Uuid)> {
        let mut tables = self.tables.lock_recover();
        tables.retain(|_, svc| !svc.is_stopped());
        if let Some(existing) = tables.get(&room.id).cloned() {
            existing.register_session(user_id, session_id);
            if !existing.is_stopped() {
                return Some((existing, session_id));
            }
            existing.unregister_session(user_id, session_id);
            tables.remove(&room.id);
        }
        match AsterionService::new_with_events(AsterionServiceInit {
            room_id: room.id,
            chip_svc: self.chip_svc.clone(),
            rooms_service: Some(self.rooms_service.clone()),
            db: self.db.clone(),
            room_event_tx: self.event_tx.clone(),
        }) {
            Ok(svc) => {
                svc.register_session(user_id, session_id);
                tables.insert(room.id, svc.clone());
                Some((svc, session_id))
            }
            Err(err) => {
                tracing::error!(error = ?err, room_id = %room.id, "failed to spawn asterion service");
                None
            }
        }
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
            stakes: format!("{ASTERION_DAILY_ESCAPE_PAYOUT} daily"),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        self.prune_stopped();
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_public();
        Some(DirectoryHints {
            occupied: snapshot.hero_count,
            total: MAX_HEROES_PER_ROOM,
        })
    }

    fn is_user_seated(&self, room_id: Uuid, user_id: Uuid) -> bool {
        self.tables
            .lock_recover()
            .get(&room_id)
            .is_some_and(|svc| svc.has_session_for_user(user_id))
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
        let (svc, session_id) = match self.get_or_create_for_session(room, user_id, Uuid::now_v7())
        {
            Some(session) => session,
            None => {
                return Box::new(MessageState {
                    room_id: room.id,
                    message: "Asterion failed to start. Press Esc to leave.",
                });
            }
        };
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
        crate::app::house::asterion::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::house::asterion::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        area.height.saturating_mul(7).saturating_div(10).max(1)
    }

    fn draw(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect, ctx: GameDrawCtx<'_>) {
        crate::app::house::asterion::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<RoomTitleDetails> {
        let public = self.public();
        let private = self.private();
        let role = if private.has_won {
            "escaped"
        } else if private.is_dead {
            "knocked out"
        } else if private.rejected {
            "room full"
        } else if private.seated {
            "escaping"
        } else {
            "joining"
        };
        Some(RoomTitleDetails {
            seated: Some(format!("{} heroes", public.hero_count)),
            role: Some(role.to_string()),
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
    fn drop_on_leave(&self) -> bool {
        true
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> MessageState {
        MessageState {
            room_id: Uuid::nil(),
            message: "test",
        }
    }

    #[test]
    fn message_state_leaves_on_escape_and_q() {
        let mut state = fixture();
        assert_eq!(state.handle_key(0x1B), InputAction::Leave);
        assert_eq!(state.handle_key(b'q'), InputAction::Leave);
        assert_eq!(state.handle_key(b'Q'), InputAction::Leave);
    }

    #[test]
    fn message_state_ignores_other_keys() {
        let mut state = fixture();
        assert_eq!(state.handle_key(b'a'), InputAction::Ignored);
        assert_eq!(state.handle_key(b' '), InputAction::Ignored);
    }

    #[test]
    fn message_state_drops_backend_on_leave() {
        let state = fixture();
        assert!(state.drop_on_leave());
    }
}
