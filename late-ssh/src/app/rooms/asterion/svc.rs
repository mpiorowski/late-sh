use std::sync::Arc;

use asterion_core::{Entity, Game, GameCommand, PlayerId};
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::publisher::ActivityPublisher,
    rooms::{backend::RoomGameEvent, svc::GameKind},
};

#[derive(Clone)]
pub struct AsterionService {
    room_id: Uuid,
    room_display_name: String,
    room_meta_label: String,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<AsterionSnapshot>,
    snapshot_rx: watch::Receiver<AsterionSnapshot>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsterionSnapshot {
    pub room_id: Uuid,
    pub heroes: Vec<HeroSummary>,
    pub status_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeroSummary {
    pub player_id: PlayerId,
    pub name: String,
    pub position: (usize, usize),
    pub maze_id: usize,
}

impl AsterionService {
    pub fn new_with_events(
        room_id: Uuid,
        _activity: ActivityPublisher,
        room_display_name: String,
        room_meta_label: String,
        room_event_tx: broadcast::Sender<RoomGameEvent>,
    ) -> anyhow::Result<Self> {
        let game = Game::new()?;
        let state = SharedState::new(room_id, game);
        let initial = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let svc = Self {
            room_id,
            room_display_name,
            room_meta_label,
            room_event_tx,
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
        };
        svc.spawn_tick_task();
        Ok(svc)
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<AsterionSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> AsterionSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn join_task(&self, user_id: Uuid, name: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            {
                let mut state = svc.state.lock().await;
                state.add_player(user_id, &name);
                svc.publish(&state);
            }
            let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                room_id: svc.room_id,
                user_id,
                game_kind: GameKind::Asterion,
                display_name: svc.room_display_name.clone(),
                seat_index: 0,
                meta: svc.room_meta_label.clone(),
            });
        });
    }

    pub fn leave_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.remove_player(user_id);
            svc.publish(&state);
        });
    }

    pub fn command_task(&self, user_id: Uuid, command: GameCommand) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.handle_command(user_id, command);
            svc.publish(&state);
        });
    }

    fn spawn_tick_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Game::update_time_step());
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let mut state = svc.state.lock().await;
                state.update();
                svc.publish(&state);
            }
        });
    }

    fn publish(&self, state: &SharedState) {
        let next = state.snapshot();
        self.snapshot_tx.send_if_modified(|cur| {
            if *cur == next {
                false
            } else {
                *cur = next;
                true
            }
        });
    }
}

struct SharedState {
    room_id: Uuid,
    game: Game,
}

impl SharedState {
    fn new(room_id: Uuid, game: Game) -> Self {
        Self { room_id, game }
    }

    fn add_player(&mut self, user_id: Uuid, name: &str) {
        self.game.add_player(user_id, name);
    }

    fn remove_player(&mut self, user_id: Uuid) {
        self.game.remove_player(&user_id);
    }

    fn handle_command(&mut self, user_id: Uuid, command: GameCommand) {
        if self.game.get_hero(&user_id).is_some() {
            self.game.handle_command(&command, user_id);
        }
    }

    fn update(&mut self) {
        self.game.update();
    }

    fn snapshot(&self) -> AsterionSnapshot {
        let heroes = self
            .game
            .top_heros()
            .iter()
            .filter_map(|(id, name, _, _)| {
                let hero = self.game.get_hero(id)?;
                Some(HeroSummary {
                    player_id: *id,
                    name: name.clone(),
                    position: hero.position(),
                    maze_id: hero.maze_id(),
                })
            })
            .collect();
        AsterionSnapshot {
            room_id: self.room_id,
            heroes,
            status_message: format!("Heroes in maze: {}", self.game.number_of_players()),
        }
    }
}
