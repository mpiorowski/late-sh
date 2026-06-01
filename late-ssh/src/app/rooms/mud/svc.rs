// Lateania world runtime: the authoritative, in-memory truth for one MUD world.
//
// One service per game room (the late "room" is the whole world). Many sessions
// share it via the manager's HashMap; each has its own `state::State`. Mutations
// serialize through `Arc<Mutex<WorldState>>`; reads are lock-free against each
// session's cached snapshot. A background tick loop advances combat rounds and
// mob respawns, then publishes a fresh snapshot.
//
// Combat is round-based on the world tick (every `TICK_SECS`), matching the
// classic MUD feel and reusing the loop shape proven by Tron.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    rooms::backend::RoomGameEvent,
};

use super::world::{Dir, MobSpawn, RoomId, World, seed_world};

/// World heartbeat. One combat round resolves per tick.
const TICK_SECS: u64 = 2;
/// A player who sends no command for this long is dropped from the world.
const PLAYER_IDLE_TIMEOUT_SECS: u64 = 10 * 60;
/// Player base stats for the slice (one class: Warrior).
const PLAYER_MAX_HP: i32 = 40;
const PLAYER_DAMAGE: i32 = 6;
/// How long a defeated player rests before respawning at the start room.
const PLAYER_RESPAWN_SECS: u64 = 8;

#[derive(Clone)]
pub struct MudService {
    room_id: Uuid,
    activity: ActivityPublisher,
    room_event_tx: broadcast::Sender<RoomGameEvent>,
    snapshot_tx: watch::Sender<MudSnapshot>,
    snapshot_rx: watch::Receiver<MudSnapshot>,
    state: Arc<Mutex<WorldState>>,
}

// ---- Snapshot (what sessions render) -------------------------------------

/// A line in a player's scrolling message log.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub text: String,
    pub kind: LogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Normal,
    Combat,
    System,
    Say,
}

/// A mob as seen in a room.
#[derive(Clone, Debug)]
pub struct MobView {
    pub name: String,
    pub hp: i32,
    pub max_hp: i32,
}

/// One other player visible in the same room.
#[derive(Clone, Debug)]
pub struct OccupantView {
    pub user_id: Uuid,
    pub hp: i32,
    pub max_hp: i32,
    pub in_combat: bool,
}

/// Per-session snapshot: filtered to the player's current room plus their own
/// character sheet and message log. This mirrors poker's public/private split -
/// each session only receives what its own player can see.
#[derive(Clone, Debug)]
pub struct MudSnapshot {
    pub room_id: Uuid,
    pub generation: u64,
    /// Per-player views keyed by user id. A session reads its own entry.
    pub players: HashMap<Uuid, PlayerView>,
}

#[derive(Clone, Debug)]
pub struct PlayerView {
    pub joined: bool,
    pub alive: bool,
    pub hp: i32,
    pub max_hp: i32,
    pub xp: i32,
    pub level: i32,
    pub room_name: String,
    pub room_desc: String,
    pub zone: String,
    pub safe: bool,
    pub exits: Vec<(Dir, String)>,
    pub mobs: Vec<MobView>,
    pub occupants: Vec<OccupantView>,
    pub in_combat_with: Option<String>,
    pub log: Vec<LogLine>,
    pub respawning: bool,
}

impl PlayerView {
    fn empty(room_id: Uuid) -> Self {
        let _ = room_id;
        Self {
            joined: false,
            alive: false,
            hp: 0,
            max_hp: 0,
            xp: 0,
            level: 1,
            room_name: String::new(),
            room_desc: String::new(),
            zone: String::new(),
            safe: true,
            exits: Vec::new(),
            mobs: Vec::new(),
            occupants: Vec::new(),
            in_combat_with: None,
            log: Vec::new(),
            respawning: false,
        }
    }
}

impl MudService {
    pub fn new(room_id: Uuid, activity: ActivityPublisher) -> Self {
        let (room_event_tx, _) = broadcast::channel::<RoomGameEvent>(16);
        Self::new_with_events(room_id, activity, room_event_tx)
    }

