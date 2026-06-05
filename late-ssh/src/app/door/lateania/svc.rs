// Lateania world runtime: the authoritative, in-memory truth for the server-wide
// MUD world.
//
// One service is shared by the process. Sessions join it only while the
// dedicated Lateania page is open; each has its own `state::State`. Mutations
// serialize through `Arc<Mutex<WorldState>>`; reads are lock-free against each
// session's cached snapshot. A background tick loop advances combat rounds,
// effects, resource regen, and respawns, then publishes a fresh snapshot.
//
// Systems wired here: five classes with a 50-level progression and a passive
// trait (classes.rs), abilities and spells unified under one effect resolver
// (abilities.rs), and an inventory / equipment / gold / shop economy (items.rs).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

use late_core::{MutexRecover, db::Db, models::mud_character::MudCharacter};
use rand::Rng;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::app::activity::{event::ActivityGame, publisher::ActivityPublisher};

use super::abilities::{Ability, AbilityEffect, learned_at, unlocked_for};
use super::classes::{Class, level_for_xp, xp_for_level};
use super::damage::{DamageType, Defense};
use super::items::{ItemKind, Slot, item, shop_at};
use super::persist::{SavedCharacter, SavedCharacterInit};
use super::world::{Dir, MobSpawn, RoomId, World, seed_world};

/// World heartbeat. One combat round resolves per tick.
const TICK_SECS: u64 = 2;
/// A player who sends no command for this long is dropped from the world.
const PLAYER_IDLE_TIMEOUT_SECS: u64 = 10 * 60;
/// How long a defeated player rests before respawning at the temple.
const PLAYER_RESPAWN_SECS: u64 = 8;
/// Gold every new adventurer starts with.
const STARTING_GOLD: i64 = 120;

/// How often the world autosaves every present character's progress.
const AUTOSAVE_SECS: u64 = 60;

#[derive(Clone)]
pub struct LateaniaService {
    activity: ActivityPublisher,
    db: Db,
    snapshot_tx: watch::Sender<MudSnapshot>,
    snapshot_rx: watch::Receiver<MudSnapshot>,
    state: Arc<Mutex<WorldState>>,
    active_sessions: Arc<StdMutex<HashMap<Uuid, HashSet<Uuid>>>>,
    persist_versions: Arc<StdMutex<HashMap<Uuid, u64>>>,
    persist_locks: Arc<StdMutex<HashMap<Uuid, Arc<Mutex<()>>>>>,
    prepared_saves: Arc<StdMutex<HashMap<Uuid, (u64, SavedCharacter)>>>,
}

// ---- Snapshot (what sessions render) -------------------------------------

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
    Loot,
}

#[derive(Clone, Debug)]
pub struct MobView {
    pub name: String,
    pub hp: i32,
    pub max_hp: i32,
}

#[derive(Clone, Debug)]
pub struct OccupantView {
    pub user_id: Uuid,
    pub hp: i32,
    pub max_hp: i32,
    pub in_combat: bool,
}

/// One known ability as shown on the action bar.
#[derive(Clone, Debug)]
pub struct AbilityView {
    pub slot: u8,
    pub name: String,
    pub cost: i32,
    pub ready: bool,
    pub effect: String,
}

/// One inventory line.
#[derive(Clone, Debug)]
pub struct InvView {
    pub item_id: u32,
    pub name: String,
    pub rarity: String,
    pub slot: Option<String>,
    pub equipped: bool,
    pub sell_price: i64,
}

/// One shop listing.
#[derive(Clone, Debug)]
pub struct ShopEntryView {
    pub item_id: u32,
    pub name: String,
    pub rarity: String,
    pub price: i64,
    pub affordable: bool,
}

#[derive(Clone, Debug)]
pub struct ShopView {
    pub npc_name: String,
    pub shop_name: String,
    pub greeting: String,
    pub entries: Vec<ShopEntryView>,
}

/// Which side panel a session is viewing (local UI mode echoed in the snapshot
/// only for the shop, which is world-driven; inventory/abilities are derived).
#[derive(Clone, Debug)]
pub struct MudSnapshot {
    pub room_id: Uuid,
    pub generation: u64,
    pub players: HashMap<Uuid, PlayerView>,
}

#[derive(Clone, Debug)]
pub struct PlayerView {
    pub joined: bool,
    pub classed: bool,
    pub class_name: String,
    pub trait_name: String,
    pub trait_desc: String,
    pub resource_name: String,
    pub resource: i32,
    pub max_resource: i32,
    pub alive: bool,
    pub hp: i32,
    pub max_hp: i32,
    pub attack: i32,
    pub armor: i32,
    pub xp: i64,
    pub xp_into_level: i64,
    pub xp_for_next: i64,
    pub level: i32,
    pub gold: i64,
    pub room_name: String,
    pub room_desc: String,
    pub zone: String,
    pub safe: bool,
    pub exits: Vec<(Dir, String)>,
    pub mobs: Vec<MobView>,
    pub occupants: Vec<OccupantView>,
    pub in_combat_with: Option<String>,
    pub abilities: Vec<AbilityView>,
    pub inventory: Vec<InvView>,
    pub shop: Option<ShopView>,
    pub log: Vec<LogLine>,
    pub respawning: bool,
}

impl PlayerView {
    fn empty() -> Self {
        Self {
            joined: false,
            classed: false,
            class_name: String::new(),
            trait_name: String::new(),
            trait_desc: String::new(),
            resource_name: String::new(),
            resource: 0,
            max_resource: 0,
            alive: false,
            hp: 0,
            max_hp: 0,
            attack: 0,
            armor: 0,
            xp: 0,
            xp_into_level: 0,
            xp_for_next: 0,
            level: 1,
            gold: 0,
            room_name: String::new(),
            room_desc: String::new(),
            zone: String::new(),
            safe: true,
            exits: Vec::new(),
            mobs: Vec::new(),
            occupants: Vec::new(),
            in_combat_with: None,
            abilities: Vec::new(),
            inventory: Vec::new(),
            shop: None,
            log: Vec::new(),
            respawning: false,
        }
    }
}

pub fn empty_player_view() -> PlayerView {
    PlayerView::empty()
}

