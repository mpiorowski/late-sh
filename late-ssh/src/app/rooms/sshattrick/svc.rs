use std::collections::{HashMap, HashSet};
use std::sync::Mutex as StdMutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use image::RgbaImage;
use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::reward::SSHATTRICK_WIN_REWARD_KEY;
use late_core::models::user::User;
use sshattrick_core::{Game, GameCommand, GameSide, GameState, Palette};
use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
    rooms::{backend::RoomGameEvent, svc::RoomsService},
};

const UPDATE_TIME_STEP: Duration = Duration::from_millis(10);
const DRAW_TIME_STEP: Duration = Duration::from_millis(33);
const EMPTY_SERVICE_TTL: Duration = Duration::from_secs(5 * 60);
const ROOM_TOUCH_INTERVAL: Duration = Duration::from_secs(60);
const SSHATTRICK_WIN_LEDGER_REASON: &str = "sshattrick_win";
pub const SSHATTRICK_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(15 * 60);
pub const SSHATTRICK_WIN_CHIP_PAYOUT: i64 = 300;

pub const SEATS_PER_ROOM: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Waiting,
    Starting,
    Running,
    AfterGoal,
    Ending,
}

impl Phase {
    fn from_game(state: &GameState) -> Self {
        match state {
            GameState::Starting { .. } => Self::Starting,
            GameState::Running => Self::Running,
            GameState::AfterGoal { .. } => Self::AfterGoal,
            GameState::Ending { .. } => Self::Ending,
        }
    }
}

