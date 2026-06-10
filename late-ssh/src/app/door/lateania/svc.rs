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

use chrono::Utc;
use late_core::{
    MutexRecover,
    db::Db,
    models::{mud_character::MudCharacter, mud_world_state::MudWorldState, user::User},
};
use rand::Rng;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::app::activity::{event::ActivityGame, publisher::ActivityPublisher};

use super::abilities::{Ability, AbilityEffect, learned_at, unlocked_for};
use super::classes::{Class, level_for_xp, xp_for_level};
use super::damage::{DamageType, Defense};
use super::items::{ItemKind, Slot, item, shop_at};
use super::persist::{
    SavedCharacter, SavedCharacterInit, SavedMob, SavedMobDot, SavedMobStun, SavedWorld,
};
use super::stats::AbilityScores;
use super::world::{
    CritterKind, Dir, FeatureKind, MiniMap, MobSpawn, Perk, RoomId, World, critter_index,
    critters_at, features_at, seed_world,
};

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
/// How often the shared world runtime snapshot is persisted.
const WORLD_AUTOSAVE_SECS: u64 = 15;
const LATEANIA_WORLD_KEY: &str = "lateania";

/// Account age (in days) at which an adventurer is a "citizen" of Lateania and
/// earns extra resurrections.
const VETERAN_DAYS: i64 = 20;
/// In-place resurrections a veteran gets per adventure (refreshed at a capital
/// fountain). Newer accounts get none and respawn at the temple as before.
const VETERAN_RESURRECTIONS: u8 = 2;

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
    character_resets: Arc<StdMutex<HashSet<Uuid>>>,
}

// ---- Snapshot (what sessions render) -------------------------------------

#[derive(Clone, Debug)]
pub struct LogLine {
    pub text: String,
    pub kind: LogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Room,
    Travel,
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
    pub level: i32,
    /// Rarity rank for colouring the name: common/uncommon/rare/epic/legendary.
    pub rank: String,
    pub boss: bool,
}

/// One Frontier zone quest and whether the player has cleared it.
#[derive(Clone, Debug)]
pub struct QuestView {
    pub name: String,
    pub done: bool,
    pub reward: String,
}

/// One wild creature in the room, for the Wildlife list.
#[derive(Clone, Debug)]
pub struct WildlifeView {
    pub name: String,
    pub note: String,
    /// "huntable", "boon", or "" for ambient/skittish.
    pub kind: String,
    /// Perk label for boons (e.g. "emboldened"); empty otherwise.
    pub perk: String,
}

#[derive(Clone, Debug)]
pub struct OccupantView {
    pub user_id: Uuid,
    pub hp: i32,
    pub max_hp: i32,
    pub in_combat: bool,
}

/// One lookable thing in the current room, as shown in the Examine panel.
#[derive(Clone, Debug)]
pub struct FeatureView {
    pub name: String,
    /// Short kind tag ("fountain", "plaque", "vista", or "" for plain scenery).
    pub kind: String,
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
    /// Compact stat summary for the panel, e.g. "+8 atk" or "heal 30".
    pub stats: String,
}