impl LateaniaService {
    pub fn new(activity: ActivityPublisher, db: Db) -> Self {
        let room_id = Uuid::from_u128(0x4c41_5445_414e_4941_0000_0000_0000_0001);
        let state = WorldState::new(room_id, seed_world());
        let initial = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let svc = Self {
            activity,
            db,
            snapshot_tx,
            snapshot_rx,
            state: Arc::new(Mutex::new(state)),
            active_sessions: Arc::new(StdMutex::new(HashMap::new())),
            persist_versions: Arc::new(StdMutex::new(HashMap::new())),
            persist_locks: Arc::new(StdMutex::new(HashMap::new())),
            prepared_saves: Arc::new(StdMutex::new(HashMap::new())),
        };
        svc.start_tick_loop();
        svc.start_autosave_loop();
        svc
    }

    pub fn subscribe_state(&self) -> watch::Receiver<MudSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn current_snapshot(&self) -> MudSnapshot {
        self.snapshot_rx.borrow().clone()
    }

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

    fn mutate<F: FnOnce(&mut WorldState) + Send + 'static>(&self, user_id: Uuid, f: F) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            f(&mut state);
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn join_task(&self, user_id: Uuid, session_id: Uuid) {
        self.mark_session_joined(user_id, session_id);
        let svc = self.clone();
        tokio::spawn(async move {
            if !svc.has_active_session(user_id) {
                return;
            }

            // Load any saved character before exposing a fresh player. A DB
            // failure must not become "no save", otherwise later autosave or
            // logout can overwrite an existing character with a starter one.
            let saved = if let Some(saved) = svc.prepared_saved(user_id) {
                Some(saved)
            } else {
                match svc.db.get().await {
                    Ok(client) => match MudCharacter::load(&client, user_id).await {
                        Ok(Some(blob)) => SavedCharacter::from_json(&blob),
                        Ok(None) => None,
                        Err(error) => {
                            tracing::warn!(%user_id, ?error, "failed to load mud character");
                            return;
                        }
                    },
                    Err(error) => {
                        tracing::warn!(%user_id, ?error, "no db client for mud character load");
                        return;
                    }
                }
            };

            let mut state = svc.state.lock().await;
            if !svc.has_active_session(user_id) {
                return;
            }
            if !state.players.contains_key(&user_id) {
                state.join(user_id);
                if let Some(saved) = saved {
                    state.hydrate(user_id, &saved);
                }
            }
            state.touch(user_id);
            svc.publish(&state);
        });
    }

    pub fn leave_task(&self, user_id: Uuid, session_id: Uuid) {
        if !self.mark_session_left(user_id, session_id) {
            return;
        }
        let svc = self.clone();
        tokio::spawn(async move {
            if svc.has_active_session(user_id) {
                return;
            }
            // Capture the durable character under the lock, then remove the player.
            let saved = {
                let mut state = svc.state.lock().await;
                if svc.has_active_session(user_id) {
                    return;
                }
                let saved = state
                    .export_saved(user_id)
                    .map(|saved| svc.prepare_persist(user_id, saved));
                state.leave(user_id);
                svc.publish(&state);
                saved
            };
            if let Some(saved) = saved {
                svc.persist(saved).await;
            }
        });
    }

    fn mark_session_joined(&self, user_id: Uuid, session_id: Uuid) {
        self.active_sessions
            .lock_recover()
            .entry(user_id)
            .or_default()
            .insert(session_id);
    }

    /// Mark one session closed. Returns true only when no sessions remain for
    /// that user, meaning the world player can be removed after re-checking.
    fn mark_session_left(&self, user_id: Uuid, session_id: Uuid) -> bool {
        let mut active_sessions = self.active_sessions.lock_recover();
        let Some(user_sessions) = active_sessions.get_mut(&user_id) else {
            return true;
        };
        user_sessions.remove(&session_id);
        if user_sessions.is_empty() {
            active_sessions.remove(&user_id);
            true
        } else {
            false
        }
    }

    fn has_active_session(&self, user_id: Uuid) -> bool {
        self.active_sessions
            .lock_recover()
            .get(&user_id)
            .is_some_and(|sessions| !sessions.is_empty())
    }

    fn clear_sessions(&self, user_id: Uuid) {
        self.active_sessions.lock_recover().remove(&user_id);
    }

    fn prepare_persist(&self, user_id: Uuid, saved: SavedCharacter) -> PendingSave {
        let mut versions = self.persist_versions.lock_recover();
        let version = versions.entry(user_id).and_modify(|v| *v += 1).or_insert(1);
        self.prepared_saves
            .lock_recover()
            .insert(user_id, (*version, saved.clone()));
        PendingSave {
            user_id,
            version: *version,
            saved,
        }
    }

    fn prepared_saved(&self, user_id: Uuid) -> Option<SavedCharacter> {
        self.prepared_saves
            .lock_recover()
            .get(&user_id)
            .map(|(_, saved)| saved.clone())
    }

    fn clear_prepared_save(&self, save: &PendingSave) {
        let mut prepared_saves = self.prepared_saves.lock_recover();
        if prepared_saves
            .get(&save.user_id)
            .is_some_and(|(version, _)| *version == save.version)
        {
            prepared_saves.remove(&save.user_id);
        }
    }

    fn is_latest_persist(&self, save: &PendingSave) -> bool {
        self.persist_versions
            .lock_recover()
            .get(&save.user_id)
            .is_some_and(|version| *version == save.version)
    }

