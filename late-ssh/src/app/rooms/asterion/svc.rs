use std::collections::{HashMap, HashSet};
use std::sync::Mutex as StdMutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use asterion_core::{AlarmLevel, Entity, Game, GameCommand, Hero, Maze};
use chrono::Utc;
use image::{Rgba, RgbaImage};
use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::user::User;
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{backend::RoomGameEvent, svc::RoomsService},
};

pub const MAX_HEROES_PER_ROOM: usize = 12;
const EMPTY_SERVICE_TTL: Duration = Duration::from_secs(5 * 60);
const ROOM_TOUCH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AsterionService {
    room_id: Uuid,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    public_tx: watch::Sender<AsterionPublicSnapshot>,
    public_rx: watch::Receiver<AsterionPublicSnapshot>,
    sessions: Arc<AsterionSessions>,
    private: Arc<StdMutex<HashMap<Uuid, watch::Sender<AsterionPrivateSnapshot>>>>,
    state: Arc<Mutex<SharedState>>,
    lifecycle: Arc<AsterionLifecycle>,
    room_in_round: Arc<AtomicBool>,
    chip_svc: ChipService,
    rooms_service: RoomsService,
    activity: ActivityPublisher,
    db: Db,
}

pub(super) struct AsterionServiceInit {
    pub(super) room_id: Uuid,
    pub(super) chip_svc: ChipService,
    pub(super) activity: ActivityPublisher,
    pub(super) rooms_service: RoomsService,
    pub(super) db: Db,
    pub(super) room_event_tx: broadcast::Sender<RoomGameEvent>,
}

#[derive(Debug)]
struct AsterionLifecycle {
    stopped: AtomicBool,
}

#[derive(Debug, Default)]
struct AsterionSessions {
    sessions: StdMutex<HashMap<Uuid, HashSet<Uuid>>>,
}

impl AsterionSessions {
    fn add(&self, user_id: Uuid, session_id: Uuid) {
        self.sessions
            .lock_recover()
            .entry(user_id)
            .or_default()
            .insert(session_id);
    }

    fn contains(&self, user_id: Uuid, session_id: Uuid) -> bool {
        self.sessions
            .lock_recover()
            .get(&user_id)
            .is_some_and(|sessions| sessions.contains(&session_id))
    }

    fn contains_user(&self, user_id: Uuid) -> bool {
        self.sessions.lock_recover().contains_key(&user_id)
    }

    fn remove(&self, user_id: Uuid, session_id: Uuid) -> bool {
        let mut sessions = self.sessions.lock_recover();
        let Some(user_sessions) = sessions.get_mut(&user_id) else {
            return false;
        };
        user_sessions.remove(&session_id);
        if !user_sessions.is_empty() {
            return false;
        }
        sessions.remove(&user_id);
        true
    }
}

