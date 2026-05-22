use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    games::chips::svc::ChipService,
    rooms::{
        backend::{
            ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
            RoomGameManager,
        },
        payout::{RoomWinPayoutLimiter, TRON_WIN_PAYOUT_COOLDOWN},
        svc::{GameKind, RoomListItem},
        tron::{
            create_modal::TronCreateModal,
            settings::TronTableSettings,
            state::{State, TronOutcome, TronPhase},
            svc::{
                TRON_FOUR_PLAYER_WIN_CHIPS, TRON_TWO_PLAYER_WIN_CHIPS, TronService,
                TronServiceContext,
            },
        },
    },
};

#[derive(Clone)]
pub struct TronTableManager {
    chip_svc: ChipService,
    activity: ActivityPublisher,
    payout_limiter: RoomWinPayoutLimiter,
    tables: Arc<Mutex<HashMap<Uuid, TronService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl TronTableManager {
    pub fn new(chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            chip_svc,
            activity,
            payout_limiter: RoomWinPayoutLimiter::new(TRON_WIN_PAYOUT_COOLDOWN),
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> TronService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room.id)
            .or_insert_with(|| {
                let settings = TronTableSettings::from_json(&room.settings);
                TronService::new_with_events(
                    room.id,
                    self.chip_svc.clone(),
                    self.activity.clone(),
                    settings,
                    TronServiceContext {
                        payout_limiter: self.payout_limiter.clone(),
                        room_display_name: room.display_name.clone(),
                        room_meta_label: settings.speed.label().to_string(),
                        room_event_tx: self.event_tx.clone(),
                    },
                )
            })
            .clone()
    }
}

impl RoomGameManager for TronTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Tron
    }

    fn label(&self) -> &'static str {
        "Tron"
    }

    fn slug_prefix(&self) -> &'static str {
        "tron"
    }

    fn default_room_name(&self) -> &'static str {
        "Tron Grid"
    }

    fn default_settings(&self) -> serde_json::Value {
        TronTableSettings::default().to_json()
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(TronCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        let settings = TronTableSettings::from_json(&room.settings);
        DirectoryMeta {
            seats: 4,
            pace: settings.speed.label().to_string(),
            stakes: format!("{TRON_TWO_PLAYER_WIN_CHIPS}-{TRON_FOUR_PLAYER_WIN_CHIPS} prize"),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        Some(DirectoryHints { occupied, total: 4 })
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭──>═════──╮", "│  /\\/\\/\\  │", "╰──═════<──╯"]
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
        crate::app::rooms::tron::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::tron::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        crate::app::rooms::tron::ui::preferred_height(area)
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::tron::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        let role = self
            .user_color()
            .map(|color| color.label().to_string())
            .unwrap_or_else(|| "viewer".to_string());
        let state = match snapshot.outcome {
            Some(TronOutcome::Winner { seat_index }) => {
                format!(
                    "{} won",
                    crate::app::rooms::tron::state::TronColor::for_seat(seat_index).label()
                )
            }
            Some(TronOutcome::Draw) => "draw".to_string(),
            None if snapshot.phase == TronPhase::Running => "running".to_string(),
            None => snapshot.speed_label.clone(),
        };
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{occupied}/4 seated")),
            role: Some(format!("{role} · {state}")),
            balance: None,
        })
    }
}