    pub fn new_with_events(
        room_id: Uuid,
        activity: ActivityPublisher,
        room_event_tx: broadcast::Sender<RoomGameEvent>,
    ) -> Self {
        let state = WorldState::new(room_id, seed_world());
        let initial = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let svc = Self {
            room_id,
            activity,
            room_event_tx,
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
        };
        svc.start_tick_loop();
        svc
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_state(&self) -> watch::Receiver<MudSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> MudSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    /// Number of adventurers currently in the world (for directory hints).
    pub fn player_count(&self) -> usize {
        self.snapshot_rx
            .borrow()
            .players
            .values()
            .filter(|p| p.joined)
            .count()
    }

    pub fn is_user_present(&self, user_id: Uuid) -> bool {
        self.snapshot_rx
            .borrow()
            .players
            .get(&user_id)
            .is_some_and(|p| p.joined)
    }

    // ---- Commands (fire-and-forget, *_task convention) -------------------

    pub fn join_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let joined = {
                let mut state = svc.state.lock().await;
                let joined = state.join(user_id);
                state.touch(user_id);
                svc.publish(&state);
                joined
            };
            if joined {
                let _ = svc.room_event_tx.send(RoomGameEvent::SeatJoined {
                    room_id: svc.room_id,
                    user_id,
                });
            }
        });
    }

    pub fn leave_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.leave(user_id);
            svc.publish(&state);
        });
    }

    pub fn move_task(&self, user_id: Uuid, dir: Dir) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.move_player(user_id, dir);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn look_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.look(user_id);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn attack_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.engage(user_id);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn flee_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.flee(user_id);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn say_task(&self, user_id: Uuid, message: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.say(user_id, &message);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn touch_activity_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.touch(user_id);
        });
    }

    // ---- Tick loop ------------------------------------------------------

    fn start_tick_loop(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
            loop {
                ticker.tick().await;
                let mut state = svc.state.lock().await;
                let outcomes = state.tick();
                if state.dirty {
                    svc.publish(&state);
                    state.dirty = false;
                }
                drop(state);
                for outcome in outcomes {
                    svc.activity.game_won_task(
                        outcome.user_id,
                        ActivityGame::Mud,
                        Some(format!("slew {}", outcome.mob_name)),
                        None,
                    );
                }
            }
        });
    }

    fn publish(&self, state: &WorldState) {
        let _ = self.snapshot_tx.send(state.snapshot());
    }
}

/// Reported when a player lands a killing blow, so the world feed can announce it.
struct KillOutcome {
    user_id: Uuid,
    mob_name: String,
}

// ---- The authoritative world state ---------------------------------------

struct PlayerState {
    user_id: Uuid,
    hp: i32,
    max_hp: i32,
    damage: i32,
    xp: i32,
    level: i32,
    room: RoomId,
    /// Mob instance id this player is fighting, if any.
    target: Option<u32>,
    last_activity: Instant,
    /// Some(deadline) while the player is downed and waiting to respawn.
    respawn_at: Option<Instant>,
    log: Vec<LogLine>,
}

struct MobInstance {
    spawn: MobSpawn,
    hp: i32,
    alive: bool,
    /// Some(deadline) while dead and waiting to respawn.
    respawn_at: Option<Instant>,
}

struct WorldState {
    room_id: Uuid,
    world: World,
    players: HashMap<Uuid, PlayerState>,
    mobs: HashMap<u32, MobInstance>,
    generation: u64,
    /// Set by tick() when something changed and a publish is warranted.
    dirty: bool,
}

const LOG_CAP: usize = 50;

impl WorldState {
    fn new(room_id: Uuid, world: World) -> Self {
        let mobs = world
            .spawns
            .iter()
            .map(|spawn| {
                (
                    spawn.id,
                    MobInstance {
                        spawn: spawn.clone(),
                        hp: spawn.max_hp,
                        alive: true,
                        respawn_at: None,
                    },
                )
            })
            .collect();
        Self {
            room_id,
            world,
            players: HashMap::new(),
            mobs,
            generation: 0,
            dirty: false,
        }
    }

    fn join(&mut self, user_id: Uuid) -> bool {
        if self.players.contains_key(&user_id) {
            return false;
        }
        let start = self.world.start_room;
        let mut player = PlayerState {
            user_id,
            hp: PLAYER_MAX_HP,
            max_hp: PLAYER_MAX_HP,
            damage: PLAYER_DAMAGE,
            xp: 0,
            level: 1,
            room: start,
            target: None,
            last_activity: Instant::now(),
            respawn_at: None,
            log: Vec::new(),
        };
        push_log(
            &mut player.log,
            LogKind::System,
            "You step into the world of Lateania.".to_string(),
        );
        self.players.insert(user_id, player);
        self.describe_room(user_id);
        true
    }

    fn leave(&mut self, user_id: Uuid) {
        self.players.remove(&user_id);
    }

