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
        ssnake::{
            create_modal::SsnakeCreateModal,
            settings::SsnakeTableSettings,
            state::{SsnakeOutcome, SsnakePhase, State},
            svc::{SSNAKE_WIN_CHIPS, SsnakeService, SsnakeServiceContext},
        },
        svc::{GameKind, RoomListItem, RoomsService},
    },
};

#[derive(Clone)]
pub struct SsnakeTableManager {
    chip_svc: ChipService,
    activity: ActivityPublisher,
    rooms_service: RoomsService,
    tables: Arc<Mutex<HashMap<Uuid, SsnakeService>>>,
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl SsnakeTableManager {
    pub fn new(
        chip_svc: ChipService,
        activity: ActivityPublisher,
        rooms_service: RoomsService,
    ) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            chip_svc,
            activity,
            rooms_service,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn get_or_create(&self, room: &RoomListItem) -> SsnakeService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room.id)
            .or_insert_with(|| {
                let settings = SsnakeTableSettings::from_json(&room.settings);
                SsnakeService::new_with_events(
                    room.id,
                    self.chip_svc.clone(),
                    self.activity.clone(),
                    settings,
                    SsnakeServiceContext {
                        room_event_tx: self.event_tx.clone(),
                        rooms_service: self.rooms_service.clone(),
                    },
                )
            })
            .clone()
    }
}

impl RoomGameManager for SsnakeTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Ssnake
    }

    fn label(&self) -> &'static str {
        "Super Snake"
    }

    fn slug_prefix(&self) -> &'static str {
        "ssnake"
    }

    fn default_room_name(&self) -> &'static str {
        "Snake Pit"
    }

    fn default_settings(&self) -> serde_json::Value {
        SsnakeTableSettings::default().to_json()
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(SsnakeCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        let settings = SsnakeTableSettings::from_json(&room.settings);
        DirectoryMeta {
            seats: settings.seats as u8,
            pace: settings.label(),
            stakes: format!("{SSNAKE_WIN_CHIPS} prize"),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        Some(DirectoryHints {
            occupied,
            total: snapshot.seat_limit,
        })
    }

    fn is_user_seated(&self, room_id: Uuid, user_id: Uuid) -> bool {
        self.tables
            .lock_recover()
            .get(&room_id)
            .is_some_and(|svc| svc.current_snapshot().seats.contains(&Some(user_id)))
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭─◉═══════──╮", "│ ~ssSNAKE~ │", "╰──═══════●─╯"]
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
        crate::app::rooms::ssnake::input::handle_key(self, byte)
    }

    fn handle_arrow(&mut self, key: u8) -> bool {
        crate::app::rooms::ssnake::input::handle_arrow(self, key)
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        crate::app::rooms::ssnake::ui::preferred_height(self, area)
    }

    fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        ctx: crate::app::rooms::backend::GameDrawCtx<'_>,
    ) {
        crate::app::rooms::ssnake::ui::draw_game(frame, area, self, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.snapshot();
        let occupied = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
        let role = self
            .user_color()
            .map(|color| color.label().to_string())
            .unwrap_or_else(|| "viewer".to_string());
        let state = match snapshot.outcome {
            Some(SsnakeOutcome::Winner { seat_index }) => {
                format!(
                    "{} won",
                    crate::app::rooms::ssnake::state::SsnakeColor::for_seat(seat_index).label()
                )
            }
            Some(SsnakeOutcome::Draw) => "draw".to_string(),
            None if snapshot.phase == SsnakePhase::Running => snapshot
                .level
                .as_ref()
                .map(|level| level.name.clone())
                .unwrap_or_else(|| "running".to_string()),
            None => format!("{} · {}", snapshot.arena_choice, snapshot.speed_label),
        };
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{occupied}/{} seated", snapshot.seat_limit)),
            role: Some(format!("{role} · {state}")),
            balance: None,
        })
    }
}
