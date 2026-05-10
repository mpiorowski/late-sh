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
        blackjack::{
            create_modal::BlackjackCreateModal,
            player::BlackjackPlayerDirectory,
            settings::BlackjackTableSettings,
            state::{BlackjackSnapshot, Phase, State},
            svc::{BlackjackEvent, BlackjackService},
        },
        svc::{GameKind, RoomListItem},
    },
};

#[derive(Clone)]
pub struct BlackjackTableManager {
    chip_svc: ChipService,
    player_directory: BlackjackPlayerDirectory,
    activity: ActivityPublisher,
    tables: Arc<Mutex<HashMap<Uuid, BlackjackService>>>,
    event_tx: broadcast::Sender<BlackjackEvent>,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
}

impl BlackjackTableManager {
    pub fn new(
        chip_svc: ChipService,
        player_directory: BlackjackPlayerDirectory,
        activity: ActivityPublisher,
    ) -> Self {
        let (event_tx, _) = broadcast::channel::<BlackjackEvent>(256);
        let (room_event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        Self {
            chip_svc,
            player_directory,
            activity,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            room_event_tx,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.event_tx.subscribe()
    }

    pub fn get_or_create(
        &self,
        room_id: Uuid,
        display_name: String,
        settings: BlackjackTableSettings,
    ) -> BlackjackService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room_id)
            .or_insert_with(|| {
                let (event_tx, _) = broadcast::channel::<BlackjackEvent>(64);
                let meta = settings.meta_label();
                self.forward_table_events(
                    room_id,
                    display_name.clone(),
                    meta,
                    event_tx.subscribe(),
                );
                BlackjackService::new_with_settings(
                    room_id,
                    self.chip_svc.clone(),
                    self.player_directory.clone(),
                    event_tx,
                    self.activity.clone(),
                    settings,
                )
            })
            .clone()
    }

    fn forward_table_events(
        &self,
        room_id: Uuid,
        display_name: String,
        meta: String,
        mut rx: broadcast::Receiver<BlackjackEvent>,
    ) {
        let event_tx = self.event_tx.clone();
        let room_event_tx = self.room_event_tx.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let BlackjackEvent::SeatJoined {
                            user_id,
                            seat_index,
                        } = &event
                        {
                            let _ = room_event_tx.send(RoomGameEvent::SeatJoined {
                                room_id,
                                user_id: *user_id,
                                game_kind: GameKind::Blackjack,
                                display_name: display_name.clone(),
                                seat_index: *seat_index,
                                meta: meta.clone(),
                            });
                        }
                        let _ = event_tx.send(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(%room_id, skipped, "blackjack table event forwarder lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn table_snapshots(&self) -> HashMap<Uuid, BlackjackSnapshot> {
        self.tables
            .lock_recover()
            .iter()
            .map(|(room_id, service)| (*room_id, service.current_snapshot()))
            .collect()
    }
}

impl RoomGameManager for BlackjackTableManager {
    fn kind(&self) -> GameKind {
        GameKind::Blackjack
    }

    fn label(&self) -> &'static str {
        "Blackjack"
    }

    fn slug_prefix(&self) -> &'static str {
        "bj"
    }

    fn default_room_name(&self) -> &'static str {
        "Blackjack Table"
    }

    fn default_settings(&self) -> serde_json::Value {
        BlackjackTableSettings::default().to_json()
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        Box::new(BlackjackCreateModal::new(self.default_room_name()))
    }

    fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        let settings = BlackjackTableSettings::from_json(&room.settings);
        DirectoryMeta {
            seats: 4,
            pace: settings.pace_label().to_string(),
            stakes: settings.stake_label(),
        }
    }

    fn directory_hints(&self, room_id: Uuid) -> Option<DirectoryHints> {
        let snapshot = self.tables.lock_recover().get(&room_id)?.current_snapshot();
        let occupied = snapshot
            .seats
            .iter()
            .filter(|seat| seat.user_id.is_some())
            .count();
        Some(DirectoryHints {
            occupied,
            total: snapshot.seats.len(),
        })
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.room_event_tx.subscribe()
    }

    fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        let settings = BlackjackTableSettings::from_json(&room.settings);
        let svc = self.get_or_create(room.id, room.display_name.clone(), settings);
        Box::new(State::new(svc, user_id, chip_balance))
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
        let byte = if matches!(byte, b'q' | b'Q') {
            0x1B
        } else {
            byte
        };
        match crate::app::rooms::blackjack::input::handle_key(self, byte) {
            crate::app::rooms::blackjack::input::InputAction::Ignored => {
                crate::app::rooms::backend::InputAction::Ignored
            }
            crate::app::rooms::blackjack::input::InputAction::Handled => {
                crate::app::rooms::backend::InputAction::Handled
            }
            crate::app::rooms::blackjack::input::InputAction::Leave => {
                crate::app::rooms::backend::InputAction::Leave
            }
        }
    }

    fn preferred_game_height(&self, area: ratatui::layout::Rect) -> u16 {
        let fancy = crate::app::rooms::blackjack::ui::fancy_game_height(area);
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
        crate::app::rooms::blackjack::ui::draw_game(frame, area, self, false, ctx.usernames);
    }

    fn title_details(&self) -> Option<crate::app::rooms::backend::RoomTitleDetails> {
        let snapshot = self.snapshot();
        let seated = snapshot
            .seats
            .iter()
            .filter(|seat| seat.user_id.is_some())
            .count();
        let role = match self.seat_index() {
            Some(index) => format!("seat {}", index + 1),
            None => "viewer".to_string(),
        };
        Some(crate::app::rooms::backend::RoomTitleDetails {
            seated: Some(format!("{seated}/{} seated", snapshot.seats.len())),
            role: Some(role),
            balance: Some(snapshot.balance),
        })
    }

    fn chip_balance(&self) -> Option<i64> {
        Some(self.balance())
    }

    fn can_sync_external_chip_balance(&self) -> bool {
        self.snapshot().phase == Phase::Betting
    }

    fn sync_external_chip_balance(&mut self, balance: i64) {
        self.set_balance(balance);
    }
}
