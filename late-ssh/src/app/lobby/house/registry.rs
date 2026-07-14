//! Process-global house-table runtimes: one lazily-created singleton service
//! per `HouseTable` variant. House tables have no DB row; the services live
//! for the whole process (Asterion keeps its stop-when-empty behavior and is
//! recreated on the next enter).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::models::voice_channel::VoiceChannel;
use late_core::{MutexRecover, db::Db, models::chat_room::ChatRoom, models::voice_channel};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    lobby::house::{
        asterion::svc::{AsterionService, AsterionServiceInit},
        blackjack::{
            player::BlackjackPlayerDirectory,
            svc::{BlackjackEvent, BlackjackService},
        },
        poker::svc::PokerService,
        state::HouseTableClient,
        tables::HouseTable,
        tron::svc::{TronService, TronServiceContext},
        types::RoomGameEvent,
    },
};

/// Live occupancy of one house table, read straight from the singleton's
/// watch snapshot. A table whose service was never created is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HouseOccupancy {
    pub seated: usize,
    pub capacity: usize,
    pub in_round: bool,
}

impl HouseOccupancy {
    /// Modal row label: `empty`, `2 seated`, `2 seated · in round`.
    pub fn label(&self) -> String {
        if self.seated == 0 {
            return "empty".to_string();
        }
        let seats = format!("{} seated", self.seated);
        if self.in_round {
            format!("{seats} · in round")
        } else {
            seats
        }
    }
}

#[derive(Clone)]
pub struct HouseTableRegistry {
    chip_svc: ChipService,
    player_directory: BlackjackPlayerDirectory,
    activity: ActivityPublisher,
    db: Db,
    /// Shared seat-join stream across all four tables; `room_id` on the
    /// event is the fixed `HouseTable::table_id`.
    event_tx: broadcast::Sender<RoomGameEvent>,
    /// Blackjack's own event stream, forwarded onto the shared seat-activity
    /// stream by `forward_blackjack_seat_joins`.
    blackjack_event_tx: broadcast::Sender<BlackjackEvent>,
    poker: Arc<Mutex<Option<PokerService>>>,
    blackjack: Arc<Mutex<Option<BlackjackService>>>,
    asterion: Arc<Mutex<Option<AsterionService>>>,
    tron: Arc<Mutex<Option<TronService>>>,
    /// Seeded permanent chat room per table, filled by `ensure_chat_rooms`.
    chat_room_ids: Arc<Mutex<HashMap<HouseTable, Uuid>>>,
}