    fn touch(&mut self, user_id: Uuid) {
        if let Some(player) = self.players.get_mut(&user_id) {
            player.last_activity = Instant::now();
        }
    }

    fn move_player(&mut self, user_id: Uuid, dir: Dir) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.respawn_at.is_some() {
            self.log_to(user_id, LogKind::System, "You are recovering.".to_string());
            return;
        }
        if player.target.is_some() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You can't leave - you're in combat! Flee first.".to_string(),
            );
            return;
        }
        let Some(room) = self.world.room(player.room) else {
            return;
        };
        let Some(&dest) = room.exits.get(&dir) else {
            self.log_to(
                user_id,
                LogKind::Normal,
                format!("You can't go {}.", dir.label()),
            );
            return;
        };
        if let Some(player) = self.players.get_mut(&user_id) {
            player.room = dest;
        }
        self.describe_room(user_id);
    }

    fn look(&mut self, user_id: Uuid) {
        self.describe_room(user_id);
    }

    fn describe_room(&mut self, user_id: Uuid) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        let room_id = player.room;
        let Some(room) = self.world.room(room_id) else {
            return;
        };
        // Extract everything from the room (an immutable borrow of self.world)
        // before any self.log_to call, which needs &mut self.
        let name = room.name.to_string();
        let desc = room.desc.to_string();
        let mut exits: Vec<&'static str> = room.exits.keys().map(|d| d.label()).collect();
        exits.sort_unstable();
        let exit_text = if exits.is_empty() {
            "none".to_string()
        } else {
            exits.join(", ")
        };
        let mob_names: Vec<String> = self
            .mobs
            .values()
            .filter(|m| m.alive && m.spawn.home == room_id)
            .map(|m| m.spawn.name.to_string())
            .collect();
        self.log_to(user_id, LogKind::Normal, format!("== {name} =="));
        self.log_to(user_id, LogKind::Normal, desc);
        self.log_to(user_id, LogKind::System, format!("Exits: {exit_text}"));
        for mob in mob_names {
            self.log_to(user_id, LogKind::Combat, format!("{mob} is here."));
        }
    }

    fn engage(&mut self, user_id: Uuid) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.respawn_at.is_some() {
            return;
        }
        let room_id = player.room;
        if self.world.room(room_id).is_some_and(|r| r.safe) {
            self.log_to(
                user_id,
                LogKind::System,
                "This is a safe haven. No fighting here.".to_string(),
            );
            return;
        }
        let target = self
            .mobs
            .values()
            .find(|m| m.alive && m.spawn.home == room_id)
            .map(|m| m.spawn.id);
        match target {
            Some(mob_id) => {
                let mob_name = self
                    .mobs
                    .get(&mob_id)
                    .map(|m| m.spawn.name.to_string())
                    .unwrap_or_default();
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.target = Some(mob_id);
                }
                self.log_to(user_id, LogKind::Combat, format!("You attack {mob_name}!"));
            }
            None => {
                self.log_to(
                    user_id,
                    LogKind::Normal,
                    "There's nothing here to fight.".to_string(),
                );
            }
        }
    }

    fn flee(&mut self, user_id: Uuid) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.target.is_none() {
            self.log_to(
                user_id,
                LogKind::Normal,
                "You're not fighting anything.".to_string(),
            );
            return;
        }
        let room_id = player.room;
        let exit = self
            .world
            .room(room_id)
            .and_then(|r| r.exits.iter().next().map(|(dir, dest)| (*dir, *dest)));
        if let Some(player) = self.players.get_mut(&user_id) {
            player.target = None;
        }
        match exit {
            Some((dir, dest)) => {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.room = dest;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("You flee {}!", dir.label()),
                );
                self.describe_room(user_id);
            }
            None => {
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    "You break off the fight.".to_string(),
                );
            }
        }
    }

    fn say(&mut self, user_id: Uuid, message: &str) {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }
        let room_id = match self.players.get(&user_id) {
            Some(player) => player.room,
            None => return,
        };
        let occupants: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.room == room_id)
            .map(|(id, _)| *id)
            .collect();
        for occupant in occupants {
            let prefix = if occupant == user_id {
                "You say".to_string()
            } else {
                "Someone says".to_string()
            };
            self.log_to(occupant, LogKind::Say, format!("{prefix}: {trimmed}"));
        }
    }

    /// Advance the world one round. Returns kills for the activity feed.
    fn tick(&mut self) -> Vec<KillOutcome> {
        let mut outcomes = Vec::new();
        let now = Instant::now();

        // Respawn mobs whose timer elapsed.
        for mob in self.mobs.values_mut() {
            if !mob.alive
                && let Some(at) = mob.respawn_at
                && now >= at
            {
                mob.alive = true;
                mob.hp = mob.spawn.max_hp;
                mob.respawn_at = None;
                self.dirty = true;
            }
        }

        // Respawn downed players whose rest elapsed.
        let resurrecting: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.respawn_at.is_some_and(|at| now >= at))
            .map(|(id, _)| *id)
            .collect();
        for user_id in resurrecting {
            let start = self.world.start_room;
            if let Some(player) = self.players.get_mut(&user_id) {
                player.hp = player.max_hp;
                player.room = start;
                player.target = None;
                player.respawn_at = None;
            }
            self.log_to(
                user_id,
                LogKind::System,
                "You wake at the Temple of the Dawn, restored.".to_string(),
            );
            self.describe_room(user_id);
            self.dirty = true;
        }

        // Resolve one combat round per fighting player.
        let fighters: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.target.is_some() && p.respawn_at.is_none())
            .map(|(id, _)| *id)
            .collect();

        for user_id in fighters {
            let (mob_id, player_damage) = match self.players.get(&user_id) {
                Some(p) => (p.target, p.damage),
                None => continue,
            };
            let Some(mob_id) = mob_id else { continue };
            let Some(mob) = self.mobs.get_mut(&mob_id) else {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.target = None;
                }
                continue;
            };
            if !mob.alive {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.target = None;
                }
                continue;
            }

            // Player strikes mob.
            mob.hp -= player_damage;
            let mob_name = mob.spawn.name.to_string();
            self.dirty = true;
            if mob.hp <= 0 {
                mob.alive = false;
                mob.hp = 0;
                mob.respawn_at = Some(now + Duration::from_secs(mob.spawn.respawn_secs));
                let xp = mob.spawn.xp;
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("You have slain {mob_name}! (+{xp} xp)"),
                );
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.target = None;
                    player.xp += xp;
                    let new_level = 1 + player.xp / 50;
                    if new_level > player.level {
                        player.level = new_level;
                        player.max_hp += 8;
                        player.hp = player.max_hp;
                        player.damage += 1;
                        let level = player.level;
                        self.log_to(
                            user_id,
                            LogKind::System,
                            format!("You reach level {level}! You feel stronger."),
                        );
                    }
                }
                outcomes.push(KillOutcome { user_id, mob_name });
                continue;
            }

            // Mob strikes back.
            let mob_damage = mob.spawn.damage;
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("You hit {mob_name}. It strikes back for {mob_damage}."),
            );
            if let Some(player) = self.players.get_mut(&user_id) {
                player.hp -= mob_damage;
                if player.hp <= 0 {
                    player.hp = 0;
                    player.target = None;
                    player.respawn_at = Some(now + Duration::from_secs(PLAYER_RESPAWN_SECS));
                    self.log_to(
                        user_id,
                        LogKind::System,
                        "You have fallen! Darkness takes you...".to_string(),
                    );
                }
            }
        }

        // Drop idle players from the world.
        let idle: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| {
                p.last_activity.elapsed() >= Duration::from_secs(PLAYER_IDLE_TIMEOUT_SECS)
            })
            .map(|(id, _)| *id)
            .collect();
        for user_id in idle {
            self.players.remove(&user_id);
            self.dirty = true;
        }

        if self.dirty {
            self.generation = self.generation.wrapping_add(1);
        }
        outcomes
    }

    fn log_to(&mut self, user_id: Uuid, kind: LogKind, text: String) {
        if let Some(player) = self.players.get_mut(&user_id) {
            push_log(&mut player.log, kind, text);
            self.dirty = true;
        }
    }

    fn snapshot(&self) -> MudSnapshot {
        let mut players = HashMap::new();
        for (user_id, player) in &self.players {
            let room = self.world.room(player.room);
            let (room_name, room_desc, zone, safe, exits) = match room {
                Some(room) => {
                    let mut exits: Vec<(Dir, String)> = room
                        .exits
                        .keys()
                        .map(|d| (*d, d.label().to_string()))
                        .collect();
                    exits.sort_by(|a, b| a.1.cmp(&b.1));
                    (
                        room.name.to_string(),
                        room.desc.to_string(),
                        room.zone.to_string(),
                        room.safe,
                        exits,
                    )
                }
                None => (String::new(), String::new(), String::new(), true, Vec::new()),
            };
            let mobs: Vec<MobView> = self
                .mobs
                .values()
                .filter(|m| m.alive && m.spawn.home == player.room)
                .map(|m| MobView {
                    name: m.spawn.name.to_string(),
                    hp: m.hp,
                    max_hp: m.spawn.max_hp,
                })
                .collect();
            let occupants: Vec<OccupantView> = self
                .players
                .values()
                .filter(|other| other.user_id != *user_id && other.room == player.room)
                .map(|other| OccupantView {
                    user_id: other.user_id,
                    hp: other.hp,
                    max_hp: other.max_hp,
                    in_combat: other.target.is_some(),
                })
                .collect();
            let in_combat_with = player.target.and_then(|mob_id| {
                self.mobs
                    .get(&mob_id)
                    .filter(|m| m.alive)
                    .map(|m| m.spawn.name.to_string())
            });
            players.insert(
                *user_id,
                PlayerView {
                    joined: true,
                    alive: player.respawn_at.is_none(),
                    hp: player.hp,
                    max_hp: player.max_hp,
                    xp: player.xp,
                    level: player.level,
                    room_name,
                    room_desc,
                    zone,
                    safe,
                    exits,
                    mobs,
                    occupants,
                    in_combat_with,
                    log: player.log.clone(),
                    respawning: player.respawn_at.is_some(),
                },
            );
        }
        MudSnapshot {
            room_id: self.room_id,
            generation: self.generation,
            players,
        }
    }
}