/// One shop listing.
#[derive(Clone, Debug)]
pub struct ShopEntryView {
    pub item_id: u32,
    pub name: String,
    pub rarity: String,
    pub price: i64,
    pub affordable: bool,
    /// Compact stat summary for the panel, e.g. "+8 atk".
    pub stats: String,
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
    /// The companion this player is auto-following, if any (for the UI tag).
    pub following: Option<Uuid>,
    /// Wild creatures sharing the room.
    pub wildlife: Vec<WildlifeView>,
    pub in_combat_with: Option<String>,
    pub abilities: Vec<AbilityView>,
    pub inventory: Vec<InvView>,
    pub shop: Option<ShopView>,
    pub log: Vec<LogLine>,
    pub respawning: bool,
    /// Rolled D&D ability scores (shown on the select screen and sheet).
    pub scores: AbilityScores,
    /// Titles earned by slaying notable foes.
    pub titles: Vec<String>,
    /// Level for each title (parallel to `titles`).
    pub title_levels: Vec<i32>,
    /// Index of the displayed title, if one is chosen.
    pub active_title: Option<usize>,
    /// The Frontier zone quests and their completion state.
    pub quests: Vec<QuestView>,
    /// Veteran in-place resurrections remaining / total this adventure.
    pub resurrections_left: u8,
    pub resurrection_cap: u8,
    /// Lookable things in the current room (Examine panel).
    pub features: Vec<FeatureView>,
    /// Overhead map of the explored neighbourhood around the player.
    pub minimap: MiniMap,
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
            following: None,
            wildlife: Vec::new(),
            in_combat_with: None,
            abilities: Vec::new(),
            inventory: Vec::new(),
            shop: None,
            log: Vec::new(),
            respawning: false,
            scores: AbilityScores::default(),
            titles: Vec::new(),
            title_levels: Vec::new(),
            active_title: None,
            quests: Vec::new(),
            resurrections_left: 0,
            resurrection_cap: 0,
            features: Vec::new(),
            minimap: MiniMap::default(),
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
            character_resets: Arc::new(StdMutex::new(HashSet::new())),
        };
        svc.load_world_state_task();
        svc.start_tick_loop();
        svc.start_autosave_loop();
        svc.start_world_autosave_loop();
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
            if svc.character_reset_in_progress(user_id) {
                return;
            }
            let load_version = svc.current_persist_version(user_id);

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

            // Accounts older than VETERAN_DAYS earn extra resurrections. Best
            // effort: any DB failure simply means "not a veteran".
            let veteran = match svc.db.get().await {
                Ok(client) => match User::get(&client, user_id).await {
                    Ok(Some(user)) => (Utc::now() - user.created).num_days() >= VETERAN_DAYS,
                    _ => false,
                },
                Err(_) => false,
            };

            let mut state = svc.state.lock().await;
            if !svc.has_active_session(user_id) {
                return;
            }
            if svc.character_reset_in_progress(user_id) {
                return;
            }
            let saved = if svc.current_persist_version(user_id) == load_version {
                saved
            } else {
                svc.prepared_saved(user_id)
            };
            if !state.players.contains_key(&user_id) {
                state.join(user_id);
                state.set_veteran(user_id, veteran);
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
                    .and_then(|saved| svc.prepare_persist(user_id, saved));
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

    fn begin_character_reset(&self, user_id: Uuid) {
        self.character_resets.lock_recover().insert(user_id);
        let mut versions = self.persist_versions.lock_recover();
        versions
            .entry(user_id)
            .and_modify(|version| *version += 1)
            .or_insert(1);
        self.prepared_saves.lock_recover().remove(&user_id);
    }

    fn finish_character_reset(&self, user_id: Uuid) {
        self.character_resets.lock_recover().remove(&user_id);
    }

    fn character_reset_in_progress(&self, user_id: Uuid) -> bool {
        self.character_resets.lock_recover().contains(&user_id)
    }

    fn current_persist_version(&self, user_id: Uuid) -> u64 {
        self.persist_versions
            .lock_recover()
            .get(&user_id)
            .copied()
            .unwrap_or(0)
    }

    fn prepare_persist(&self, user_id: Uuid, saved: SavedCharacter) -> Option<PendingSave> {
        let resets = self.character_resets.lock_recover();
        if resets.contains(&user_id) {
            return None;
        }
        let mut versions = self.persist_versions.lock_recover();
        let version = versions.entry(user_id).and_modify(|v| *v += 1).or_insert(1);
        self.prepared_saves
            .lock_recover()
            .insert(user_id, (*version, saved.clone()));
        Some(PendingSave {
            user_id,
            version: *version,
            saved,
        })
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
        if self.character_reset_in_progress(save.user_id) {
            return;
        }
        if !self.is_latest_persist(&save) {
            return;
        }
        let lock = self.persist_lock(save.user_id);
        let _guard = lock.lock().await;
        if self.character_reset_in_progress(save.user_id) {
            return;
        }
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
                        .filter_map(|(user_id, saved)| svc.prepare_persist(user_id, saved))
                        .collect()
                };
                for save in saves {
                    svc.persist(save).await;
                }
            }
        });
    }

    fn load_world_state_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let saved = match svc.db.get().await {
                Ok(client) => match MudWorldState::load(&client, LATEANIA_WORLD_KEY).await {
                    Ok(Some(blob)) => SavedWorld::from_json(&blob),
                    Ok(None) => None,
                    Err(error) => {
                        tracing::warn!(?error, "failed to load mud world state");
                        None
                    }
                },
                Err(error) => {
                    tracing::warn!(?error, "no db client for mud world state load");
                    None
                }
            };
            let Some(saved) = saved else {
                return;
            };
            let mut state = svc.state.lock().await;
            if state.world_revision != 0 {
                tracing::warn!(
                    world_revision = state.world_revision,
                    "skipping stale mud world state load after live mutations"
                );
                return;
            }
            state.hydrate_world(&saved);
            svc.publish(&state);
        });
    }

    fn start_world_autosave_loop(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(WORLD_AUTOSAVE_SECS));
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                let saved = {
                    let mut state = svc.state.lock().await;
                    if !state.world_dirty {
                        None
                    } else {
                        state.world_dirty = false;
                        Some(state.export_world_saved())
                    }
                };
                if let Some(saved) = saved
                    && !svc.persist_world(saved).await
                {
                    let mut state = svc.state.lock().await;
                    state.world_dirty = true;
                }
            }
        });
    }

    async fn persist_world(&self, saved: SavedWorld) -> bool {
        match self.db.get().await {
            Ok(client) => {
                if let Err(error) =
                    MudWorldState::save(&client, LATEANIA_WORLD_KEY, saved.to_json()).await
                {
                    tracing::warn!(?error, "failed to save mud world state");
                    false
                } else {
                    true
                }
            }
            Err(error) => {
                tracing::warn!(?error, "no db client for mud world state save");
                false
            }
        }
    }

    /// Persist every present character right now. Called on graceful server
    /// shutdown so an adventure in progress is not lost to the gap between
    /// autosaves; mirrors the artboard/pinstar shutdown flushes in main. Saves
    /// are best-effort (each logs on failure), so this always returns Ok.
    pub async fn flush_all(&self) -> anyhow::Result<()> {
        let (saves, world_save): (Vec<PendingSave>, Option<SavedWorld>) = {
            let mut state = self.state.lock().await;
            let saves = state
                .export_all_saved()
                .into_iter()
                .filter_map(|(user_id, saved)| self.prepare_persist(user_id, saved))
                .collect();
            let world_save = if state.world_dirty {
                state.world_dirty = false;
                Some(state.export_world_saved())
            } else {
                None
            };
            (saves, world_save)
        };
        let count = saves.len();
        for save in saves {
            self.persist(save).await;
        }
        let mut world_flushed = false;
        if let Some(saved) = world_save {
            world_flushed = true;
            if !self.persist_world(saved).await {
                let mut state = self.state.lock().await;
                state.world_dirty = true;
            }
        }
        tracing::info!(count, world_flushed, "flushed lateania during shutdown");
        Ok(())
    }

    pub fn choose_class_task(&self, user_id: Uuid, class: Class) {
        self.mutate(user_id, move |s| s.choose_class(user_id, class));
    }

    pub fn move_task(&self, user_id: Uuid, dir: Dir) {
        self.mutate(user_id, move |s| s.move_player(user_id, dir));
    }

    pub fn recall_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.recall(user_id));
    }

    pub fn follow_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.follow_toggle(user_id));
    }

    pub fn follow_to_task(&self, user_id: Uuid, target: Uuid) {
        self.mutate(user_id, move |s| s.follow_to(user_id, target));
    }

    pub fn stop_follow_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.stop_follow(user_id));
    }

    pub fn look_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.look(user_id));
    }

    /// Re-roll ability scores on the selection screen (before a class is chosen).
    pub fn reroll_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.reroll(user_id));
    }

    /// Examine the indexed lookable feature in the current room (and use it,
    /// for fountains).
    pub fn interact_task(&self, user_id: Uuid, idx: usize) {
        self.mutate(user_id, move |s| s.interact(user_id, idx));
    }

    pub fn set_active_title_task(&self, user_id: Uuid, idx: usize) {
        self.mutate(user_id, move |s| s.set_active_title(user_id, idx));
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

    pub fn delete_character_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            svc.begin_character_reset(user_id);
            svc.clear_sessions(user_id);

            {
                let mut state = svc.state.lock().await;
                state.delete_character(user_id);
                svc.publish(&state);
            }

            let lock = svc.persist_lock(user_id);
            let _guard = lock.lock().await;
            match svc.db.get().await {
                Ok(client) => {
                    if let Err(error) = MudCharacter::delete_by_user_id(&client, user_id).await {
                        tracing::warn!(%user_id, ?error, "failed to delete mud character");
                    }
                }
                Err(error) => {
                    tracing::warn!(%user_id, ?error, "no db client for mud character delete");
                }
            }
            svc.prepared_saves.lock_recover().remove(&user_id);
            svc.finish_character_reset(user_id);
        });
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
                let idle_saves = tick.idle_saves;
                if state.dirty {
                    svc.publish(&state);
                    state.dirty = false;
                }
                drop(state);
                for (user_id, saved) in idle_saves {
                    svc.clear_sessions(user_id);
                    if let Some(save) = svc.prepare_persist(user_id, saved) {
                        svc.persist(save).await;
                    }
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
    /// Previous room entered from, for the highlighted minimap trail.
    previous_room: Option<RoomId>,
    /// Every room this character has stood in, for the overhead map.
    visited: HashSet<RoomId>,
    target: Option<u32>,
    /// Another player this character auto-follows when they move (set with `f`).
    following: Option<Uuid>,
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
    /// Rolled D&D ability scores; feed bonus HP (CON) and attack (class key).
    scores: AbilityScores,
    /// Titles earned by slaying notable foes.
    titles: Vec<String>,
    /// Level for each title, parallel to `titles`.
    title_levels: Vec<i32>,
    /// Index into `titles` of the player's chosen display title.
    active_title: Option<usize>,
    /// Frontier zone indices whose quest (slay the boss) the player has cleared.
    completed_quests: Vec<usize>,
    /// Veteran in-place resurrections: total this adventure and how many remain.
    resurrection_cap: u8,
    resurrections_left: u8,
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
        (self.base_max_hp + hp + self.scores.hp_bonus(self.level)).max(1)
    }

    fn attack(&self) -> i32 {
        let (atk, _, _) = self.equipment_mods();
        let stat = self.class.map(|c| self.scores.attack_bonus(c)).unwrap_or(0);
        (self.base_attack + atk + self.empower + stat).max(1)
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
    world_dirty: bool,
    world_revision: u64,
    /// Hunt cooldowns for `Game` critters, keyed by global WILDLIFE index.
    hunted: HashMap<usize, Instant>,
}