impl HouseTableRegistry {
    pub fn new(
        chip_svc: ChipService,
        player_directory: BlackjackPlayerDirectory,
        activity: ActivityPublisher,
        db: Db,
    ) -> Self {
        let (event_tx, _) = broadcast::channel::<RoomGameEvent>(256);
        let (blackjack_event_tx, _) = broadcast::channel::<BlackjackEvent>(64);
        Self {
            chip_svc,
            player_directory,
            activity,
            db,
            event_tx,
            blackjack_event_tx,
            poker: Arc::new(Mutex::new(None)),
            blackjack: Arc::new(Mutex::new(None)),
            asterion: Arc::new(Mutex::new(None)),
            tron: Arc::new(Mutex::new(None)),
            chat_room_ids: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Idempotently ensure each table's permanent public chat room
    /// (`chat_rooms(kind='game')`, slug from the roster) plus an enabled
    /// voice channel. Run once at startup, like `ChatRoom::ensure_lounge`.
    pub async fn ensure_chat_rooms(&self) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        for table in HouseTable::ALL {
            let room =
                ChatRoom::get_or_create_game_room(&client, table.game_kind(), table.chat_slug())
                    .await?;
            VoiceChannel::upsert_for_target(
                &client,
                voice_channel::TARGET_CHAT_ROOM,
                room.id,
                table.display_name(),
                true,
            )
            .await?;
            self.chat_room_ids.lock_recover().insert(table, room.id);
            tracing::info!(table = table.chat_slug(), room_id = %room.id, "ensured house table chat room");
        }
        Ok(())
    }

    /// The one choke point for house-table "sat down" activity: every
    /// runtime's seat join converges on `event_tx` (poker/asterion/tron
    /// publish directly, blackjack through the forwarder below).
    pub fn start_seat_activity_task(&self) {
        let mut rx = self.event_tx.subscribe();
        let activity = self.activity.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(RoomGameEvent::SeatJoined { room_id, user_id }) => {
                        let Some(table) = HouseTable::ALL
                            .into_iter()
                            .find(|table| table.table_id() == room_id)
                        else {
                            continue;
                        };
                        activity.sat_down_task(user_id, activity_game_for(table));
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "house seat-join feed lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn chat_room_id(&self, table: HouseTable) -> Option<Uuid> {
        self.chat_room_ids.lock_recover().get(&table).copied()
    }

    /// Enter a table: create/reuse the singleton service and wrap it in a
    /// fresh per-session client. `None` only when Asterion fails to spawn.
    pub fn enter(
        &self,
        table: HouseTable,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Option<HouseTableClient> {
        match table {
            HouseTable::Poker => Some(HouseTableClient::Poker(
                crate::app::lobby::house::poker::state::State::new(
                    self.poker_service(),
                    user_id,
                    chip_balance,
                ),
            )),
            HouseTable::Blackjack => Some(HouseTableClient::Blackjack(
                crate::app::lobby::house::blackjack::state::State::new(
                    self.blackjack_service(),
                    user_id,
                    chip_balance,
                ),
            )),
            HouseTable::Asterion => {
                let session_id = Uuid::now_v7();
                let svc = self.asterion_service(user_id, session_id)?;
                Some(HouseTableClient::Asterion(
                    crate::app::lobby::house::asterion::state::State::new(svc, user_id, session_id),
                ))
            }
            HouseTable::Tron => Some(HouseTableClient::Tron(Box::new(
                crate::app::lobby::house::tron::state::State::new(self.tron_service(), user_id),
            ))),
        }
    }

    pub fn occupancy(&self, table: HouseTable) -> HouseOccupancy {
        let capacity = table.seat_capacity();
        let empty = HouseOccupancy {
            seated: 0,
            capacity,
            in_round: false,
        };
        match table {
            HouseTable::Poker => {
                let Some(svc) = self.poker.lock_recover().clone() else {
                    return empty;
                };
                let snapshot = svc.current_snapshot();
                HouseOccupancy {
                    seated: snapshot
                        .seats
                        .iter()
                        .filter(|seat| seat.user_id.is_some())
                        .count(),
                    capacity,
                    in_round: !matches!(
                        snapshot.phase,
                        crate::app::lobby::house::poker::svc::PokerPhase::Waiting
                            | crate::app::lobby::house::poker::svc::PokerPhase::Showdown
                    ),
                }
            }
            HouseTable::Blackjack => {
                let Some(svc) = self.blackjack.lock_recover().clone() else {
                    return empty;
                };
                let snapshot = svc.current_snapshot();
                HouseOccupancy {
                    seated: snapshot
                        .seats
                        .iter()
                        .filter(|seat| seat.user_id.is_some())
                        .count(),
                    capacity,
                    in_round: matches!(
                        snapshot.phase,
                        crate::app::lobby::house::blackjack::state::Phase::BetPending
                            | crate::app::lobby::house::blackjack::state::Phase::PlayerTurn
                            | crate::app::lobby::house::blackjack::state::Phase::DealerTurn
                            | crate::app::lobby::house::blackjack::state::Phase::Settling
                    ),
                }
            }
            HouseTable::Asterion => {
                let Some(svc) = self.asterion.lock_recover().clone() else {
                    return empty;
                };
                if svc.is_stopped() {
                    return empty;
                }
                let snapshot = svc.current_public();
                HouseOccupancy {
                    seated: snapshot.hero_count,
                    capacity,
                    in_round: snapshot.hero_count > 0,
                }
            }
            HouseTable::Tron => {
                let Some(svc) = self.tron.lock_recover().clone() else {
                    return empty;
                };
                let snapshot = svc.current_snapshot();
                HouseOccupancy {
                    seated: snapshot.seats.iter().filter(|seat| seat.is_some()).count(),
                    capacity,
                    in_round: snapshot.phase
                        == crate::app::lobby::house::tron::state::TronPhase::Running,
                }
            }
        }
    }

    /// Whether the user currently holds a seat (poker/blackjack/tron) or a
    /// hero slot (asterion). Drives the backtick workspace cycle.
    pub fn is_user_seated(&self, table: HouseTable, user_id: Uuid) -> bool {
        match table {
            HouseTable::Poker => self.poker.lock_recover().clone().is_some_and(|svc| {
                svc.current_snapshot()
                    .seats
                    .iter()
                    .any(|seat| seat.user_id == Some(user_id))
            }),
            HouseTable::Blackjack => self.blackjack.lock_recover().clone().is_some_and(|svc| {
                svc.current_snapshot()
                    .seats
                    .iter()
                    .any(|seat| seat.user_id == Some(user_id))
            }),
            HouseTable::Asterion => self
                .asterion
                .lock_recover()
                .clone()
                .is_some_and(|svc| !svc.is_stopped() && svc.has_session_for_user(user_id)),
            HouseTable::Tron => self
                .tron
                .lock_recover()
                .clone()
                .is_some_and(|svc| svc.current_snapshot().seats.contains(&Some(user_id))),
        }
    }

    /// Whether the game at `table` is currently blocked on `user_id`'s
    /// action (their poker or blackjack turn). Read from the live singleton
    /// snapshot so it holds while the player is off-screen; drives the
    /// your-turn desktop notification. Asterion/Tron have no turn concept.
    pub fn awaiting_action(&self, table: HouseTable, user_id: Uuid) -> bool {
        use crate::app::lobby::house::{
            blackjack::state::{Phase as BlackjackPhase, SeatPhase},
            poker::svc::PokerPhase,
        };
        match table {
            HouseTable::Poker => {
                let Some(svc) = self.poker.lock_recover().clone() else {
                    return false;
                };
                let snapshot = svc.current_snapshot();
                let action_phase = matches!(
                    snapshot.phase,
                    PokerPhase::PreFlop | PokerPhase::Flop | PokerPhase::Turn | PokerPhase::River
                );
                action_phase
                    && snapshot
                        .active_seat
                        .and_then(|seat| snapshot.seats.get(seat))
                        .and_then(|seat| seat.user_id)
                        == Some(user_id)
            }
            HouseTable::Blackjack => {
                let Some(svc) = self.blackjack.lock_recover().clone() else {
                    return false;
                };
                let snapshot = svc.current_snapshot();
                snapshot.phase == BlackjackPhase::PlayerTurn
                    && snapshot.seats.iter().any(|seat| {
                        seat.user_id == Some(user_id) && seat.phase == SeatPhase::Playing
                    })
            }
            HouseTable::Asterion | HouseTable::Tron => false,
        }
    }

    fn poker_service(&self) -> PokerService {
        let mut slot = self.poker.lock_recover();
        slot.get_or_insert_with(|| {
            PokerService::new_with_settings_and_events(
                HouseTable::Poker.table_id(),
                self.chip_svc.clone(),
                HouseTable::poker_settings(),
                self.event_tx.clone(),
            )
        })
        .clone()
    }

    fn blackjack_service(&self) -> BlackjackService {
        let mut slot = self.blackjack.lock_recover();
        slot.get_or_insert_with(|| {
            self.forward_blackjack_seat_joins(self.blackjack_event_tx.subscribe());
            BlackjackService::new_with_settings(
                HouseTable::Blackjack.table_id(),
                self.chip_svc.clone(),
                self.player_directory.clone(),
                self.blackjack_event_tx.clone(),
                HouseTable::blackjack_settings(),
            )
        })
        .clone()
    }

    /// Blackjack publishes its own event type; translate seat joins onto the
    /// shared stream like the rooms-era manager did.
    fn forward_blackjack_seat_joins(&self, mut rx: broadcast::Receiver<BlackjackEvent>) {
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(BlackjackEvent::SeatJoined { user_id, .. }) => {
                        let _ = event_tx.send(RoomGameEvent::SeatJoined {
                            room_id: HouseTable::Blackjack.table_id(),
                            user_id,
                        });
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "house blackjack event forwarder lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Asterion keeps its stop-when-empty lifecycle: a stopped singleton is
    /// discarded and a fresh maze spawned, exactly like the rooms manager.
    fn asterion_service(&self, user_id: Uuid, session_id: Uuid) -> Option<AsterionService> {
        let mut slot = self.asterion.lock_recover();
        if let Some(existing) = slot.clone() {
            existing.register_session(user_id, session_id);
            if !existing.is_stopped() {
                return Some(existing);
            }
            existing.unregister_session(user_id, session_id);
            *slot = None;
        }
        match AsterionService::new_with_events(AsterionServiceInit {
            room_id: HouseTable::Asterion.table_id(),
            chip_svc: self.chip_svc.clone(),
            db: self.db.clone(),
            room_event_tx: self.event_tx.clone(),
        }) {
            Ok(svc) => {
                svc.register_session(user_id, session_id);
                *slot = Some(svc.clone());
                Some(svc)
            }
            Err(err) => {
                tracing::error!(error = ?err, "failed to spawn house asterion service");
                None
            }
        }
    }

    fn tron_service(&self) -> TronService {
        let mut slot = self.tron.lock_recover();
        slot.get_or_insert_with(|| {
            TronService::new_with_events(
                HouseTable::Tron.table_id(),
                self.chip_svc.clone(),
                HouseTable::tron_settings(),
                TronServiceContext {
                    room_event_tx: self.event_tx.clone(),
                },
            )
        })
        .clone()
    }
}

fn activity_game_for(table: HouseTable) -> ActivityGame {
    match table {
        HouseTable::Poker => ActivityGame::Poker,
        HouseTable::Blackjack => ActivityGame::Blackjack,
        HouseTable::Asterion => ActivityGame::Asterion,
        HouseTable::Tron => ActivityGame::Tron,
    }
}
