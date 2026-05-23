use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use asterion_core::{Entity, Game, GameCommand, Maze};
use image::{Rgba, RgbaImage};
use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::user::User;
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
    public_tx: watch::Sender<AsterionPublicSnapshot>,
    public_rx: watch::Receiver<AsterionPublicSnapshot>,
    private: Arc<StdMutex<HashMap<Uuid, watch::Sender<AsterionPrivateSnapshot>>>>,
    state: Arc<Mutex<SharedState>>,
    db: Db,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsterionPublicSnapshot {
    pub room_id: Uuid,
    pub hero_count: usize,
    pub status_message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AsterionPrivateSnapshot {
    pub user_id: Uuid,
    pub seated: bool,
    pub maze_id: usize,
    pub position: (usize, usize),
    pub is_dead: bool,
    pub has_won: bool,
    pub view: Option<RenderedView>,
}

#[derive(Clone, Debug)]
pub struct RenderedView {
    pub image: RgbaImage,
    pub overrides: HashMap<(u32, u32), char>,
    pub background: Rgba<u8>,
}

impl PartialEq for RenderedView {
    fn eq(&self, other: &Self) -> bool {
        self.background == other.background
            && self.overrides == other.overrides
            && self.image.dimensions() == other.image.dimensions()
            && self.image.as_raw() == other.image.as_raw()
    }
}

impl AsterionService {
    pub fn new_with_events(
        room_id: Uuid,
        _activity: ActivityPublisher,
        db: Db,
        room_display_name: String,
        room_meta_label: String,
        room_event_tx: broadcast::Sender<RoomGameEvent>,
    ) -> anyhow::Result<Self> {
        let game = Game::new()?;
        let state = SharedState::new(room_id, game);
        let initial = state.public_snapshot();
        let (public_tx, public_rx) = watch::channel(initial);
        let svc = Self {
            room_id,
            room_display_name,
            room_meta_label,
            room_event_tx,
            public_tx,
            public_rx,
            private: Arc::new(StdMutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(state)),
            db,
        };
        svc.spawn_update_task();
        svc.spawn_render_task();
        Ok(svc)
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_public(&self) -> watch::Receiver<AsterionPublicSnapshot> {
        self.public_rx.clone()
    }

    pub fn subscribe_private(&self, user_id: Uuid) -> watch::Receiver<AsterionPrivateSnapshot> {
        let mut private = self.private.lock_recover();
        if let Some(existing) = private.get(&user_id) {
            return existing.subscribe();
        }
        let (tx, rx) = watch::channel(AsterionPrivateSnapshot {
            user_id,
            seated: false,
            maze_id: 0,
            position: (0, 0),
            is_dead: false,
            has_won: false,
            view: None,
        });
        private.insert(user_id, tx);
        rx
    }

    pub fn current_public(&self) -> AsterionPublicSnapshot {
        self.public_rx.borrow().clone()
    }

    pub fn join_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let name = lookup_username(&svc.db, user_id)
                .await
                .unwrap_or_else(|| fallback_name(user_id));
            {
                let mut state = svc.state.lock().await;
                state.add_player(user_id, &name);
                svc.publish_public(&state);
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
            {
                let mut state = svc.state.lock().await;
                state.remove_player(user_id);
                svc.publish_public(&state);
            }
            svc.private.lock_recover().remove(&user_id);
        });
    }

    pub fn command_task(&self, user_id: Uuid, command: GameCommand) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.handle_command(user_id, command);
            svc.publish_public(&state);
        });
    }

    fn spawn_update_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Game::update_time_step());
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let mut state = svc.state.lock().await;
                state.update();
                svc.publish_public(&state);
            }
        });
    }

    fn spawn_render_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Game::draw_time_step());
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let background = Maze::background_color();
            loop {
                ticker.tick().await;
                let recipients: Vec<(Uuid, watch::Sender<AsterionPrivateSnapshot>)> = svc
                    .private
                    .lock_recover()
                    .iter()
                    .map(|(id, tx)| (*id, tx.clone()))
                    .collect();
                if recipients.is_empty() {
                    continue;
                }
                let state = svc.state.lock().await;
                for (user_id, tx) in recipients {
                    let next = state.private_snapshot(user_id, background);
                    tx.send_if_modified(|cur| {
                        if *cur == next {
                            false
                        } else {
                            *cur = next;
                            true
                        }
                    });
                }
            }
        });
    }

    fn publish_public(&self, state: &SharedState) {
        let next = state.public_snapshot();
        self.public_tx.send_if_modified(|cur| {
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

    fn hero_count(&self) -> usize {
        self.game.number_of_players()
    }

    fn public_snapshot(&self) -> AsterionPublicSnapshot {
        AsterionPublicSnapshot {
            room_id: self.room_id,
            hero_count: self.hero_count(),
            status_message: format!("Heroes in maze: {}", self.hero_count()),
        }
    }

    fn private_snapshot(&self, user_id: Uuid, background: Rgba<u8>) -> AsterionPrivateSnapshot {
        let Some(hero) = self.game.get_hero(&user_id) else {
            return AsterionPrivateSnapshot {
                user_id,
                seated: false,
                maze_id: 0,
                position: (0, 0),
                is_dead: false,
                has_won: false,
                view: None,
            };
        };
        let is_dead = hero.is_dead();
        let has_won = hero.has_won().is_some();
        let maze_id = hero.maze_id();
        let position = hero.position();
        let view = match self.game.draw(user_id) {
            Ok(image) => {
                let overrides = self
                    .game
                    .image_char_overrides(user_id, &image)
                    .unwrap_or_default();
                Some(RenderedView {
                    image,
                    overrides,
                    background,
                })
            }
            Err(err) => {
                tracing::warn!(error = ?err, %user_id, "asterion draw failed");
                None
            }
        };
        AsterionPrivateSnapshot {
            user_id,
            seated: true,
            maze_id,
            position,
            is_dead,
            has_won,
            view,
        }
    }
}

async fn lookup_username(db: &Db, user_id: Uuid) -> Option<String> {
    let client = db.get().await.ok()?;
    let mut map = User::list_usernames_by_ids(&client, &[user_id]).await.ok()?;
    let raw = map.remove(&user_id)?;
    let sanitized: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn fallback_name(user_id: Uuid) -> String {
    let s = user_id.simple().to_string();
    format!("u-{}", &s[..8])
}