const LOG_CAP: usize = 60;
const TEMPLE_ROOM: RoomId = 4;
/// How long a hunted game critter stays gone before it wanders back.
const GAME_RESPAWN: Duration = Duration::from_secs(40);

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
            world_dirty: false,
            world_revision: 0,
            hunted: HashMap::new(),
        }
    }

    fn mark_world_dirty(&mut self) {
        self.world_dirty = true;
        self.world_revision = self.world_revision.wrapping_add(1);
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
            previous_room: None,
            visited: HashSet::from([start]),
            target: None,
            following: None,
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
            scores: AbilityScores::roll(),
            titles: Vec::new(),
            title_levels: Vec::new(),
            active_title: None,
            completed_quests: Vec::new(),
            resurrection_cap: 0,
            resurrections_left: 0,
            last_activity: Instant::now(),
            respawn_at: None,
            log: Vec::new(),
        };
        push_log(
            &mut player.log,
            LogKind::System,
            "Welcome to Lateania. Your fate is rolled - reroll it (r) if you dare, then choose your calling."
                .to_string(),
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

    /// Grant (or clear) the veteran resurrection allowance for this adventure.
    /// Called once on join from the account-age check; a fresh adventure starts
    /// with a full set of charges.
    fn set_veteran(&mut self, user_id: Uuid, veteran: bool) {
        let cap = if veteran { VETERAN_RESURRECTIONS } else { 0 };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.resurrection_cap = cap;
            p.resurrections_left = cap;
        }
        if veteran {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "Twenty days a citizen of Lateania - the world grants you {cap} resurrections this adventure."
                ),
            );
        }
    }

    /// Re-roll ability scores. Only allowed before a class is chosen, so a build
    /// is locked the moment you commit to a calling.
    fn reroll(&mut self, user_id: Uuid) {
        let unclassed = self
            .players
            .get(&user_id)
            .map(|p| p.class.is_none())
            .unwrap_or(false);
        if !unclassed {
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.scores = AbilityScores::roll();
        }
        self.log_to(
            user_id,
            LogKind::System,
            "You cast the bones of fate anew. Fresh scores settle into place.".to_string(),
        );
    }

    fn leave(&mut self, user_id: Uuid) {
        self.players.remove(&user_id);
    }

    fn delete_character(&mut self, user_id: Uuid) {
        self.players.remove(&user_id);
        let before: usize = self.mob_dots.values().map(Vec::len).sum();
        for stacks in self.mob_dots.values_mut() {
            stacks.retain(|(owner, _, _)| *owner != user_id);
        }
        self.mob_dots.retain(|_, stacks| !stacks.is_empty());
        let after: usize = self.mob_dots.values().map(Vec::len).sum();
        if after != before {
            self.mark_world_dirty();
        }
        self.dirty = true;
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
            p.previous_room = None;
            p.visited = saved.visited.iter().copied().collect();
            p.visited.insert(room);
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
            // Rolled scores and earned titles persist across sessions.
            p.scores = saved.scores;
            p.titles = saved.titles.clone();
            p.title_levels = saved.title_levels.clone();
            p.title_levels.resize(p.titles.len(), 1);
            p.active_title = saved.active_title.filter(|&i| i < p.titles.len());
            p.completed_quests = saved.completed_quests.clone();
            // Restore vitals last so equipment and CON max-hp are already in effect.
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
            visited: {
                let mut rooms: Vec<RoomId> = p.visited.iter().copied().collect();
                rooms.sort_unstable();
                rooms
            },
            inventory: p.inventory.clone(),
            equipped,
            scores: p.scores,
            titles: p.titles.clone(),
            title_levels: p.title_levels.clone(),
            active_title: p.active_title,
            completed_quests: p.completed_quests.clone(),
        }))
    }

    fn export_all_saved(&self) -> Vec<(Uuid, SavedCharacter)> {
        self.players
            .keys()
            .filter_map(|uid| self.export_saved(*uid).map(|s| (*uid, s)))
            .collect()
    }

    fn export_world_saved(&self) -> SavedWorld {
        let now = Instant::now();
        let mut mobs = self
            .mobs
            .values()
            .map(|mob| SavedMob {
                id: mob.spawn.id,
                hp: mob.hp,
                alive: mob.alive,
                respawn_remaining_secs: mob
                    .respawn_at
                    .map(|at| at.saturating_duration_since(now).as_secs()),
            })
            .collect::<Vec<_>>();
        mobs.sort_by_key(|mob| mob.id);

        let mut mob_stuns = self
            .mob_stuns
            .iter()
            .filter_map(|(mob_id, remaining_ticks)| {
                (*remaining_ticks > 0).then_some(SavedMobStun {
                    mob_id: *mob_id,
                    remaining_ticks: *remaining_ticks,
                })
            })
            .collect::<Vec<_>>();
        mob_stuns.sort_by_key(|stun| stun.mob_id);

        let mut mob_dots = self
            .mob_dots
            .iter()
            .flat_map(|(mob_id, stacks)| {
                stacks
                    .iter()
                    .filter_map(|(owner, damage, remaining_ticks)| {
                        (*remaining_ticks > 0).then_some(SavedMobDot {
                            mob_id: *mob_id,
                            owner: *owner,
                            damage: *damage,
                            remaining_ticks: *remaining_ticks,
                        })
                    })
            })
            .collect::<Vec<_>>();
        mob_dots.sort_by_key(|dot| (dot.mob_id, dot.owner));

        SavedWorld::new(mobs, mob_stuns, mob_dots)
    }

    fn hydrate_world(&mut self, saved: &SavedWorld) {
        let now = Instant::now();
        for saved_mob in &saved.mobs {
            let Some(mob) = self.mobs.get_mut(&saved_mob.id) else {
                continue;
            };
            mob.alive = saved_mob.alive;
            mob.hp = if saved_mob.alive {
                saved_mob.hp.clamp(1, mob.spawn.max_hp)
            } else {
                0
            };
            mob.respawn_at = if saved_mob.alive {
                None
            } else {
                let secs = saved_mob
                    .respawn_remaining_secs
                    .unwrap_or(mob.spawn.respawn_secs);
                Some(now + Duration::from_secs(secs))
            };
        }

        self.mob_stuns.clear();
        for stun in &saved.mob_stuns {
            if stun.remaining_ticks > 0 && self.mobs.contains_key(&stun.mob_id) {
                self.mob_stuns.insert(stun.mob_id, stun.remaining_ticks);
            }
        }

        self.mob_dots.clear();
        for dot in &saved.mob_dots {
            if dot.remaining_ticks > 0 && self.mobs.contains_key(&dot.mob_id) {
                self.mob_dots.entry(dot.mob_id).or_default().push((
                    dot.owner,
                    dot.damage,
                    dot.remaining_ticks,
                ));
            }
        }

        self.dirty = true;
        self.world_dirty = false;
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
        let from = self.players.get(&user_id).map(|p| p.room).unwrap_or(dest);
        if let Some(player) = self.players.get_mut(&user_id) {
            player.previous_room = Some(from);
            player.room = dest;
            player.visited.insert(dest);
        }
        self.describe_room(user_id);
        self.apply_critter_perks(user_id);
        self.move_followers(user_id, from, dest, dir);
    }

    /// Drag everyone following the mover from `from` into `dest`, walking the
    /// whole follow-chain. Followers who are mid-combat or downed stay put.
    fn move_followers(&mut self, leader: Uuid, from: RoomId, dest: RoomId, dir: Dir) {
        if from == dest {
            return;
        }
        let mut queue = vec![leader];
        while let Some(lead) = queue.pop() {
            let followers: Vec<Uuid> = self
                .players
                .values()
                .filter(|p| {
                    p.following == Some(lead)
                        && p.room == from
                        && p.target.is_none()
                        && p.respawn_at.is_none()
                })
                .map(|p| p.user_id)
                .collect();
            for f in followers {
                if let Some(p) = self.players.get_mut(&f) {
                    p.previous_room = Some(from);
                    p.room = dest;
                    p.visited.insert(dest);
                }
                self.log_to(
                    f,
                    LogKind::Normal,
                    format!("You follow along, heading {}.", dir.label()),
                );
                self.describe_room(f);
                self.apply_critter_perks(f);
                queue.push(f);
            }
        }
        self.dirty = true;
    }

    /// Speak the word of recall: return to Embergate's Town Square from anywhere,
    /// so long as you are not in combat. A universal escape, not a class spell.
    fn recall(&mut self, user_id: Uuid) {
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
                "You can't recall in the thick of combat - flee (z) first.".to_string(),
            );
            return;
        }
        let home = self.world.start_room;
        if player.room == home {
            self.log_to(
                user_id,
                LogKind::Normal,
                "You speak the word of recall, but Embergate's lanterns already stand around you."
                    .to_string(),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.previous_room = Some(p.room);
            p.room = home;
            p.visited.insert(home);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            "You speak the word of recall. The world folds soft as cloth, and the lanternlight of Embergate's Town Square rises around you."
                .to_string(),
        );
        self.describe_room(user_id);
        self.apply_critter_perks(user_id);
        self.dirty = true;
    }

    /// Toggle auto-following: with no companion set, begin following another
    /// adventurer in this room; otherwise stop following.
    fn follow_toggle(&mut self, user_id: Uuid) {
        if !self.is_classed(user_id) {
            return;
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.following.is_some() {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.following = None;
            }
            self.log_to(user_id, LogKind::Normal, "You stop following.".to_string());
            self.dirty = true;
            return;
        }
        let room = player.room;
        let target = self
            .players
            .values()
            .find(|other| other.user_id != user_id && other.room == room && other.class.is_some())
            .map(|other| other.user_id);
        match target {
            Some(t) => {
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.following = Some(t);
                }
                self.log_to(
                    user_id,
                    LogKind::Normal,
                    "You fall into step behind a companion - you move with them now (f to stop)."
                        .to_string(),
                );
            }
            None => {
                self.log_to(
                    user_id,
                    LogKind::Normal,
                    "There's no one here to follow.".to_string(),
                );
            }
        }
        self.dirty = true;
    }

    /// Follow (or stop following) a specific adventurer chosen from the Follow
    /// panel; picking your current companion again clears the follow.
    fn follow_to(&mut self, user_id: Uuid, target: Uuid) {
        if !self.is_classed(user_id) || user_id == target {
            return;
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        let room = player.room;
        let already = player.following == Some(target);
        let valid = self
            .players
            .get(&target)
            .is_some_and(|o| o.class.is_some() && o.room == room);
        let msg = if already {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.following = None;
            }
            "You stop following.".to_string()
        } else if valid {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.following = Some(target);
            }
            "You fall into step behind them - you move together now (f to manage).".to_string()
        } else {
            "They're no longer here to follow.".to_string()
        };
        self.log_to(user_id, LogKind::Normal, msg);
        self.dirty = true;
    }

    fn stop_follow(&mut self, user_id: Uuid) {
        if !self.is_classed(user_id) {
            return;
        }
        let was_following = self
            .players
            .get(&user_id)
            .is_some_and(|p| p.following.is_some());
        if !was_following {
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.following = None;
        }
        self.log_to(user_id, LogKind::Normal, "You stop following.".to_string());
        self.dirty = true;
    }

    /// Apply any Boon-creature perks for the room a player just entered.
    fn apply_critter_perks(&mut self, user_id: Uuid) {
        let room_id = match self.players.get(&user_id) {
            Some(p) => p.room,
            None => return,
        };
        let boons: Vec<(Perk, &'static str)> = critters_at(room_id)
            .into_iter()
            .filter_map(|c| match c.kind {
                CritterKind::Boon(p) => Some((p, c.name)),
                _ => None,
            })
            .collect();
        for (perk, name) in boons {
            if let Some(p) = self.players.get_mut(&user_id) {
                match perk {
                    Perk::Embolden => {
                        p.empower = p.empower.max(3);
                        p.empower_ticks = p.empower_ticks.max(6);
                    }
                    Perk::Mend => {
                        let max = p.max_hp();
                        p.hp = (p.hp + max / 8 + 2).min(max);
                    }
                    Perk::Quicken => {
                        p.resource = (p.resource + p.max_resource / 4 + 1).min(p.max_resource);
                    }
                }
            }
            self.log_to(
                user_id,
                LogKind::Loot,
                format!(
                    "{name} lends you a moment's grace - you feel {}.",
                    perk.label()
                ),
            );
        }
    }

    /// Hunt a small-game critter in this room (no foe present): a little xp, and
    /// it slips away for a while. Returns true if something was caught.
    fn try_hunt(&mut self, user_id: Uuid, room_id: RoomId) -> bool {
        let now = Instant::now();
        let caught = critters_at(room_id).into_iter().find_map(|c| {
            if c.kind != CritterKind::Game {
                return None;
            }
            let gi = critter_index(c)?;
            let available = match self.hunted.get(&gi) {
                Some(t) => now.duration_since(*t) >= GAME_RESPAWN,
                None => true,
            };
            available.then_some((gi, c.name, c.xp))
        });
        let Some((gi, name, xp)) = caught else {
            return false;
        };
        self.hunted.insert(gi, now);
        if let Some(p) = self.players.get_mut(&user_id) {
            p.xp += xp as i64;
        }
        self.check_level_up(user_id);
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You stalk and catch {name}. (+{xp} xp)"),
        );
        self.dirty = true;
        true
    }

    fn look(&mut self, user_id: Uuid) {
        self.describe_room_context(user_id, false);
    }

    fn describe_room(&mut self, user_id: Uuid) {
        self.describe_room_context(user_id, true);
    }

    fn describe_room_context(&mut self, user_id: Uuid, announce_travel: bool) {
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
        if announce_travel {
            self.log_to(user_id, LogKind::Travel, format!("Arrived at {name}."));
        }
        self.log_to(user_id, LogKind::Room, format!("== {name} =="));
        self.log_to(user_id, LogKind::Room, desc);
        self.log_to(user_id, LogKind::Room, format!("Exits: {exit_text}"));
        if let Some(shop) = shop {
            self.log_to(
                user_id,
                LogKind::Room,
                format!(
                    "{} tends {} here. Press b to browse.",
                    shop.npc_name, shop.shop_name
                ),
            );
        }
        for mob in mob_names {
            self.log_to(user_id, LogKind::Room, format!("{mob} is here."));
        }
        // Note lookable things without revealing them - you must look (o) to see
        // their description.
        let features = features_at(room_id);
        if !features.is_empty() {
            let names: Vec<&str> = features.iter().map(|f| f.name).collect();
            self.log_to(
                user_id,
                LogKind::Room,
                format!(
                    "You notice {} here. Press o to look closer.",
                    join_with_and(&names)
                ),
            );
        }
    }

    /// Examine the indexed lookable feature in the current room. The feature's
    /// description is revealed only here (the "look at things" rule); fountains
    /// in a safe capital also restore vitals and refresh resurrection charges.
    fn interact(&mut self, user_id: Uuid, idx: usize) {
        let room_id = match self.players.get(&user_id) {
            Some(p) => p.room,
            None => return,
        };
        let features = features_at(room_id);
        let Some(feat) = features.get(idx) else {
            return;
        };
        self.log_to(
            user_id,
            LogKind::Normal,
            format!("You look at {}.", feat.name),
        );
        self.log_to(user_id, LogKind::Normal, feat.desc.to_string());
        if feat.kind == FeatureKind::Fountain {
            let safe = self.world.room(room_id).is_some_and(|r| r.safe);
            if safe {
                if let Some(p) = self.players.get_mut(&user_id) {
                    let max = p.max_hp();
                    p.hp = max;
                    p.resource = p.max_resource;
                    p.resurrections_left = p.resurrection_cap;
                }
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    "The fountain's clear waters wash through you. Health and power are restored, and your strength to rise again renews."
                        .to_string(),
                );
            }
        }
        self.dirty = true;
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
                // No foe: if there's small game about, hunt it instead.
                if !self.try_hunt(user_id, room_id) {
                    self.log_to(
                        user_id,
                        LogKind::Normal,
                        "There's nothing here to fight.".to_string(),
                    );
                }
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
                    self.mark_world_dirty();
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
        self.mark_world_dirty();
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
        self.mark_world_dirty();
        self.log_to(
            user_id,
            LogKind::Combat,
            format!("{source} festers in the foe ({} damage).", dtype.label()),
        );
        self.dirty = true;
    }

    fn kill_mob(&mut self, user_id: Uuid, mob_id: u32) {
        let (mob_name, xp, loot, boss, mob_level) = match self.mobs.get_mut(&mob_id) {
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
                    mob.spawn.level(),
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
        self.grant_title(user_id, &mob_name, boss, mob_level);
        if boss && let Some(zone) = super::world::frontier_zone_of_boss(&mob_name) {
            self.complete_quest(user_id, zone, mob_level);
        }
        self.check_level_up(user_id);
        self.pending_kills.push(KillOutcome { user_id, mob_name });
        self.dirty = true;
        self.mark_world_dirty();
    }

    /// Set the displayed title to the one at `idx`; selecting the active title
    /// again (or an out-of-range index) clears it.
    fn set_active_title(&mut self, user_id: Uuid, idx: usize) {
        if let Some(p) = self.players.get_mut(&user_id) {
            p.active_title = if p.active_title == Some(idx) || idx >= p.titles.len() {
                None
            } else {
                Some(idx)
            };
            self.dirty = true;
        }
    }

    /// Add a title with its level the first time it is earned, and announce it.
    /// Returns whether it was newly granted.
    fn award_title(&mut self, user_id: Uuid, title: String, level: i32) -> bool {
        let is_new = self
            .players
            .get(&user_id)
            .map(|p| !p.titles.contains(&title))
            .unwrap_or(false);
        if !is_new {
            return false;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.titles.push(title.clone());
            p.title_levels.push(level.max(1));
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("A new title is yours: {title} (Lv {}).", level.max(1)),
        );
        true
    }

    /// Award a title themed on a slain foe, the first time that foe is felled.
    /// Bosses confer a "Bane of ..." honorific; lesser foes a "...bane" epithet.
    fn grant_title(&mut self, user_id: Uuid, mob_name: &str, boss: bool, level: i32) {
        let title = title_for(mob_name, boss);
        self.award_title(user_id, title, level);
    }

    /// Complete the Frontier quest for `zone` (slaying its boss) the first time:
    /// award the "Champion of the ..." title plus an xp/gold bounty.
    fn complete_quest(&mut self, user_id: Uuid, zone: usize, boss_level: i32) {
        let already = self
            .players
            .get(&user_id)
            .map(|p| p.completed_quests.contains(&zone))
            .unwrap_or(true);
        if already {
            return;
        }
        let Some((zname, _boss)) = super::world::frontier_zone_info(zone) else {
            return;
        };
        let bonus_xp = (100 + boss_level * 40) as i64;
        let bonus_gold = (50 + boss_level * 10) as i64;
        if let Some(p) = self.players.get_mut(&user_id) {
            p.completed_quests.push(zone);
            p.xp += bonus_xp;
            p.gold += bonus_gold;
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "Quest complete - the {zname} is cleared! (+{bonus_xp} xp, +{bonus_gold} gold)"
            ),
        );
        self.award_title(user_id, format!("Champion of the {zname}"), boss_level);
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
                    player.previous_room = Some(room_id);
                    player.room = dest;
                    player.visited.insert(dest);
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

        let mut world_changed = false;
        for mob in self.mobs.values_mut() {
            if !mob.alive
                && let Some(at) = mob.respawn_at
                && now >= at
            {
                mob.alive = true;
                mob.hp = mob.spawn.max_hp;
                mob.respawn_at = None;
                self.dirty = true;
                world_changed = true;
            }
        }
        if world_changed {
            self.mark_world_dirty();
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
                self.mark_world_dirty();
            }
            if total > 0
                && let Some(mob) = self.mobs.get_mut(&mob_id)
                && mob.alive
            {
                mob.hp -= total;
                self.dirty = true;
                let dead = mob.hp <= 0;
                self.mark_world_dirty();
                if dead && let Some(uid) = owner {
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
                player.previous_room = Some(player.room);
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
            let (mob_name, dealt, defense, dead) = {
                let Some(mob) = self.mobs.get_mut(&mob_id) else {
                    continue;
                };
                let (dealt, defense) = mob.spawn.profile.apply(player_atk, DamageType::Physical);
                mob.hp -= dealt;
                self.dirty = true;
                (mob.spawn.name.to_string(), dealt, defense, mob.hp <= 0)
            };
            self.mark_world_dirty();
            let tag = defense_tag(defense, DamageType::Physical);
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("You strike {mob_name} for {dealt} physical{tag}."),
            );
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
                self.mark_world_dirty();
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
            // Veteran resurrection: a citizen of twenty days rises where they fell
            // instead of waking back at the temple. Refreshes at a capital fountain.
            if p.resurrections_left > 0 {
                p.resurrections_left -= 1;
                let left = p.resurrections_left;
                let max = p.max_hp();
                p.hp = max;
                p.resource = p.max_resource;
                p.target = None;
                p.shield = 0;
                p.empower = 0;
                p.death_save_used = false;
                let plural = if left == 1 { "" } else { "s" };
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!(
                        "{mob_name} {verb} you down - but Lateania will not have you yet. You rise where you stand. ({left} resurrection{plural} left this adventure.)"
                    ),
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
                    level: m.spawn.level(),
                    rank: m.spawn.rank().to_string(),
                    boss: m.spawn.boss,
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
            let now = Instant::now();
            let wildlife: Vec<WildlifeView> = critters_at(player.room)
                .into_iter()
                .filter(|c| match c.kind {
                    CritterKind::Game => {
                        match critter_index(c).and_then(|gi| self.hunted.get(&gi)) {
                            Some(t) => now.duration_since(*t) >= GAME_RESPAWN,
                            None => true,
                        }
                    }
                    _ => true,
                })
                .map(|c| WildlifeView {
                    name: c.name.to_string(),
                    note: c.note.to_string(),
                    kind: match c.kind {
                        CritterKind::Game => "huntable".to_string(),
                        CritterKind::Boon(_) => "boon".to_string(),
                        CritterKind::Skittish => String::new(),
                    },
                    perk: match c.kind {
                        CritterKind::Boon(p) => p.label().to_string(),
                        _ => String::new(),
                    },
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
                    stats: it.stat_summary(),
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
                            stats: it.stat_summary(),
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
                        stats: it.stat_summary(),
                    })
                    .collect(),
            });

            let xp_into = player.xp - xp_for_level(player.level);
            let xp_next = if player.level >= Class::MAX_LEVEL {
                0
            } else {
                xp_for_level(player.level + 1) - xp_for_level(player.level)
            };

            let features: Vec<FeatureView> = features_at(player.room)
                .iter()
                .map(|f| FeatureView {
                    name: f.name.to_string(),
                    kind: f.kind.tag().to_string(),
                })
                .collect();

            let minimap =
                self.world
                    .minimap(player.room, player.previous_room, &player.visited, 3, 2);
            let quests: Vec<QuestView> = (0..super::world::frontier_zone_count())
                .filter_map(|z| {
                    super::world::frontier_zone_info(z).map(|(zname, boss)| QuestView {
                        name: format!("{zname} - slay {boss}"),
                        done: player.completed_quests.contains(&z),
                        reward: format!("title: Champion of the {zname}"),
                    })
                })
                .collect();

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
                    following: player.following,
                    wildlife,
                    in_combat_with,
                    abilities,
                    inventory,
                    shop,
                    log: player.log.clone(),
                    respawning: player.respawn_at.is_some(),
                    scores: player.scores,
                    titles: player.titles.clone(),
                    title_levels: player.title_levels.clone(),
                    active_title: player.active_title,
                    quests,
                    resurrections_left: player.resurrections_left,
                    resurrection_cap: player.resurrection_cap,
                    features,
                    minimap,
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

/// Derive a title from a slain foe. Bosses already read as proper names ("the
/// Barrow King") and become "Bane of ..."; lesser foes ("a frost-bound wretch")
/// lend their creature word to a "...bane" epithet ("Wretchbane").
fn title_for(mob_name: &str, boss: bool) -> String {
    let trimmed = mob_name.trim();
    let core = trimmed
        .strip_prefix("a ")
        .or_else(|| trimmed.strip_prefix("an "))
        .unwrap_or(trimmed);
    if boss {
        return format!("Bane of {core}");
    }
    let last = core
        .rsplit([' ', '-'])
        .find(|w| !w.is_empty())
        .unwrap_or("Foe");
    let mut chars = last.chars();
    let capitalized = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Foe".to_string(),
    };
    format!("{capitalized}bane")
}