impl AsterionLifecycle {
    fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
        }
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }
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
    pub daily_prize_claimed: bool,
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
            daily_prize_claimed: false,
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
    pub(super) fn new_with_events(init: AsterionServiceInit) -> anyhow::Result<Self> {
        let AsterionServiceInit {
            room_id,
            chip_svc,
            activity,
            rooms_service,
            db,
            room_event_tx,
        } = init;
        let game = Game::new()?;
        let state = SharedState::new(room_id, game);
        let initial = state.public_snapshot();
        let (public_tx, public_rx) = watch::channel(initial);
        let svc = Self {
            room_id,
            room_event_tx,
            public_tx,
            public_rx,
            sessions: Arc::new(AsterionSessions::default()),
            private: Arc::new(StdMutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(state)),
            lifecycle: Arc::new(AsterionLifecycle::new()),
            room_in_round: Arc::new(AtomicBool::new(false)),
            chip_svc,
            rooms_service,
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

    pub fn is_stopped(&self) -> bool {
        self.lifecycle.is_stopped()
    }

    pub fn register_session(&self, user_id: Uuid, session_id: Uuid) {
        self.sessions.add(user_id, session_id);
    }

    pub fn has_session_for_user(&self, user_id: Uuid) -> bool {
        self.sessions.contains_user(user_id)
    }

    pub(super) fn unregister_session(&self, user_id: Uuid, session_id: Uuid) {
        self.sessions.remove(user_id, session_id);
    }

    pub fn join_task(&self, user_id: Uuid, session_id: Uuid) {
        self.sessions.add(user_id, session_id);
        let svc = self.clone();
        tokio::spawn(async move {
            let name = lookup_username(&svc.db, user_id)
                .await
                .unwrap_or_else(|| fallback_name(user_id));
            let daily_prize_claimed = match svc
                .chip_svc
                .has_asterion_daily_escape(user_id, Utc::now().date_naive())
                .await
            {
                Ok(claimed) => claimed,
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        %user_id,
                        "failed to load Asterion daily escape prize status"
                    );
                    false
                }
            };
            let join = {
                let mut state = svc.state.lock().await;
                if !svc.sessions.contains(user_id, session_id) {
                    return;
                }
                let join = state.add_player(user_id, &name, daily_prize_claimed);
                svc.publish_public(&state);
                join
            };
            match join {
                PlayerJoin::Added => {
                    let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                        room_id: svc.room_id,
                        user_id,
                    });
                }
                PlayerJoin::AlreadyPresent => {}
                PlayerJoin::Full => {
                    svc.publish_rejected(user_id);
                }
            }
        });
    }

    pub fn leave_task(&self, user_id: Uuid, session_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            let mut sessions = svc.sessions.sessions.lock_recover();
            let Some(user_sessions) = sessions.get_mut(&user_id) else {
                return;
            };
            user_sessions.remove(&session_id);
            if !user_sessions.is_empty() {
                return;
            }
            sessions.remove(&user_id);
            state.remove_player(user_id);
            svc.publish_public(&state);
            svc.private.lock_recover().remove(&user_id);
        });
    }

    pub fn touch_activity_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let should_touch_room = {
                let mut state = svc.state.lock().await;
                state.record_activity(user_id, Instant::now())
            };
            if should_touch_room {
                svc.rooms_service.touch_room_task(svc.room_id);
            }
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
                if svc.lifecycle.is_stopped() {
                    break;
                }
                let new_wins = {
                    let mut state = svc.state.lock().await;
                    let sessions = svc.sessions.sessions.lock_recover();
                    if sessions.is_empty() && state.should_stop(Instant::now(), EMPTY_SERVICE_TTL) {
                        svc.lifecycle.stop();
                        break;
                    }
                    drop(sessions);
                    if state.hero_count() == 0 {
                        continue;
                    }
                    state.update();
                    let wins = state.drain_new_wins();
                    svc.publish_public(&state);
                    wins
                };
                for user_id in new_wins {
                    svc.handle_escape_task(user_id);
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
                if svc.lifecycle.is_stopped() {
                    break;
                }
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

    fn handle_escape_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let escape_date = Utc::now().date_naive();
            let payout = match svc
                .chip_svc
                .credit_asterion_daily_escape(user_id, escape_date)
                .await
            {
                Ok(payout) => payout,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        %user_id,
                        "failed to credit Asterion daily escape payout"
                    );
                    svc.activity
                        .game_won_task(user_id, ActivityGame::Asterion, None, None);
                    return;
                }
            };
            {
                let mut state = svc.state.lock().await;
                state.mark_daily_prize_claimed(user_id);
            }
            let detail = payout.credited.then(|| format!("{} chips", payout.amount));
            svc.activity
                .game_won_task(user_id, ActivityGame::Asterion, detail, None);
        });
    }

    fn publish_public(&self, state: &SharedState) {
        diff_set(&self.public_tx, state.public_snapshot());
        self.sync_room_status(state.round_active());
    }

    fn sync_room_status(&self, in_round: bool) {
        self.rooms_service.sync_room_status_task(
            self.room_id,
            self.room_in_round.clone(),
            in_round,
        );
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
    rejected: HashSet<Uuid>,
    wins_announced: HashSet<Uuid>,
    daily_prize_claimed: HashSet<Uuid>,
    empty_since: Option<Instant>,
    last_room_touch: Option<Instant>,
}

impl SharedState {
    fn new(room_id: Uuid, game: Game) -> Self {
        Self {
            room_id,
            game,
            players: HashSet::new(),
            rejected: HashSet::new(),
            wins_announced: HashSet::new(),
            daily_prize_claimed: HashSet::new(),
            empty_since: Some(Instant::now()),
            last_room_touch: None,
        }
    }

