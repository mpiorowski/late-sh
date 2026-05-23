use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use asterion_core::{AlarmLevel, Entity, Game, GameCommand, Hero, Maze};
use image::{Rgba, RgbaImage};
use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::user::User;
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    rooms::{backend::RoomGameEvent, svc::GameKind},
};

pub const MAX_HEROES_PER_ROOM: usize = 12;

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
    activity: ActivityPublisher,
    db: Db,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsterionPublicSnapshot {
    pub room_id: Uuid,
    pub hero_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AsterionPrivateSnapshot {
    pub user_id: Uuid,
    pub seated: bool,
    pub rejected: bool,
    pub maze_id: usize,
    pub position: (usize, usize),
    pub is_dead: bool,
    pub has_won: bool,
    pub speed: u64,
    pub vision: usize,
    pub memory: u64,
    pub power_ups_collected: usize,
    pub alarm_level: AlarmLevel,
    pub nearest_minotaur_distance_sq: usize,
    pub minotaurs_in_maze: usize,
    pub view: Option<RenderedView>,
}

impl AsterionPrivateSnapshot {
    fn empty(user_id: Uuid) -> Self {
        Self {
            user_id,
            seated: false,
            rejected: false,
            maze_id: 0,
            position: (0, 0),
            is_dead: false,
            has_won: false,
            speed: Hero::INITIAL_SPEED,
            vision: Hero::INITIAL_VISION,
            memory: Hero::INITIAL_MEMORY,
            power_ups_collected: 0,
            alarm_level: AlarmLevel::NoMinotaurs,
            nearest_minotaur_distance_sq: usize::MAX,
            minotaurs_in_maze: 0,
            view: None,
        }
    }
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
            && self.image.dimensions() == other.image.dimensions()
            && self.image.as_raw() == other.image.as_raw()
            && self.overrides == other.overrides
    }
}

fn diff_set<T: PartialEq>(tx: &watch::Sender<T>, next: T) {
    tx.send_if_modified(|cur| {
        if *cur == next {
            false
        } else {
            *cur = next;
            true
        }
    });
}

impl AsterionService {
    pub fn new_with_events(
        room_id: Uuid,
        activity: ActivityPublisher,
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
            activity,
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
        let (tx, rx) = watch::channel(AsterionPrivateSnapshot::empty(user_id));
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
            let added = {
                let mut state = svc.state.lock().await;
                let added = state.add_player(user_id, &name);
                svc.publish_public(&state);
                added
            };
            if !added {
                svc.publish_rejected(user_id);
                return;
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
                let new_wins = {
                    let mut state = svc.state.lock().await;
                    state.update();
                    let wins = state.drain_new_wins();
                    svc.publish_public(&state);
                    wins
                };
                for user_id in new_wins {
                    svc.activity
                        .game_won_task(user_id, ActivityGame::Asterion, None, None);
                }
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
                for (user_id, tx) in recipients {
                    let next = {
                        let state = svc.state.lock().await;
                        state.private_snapshot(user_id, background)
                    };
                    diff_set(&tx, next);
                }
            }
        });
    }

    fn publish_public(&self, state: &SharedState) {
        diff_set(&self.public_tx, state.public_snapshot());
    }

    fn publish_rejected(&self, user_id: Uuid) {
        let private = self.private.lock_recover();
        if let Some(tx) = private.get(&user_id) {
            diff_set(
                tx,
                AsterionPrivateSnapshot {
                    rejected: true,
                    ..AsterionPrivateSnapshot::empty(user_id)
                },
            );
        }
    }
}

struct SharedState {
    room_id: Uuid,
    game: Game,
    players: HashSet<Uuid>,
    wins_announced: HashSet<Uuid>,
}

impl SharedState {
    fn new(room_id: Uuid, game: Game) -> Self {
        Self {
            room_id,
            game,
            players: HashSet::new(),
            wins_announced: HashSet::new(),
        }
    }

    fn add_player(&mut self, user_id: Uuid, name: &str) -> bool {
        if self.players.contains(&user_id) {
            return true;
        }
        if self.players.len() >= MAX_HEROES_PER_ROOM {
            return false;
        }
        self.players.insert(user_id);
        self.game.add_player(user_id, name);
        true
    }

    fn remove_player(&mut self, user_id: Uuid) {
        if self.players.remove(&user_id) {
            self.game.remove_player(&user_id);
        }
        self.wins_announced.remove(&user_id);
    }

    fn handle_command(&mut self, user_id: Uuid, command: GameCommand) {
        if self.game.get_hero(&user_id).is_some() {
            self.game.handle_command(&command, user_id);
        }
    }

    fn update(&mut self) {
        self.game.update();
    }

    fn drain_new_wins(&mut self) -> Vec<Uuid> {
        let mut wins = Vec::new();
        for user_id in &self.players {
            if self.wins_announced.contains(user_id) {
                continue;
            }
            if let Some(hero) = self.game.get_hero(user_id) {
                if hero.has_won().is_some() {
                    wins.push(*user_id);
                }
            }
        }
        self.wins_announced.extend(wins.iter().copied());
        wins
    }

    fn hero_count(&self) -> usize {
        self.players.len()
    }

    fn public_snapshot(&self) -> AsterionPublicSnapshot {
        AsterionPublicSnapshot {
            room_id: self.room_id,
            hero_count: self.hero_count(),
        }
    }

    fn private_snapshot(&self, user_id: Uuid, background: Rgba<u8>) -> AsterionPrivateSnapshot {
        let Some(hero) = self.game.get_hero(&user_id) else {
            return AsterionPrivateSnapshot::empty(user_id);
        };
        let is_dead = hero.is_dead();
        let has_won = hero.has_won().is_some();
        let maze_id = hero.maze_id();
        let position = hero.position();
        let speed = hero.speed();
        let vision = hero.vision();
        let memory = hero.memory();
        let power_ups_collected = hero.power_ups_collected_in_maze(maze_id);
        let (alarm_level, nearest_minotaur_distance_sq) = self.game.alarm_level(&user_id);
        let minotaurs_in_maze = self.game.minotaurs_in_maze(maze_id);
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
            rejected: false,
            maze_id,
            position,
            is_dead,
            has_won,
            speed,
            vision,
            memory,
            power_ups_collected,
            alarm_level,
            nearest_minotaur_distance_sq,
            minotaurs_in_maze,
            view,
        }
    }
}

async fn lookup_username(db: &Db, user_id: Uuid) -> Option<String> {
    let client = db.get().await.ok()?;
    let mut map = User::list_usernames_by_ids(&client, &[user_id]).await.ok()?;
    let raw = map.remove(&user_id)?;
    sanitize_username(&raw)
}

fn sanitize_username(raw: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::{fallback_name, sanitize_username};
    use uuid::Uuid;

    #[test]
    fn sanitize_strips_control_chars_and_trims() {
        assert_eq!(
            sanitize_username("  alice\nbob\t  "),
            Some("alicebob".to_string())
        );
    }

    #[test]
    fn sanitize_returns_none_for_blank_after_strip() {
        assert_eq!(sanitize_username("   \r\n\t  "), None);
    }

    #[test]
    fn sanitize_keeps_unicode_graphemes() {
        assert_eq!(sanitize_username("björn"), Some("björn".to_string()));
    }

    #[test]
    fn fallback_name_is_prefixed_and_eight_hex_chars() {
        let id = Uuid::nil();
        let name = fallback_name(id);
        assert_eq!(name, "u-00000000");
    }
}
