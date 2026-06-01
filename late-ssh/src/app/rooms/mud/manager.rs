// Manager + per-session backend wiring for Lateania.
//
// The manager is the process-wide singleton registered with the
// RoomGameRegistry. It maps each game room id to one MudService (the world) and
// hands every entering session a State wrapper. Unlike the seated games, a
// Lateania "room" is a whole persistent world; "seats" map to adventurers
// present, with no fixed cap.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::{MutexRecover, db::Db};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    rooms::{
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
            RoomGameManager, RoomTitleDetails,
        },
        mud::{create_modal::MudCreateModal, state::State, svc::MudService},
        svc::{GameKind, RoomListItem},
    },
};

/// Soft cap shown in directory hints (worlds are not really seat-limited).
const WORLD_CAPACITY_HINT: usize = 64;

#[derive(Clone)]
pub struct MudTableManager {
    activity: ActivityPublisher,
    db: Db,
    tables: Arc<Mutex<HashMap<Uuid, MudService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl MudTableManager {
    pub fn new(activity: ActivityPublisher, db: Db) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            activity,
            db,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> MudService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room.id)
            .or_insert_with(|| {
                MudService::new_with_events(
                    room.id,
                    self.activity.clone(),
                    self.db.clone(),
                    self.event_tx.clone(),
                )
            })
            .clone()
    }
}

impl RoomGameManager for MudTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Mud
    }

    fn label(&self) -> &'static str {
        "Lateania"
    }

    fn slug_prefix(&self) -> &'static str {
        "mud"
    }

    fn default_room_name(&self) -> &'static str {
        "Lateania"
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(MudCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        DirectoryMeta {
            seats: WORLD_CAPACITY_HINT as u8,
            pace: "real-time".to_string(),
            stakes: "swords & sorcery".to_string(),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let occupied = self.tables.lock_recover().get(&room_id)?.player_count();
        Some(DirectoryHints {
            occupied,
            total: WORLD_CAPACITY_HINT,
        })
    }

    fn is_user_seated(&self, room_id: Uuid, user_id: Uuid) -> bool {
        self.tables
            .lock_recover()
            .get(&room_id)
            .is_some_and(|svc| svc.is_user_present(user_id))
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &[
            r"  /\  ",
            r" |==| ",
            r" |  | ",
        ]
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
        crate::app::rooms::mud::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::mud::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        // The adventure log wants vertical room; ask for most of the pane.
        let scaled = area.height.saturating_mul(13) / 20;
        scaled.clamp(8, 24)
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::mud::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<RoomTitleDetails> {
        let view = self.view();
        if !view.joined {
            return Some(RoomTitleDetails {
                seated: Some("entering".to_string()),
                role: None,
                balance: None,
            });
        }
        let here = format!("{} online", self.player_count());
        let role = if view.respawning {
            "recovering".to_string()
        } else if let Some(foe) = view.in_combat_with.as_ref() {
            format!("fighting {foe}")
        } else {
            format!("lvl {} - {}", view.level, view.room_name)
        };
        Some(RoomTitleDetails {
            seated: Some(here),
            role: Some(role),
            balance: None,
        })
    }

    fn drop_on_leave(&self) -> bool {
        // The per-session wrapper owns the player's presence; dropping it should
        // remove the adventurer from the world.
        true
    }
}