enum SitOutcome {
    Seated,
    AlreadySeated,
    Full,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub user_id: Uuid,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshattrickPublicSnapshot {
    pub room_id: Uuid,
    pub red: Option<Seat>,
    pub blue: Option<Seat>,
    pub red_score: u8,
    pub blue_score: u8,
    pub time_left_ms: u128,
    pub phase: Phase,
    pub winner: Option<GameSide>,
    pub scored: Option<GameSide>,
    pub by_disconnect: bool,
    pub palette: Palette,
    pub starting_remaining_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SshattrickPrivateSnapshot {
    pub user_id: Uuid,
    pub seated_as: Option<GameSide>,
    pub view: Option<RenderedView>,
}

impl SshattrickPrivateSnapshot {
    fn empty(user_id: Uuid) -> Self {
        Self {
            user_id,
            seated_as: None,
            view: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderedView {
    pub image: Arc<RgbaImage>,
}

impl PartialEq for RenderedView {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.image, &other.image)
            || (self.image.dimensions() == other.image.dimensions()
                && self.image.as_raw() == other.image.as_raw())
    }
}

#[derive(Clone)]
pub struct SshattrickService {
    room_id: Uuid,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    public_tx: watch::Sender<SshattrickPublicSnapshot>,
    public_rx: watch::Receiver<SshattrickPublicSnapshot>,
    sessions: Arc<Sessions>,
    private: Arc<StdMutex<HashMap<Uuid, watch::Sender<SshattrickPrivateSnapshot>>>>,
    state: Arc<Mutex<SharedState>>,
    lifecycle: Arc<Lifecycle>,
    room_in_round: Arc<AtomicBool>,
    rooms_service: RoomsService,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    db: Db,
}

pub(super) struct SshattrickServiceInit {
    pub(super) room_id: Uuid,
    pub(super) rooms_service: RoomsService,
    pub(super) chip_svc: ChipService,
    pub(super) activity: ActivityPublisher,
    pub(super) db: Db,
    pub(super) room_event_tx: broadcast::Sender<RoomGameEvent>,
}

#[derive(Debug)]
struct Lifecycle {
    stopped: AtomicBool,
}

impl Lifecycle {
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

#[derive(Debug, Default)]
struct Sessions {
    sessions: StdMutex<HashMap<Uuid, HashSet<Uuid>>>,
}

impl Sessions {
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

impl SshattrickService {
    pub(super) fn new_with_events(init: SshattrickServiceInit) -> Self {
        let SshattrickServiceInit {
            room_id,
            rooms_service,
            chip_svc,
            activity,
            db,
            room_event_tx,
        } = init;
        let state = SharedState::new(room_id);
        let initial = state.public_snapshot();
        let (public_tx, public_rx) = watch::channel(initial);
        let svc = Self {
            room_id,
            room_event_tx,
            public_tx,
            public_rx,
            sessions: Arc::new(Sessions::default()),
            private: Arc::new(StdMutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(state)),
            lifecycle: Arc::new(Lifecycle::new()),
            room_in_round: Arc::new(AtomicBool::new(false)),
            rooms_service,
            chip_svc,
            activity,
            db,
        };
        svc.spawn_update_task();
        svc.spawn_render_task();
        svc
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_public(&self) -> watch::Receiver<SshattrickPublicSnapshot> {
        self.public_rx.clone()
    }

    pub fn subscribe_private(&self, user_id: Uuid) -> watch::Receiver<SshattrickPrivateSnapshot> {
        let mut private = self.private.lock_recover();
        if let Some(existing) = private.get(&user_id) {
            return existing.subscribe();
        }
        let (tx, rx) = watch::channel(SshattrickPrivateSnapshot::empty(user_id));
        private.insert(user_id, tx);
        rx
    }

    pub fn current_public(&self) -> SshattrickPublicSnapshot {
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

    pub fn seated_user_ids(&self) -> (Option<Uuid>, Option<Uuid>) {
        let snapshot = self.public_rx.borrow();
        (
            snapshot.red.as_ref().map(|s| s.user_id),
            snapshot.blue.as_ref().map(|s| s.user_id),
        )
    }

    pub fn join_task(&self, user_id: Uuid, session_id: Uuid) {
        self.sessions.add(user_id, session_id);
        let svc = self.clone();
        tokio::spawn(async move {
            let name = lookup_username(&svc.db, user_id)
                .await
                .unwrap_or_else(|| fallback_name(user_id));
            let mut state = svc.state.lock().await;
            if !svc.sessions.contains(user_id, session_id) {
                return;
            }
            state.register_user(user_id, name);
            svc.publish_public(&state);
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
            drop(sessions);
            let winner_user_id = state.remove_user(user_id);
            svc.publish_public(&state);
            svc.private.lock_recover().remove(&user_id);
            svc.publish_win(winner_user_id);
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
        });
    }

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let newly_seated = {
                let mut state = svc.state.lock().await;
                let outcome = state.sit(user_id);
                svc.publish_public(&state);
                matches!(outcome, SitOutcome::Seated)
            };
            if newly_seated {
                let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                    room_id: svc.room_id,
                    user_id,
                });
            }
        });
    }

    pub fn reset_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.reset_game();
            svc.publish_public(&state);
        });
    }

    fn spawn_update_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(UPDATE_TIME_STEP);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                if svc.lifecycle.is_stopped() {
                    break;
                }
                let mut state = svc.state.lock().await;
                let sessions_empty = svc.sessions.sessions.lock_recover().is_empty();
                if sessions_empty && state.should_stop(Instant::now(), EMPTY_SERVICE_TTL) {
                    svc.lifecycle.stop();
                    break;
                }
                let update = state.update();
                if update.changed {
                    svc.publish_public(&state);
                }
                drop(state);
                svc.publish_win(update.winner_user_id);
            }
        });
    }

    fn spawn_render_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(DRAW_TIME_STEP);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                if svc.lifecycle.is_stopped() {
                    break;
                }
                let recipients: Vec<(Uuid, watch::Sender<SshattrickPrivateSnapshot>)> = svc
                    .private
                    .lock_recover()
                    .iter()
                    .map(|(id, tx)| (*id, tx.clone()))
                    .collect();
                if recipients.is_empty() {
                    continue;
                }
                let (image, red_uuid, blue_uuid) = {
                    let state = svc.state.lock().await;
                    let red = state.red.as_ref().map(|s| s.user_id);
                    let blue = state.blue.as_ref().map(|s| s.user_id);
                    let image = match state.game.as_ref() {
                        Some(game) => match game.draw() {
                            Ok(img) => Some(Arc::new(img)),
                            Err(err) => {
                                tracing::warn!(error = ?err, "sshattrick draw failed");
                                None
                            }
                        },
                        None => None,
                    };
                    (image, red, blue)
                };
                for (user_id, tx) in recipients {
                    let seated_as = if Some(user_id) == red_uuid {
                        Some(GameSide::Red)
                    } else if Some(user_id) == blue_uuid {
                        Some(GameSide::Blue)
                    } else {
                        None
                    };
                    let view = image.as_ref().map(|img| RenderedView {
                        image: Arc::clone(img),
                    });
                    let next = SshattrickPrivateSnapshot {
                        user_id,
                        seated_as,
                        view,
                    };
                    diff_set(&tx, next);
                }
            }
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

    fn publish_win(&self, winner_user_id: Option<Uuid>) {
        let Some(user_id) = winner_user_id else {
            return;
        };
        let chip_svc = self.chip_svc.clone();
        let activity = self.activity.clone();
        tokio::spawn(async move {
            match chip_svc
                .credit_cooldown_reward_template(
                    user_id,
                    SSHATTRICK_WIN_REWARD_KEY,
                    SSHATTRICK_WIN_LEDGER_REASON,
                )
                .await
            {
                Ok(payout) => {
                    let detail = payout.credited.then(|| format!("{} chips", payout.amount));
                    activity.game_won_task(user_id, ActivityGame::Sshattrick, detail, None);
                    if !payout.credited {
                        tracing::info!(
                            user_id = %user_id,
                            payout = payout.amount,
                            "suppressed ssHattrick win chips due to payout cooldown"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        "failed to credit ssHattrick win chips"
                    );
                    activity.game_won_task(user_id, ActivityGame::Sshattrick, None, None);
                }
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct UpdateOutcome {
    changed: bool,
    winner_user_id: Option<Uuid>,
}

struct SharedState {
    room_id: Uuid,
    game: Option<Game>,
    red: Option<Seat>,
    blue: Option<Seat>,
    known_users: HashMap<Uuid, String>,
    empty_since: Option<Instant>,
    last_room_touch: Option<Instant>,
    winner: Option<GameSide>,
}

impl SharedState {
    fn new(room_id: Uuid) -> Self {
        Self {
            room_id,
            game: None,
            red: None,
            blue: None,
            known_users: HashMap::new(),
            empty_since: Some(Instant::now()),
            last_room_touch: None,
            winner: None,
        }
    }

    fn register_user(&mut self, user_id: Uuid, name: String) {
        self.known_users.insert(user_id, name);
        self.empty_since = None;
    }

    fn remove_user(&mut self, user_id: Uuid) -> Option<Uuid> {
        self.known_users.remove(&user_id);
        let was_seated_red = self.red.as_ref().is_some_and(|s| s.user_id == user_id);
        let was_seated_blue = self.blue.as_ref().is_some_and(|s| s.user_id == user_id);
        let mut winner_user_id = None;
        if was_seated_red {
            winner_user_id = self.blue.as_ref().map(|seat| seat.user_id);
            self.red = None;
        }
        if was_seated_blue {
            winner_user_id = self.red.as_ref().map(|seat| seat.user_id);
            self.blue = None;
        }
        if let Some(game) = self.game.as_mut()
            && (was_seated_red || was_seated_blue)
            && !matches!(game.state, GameState::Ending { .. })
        {
            let winner = if was_seated_red {
                Some(GameSide::Blue)
            } else {
                Some(GameSide::Red)
            };
            game.end_with_winner(winner, true);
            self.winner = winner;
        } else if was_seated_red || was_seated_blue {
            winner_user_id = None;
        }
        if self.known_users.is_empty() {
            self.empty_since = Some(Instant::now());
            self.game = None;
            // Keep `self.winner` so post-mortem snapshots reflect the final
            // outcome instead of flipping to None on the trailing publish.
        }
        winner_user_id
    }

    fn sit(&mut self, user_id: Uuid) -> SitOutcome {
        let Some(name) = self.known_users.get(&user_id).cloned() else {
            return SitOutcome::Unknown;
        };
        if self.red.as_ref().is_some_and(|s| s.user_id == user_id)
            || self.blue.as_ref().is_some_and(|s| s.user_id == user_id)
        {
            return SitOutcome::AlreadySeated;
        }
        if self.red.is_none() {
            self.red = Some(Seat { user_id, name });
        } else if self.blue.is_none() {
            self.blue = Some(Seat { user_id, name });
        } else {
            return SitOutcome::Full;
        }
        if self.red.is_some() && self.blue.is_some() && self.game.is_none() {
            self.game = Some(Game::new());
            self.winner = None;
        }
        SitOutcome::Seated
    }

    /// Called from the survivor's "press N for rematch" path. If both seats
    /// are filled, recreate the game. Otherwise (one seat empty because the
    /// other player disconnected mid-match) drop the Ending state back to
    /// `Phase::Waiting` so the survivor isn't stuck on the win banner.
    fn reset_game(&mut self) {
        if self.red.is_some() && self.blue.is_some() {
            self.game = Some(Game::new());
            self.winner = None;
        } else {
            self.game = None;
            self.winner = None;
        }
    }

    fn record_activity(&mut self, user_id: Uuid, now: Instant) -> bool {
        if !self.known_users.contains_key(&user_id) {
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
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let side = if self.red.as_ref().is_some_and(|s| s.user_id == user_id) {
            GameSide::Red
        } else if self.blue.as_ref().is_some_and(|s| s.user_id == user_id) {
            GameSide::Blue
        } else {
            return;
        };
        game.handle_command(side, command);
    }

    /// Advances the game state by one tick and reports snapshot/payout effects.
    fn update(&mut self) -> UpdateOutcome {
        let Some(game) = self.game.as_mut() else {
            return UpdateOutcome::default();
        };
        if let Err(err) = game.update() {
            tracing::warn!(error = ?err, "sshattrick update error");
        }
        if let GameState::Ending { winner, .. } = game.state
            && self.winner != winner
        {
            self.winner = winner;
            return UpdateOutcome {
                changed: true,
                winner_user_id: winner.and_then(|side| self.user_for_side(side)),
            };
        }
        UpdateOutcome {
            changed: true,
            winner_user_id: None,
        }
    }

    fn round_active(&self) -> bool {
        self.game.as_ref().is_some_and(|game| {
            matches!(
                Phase::from_game(&game.state),
                Phase::Starting | Phase::Running | Phase::AfterGoal
            )
        })
    }

    fn user_for_side(&self, side: GameSide) -> Option<Uuid> {
        match side {
            GameSide::Red => self.red.as_ref().map(|seat| seat.user_id),
            GameSide::Blue => self.blue.as_ref().map(|seat| seat.user_id),
        }
    }

    fn should_stop(&self, now: Instant, ttl: Duration) -> bool {
        self.empty_since
            .is_some_and(|empty_since| now.duration_since(empty_since) >= ttl)
    }

    fn public_snapshot(&self) -> SshattrickPublicSnapshot {
        let (
            red_score,
            blue_score,
            time_left_ms,
            phase,
            scored,
            by_disconnect,
            palette,
            starting_remaining_ms,
        ) = match self.game.as_ref() {
            Some(game) => {
                let phase = Phase::from_game(&game.state);
                let time_left = Game::DURATION_MILLISECONDS.saturating_sub(game.timer);
                let scored = match game.state {
                    GameState::AfterGoal { scored, .. } => Some(scored),
                    _ => None,
                };
                let by_disconnect = match game.state {
                    GameState::Ending { by_disconnect, .. } => by_disconnect,
                    _ => false,
                };
                let starting_remaining_ms = match game.state {
                    GameState::Starting { time } => {
                        let elapsed = time.elapsed().as_millis() as u64;
                        Some(Game::STARTING_DELAY_MILLISECONDS.saturating_sub(elapsed))
                    }
                    _ => None,
                };
                (
                    game.red_data.score,
                    game.blue_data.score,
                    time_left,
                    phase,
                    scored,
                    by_disconnect,
                    game.palette,
                    starting_remaining_ms,
                )
            }
            None => (
                0,
                0,
                Game::DURATION_MILLISECONDS,
                Phase::Waiting,
                None,
                false,
                Palette::default(),
                None,
            ),
        };
        SshattrickPublicSnapshot {
            room_id: self.room_id,
            red: self.red.clone(),
            blue: self.blue.clone(),
            red_score,
            blue_score,
            time_left_ms,
            phase,
            winner: self.winner,
            scored,
            by_disconnect,
            palette,
            starting_remaining_ms,
        }
    }
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