    fn add_player(&mut self, user_id: Uuid, name: &str, daily_prize_claimed: bool) -> PlayerJoin {
        if self.players.contains(&user_id) {
            self.rejected.remove(&user_id);
            if daily_prize_claimed {
                self.daily_prize_claimed.insert(user_id);
            }
            return PlayerJoin::AlreadyPresent;
        }
        if self.players.len() >= MAX_HEROES_PER_ROOM {
            self.rejected.insert(user_id);
            return PlayerJoin::Full;
        }
        self.rejected.remove(&user_id);
        if daily_prize_claimed {
            self.daily_prize_claimed.insert(user_id);
        } else {
            self.daily_prize_claimed.remove(&user_id);
        }
        self.players.insert(user_id);
        self.empty_since = None;
        self.game.add_player(user_id, name);
        PlayerJoin::Added
    }

    fn remove_player(&mut self, user_id: Uuid) {
        if self.players.remove(&user_id) {
            self.game.remove_player(&user_id);
            if self.players.is_empty() {
                self.empty_since = Some(Instant::now());
            }
        }
        self.wins_announced.remove(&user_id);
        self.rejected.remove(&user_id);
        self.daily_prize_claimed.remove(&user_id);
    }

    fn record_activity(&mut self, user_id: Uuid, now: Instant) -> bool {
        if !self.players.contains(&user_id) {
            return false;
        }
        if self
            .last_room_touch
            .is_some_and(|last| now.duration_since(last) < ROOM_TOUCH_INTERVAL)
        {
            return false;
        }
        self.last_room_touch = Some(now);
        true
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
            match self.game.get_hero(user_id) {
                Some(hero) if hero.has_won().is_some() => {
                    if !self.wins_announced.contains(user_id) {
                        wins.push(*user_id);
                    }
                }
                _ => {
                    self.wins_announced.remove(user_id);
                }
            }
        }
        self.wins_announced.extend(wins.iter().copied());
        wins
    }

    fn mark_daily_prize_claimed(&mut self, user_id: Uuid) {
        self.daily_prize_claimed.insert(user_id);
    }

    fn hero_count(&self) -> usize {
        self.players.len()
    }

    fn round_active(&self) -> bool {
        self.hero_count() > 0
    }

    fn should_stop(&self, now: Instant, ttl: Duration) -> bool {
        self.empty_since
            .is_some_and(|empty_since| now.duration_since(empty_since) >= ttl)
    }

    fn public_snapshot(&self) -> AsterionPublicSnapshot {
        AsterionPublicSnapshot {
            room_id: self.room_id,
            hero_count: self.hero_count(),
        }
    }

    fn private_snapshot(&self, user_id: Uuid, background: Rgba<u8>) -> AsterionPrivateSnapshot {
        let Some(hero) = self.game.get_hero(&user_id) else {
            if self.rejected.contains(&user_id) {
                return AsterionPrivateSnapshot {
                    rejected: true,
                    ..AsterionPrivateSnapshot::empty(user_id)
                };
            }
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
            daily_prize_claimed: self.daily_prize_claimed.contains(&user_id),
            view,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerJoin {
    Added,
    AlreadyPresent,
    Full,
}

async fn lookup_username(db: &Db, user_id: Uuid) -> Option<String> {
    let client = db.get().await.ok()?;
    let mut map = User::list_usernames_by_ids(&client, &[user_id])
        .await
        .ok()?;
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
    use super::{AsterionSessions, SharedState, fallback_name, sanitize_username};
    use asterion_core::Game;
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

    #[test]
    fn sessions_only_remove_player_after_last_session_leaves() {
        let sessions = AsterionSessions::default();
        let user_id = Uuid::now_v7();
        let first = Uuid::now_v7();
        let second = Uuid::now_v7();

        sessions.add(user_id, first);
        sessions.add(user_id, second);

        assert!(sessions.contains(user_id, first));
        assert!(!sessions.remove(user_id, first));
        assert!(sessions.contains(user_id, second));
        assert!(sessions.remove(user_id, second));
        assert!(!sessions.contains(user_id, second));
    }

    #[test]
    fn drain_new_wins_clears_announced_state_after_reset() {
        let user_id = Uuid::now_v7();
        let mut state = SharedState::new(Uuid::now_v7(), Game::new().expect("game builds"));
        state.add_player(user_id, "alice", false);
        state.wins_announced.insert(user_id);

        assert!(state.drain_new_wins().is_empty());
        assert!(!state.wins_announced.contains(&user_id));
    }
}