    fn persist_lock(&self, user_id: Uuid) -> Arc<Mutex<()>> {
        self.persist_locks
            .lock_recover()
            .entry(user_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Write one character blob to the database (best-effort).
    async fn persist(&self, save: PendingSave) {
        if !self.is_latest_persist(&save) {
            return;
        }
        let lock = self.persist_lock(save.user_id);
        let _guard = lock.lock().await;
        if !self.is_latest_persist(&save) {
            return;
        }
        match self.db.get().await {
            Ok(client) => {
                match MudCharacter::save(&client, save.user_id, save.saved.to_json()).await {
                    Ok(()) => self.clear_prepared_save(&save),
                    Err(error) => {
                        tracing::warn!(user_id = %save.user_id, ?error, "failed to save mud character");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(user_id = %save.user_id, ?error, "no db client for mud character save");
            }
        }
    }

    fn start_autosave_loop(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(AUTOSAVE_SECS));
            ticker.tick().await; // skip the immediate first tick
            loop {
                ticker.tick().await;
                let saves: Vec<PendingSave> = {
                    let state = svc.state.lock().await;
                    state
                        .export_all_saved()
                        .into_iter()
                        .map(|(user_id, saved)| svc.prepare_persist(user_id, saved))
                        .collect()
                };
                for save in saves {
                    svc.persist(save).await;
                }
            }
        });
    }

    pub fn choose_class_task(&self, user_id: Uuid, class: Class) {
        self.mutate(user_id, move |s| s.choose_class(user_id, class));
    }

    pub fn move_task(&self, user_id: Uuid, dir: Dir) {
        self.mutate(user_id, move |s| s.move_player(user_id, dir));
    }

    pub fn look_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.look(user_id));
    }

    pub fn attack_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.engage(user_id));
    }

    pub fn ability_task(&self, user_id: Uuid, slot: u8) {
        self.mutate(user_id, move |s| s.use_ability(user_id, slot));
    }

    pub fn flee_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.flee(user_id));
    }

    pub fn say_task(&self, user_id: Uuid, message: String) {
        self.mutate(user_id, move |s| s.say(user_id, &message));
    }

    pub fn equip_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.equip(user_id, item_id));
    }

    pub fn use_item_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.use_item(user_id, item_id));
    }

    pub fn buy_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.buy(user_id, item_id));
    }

    pub fn sell_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.sell(user_id, item_id));
    }

    pub fn touch_activity_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            state.touch(user_id);
        });
    }

    fn start_tick_loop(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
            loop {
                ticker.tick().await;
                let mut state = svc.state.lock().await;
                let tick = state.tick();
                let idle_saves: Vec<PendingSave> = tick
                    .idle_saves
                    .into_iter()
                    .map(|(user_id, saved)| svc.prepare_persist(user_id, saved))
                    .collect();
                if state.dirty {
                    svc.publish(&state);
                    state.dirty = false;
                }
                drop(state);
                for save in idle_saves {
                    svc.clear_sessions(save.user_id);
                    svc.persist(save).await;
                }
                for outcome in tick.kills {
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

struct KillOutcome {
    user_id: Uuid,
    mob_name: String,
}

#[derive(Default)]
struct TickOutput {
    kills: Vec<KillOutcome>,
    idle_saves: Vec<(Uuid, SavedCharacter)>,
}

struct PendingSave {
    user_id: Uuid,
    version: u64,
    saved: SavedCharacter,
}

// ---- Active effects (spells, poisons, buffs unified) ---------------------

#[derive(Clone, Copy)]
struct ActiveEffect {
    kind: AbilityEffect,
    magnitude: i32,
    remaining: u8,
}

// ---- The authoritative world state ---------------------------------------

struct PlayerState {
    user_id: Uuid,
    class: Option<Class>,
    hp: i32,
    base_max_hp: i32,
    resource: i32,
    max_resource: i32,
    resource_regen: i32,
    base_attack: i32,
    xp: i64,
    level: i32,
    gold: i64,
    room: RoomId,
    target: Option<u32>,
    /// True from engaging until the first auto-attack lands (Rogue opening crit).
    opening_strike: bool,
    /// Outgoing-damage buff remaining ticks and magnitude.
    empower: i32,
    empower_ticks: u8,
    /// Absorb shield remaining.
    shield: i32,
    shield_ticks: u8,
    /// Ticks the player is stunned (skips their action).
    stunned: u8,
    /// Healing-over-time on self.
    self_effects: Vec<ActiveEffect>,
    /// Per-ability cooldowns: ability id -> ticks remaining.
    cooldowns: HashMap<u32, u32>,
    inventory: Vec<u32>,
    equipped: HashMap<Slot, u32>,
    /// True once the class trait's death-save has been spent this life (Warrior).
    death_save_used: bool,
    last_activity: Instant,
    respawn_at: Option<Instant>,
    log: Vec<LogLine>,
}

impl PlayerState {
    fn equipment_mods(&self) -> (i32, i32, i32) {
        let mut attack = 0;
        let mut hp = 0;
        let mut armor = 0;
        for id in self.equipped.values() {
            if let Some(it) = item(*id) {
                attack += it.mods.attack;
                hp += it.mods.max_hp;
                armor += it.mods.armor;
            }
        }
        (attack, hp, armor)
    }

    fn max_hp(&self) -> i32 {
        let (_, hp, _) = self.equipment_mods();
        self.base_max_hp + hp
    }

    fn attack(&self) -> i32 {
        let (atk, _, _) = self.equipment_mods();
        self.base_attack + atk + self.empower
    }

    fn armor(&self) -> i32 {
        let (_, _, armor) = self.equipment_mods();
        armor
    }
}

struct MobInstance {
    spawn: MobSpawn,
    hp: i32,
    alive: bool,
    respawn_at: Option<Instant>,
}

struct WorldState {
    room_id: Uuid,
    world: World,
    players: HashMap<Uuid, PlayerState>,
    mobs: HashMap<u32, MobInstance>,
    /// mob id -> stun ticks remaining.
    mob_stuns: HashMap<u32, u8>,
    /// mob id -> active damage-over-time stacks (owner, per-tick, remaining).
    mob_dots: HashMap<u32, Vec<(Uuid, i32, u8)>>,
    /// Kills accumulated during a tick, drained for the activity feed.
    pending_kills: Vec<KillOutcome>,
    generation: u64,
    dirty: bool,
}

const LOG_CAP: usize = 60;
const TEMPLE_ROOM: RoomId = 4;

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
            mob_stuns: HashMap::new(),
            mob_dots: HashMap::new(),
            pending_kills: Vec::new(),
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
            class: None,
            hp: 30,
            base_max_hp: 30,
            resource: 0,
            max_resource: 0,
            resource_regen: 0,
            base_attack: 4,
            xp: 0,
            level: 1,
            gold: STARTING_GOLD,
            room: start,
            target: None,
            opening_strike: false,
            empower: 0,
            empower_ticks: 0,
            shield: 0,
            shield_ticks: 0,
            stunned: 0,
            self_effects: Vec::new(),
            cooldowns: HashMap::new(),
            inventory: vec![1000, 1300, 1300], // a rusty sword and two minor draughts
            equipped: HashMap::new(),
            death_save_used: false,
            last_activity: Instant::now(),
            respawn_at: None,
            log: Vec::new(),
        };
        push_log(
            &mut player.log,
            LogKind::System,
            "Welcome to Lateania. Choose your calling to begin.".to_string(),
        );
        self.players.insert(user_id, player);
        true
    }

    fn choose_class(&mut self, user_id: Uuid, class: Class) {
        let already = self
            .players
            .get(&user_id)
            .map(|p| p.class.is_some())
            .unwrap_or(true);
        if already {
            return;
        }
        let stats = class.stats_at(1);
        if let Some(p) = self.players.get_mut(&user_id) {
            p.class = Some(class);
            p.base_max_hp = stats.max_hp;
            p.max_resource = stats.max_resource;
            p.resource = stats.max_resource;
            p.resource_regen = stats.resource_regen;
            p.base_attack = stats.attack;
            p.hp = p.max_hp();
        }
        let name = class.name();
        let trait_name = class.trait_name();
        self.log_to(
            user_id,
            LogKind::System,
            format!("You are now a {name}. Your trait: {trait_name}."),
        );
        self.describe_room(user_id);
    }

    fn leave(&mut self, user_id: Uuid) {
        self.players.remove(&user_id);
    }

    /// Apply a saved character onto a freshly-joined player. Restores class,
    /// progression, gold, gear, and inventory; reloads at a safe room with full
    /// vitals so a logged-out fight never resumes mid-swing.
    fn hydrate(&mut self, user_id: Uuid, saved: &SavedCharacter) {
        let Some(class) = saved.class() else {
            // No class chosen last time; leave the player at the select screen.
            return;
        };
        let level = saved.level.clamp(1, Class::MAX_LEVEL);
        let stats = class.stats_at(level);
        let room = if self.world.room(saved.room).is_some_and(|r| r.safe) {
            saved.room
        } else {
            self.world.start_room
        };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.class = Some(class);
            p.level = level;
            p.xp = saved.xp.max(0);
            p.gold = saved.gold.max(0);
            p.base_max_hp = stats.max_hp;
            p.max_resource = stats.max_resource;
            p.resource = stats.max_resource;
            p.resource_regen = stats.resource_regen;
            p.base_attack = stats.attack;
            p.room = room;
            p.inventory = saved
                .inventory
                .iter()
                .copied()
                .filter(|id| item(*id).is_some())
                .collect();
            p.equipped.clear();
            for (slot_key, id) in &saved.equipped {
                if let Some(it) = item(*id)
                    && let Some(slot) = it.slot()
                    && slot.label() == slot_key
                {
                    p.equipped.insert(slot, *id);
                }
            }
            // Restore vitals last so equipment max-hp is already in effect.
            let max = p.max_hp();
            p.hp = if saved.hp > 0 { saved.hp.min(max) } else { max };
        }
        let name = class.name();
        self.log_to(
            user_id,
            LogKind::System,
            format!("Welcome back. Your {name} stands ready (level {level})."),
        );
        self.describe_room(user_id);
    }

    /// The durable slice of one player, if they have chosen a class (otherwise
    /// there is nothing worth saving yet).
    fn export_saved(&self, user_id: Uuid) -> Option<SavedCharacter> {
        let p = self.players.get(&user_id)?;
        p.class?; // unclassed -> nothing to persist
        let equipped: Vec<(String, u32)> = p
            .equipped
            .iter()
            .map(|(slot, id)| (slot.label().to_string(), *id))
            .collect();
        Some(SavedCharacter::new_for(SavedCharacterInit {
            class: p.class,
            xp: p.xp,
            level: p.level,
            gold: p.gold,
            hp: p.hp.max(1),
            room: p.room,
            inventory: p.inventory.clone(),
            equipped,
        }))
    }

    fn export_all_saved(&self) -> Vec<(Uuid, SavedCharacter)> {
        self.players
            .keys()
            .filter_map(|uid| self.export_saved(*uid).map(|s| (*uid, s)))
            .collect()
    }

    fn touch(&mut self, user_id: Uuid) {
        if let Some(player) = self.players.get_mut(&user_id) {
            player.last_activity = Instant::now();
        }
    }

    fn is_classed(&self, user_id: Uuid) -> bool {
        self.players
            .get(&user_id)
            .map(|p| p.class.is_some())
            .unwrap_or(false)
    }

    fn move_player(&mut self, user_id: Uuid, dir: Dir) {
        if !self.is_classed(user_id) {
            return;
        }
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
                "You can't leave - you're in combat! Flee (z) first.".to_string(),
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
        let shop = shop_at(room_id);
        self.log_to(user_id, LogKind::Normal, format!("== {name} =="));
        self.log_to(user_id, LogKind::Normal, desc);
        self.log_to(user_id, LogKind::System, format!("Exits: {exit_text}"));
        if let Some(shop) = shop {
            self.log_to(
                user_id,
                LogKind::Loot,
                format!(
                    "{} tends {} here. Press b to browse.",
                    shop.npc_name, shop.shop_name
                ),
            );
        }
        for mob in mob_names {
            self.log_to(user_id, LogKind::Combat, format!("{mob} is here."));
        }
    }

    fn engage(&mut self, user_id: Uuid) {
        if !self.is_classed(user_id) {
            return;
        }
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
                    // Opportunist: the Rogue's first strike of a fight always crits.
                    player.opening_strike = player.class == Some(Class::Rogue);
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("You close with {mob_name}!"),
                );
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

    /// Cast/use the ability in the given action-bar slot (1-based).
    fn use_ability(&mut self, user_id: Uuid, slot: u8) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        let Some(class) = player.class else {
            return;
        };
        if player.respawn_at.is_some() {
            return;
        }
        let known = unlocked_for(class, player.level);
        let Some(ability) = known.get(slot.saturating_sub(1) as usize).copied() else {
            self.log_to(
                user_id,
                LogKind::System,
                "No ability in that slot.".to_string(),
            );
            return;
        };
        // Validate cost + cooldown against the truth.
        let on_cd = player.cooldowns.get(&ability.id).copied().unwrap_or(0) > 0;
        if on_cd {
            self.log_to(
                user_id,
                LogKind::System,
                format!("{} is not ready.", ability.name),
            );
            return;
        }
        if player.resource < ability.cost {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "Not enough {} for {}.",
                    class.resource().label(),
                    ability.name
                ),
            );
            return;
        }
        // Targeted offensive abilities need a foe.
        let needs_target = matches!(
            ability.effect,
            AbilityEffect::Strike
                | AbilityEffect::DamageOverTime
                | AbilityEffect::Stun
                | AbilityEffect::Finisher
        );
        if needs_target && player.target.is_none() {
            self.log_to(user_id, LogKind::Combat, "You have no target.".to_string());
            return;
        }
        // Spend and set cooldown.
        if let Some(p) = self.players.get_mut(&user_id) {
            p.resource -= ability.cost;
            p.cooldowns.insert(ability.id, ability.cooldown_ticks);
        }
        self.apply_ability(user_id, class, ability);
    }

    fn apply_ability(&mut self, user_id: Uuid, class: Class, ability: &Ability) {
        match ability.effect {
            AbilityEffect::Heal => {
                let amount = self.amplified_heal(class, ability.magnitude);
                self.heal_player(user_id, amount);
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{} restores {} health.", ability.name, amount),
                );
            }
            AbilityEffect::HealOverTime => {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.self_effects.push(ActiveEffect {
                        kind: AbilityEffect::HealOverTime,
                        magnitude: ability.magnitude,
                        remaining: ability.duration,
                    });
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{} begins to mend you.", ability.name),
                );
            }
            AbilityEffect::Empower => {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.empower = ability.magnitude;
                    p.empower_ticks = ability.duration;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!(
                        "{} surges through you (+{} damage).",
                        ability.name, ability.magnitude
                    ),
                );
            }
            AbilityEffect::Ward => {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.shield = ability.magnitude;
                    p.shield_ticks = ability.duration;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!(
                        "{} shields you ({} absorb).",
                        ability.name, ability.magnitude
                    ),
                );
            }
            AbilityEffect::Strike => {
                let dmg = self.spell_damage(class, ability.magnitude, user_id);
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
            }
            AbilityEffect::Finisher => {
                let dmg = self.spell_damage(class, ability.magnitude, user_id);
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.empower = p.empower.max(ability.magnitude / 8);
                    p.empower_ticks = p.empower_ticks.max(ability.duration);
                }
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
            }
            AbilityEffect::DamageOverTime => {
                let tick = self.spell_damage(class, ability.magnitude, user_id);
                self.seed_mob_dot(
                    user_id,
                    tick,
                    ability.damage_type,
                    ability.duration,
                    ability.name,
                );
            }
            AbilityEffect::Stun => {
                let target = self.players.get(&user_id).and_then(|p| p.target);
                let dmg = self.spell_damage(class, ability.magnitude, user_id);
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
                // Only stun if the target survived the hit.
                if let Some(mob_id) = target
                    && self.mobs.get(&mob_id).is_some_and(|m| m.alive)
                {
                    self.mob_stuns.insert(mob_id, ability.duration);
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{} leaves the foe reeling!", ability.name),
                    );
                }
            }
        }
    }

    fn amplified_heal(&self, class: Class, base: i32) -> i32 {
        if class == Class::Cleric {
            base + base / 4 // Light of the Dawn
        } else {
            base
        }
    }

    fn spell_damage(&self, class: Class, base: i32, user_id: Uuid) -> i32 {
        let mut dmg = base;
        if class == Class::Mage {
            dmg += dmg / 5; // Arcane Mastery
        }
        if class == Class::Ranger {
            // Hunter's Instinct: more vs wounded foe.
            if let Some(mob_id) = self.players.get(&user_id).and_then(|p| p.target)
                && let Some(mob) = self.mobs.get(&mob_id)
                && mob.hp * 2 < mob.spawn.max_hp
            {
                dmg += dmg / 4;
            }
        }
        dmg
    }

    fn heal_player(&mut self, user_id: Uuid, amount: i32) {
        if let Some(p) = self.players.get_mut(&user_id) {
            let max = p.max_hp();
            p.hp = (p.hp + amount).min(max);
            self.dirty = true;
        }
    }

    fn damage_target(&mut self, user_id: Uuid, raw: i32, dtype: DamageType, source: &str) {
        let Some(mob_id) = self.players.get(&user_id).and_then(|p| p.target) else {
            return;
        };
        let (mob_name, dmg, defense, dead) = {
            let Some(mob) = self.mobs.get_mut(&mob_id) else {
                return;
            };
            if !mob.alive {
                return;
            }
            let (dmg, defense) = mob.spawn.profile.apply(raw, dtype);
            mob.hp -= dmg;
            (mob.spawn.name.to_string(), dmg, defense, mob.hp <= 0)
        };
        self.dirty = true;
        let tag = defense_tag(defense, dtype);
        self.log_to(
            user_id,
            LogKind::Combat,
            format!(
                "{source} hits {mob_name} for {dmg} {}{}.",
                dtype.label(),
                tag
            ),
        );
        if dead {
            self.kill_mob(user_id, mob_id);
        }
    }

    fn seed_mob_dot(
        &mut self,
        user_id: Uuid,
        per_tick: i32,
        dtype: DamageType,
        duration: u8,
        source: &str,
    ) {
        let Some(mob_id) = self.players.get(&user_id).and_then(|p| p.target) else {
            return;
        };
        // Bake the resist/weak multiplier into the per-tick number once, up front.
        let scaled = self
            .mobs
            .get(&mob_id)
            .map(|m| m.spawn.profile.apply(per_tick, dtype).0)
            .unwrap_or(per_tick);
        self.mob_dots
            .entry(mob_id)
            .or_default()
            .push((user_id, scaled, duration));
        self.log_to(
            user_id,
            LogKind::Combat,
            format!("{source} festers in the foe ({} damage).", dtype.label()),
        );
        self.dirty = true;
    }

    fn kill_mob(&mut self, user_id: Uuid, mob_id: u32) {
        let (mob_name, xp, loot, boss) = match self.mobs.get_mut(&mob_id) {
            Some(mob) => {
                mob.alive = false;
                mob.hp = 0;
                let r = mob.spawn.respawn_secs;
                mob.respawn_at = Some(Instant::now() + Duration::from_secs(r));
                (
                    mob.spawn.name.to_string(),
                    mob.spawn.xp,
                    mob.spawn.loot,
                    mob.spawn.boss,
                )
            }
            None => return,
        };
        let gold = 3 + xp / 4;
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You have slain {mob_name}! (+{xp} xp, +{gold} gold)"),
        );
        if let Some(p) = self.players.get_mut(&user_id) {
            p.target = None;
            p.xp += xp as i64;
            p.gold += gold as i64;
        }
        self.roll_loot(user_id, &mob_name, loot, boss);
        self.check_level_up(user_id);
        self.pending_kills.push(KillOutcome { user_id, mob_name });
        self.dirty = true;
    }

    /// Award loot from a slain mob. Bosses always drop one item from their table;
    /// regular mobs have a modest chance at a common drop.
    fn roll_loot(&mut self, user_id: Uuid, mob_name: &str, loot: &'static [u32], boss: bool) {
        if loot.is_empty() {
            return;
        }
        let mut rng = rand::thread_rng();
        // Regular mobs: roughly one kill in four yields something.
        if !boss && rng.gen_range(0..100) >= 25 {
            return;
        }
        let pick = loot[rng.gen_range(0..loot.len())];
        let Some(it) = item(pick) else { return };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.inventory.push(pick);
        }
        if boss {
            self.log_to(
                user_id,
                LogKind::Loot,
                format!(
                    "{mob_name} drops {} ({})! It falls into your pack.",
                    it.name,
                    it.rarity.label()
                ),
            );
        } else {
            self.log_to(
                user_id,
                LogKind::Loot,
                format!("You loot {} from the corpse.", it.name),
            );
        }
    }

    fn check_level_up(&mut self, user_id: Uuid) {
        let (class, xp, old_level) = match self.players.get(&user_id) {
            Some(p) => (p.class, p.xp, p.level),
            None => return,
        };
        let Some(class) = class else { return };
        let new_level = level_for_xp(xp);
        if new_level <= old_level {
            return;
        }
        let stats = class.stats_at(new_level);
        if let Some(p) = self.players.get_mut(&user_id) {
            p.level = new_level;
            p.base_max_hp = stats.max_hp;
            p.max_resource = stats.max_resource;
            p.base_attack = stats.attack;
            p.resource_regen = stats.resource_regen;
            p.hp = p.max_hp();
            p.resource = p.max_resource;
        }
        self.log_to(
            user_id,
            LogKind::System,
            format!("You reach level {new_level}!"),
        );
        // Announce any abilities gained between old and new level.
        for lvl in (old_level + 1)..=new_level {
            if let Some(a) = learned_at(class, lvl) {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("You learn {} (level {}): {}", a.name, lvl, a.desc),
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

    // ---- Inventory / equipment / economy --------------------------------

    fn equip(&mut self, user_id: Uuid, item_id: u32) {
        let Some(it) = item(item_id) else { return };
        let Some(slot) = it.slot() else {
            self.log_to(
                user_id,
                LogKind::System,
                format!("{} cannot be equipped.", it.name),
            );
            return;
        };
        let has = self
            .players
            .get(&user_id)
            .map(|p| p.inventory.contains(&item_id))
            .unwrap_or(false);
        if !has {
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            // Return the currently-equipped item to the pack.
            if let Some(old) = p.equipped.insert(slot, item_id) {
                p.inventory.push(old);
            }
            if let Some(pos) = p.inventory.iter().position(|i| *i == item_id) {
                p.inventory.remove(pos);
            }
            let max = p.max_hp();
            p.hp = p.hp.min(max);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You equip {} ({}).", it.name, slot.label()),
        );
    }

    fn use_item(&mut self, user_id: Uuid, item_id: u32) {
        let Some(it) = item(item_id) else { return };
        let ItemKind::Consumable { heal, restore } = it.kind else {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You can't use {}.", it.name),
            );
            return;
        };
        let has = self
            .players
            .get(&user_id)
            .map(|p| p.inventory.contains(&item_id))
            .unwrap_or(false);
        if !has {
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            if let Some(pos) = p.inventory.iter().position(|i| *i == item_id) {
                p.inventory.remove(pos);
            }
            let max = p.max_hp();
            p.hp = (p.hp + heal).min(max);
            p.resource = (p.resource + restore).min(p.max_resource);
        }
        self.log_to(user_id, LogKind::Loot, format!("You use {}.", it.name));
        self.dirty = true;
    }

    fn buy(&mut self, user_id: Uuid, item_id: u32) {
        let room_id = match self.players.get(&user_id) {
            Some(p) => p.room,
            None => return,
        };
        let Some(shop) = shop_at(room_id) else {
            self.log_to(
                user_id,
                LogKind::System,
                "There is no shop here.".to_string(),
            );
            return;
        };
        if !shop.stock.contains(&item_id) {
            return;
        }
        let Some(it) = item(item_id) else { return };
        let gold = self.players.get(&user_id).map(|p| p.gold).unwrap_or(0);
        if gold < it.price {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You can't afford {} ({}g).", it.name, it.price),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= it.price;
            p.inventory.push(item_id);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You buy {} for {}g.", it.name, it.price),
        );
    }

    fn sell(&mut self, user_id: Uuid, item_id: u32) {
        if shop_at(self.players.get(&user_id).map(|p| p.room).unwrap_or(0)).is_none() {
            self.log_to(
                user_id,
                LogKind::System,
                "You need a merchant to sell.".to_string(),
            );
            return;
        }
        let Some(it) = item(item_id) else { return };
        let price = it.sell_price();
        if let Some(p) = self.players.get_mut(&user_id) {
            if let Some(pos) = p.inventory.iter().position(|i| *i == item_id) {
                p.inventory.remove(pos);
                p.gold += price;
            } else {
                return;
            }
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You sell {} for {}g.", it.name, price),
        );
    }

    // ---- Tick -----------------------------------------------------------

    fn tick(&mut self) -> TickOutput {
        self.pending_kills.clear();
        let now = Instant::now();

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

        // Mob damage-over-time from player abilities.
        let dot_ids: Vec<u32> = self.mob_dots.keys().copied().collect();
        for mob_id in dot_ids {
            let mut total = 0;
            let mut owner = None;
            if let Some(stacks) = self.mob_dots.get_mut(&mob_id) {
                for (uid, per, rem) in stacks.iter_mut() {
                    if *rem > 0 {
                        total += *per;
                        *rem -= 1;
                        owner = Some(*uid);
                    }
                }
                stacks.retain(|(_, _, rem)| *rem > 0);
                if stacks.is_empty() {
                    self.mob_dots.remove(&mob_id);
                }
            }
            if total > 0
                && let Some(mob) = self.mobs.get_mut(&mob_id)
                && mob.alive
            {
                mob.hp -= total;
                self.dirty = true;
                if mob.hp <= 0
                    && let Some(uid) = owner
                {
                    self.kill_mob(uid, mob_id);
                }
            }
        }

        // Respawn downed players.
        let resurrecting: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.respawn_at.is_some_and(|at| now >= at))
            .map(|(id, _)| *id)
            .collect();
        for user_id in resurrecting {
            if let Some(player) = self.players.get_mut(&user_id) {
                player.hp = player.max_hp();
                player.resource = player.max_resource;
                player.room = TEMPLE_ROOM;
                player.target = None;
                player.respawn_at = None;
                player.death_save_used = false;
                player.shield = 0;
                player.empower = 0;
            }
            self.log_to(
                user_id,
                LogKind::System,
                "You wake at the Temple of the Dawn, restored.".to_string(),
            );
            self.describe_room(user_id);
            self.dirty = true;
        }

        // Per-player upkeep: regen, buff/shield/effect timers, cooldowns.
        let player_ids: Vec<Uuid> = self.players.keys().copied().collect();
        for uid in &player_ids {
            let mut hot_heal = 0;
            if let Some(p) = self.players.get_mut(uid) {
                if p.class.is_some() && p.respawn_at.is_none() {
                    p.resource = (p.resource + p.resource_regen).min(p.max_resource);
                }
                if p.empower_ticks > 0 {
                    p.empower_ticks -= 1;
                    if p.empower_ticks == 0 {
                        p.empower = 0;
                    }
                }
                if p.shield_ticks > 0 {
                    p.shield_ticks -= 1;
                    if p.shield_ticks == 0 {
                        p.shield = 0;
                    }
                }
                if p.stunned > 0 {
                    p.stunned -= 1;
                }
                for e in p.self_effects.iter_mut() {
                    if e.kind == AbilityEffect::HealOverTime && e.remaining > 0 {
                        hot_heal += e.magnitude;
                        e.remaining -= 1;
                    }
                }
                p.self_effects.retain(|e| e.remaining > 0);
                for cd in p.cooldowns.values_mut() {
                    if *cd > 0 {
                        *cd -= 1;
                    }
                }
            }
            if hot_heal > 0 {
                self.heal_player(*uid, hot_heal);
            }
        }

        // Resolve a combat round for each engaged player.
        let fighters: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.target.is_some() && p.respawn_at.is_none())
            .map(|(id, _)| *id)
            .collect();

        for user_id in fighters {
            let (mob_id, base_atk, opening) = match self.players.get(&user_id) {
                Some(p) => (p.target, p.attack(), p.opening_strike),
                None => continue,
            };
            let Some(mob_id) = mob_id else { continue };
            let alive = self.mobs.get(&mob_id).map(|m| m.alive).unwrap_or(false);
            if !alive {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.target = None;
                }
                continue;
            }
            // Opportunist: the Rogue's opening strike of a fight lands as a crit.
            let player_atk = if opening { base_atk * 2 } else { base_atk };
            if opening {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.opening_strike = false;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    "Opportunist! Your opening strike lands true.".to_string(),
                );
            }
            // Auto-attack is physical and runs through the mob's resistances,
            // so a physical-resistant foe rewards switching to spells.
            let dead = {
                let Some(mob) = self.mobs.get_mut(&mob_id) else {
                    continue;
                };
                let (dealt, _) = mob.spawn.profile.apply(player_atk, DamageType::Physical);
                mob.hp -= dealt;
                self.dirty = true;
                mob.hp <= 0
            };
            if dead {
                self.kill_mob(user_id, mob_id);
                continue;
            }
            // Mob strikes back unless stunned.
            let stunned = self.mob_stuns.get(&mob_id).copied().unwrap_or(0) > 0;
            if let Some(v) = self.mob_stuns.get_mut(&mob_id)
                && *v > 0
            {
                *v -= 1;
            }
            if stunned {
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    "The foe is stunned and cannot strike.".to_string(),
                );
                continue;
            }
            let (mob_damage, mob_dtype, mob_name) = self
                .mobs
                .get(&mob_id)
                .map(|m| {
                    (
                        m.spawn.damage,
                        m.spawn.profile.attack_type,
                        m.spawn.name.to_string(),
                    )
                })
                .unwrap_or((0, DamageType::Physical, String::new()));
            self.strike_player(user_id, mob_damage, mob_dtype, &mob_name);
        }

        // Drop idle players.
        let idle: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| {
                p.last_activity.elapsed() >= Duration::from_secs(PLAYER_IDLE_TIMEOUT_SECS)
            })
            .map(|(id, _)| *id)
            .collect();
        let mut idle_saves = Vec::new();
        for user_id in idle {
            if let Some(saved) = self.export_saved(user_id) {
                idle_saves.push((user_id, saved));
            }
            self.players.remove(&user_id);
            self.dirty = true;
        }

        if self.dirty {
            self.generation = self.generation.wrapping_add(1);
        }
        TickOutput {
            kills: std::mem::take(&mut self.pending_kills),
            idle_saves,
        }
    }

    fn strike_player(&mut self, user_id: Uuid, raw: i32, dtype: DamageType, mob_name: &str) {
        let now = Instant::now();
        let Some(p) = self.players.get_mut(&user_id) else {
            return;
        };
        // Armor blunts physical blows fully but only half-protects against
        // elemental and other schools, so caster foes hit harder through plate.
        let armor = p.armor();
        let reduction = if dtype == DamageType::Physical {
            armor / 2
        } else {
            armor / 4
        };
        let mut dmg = (raw - reduction).max(1);
        if p.shield > 0 {
            let absorbed = p.shield.min(dmg);
            p.shield -= absorbed;
            dmg -= absorbed;
        }
        p.hp -= dmg;
        self.dirty = true;
        let verb = dtype.verb();
        if p.hp <= 0 {
            // Warrior trait: survive the first lethal blow at 1 HP.
            if p.class == Some(Class::Warrior) && !p.death_save_used {
                p.death_save_used = true;
                p.hp = 1;
                self.log_to(
                    user_id,
                    LogKind::System,
                    "Unbreakable! You refuse to fall.".to_string(),
                );
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{mob_name} {verb} you to the brink."),
                );
                return;
            }
            p.hp = 0;
            p.target = None;
            p.respawn_at = Some(now + Duration::from_secs(PLAYER_RESPAWN_SECS));
            self.log_to(
                user_id,
                LogKind::System,
                "You have fallen! Darkness takes you...".to_string(),
            );
        } else {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{mob_name} {verb} you for {dmg}."),
            );
        }
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
                None => (
                    String::new(),
                    String::new(),
                    String::new(),
                    true,
                    Vec::new(),
                ),
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
                    max_hp: other.max_hp(),
                    in_combat: other.target.is_some(),
                })
                .collect();
            let in_combat_with = player.target.and_then(|mob_id| {
                self.mobs
                    .get(&mob_id)
                    .filter(|m| m.alive)
                    .map(|m| m.spawn.name.to_string())
            });

            let (classed, class_name, trait_name, trait_desc, resource_name) = match player.class {
                Some(c) => (
                    true,
                    c.name().to_string(),
                    c.trait_name().to_string(),
                    c.trait_desc().to_string(),
                    c.resource().label().to_string(),
                ),
                None => (
                    false,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            };

            let abilities: Vec<AbilityView> = match player.class {
                Some(c) => unlocked_for(c, player.level)
                    .iter()
                    .enumerate()
                    .map(|(i, a)| AbilityView {
                        slot: (i + 1) as u8,
                        name: a.name.to_string(),
                        cost: a.cost,
                        ready: player.cooldowns.get(&a.id).copied().unwrap_or(0) == 0
                            && player.resource >= a.cost,
                        effect: a.effect.label().to_string(),
                    })
                    .collect(),
                None => Vec::new(),
            };

            let inventory: Vec<InvView> = player
                .inventory
                .iter()
                .filter_map(|id| item(*id))
                .map(|it| InvView {
                    item_id: it.id,
                    name: it.name.to_string(),
                    rarity: it.rarity.label().to_string(),
                    slot: it.slot().map(|s| s.label().to_string()),
                    equipped: false,
                    sell_price: it.sell_price(),
                })
                .chain(
                    player
                        .equipped
                        .values()
                        .filter_map(|id| item(*id))
                        .map(|it| InvView {
                            item_id: it.id,
                            name: it.name.to_string(),
                            rarity: it.rarity.label().to_string(),
                            slot: it.slot().map(|s| s.label().to_string()),
                            equipped: true,
                            sell_price: it.sell_price(),
                        }),
                )
                .collect();

            let shop = shop_at(player.room).map(|shop| ShopView {
                npc_name: shop.npc_name.to_string(),
                shop_name: shop.shop_name.to_string(),
                greeting: shop.greeting.to_string(),
                entries: shop
                    .stock
                    .iter()
                    .filter_map(|id| item(*id))
                    .map(|it| ShopEntryView {
                        item_id: it.id,
                        name: it.name.to_string(),
                        rarity: it.rarity.label().to_string(),
                        price: it.price,
                        affordable: player.gold >= it.price,
                    })
                    .collect(),
            });

            let xp_into = player.xp - xp_for_level(player.level);
            let xp_next = if player.level >= Class::MAX_LEVEL {
                0
            } else {
                xp_for_level(player.level + 1) - xp_for_level(player.level)
            };

            players.insert(
                *user_id,
                PlayerView {
                    joined: true,
                    classed,
                    class_name,
                    trait_name,
                    trait_desc,
                    resource_name,
                    resource: player.resource,
                    max_resource: player.max_resource,
                    alive: player.respawn_at.is_none(),
                    hp: player.hp,
                    max_hp: player.max_hp(),
                    attack: player.attack(),
                    armor: player.armor(),
                    xp: player.xp,
                    xp_into_level: xp_into.max(0),
                    xp_for_next: xp_next,
                    level: player.level,
                    gold: player.gold,
                    room_name,
                    room_desc,
                    zone,
                    safe,
                    exits,
                    mobs,
                    occupants,
                    in_combat_with,
                    abilities,
                    inventory,
                    shop,
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

/// A short combat-log suffix announcing a resist or weakness, empty for normal.
fn defense_tag(defense: Defense, _dtype: DamageType) -> &'static str {
    match defense {
        Defense::Weak => " - it's weak to this!",
        Defense::Resist => " - resisted",
        Defense::Normal => "",
    }
}

fn push_log(log: &mut Vec<LogLine>, kind: LogKind, text: String) {
    log.push(LogLine { text, kind });
    if log.len() > LOG_CAP {
        let overflow = log.len() - LOG_CAP;
        log.drain(0..overflow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn world() -> WorldState {
        WorldState::new(uid(999), seed_world())
    }

    #[test]
    fn join_then_choose_class_sets_stats() {
        let mut s = world();
        assert!(s.join(uid(1)));
        assert!(!s.is_classed(uid(1)));
        s.choose_class(uid(1), Class::Mage);
        assert!(s.is_classed(uid(1)));
        let p = s.players.get(&uid(1)).unwrap();
        assert_eq!(p.class, Some(Class::Mage));
        assert!(p.max_resource > 0);
        assert_eq!(p.hp, p.max_hp());
    }

    #[test]
    fn unclassed_player_cannot_move_or_fight() {
        let mut s = world();
        s.join(uid(1));
        s.move_player(uid(1), Dir::South);
        assert_eq!(s.players[&uid(1)].room, s.world.start_room);
        s.engage(uid(1));
        assert!(s.players[&uid(1)].target.is_none());
    }

    #[test]
    fn buying_costs_gold_and_adds_item() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        // Walk to the smith (room 3, east of square).
        s.move_player(uid(1), Dir::East);
        assert_eq!(s.players[&uid(1)].room, 3);
        let before = s.players[&uid(1)].gold;
        s.buy(uid(1), 1001); // Iron Longsword, 80g
        let p = &s.players[&uid(1)];
        assert_eq!(p.gold, before - 80);
        assert!(p.inventory.contains(&1001));
    }

    #[test]
    fn equipping_a_weapon_raises_attack() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let base = s.players[&uid(1)].attack();
        s.players.get_mut(&uid(1)).unwrap().inventory.push(1006); // greatsword +16
        s.equip(uid(1), 1006);
        assert!(s.players[&uid(1)].attack() > base);
    }

    #[test]
    fn rogue_opening_strike_is_flagged_then_consumed() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Rogue);
        // Move to a combat room with a mob (room 6, goblin) and engage.
        s.move_player(uid(1), Dir::South);
        s.move_player(uid(1), Dir::South);
        s.engage(uid(1));
        assert!(s.players[&uid(1)].opening_strike, "rogue arms opening crit");
        // One tick resolves the auto-attack and consumes the opening strike.
        s.tick();
        assert!(!s.players[&uid(1)].opening_strike, "opening crit is spent");
    }

    #[test]
    fn warrior_does_not_arm_opening_strike() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.move_player(uid(1), Dir::South);
        s.move_player(uid(1), Dir::South);
        s.engage(uid(1));
        assert!(
            !s.players[&uid(1)].opening_strike,
            "only rogues get the crit"
        );
    }

    #[test]
    fn warrior_survives_first_lethal_blow() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
        assert_eq!(
            s.players[&uid(1)].hp,
            1,
            "Unbreakable should save the warrior"
        );
        s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
        assert!(s.players[&uid(1)].respawn_at.is_some(), "second blow falls");
    }
}