fn push_log(log: &mut Vec<LogLine>, kind: LogKind, text: String) {
    log.push(LogLine { text, kind });
    if log.len() > LOG_CAP {
        let overflow = log.len() - LOG_CAP;
        log.drain(0..overflow);
    }
}

/// A session whose player hasn't joined yet still needs a view to render.
pub fn empty_player_view(room_id: Uuid) -> PlayerView {
    PlayerView::empty(room_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn test_state() -> WorldState {
        WorldState::new(uid(999), seed_world())
    }

    #[test]
    fn join_places_player_in_safe_start_room() {
        let mut state = test_state();
        assert!(state.join(uid(1)));
        let player = state.players.get(&uid(1)).expect("joined");
        assert_eq!(player.room, state.world.start_room);
        assert_eq!(player.hp, PLAYER_MAX_HP);
        // Re-join is a no-op.
        assert!(!state.join(uid(1)));
    }

    #[test]
    fn movement_follows_exits_and_blocks_walls() {
        let mut state = test_state();
        state.join(uid(1));
        // Square (1) -> south -> gate (5).
        state.move_player(uid(1), Dir::South);
        assert_eq!(state.players[&uid(1)].room, 5);
        // Gate has no east exit.
        state.move_player(uid(1), Dir::East);
        assert_eq!(state.players[&uid(1)].room, 5);
    }

    #[test]
    fn cannot_fight_in_safe_room() {
        let mut state = test_state();
        state.join(uid(1));
        state.engage(uid(1));
        assert!(state.players[&uid(1)].target.is_none());
    }

    #[test]
    fn combat_kills_mob_and_awards_xp() {
        let mut state = test_state();
        state.join(uid(1));
        // Walk to room 6 (goblin home): square -> gate -> open country.
        state.move_player(uid(1), Dir::South);
        state.move_player(uid(1), Dir::South);
        assert_eq!(state.players[&uid(1)].room, 6);
        state.engage(uid(1));
        assert!(state.players[&uid(1)].target.is_some());
        // Tick until the goblin (18 hp, player 6 dmg) dies.
        let mut kills = Vec::new();
        for _ in 0..10 {
            kills = state.tick();
            if state.players[&uid(1)].target.is_none() {
                break;
            }
        }
        assert!(state.players[&uid(1)].xp > 0, "player should gain xp");
        assert_eq!(kills.len(), 1);
    }

    #[test]
    fn say_reaches_others_in_same_room_only() {
        let mut state = test_state();
        state.join(uid(1));
        state.join(uid(2));
        // uid(2) moves away.
        state.move_player(uid(2), Dir::South);
        state.say(uid(1), "hello");
        let p1_heard = state.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("hello"));
        let p2_heard = state.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text.contains("hello"));
        assert!(p1_heard, "speaker hears own message");
        assert!(!p2_heard, "player in another room does not hear");
    }
}
