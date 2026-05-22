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
        chess::{
            create_modal::ChessCreateModal,
            settings::ChessTableSettings,
            state::{ChessGameResult, ChessPhase, State},
            svc::{CHESS_WIN_CHIP_PAYOUT, ChessService, ChessServiceContext},
        },
        payout::{CHESS_WIN_PAYOUT_COOLDOWN, RoomWinPayoutLimiter},
        svc::{GameKind, RoomListItem},
    },
};

#[derive(Clone)]
pub struct ChessTableManager {
    chip_svc: ChipService,
    activity: ActivityPublisher,
    payout_limiter: RoomWinPayoutLimiter,
    tables: Arc<Mutex<HashMap<Uuid, ChessService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl ChessTableManager {
    pub fn new(chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            chip_svc,
            activity,
            payout_limiter: RoomWinPayoutLimiter::new(CHESS_WIN_PAYOUT_COOLDOWN),
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> ChessService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room.id)
            .or_insert_with(|| {
                let settings = ChessTableSettings::from_json(&room.settings);
                ChessService::new_with_events(
                    room.id,
                    self.chip_svc.clone(),
                    self.activity.clone(),
                    settings,
                    ChessServiceContext {
                        payout_limiter: self.payout_limiter.clone(),
                        room_display_name: room.display_name.clone(),
                        room_meta_label: settings.time_control.short_label().to_string(),
                        room_event_tx: self.event_tx.clone(),
                    },
                )
            })
            .clone()
    }
}

impl RoomGameManager for ChessTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Chess
    }

    fn label(&self) -> &'static str {
        "Chess"
    }

    fn slug_prefix(&self) -> &'static str {
        "chess"
    }

    fn default_room_name(&self) -> &'static str {
        "Chess Board"
    }

    fn default_settings(&self) -> serde_json::Value {
        ChessTableSettings::default().to_json()
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(ChessCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        let settings = ChessTableSettings::from_json(&room.settings);
        DirectoryMeta {
            seats: 2,
            pace: settings.time_control.label().to_string(),
            stakes: format!("{} prize", CHESS_WIN_CHIP_PAYOUT),
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

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭♜♞♝♛♚♝♞♜╮", "│░▓░▓░▓░▓│", "╰♖♘♗♕♔♗♘♖╯"]
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
        crate::app::rooms::chess::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::chess::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        crate::app::rooms::chess::ui::preferred_height(area)
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::chess::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        let role = self
            .user_color()
            .map(|color| color.label().to_string())
            .unwrap_or_else(|| "viewer".to_string());
        let state = match snapshot.result {
            Some(ChessGameResult::Checkmate { winner }) => format!("{} mate", winner.label()),
            Some(ChessGameResult::Timeout { winner }) => format!("{} on time", winner.label()),
            Some(ChessGameResult::Resignation { winner }) => {
                format!("{} by resignation", winner.label())
            }
            Some(ChessGameResult::Draw) => "draw".to_string(),
            None if snapshot.phase == ChessPhase::Active => {
                format!("{} turn", snapshot.turn.label())
            }
            None => snapshot.time_control_label.clone(),
        };
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{occupied}/2 seated")),
            role: Some(format!("{role} · {state}")),
            balance: None,
        })
    }
}