/// Join a short list into prose: "the fountain", "the fountain and the plaque",
/// "the fountain, the plaque, and the vista".
fn join_with_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => only.to_string(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
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
    fn recall_returns_to_the_town_square() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let home = s.world.start_room;
        s.move_player(uid(1), Dir::North); // 1 -> 2, off the square
        assert_ne!(s.players[&uid(1)].room, home, "should have left the square");
        s.recall(uid(1));
        assert_eq!(
            s.players[&uid(1)].room,
            home,
            "recall returns to the square"
        );
    }

    #[test]
    fn following_pulls_a_companion_along() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.join(uid(2));
        s.choose_class(uid(2), Class::Mage);
        // uid(1) follows the only other adventurer in the square.
        s.follow_toggle(uid(1));
        assert_eq!(s.players[&uid(1)].following, Some(uid(2)));
        // When uid(2) walks north, uid(1) is dragged along to the same room.
        s.move_player(uid(2), Dir::North);
        let dest = s.players[&uid(2)].room;
        assert_eq!(s.players[&uid(1)].room, dest);
        // Toggling again stops the follow.
        s.follow_toggle(uid(1));
        assert_eq!(s.players[&uid(1)].following, None);
    }

    #[test]
    fn follow_to_rejects_target_no_longer_in_room() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.join(uid(2));
        s.choose_class(uid(2), Class::Mage);

        s.move_player(uid(2), Dir::North);
        s.follow_to(uid(1), uid(2));

        assert_eq!(s.players[&uid(1)].following, None);
    }

    #[test]
    fn stop_follow_clears_absent_target() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.join(uid(2));
        s.choose_class(uid(2), Class::Mage);

        s.follow_to(uid(1), uid(2));
        assert_eq!(s.players[&uid(1)].following, Some(uid(2)));
        if let Some(p) = s.players.get_mut(&uid(2)) {
            p.room = 2;
        }
        s.stop_follow(uid(1));

        assert_eq!(s.players[&uid(1)].following, None);
    }

    #[test]
    fn hunting_small_game_grants_xp_then_cools_down() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Ranger);
        let before = s.players[&uid(1)].xp;
        // Room 600 (the Greatroad) hosts a fat marsh-rat (Game).
        assert!(s.try_hunt(uid(1), 600), "should catch the game");
        assert!(s.players[&uid(1)].xp > before, "hunting grants xp");
        // It has slipped away, so an immediate second hunt finds nothing.
        assert!(!s.try_hunt(uid(1), 600), "game is on cooldown");
    }

    #[test]
    fn a_boon_creature_mends_on_arrival() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        if let Some(p) = s.players.get_mut(&uid(1)) {
            p.hp = 1;
        }
        // The player starts in the town square, home of the hearth-cat (Mend boon).
        s.apply_critter_perks(uid(1));
        assert!(s.players[&uid(1)].hp > 1, "the hearth-cat should mend you");
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
    fn combat_tick_logs_player_auto_attack() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        // Move to a combat room with a mob (room 6, goblin) and engage.
        s.move_player(uid(1), Dir::South);
        s.move_player(uid(1), Dir::South);
        s.engage(uid(1));

        s.tick();

        let log = &s.players[&uid(1)].log;
        assert!(
            log.iter()
                .any(|line| line.kind == LogKind::Combat && line.text.starts_with("You strike ")),
            "auto-attacks should be visible in the combat log"
        );
    }

    #[test]
    fn movement_keeps_a_compact_travel_line_in_recent_log() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Mage);
        s.move_player(uid(1), Dir::North);

        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.kind == LogKind::Travel
                    && line.text == "Arrived at Embergate - The Gilded Flagon."),
            "movement should leave a compact room-visit breadcrumb"
        );
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

    #[test]
    fn slaying_a_foe_grants_a_themed_title() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Mage);
        s.grant_title(uid(1), "a frost-bound wretch", false, 4);
        s.grant_title(uid(1), "the Barrow King", true, 21);
        // Re-slaying the same foe must not duplicate its title.
        s.grant_title(uid(1), "a frost-bound wretch", false, 4);
        let titles = s.players[&uid(1)].titles.clone();
        assert!(
            titles.iter().any(|t| t == "Wretchbane"),
            "lesser foe -> ...bane"
        );
        assert!(
            titles.iter().any(|t| t == "Bane of the Barrow King"),
            "boss -> Bane of ..."
        );
        assert_eq!(titles.iter().filter(|t| *t == "Wretchbane").count(), 1);
    }

    #[test]
    fn veteran_resurrects_in_place_then_falls_when_spent() {
        let mut s = world();
        s.join(uid(1));
        s.set_veteran(uid(1), true);
        s.choose_class(uid(1), Class::Mage); // mage has no Warrior death-save
        assert_eq!(s.players[&uid(1)].resurrection_cap, VETERAN_RESURRECTIONS);
        for expected_left in (0..VETERAN_RESURRECTIONS).rev() {
            s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
            let p = &s.players[&uid(1)];
            assert!(p.respawn_at.is_none(), "veteran rises where they fall");
            assert_eq!(p.hp, p.max_hp(), "revived at full health");
            assert_eq!(p.resurrections_left, expected_left);
        }
        s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
        assert!(
            s.players[&uid(1)].respawn_at.is_some(),
            "out of charges, falls"
        );
    }

    #[test]
    fn a_capital_fountain_restores_vitals_and_revives() {
        let mut s = world();
        s.join(uid(1));
        s.set_veteran(uid(1), true);
        s.choose_class(uid(1), Class::Mage);
        if let Some(p) = s.players.get_mut(&uid(1)) {
            p.room = 620; // Tasmania's Harborgate Square (safe capital)
            p.hp = 1;
            p.resource = 0;
            p.resurrections_left = 0;
        }
        s.interact(uid(1), 0); // feature 0 in the square is the fountain
        let p = &s.players[&uid(1)];
        assert_eq!(p.hp, p.max_hp(), "fountain heals to full");
        assert_eq!(p.resource, p.max_resource, "fountain restores resource");
        assert_eq!(
            p.resurrections_left, p.resurrection_cap,
            "fountain refreshes resurrection charges"
        );
    }

    #[test]
    fn ability_scores_change_derived_stats() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior); // STR is the warrior's key score
        let base_attack = s.players[&uid(1)].attack();
        let base_hp = s.players[&uid(1)].max_hp();
        if let Some(p) = s.players.get_mut(&uid(1)) {
            p.scores.strength = 18; // +4
            p.scores.constitution = 18; // +4
        }
        assert!(
            s.players[&uid(1)].attack() > base_attack,
            "STR raises attack"
        );
        assert!(s.players[&uid(1)].max_hp() > base_hp, "CON raises max HP");
    }
}
