use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    games::chips::svc::ChipService,
    rooms::{
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameManager,
        },
        poker::{
            create_modal::PokerCreateModal,
            state::State,
            svc::{BIG_BLIND, PokerService, SMALL_BLIND},
        },
        svc::{GameKind, RoomListItem},
    },
};

#[derive(Clone)]
pub struct PokerTableManager {
    chip_svc: ChipService,
    activity: ActivityPublisher,
    tables: Arc<Mutex<HashMap<Uuid, PokerService>>>,
}

impl PokerTableManager {
    pub fn new(chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        Self {
            chip_svc,
            activity,
            tables: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_or_create(&self, room_id: Uuid) -> PokerService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room_id)
            .or_insert_with(|| {
                PokerService::new(room_id, self.chip_svc.clone(), self.activity.clone())
            })
            .clone()
    }
}

impl RoomGameManager for PokerTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Poker
    }

    fn label(&self) -> &'static str {
        "Poker"
    }

    fn slug_prefix(&self) -> &'static str {
        "pk"
    }

    fn default_room_name(&self) -> &'static str {
        "Poker Table"
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(PokerCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        DirectoryMeta {
            seats: 4,
            pace: "turn-based".to_string(),
            stakes: format!("{SMALL_BLIND}/{BIG_BLIND} blinds"),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        let occupied = snapshot
            .seats
            .iter()
            .filter(|seat| seat.user_id.is_some())
            .count();
        Some(DirectoryHints { occupied, total: 4 })
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        Box::new(State::new(
            self.get_or_create(room.id),
            user_id,
            chip_balance,
        ))
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
        crate::app::rooms::poker::input::handle_key(self, byte)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        let fancy = crate::app::rooms::poker::ui::fancy_game_height(area);
        if fancy > 0 {
            fancy
        } else {
            area.height.saturating_mul(7) / 10
        }
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::poker::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.public_snapshot();
        let occupied = snapshot
            .seats
            .iter()
            .filter(|seat| seat.user_id.is_some())
            .count();
        let role = self
            .seat_index()
            .map(|index| format!("seat {}", index + 1))
            .unwrap_or_else(|| "viewer".to_string());
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{occupied}/4 seated")),
            role: Some(format!("{role} · {}", snapshot.phase.label())),
            balance: Some(self.balance()),
        })
    }

    fn chip_balance(&self) -> Option<i64> {
        Some(self.balance())
    }

    fn can_sync_external_chip_balance(&self) -> bool {
        State::can_sync_external_chip_balance(self)
    }

    fn sync_external_chip_balance(&mut self, balance: i64) {
        State::sync_external_chip_balance(self, balance);
    }
}
