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
    models::{
        mud_character::MudCharacter,
        mud_world_state::MudWorldState,
        profile_award::{
            LATEANIA_ARCHDEMON_AWARD_CATEGORY, LATEANIA_FRONTIER_KING_AWARD_CATEGORY, award_badge,
            grant_unique_milestone_award,
        },
        reward::{LATEANIA_ARCHDEMON_REWARD_KEY, LATEANIA_FRONTIER_KING_REWARD_KEY},
        user::User,
    },
};
use rand::Rng;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::chips::svc::ChipService,
};

use super::abilities::{Ability, AbilityEffect, learned_at, unlocked_for};
use super::classes::{Class, level_for_xp, xp_for_level};
use super::damage::{DamageProfile, DamageType, Defense};
use super::items::{
    CATACOMBS_RELIC_ID, CAVERNS_RELIC_ID, ItemKind, Slot, THORNWOOD_RELIC_ID, item, shop_at,
};
use super::persist::{
    SavedCharacter, SavedCharacterInit, SavedMob, SavedMobDot, SavedMobStun, SavedWorld,
};
use super::stats::AbilityScores;
use super::world::{
    CritterKind, Dir, FeatureKind, MiniMap, MobBehavior, MobSpawn, Perk, RoomId, World,
    critter_index, critters_at, features_at, frontier_entrance_room, is_frontier_room, seed_world,
};

/// World heartbeat. One combat round resolves per tick.
const TICK_SECS: u64 = 2;
/// First id handed out to runtime-only summoned adds, kept far clear of the
/// authored spawn-id ranges (base game, Catacombs 800k+, Frontier 900k+).
const SUMMON_ID_START: u32 = 990_000_000;
/// A roamer takes a step at most this often (in ticks); at 2s/tick that is ~8s.
const MOB_MOVE_COOLDOWN: u8 = 4;
/// Ticks per time-of-day phase. Four phases => a ~16-minute day at 2s/tick.
const PHASE_TICKS: u64 = 120;
/// Ticks the weather holds before it rolls over (~3 minutes).
const WEATHER_TICKS: u64 = 90;
/// Fixed id for the lone wandering world boss (reaped like a summon on death).
const WORLD_BOSS_ID: u32 = 999_000_000;
/// The first world boss stirs this many ticks after boot (~2 minutes).
const WORLD_BOSS_FIRST_TICK: u64 = 60;
/// Ticks between one world boss falling and the next rising (~10 minutes).
const WORLD_BOSS_INTERVAL: u64 = 300;

fn now_unix_secs() -> u64 {
    Utc::now().timestamp().max(0) as u64
}

/// The world clock's coarse phase, derived from the tick count. Dusk and Night
/// count as "dark", when the dead grow bolder and stronger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl TimeOfDay {
    fn from_ticks(t: u64) -> Self {
        match (t / PHASE_TICKS) % 4 {
            0 => Self::Dawn,
            1 => Self::Day,
            2 => Self::Dusk,
            _ => Self::Night,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
            Self::Night => "night",
        }
    }
    fn is_dark(self) -> bool {
        matches!(self, Self::Dusk | Self::Night)
    }
    /// Multiplier (percent) applied to mob damage; the dark hits harder.
    fn mob_damage_pct(self) -> i32 {
        if self.is_dark() { 125 } else { 100 }
    }
}

/// The current weather, derived from the tick count. Beyond flavor, fog feeds
/// ambushers and storms charge spellcasters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Fog,
    Storm,
}

impl Weather {
    fn from_ticks(t: u64) -> Self {
        // Offset from the day phase so weather and time drift independently.
        match (t / WEATHER_TICKS + 1) % 4 {
            0 => Self::Clear,
            1 => Self::Rain,
            2 => Self::Fog,
            _ => Self::Storm,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Fog => "fog",
            Self::Storm => "storm",
        }
    }
}
/// A player who sends no command for this long is dropped from the world.
const PLAYER_IDLE_TIMEOUT_SECS: u64 = 10 * 60;
/// How long a defeated player rests before respawning at the temple.
const PLAYER_RESPAWN_SECS: u64 = 8;
/// Gold every new adventurer starts with.
const STARTING_GOLD: i64 = 120;
/// Normal death removes this share of carried gold; banked gold is protected.
const DEATH_GOLD_LOSS_PERCENT: i64 = 20;
const FIRST_DUNGEON_GATE_FROM: RoomId = 30;
const FIRST_DUNGEON_GATE_TO: RoomId = 31;
const FIRST_DUNGEON_GATE_TITLE: &str = "Bane of the Elder Treant";
const FRONTIER_GATE_TITLE: &str = "Bane of the Archdemon Mal'gareth";
const CATACOMBS_GATE_TITLE: &str = "Bane of The Bonewright Lich";
const THORNWOOD_GATE_TITLE: &str = "Bane of the Elder Dryad";
const CAVERNS_GATE_TITLE: &str = "Bane of the Abyss-Thing";
const FRONTIER_REQUIRED_TITLES: [&str; 4] = [
    FRONTIER_GATE_TITLE,
    CATACOMBS_GATE_TITLE,
    THORNWOOD_GATE_TITLE,
    CAVERNS_GATE_TITLE,
];

/// How often the world autosaves every present character's progress.
const AUTOSAVE_SECS: u64 = 60;
/// How often the shared world runtime snapshot is persisted.
const WORLD_AUTOSAVE_SECS: u64 = 15;
const LATEANIA_WORLD_KEY: &str = "lateania";
const LATEANIA_ARCHDEMON_LEDGER_REASON: &str = "lateania_archdemon_defeat";
const LATEANIA_FRONTIER_KING_LEDGER_REASON: &str = "lateania_frontier_king_defeat";

#[derive(Clone, Copy)]
struct BossAchievement {
    mob_name: &'static str,
    reward_key: &'static str,
    ledger_reason: &'static str,
    award_category: &'static str,
}

const ARCHDEMON_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "the Archdemon Mal'gareth",
    reward_key: LATEANIA_ARCHDEMON_REWARD_KEY,
    ledger_reason: LATEANIA_ARCHDEMON_LEDGER_REASON,
    award_category: LATEANIA_ARCHDEMON_AWARD_CATEGORY,
};

const FRONTIER_KING_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "the King Who Was Promised Nothing",
    reward_key: LATEANIA_FRONTIER_KING_REWARD_KEY,
    ledger_reason: LATEANIA_FRONTIER_KING_LEDGER_REASON,
    award_category: LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
};

/// Account age (in days) at which an adventurer is a "citizen" of Lateania and
/// earns extra resurrections.
const VETERAN_DAYS: i64 = 20;
/// In-place resurrections a veteran gets per adventure (refreshed at a capital
/// fountain). Newer accounts get none and respawn at the temple as before.
const VETERAN_RESURRECTIONS: u8 = 2;

#[derive(Clone)]
pub struct LateaniaService {
    activity: ActivityPublisher,
    chip_svc: ChipService,
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

/// One quest row in the journal.
#[derive(Clone, Debug)]
pub struct QuestView {
    pub name: String,
    pub done: bool,
    pub reward: String,
    pub frontier: bool,
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
    pub banked_gold: i64,
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
    /// The world clock phase, e.g. "dawn"/"day"/"dusk"/"night".
    pub time_of_day: &'static str,
    /// The current weather, e.g. "clear"/"rain"/"fog"/"storm".
    pub weather: &'static str,
    /// An active escort, if any: (name, hp, max_hp, destination zone).
    pub escort: Option<(String, i32, i32, String)>,
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
            banked_gold: 0,
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
            time_of_day: "day",
            weather: "clear",
            escort: None,
        }
    }
}

pub fn empty_player_view() -> PlayerView {
    PlayerView::empty()
}

impl LateaniaService {
    pub fn new(activity: ActivityPublisher, chip_svc: ChipService, db: Db) -> Self {
        let room_id = Uuid::from_u128(0x4c41_5445_414e_4941_0000_0000_0000_0001);
        let state = WorldState::new(room_id, seed_world());
        let initial = state.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let svc = Self {
            activity,
            chip_svc,
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
        self.mutate_with_frontier_warning_clear(user_id, true, f);
    }

    fn mutate_preserving_frontier_warning<F: FnOnce(&mut WorldState) + Send + 'static>(
        &self,
        user_id: Uuid,
        f: F,
    ) {
        self.mutate_with_frontier_warning_clear(user_id, false, f);
    }

    fn mutate_with_frontier_warning_clear<F: FnOnce(&mut WorldState) + Send + 'static>(
        &self,
        user_id: Uuid,
        clear_frontier_warning: bool,
        f: F,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            if clear_frontier_warning {
                state.clear_frontier_descent_pending(user_id);
            }
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
        self.mutate_preserving_frontier_warning(user_id, move |s| s.move_player(user_id, dir));
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
                    svc.publish_kill_outcome(outcome);
                }
            }
        });
    }

    fn publish_kill_outcome(&self, outcome: KillOutcome) {
        let Some(achievement) = outcome.achievement else {
            self.activity.game_won_task(
                outcome.user_id,
                ActivityGame::Mud,
                Some(format!("slew {}", outcome.mob_name)),
                None,
            );
            return;
        };

        let chip_svc = self.chip_svc.clone();
        let activity = self.activity.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let payout = chip_svc
                .credit_lifetime_reward_template(
                    outcome.user_id,
                    achievement.reward_key,
                    achievement.ledger_reason,
                )
                .await;
            match &payout {
                Ok(grant) if !grant.credited => {
                    tracing::info!(
                        user_id = %outcome.user_id,
                        payout = grant.amount,
                        boss = achievement.mob_name,
                        "suppressed Lateania boss chips because lifetime payout was already claimed"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %outcome.user_id,
                        boss = achievement.mob_name,
                        "failed to credit Lateania boss chips"
                    );
                }
            }

            let badge = award_badge(achievement.award_category, 1);
            if let Ok(grant) = &payout {
                match db.get().await {
                    Ok(client) => {
                        if let Err(error) = grant_unique_milestone_award(
                            &client,
                            outcome.user_id,
                            achievement.award_category,
                            grant.amount,
                        )
                        .await
                        {
                            tracing::error!(
                                ?error,
                                user_id = %outcome.user_id,
                                badge = %badge,
                                "failed to grant Lateania profile award badge"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            ?error,
                            user_id = %outcome.user_id,
                            badge = %badge,
                            "no db client for Lateania profile award badge"
                        );
                    }
                }
            }

            // Keep the feed line short: chips/badge are recorded on the profile,
            // not spelled out in the activity stream.
            let detail = Some(format!("defeated {}", achievement.mob_name));
            activity.game_won_task(outcome.user_id, ActivityGame::Mud, detail, None);
        });
    }

    fn publish(&self, state: &WorldState) {
        let _ = self.snapshot_tx.send(state.snapshot());
    }
}

struct KillOutcome {
    user_id: Uuid,
    mob_name: String,
    achievement: Option<BossAchievement>,
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
    banked_gold: i64,
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
    /// Accepted board bounties and their progress: (quest id, count so far).
    board_progress: Vec<(u32, u32)>,
    /// Board bounty ids the player has claimed (and cannot take again).
    board_done: Vec<u32>,
    /// Unix time at which each repeatable bounty was last claimed (id, seconds).
    quest_cooldowns: Vec<(u32, u64)>,
    /// The friendly NPC the player is currently escorting, if any (transient).
    escort: Option<EscortState>,
    /// Transient warning gate for the start-room Frontier entrance.
    frontier_descent_pending: bool,
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

/// A board-quest objective. `Reach` completes the moment the player enters any
/// room of the named zone; the others count up to a target.
#[derive(Clone, Copy, Debug)]
enum Objective {
    /// Slay foes whose name contains this fragment (e.g. "Wolf").
    Bounty {
        name_contains: &'static str,
        count: u32,
    },
    /// Recover this many of a specific dropped item id.
    Collect { item: u32, count: u32 },
    /// Set foot in the named zone.
    Reach { zone: &'static str },
    /// Lead a friendly NPC alive into the named zone. Tracked via the player's
    /// transient `escort` state rather than a `board_progress` counter.
    Escort {
        npc: &'static str,
        dest_zone: &'static str,
    },
}

impl Objective {
    fn target(self) -> u32 {
        match self {
            Objective::Bounty { count, .. } | Objective::Collect { count, .. } => count,
            Objective::Reach { .. } | Objective::Escort { .. } => 1,
        }
    }
    fn describe(self) -> String {
        match self {
            Objective::Bounty {
                name_contains,
                count,
            } => format!("slay {count} of {name_contains}-kind"),
            Objective::Collect { count, .. } => format!("recover {count} relics"),
            Objective::Reach { zone } => format!("reach {zone}"),
            Objective::Escort { npc, dest_zone } => format!("lead {npc} to {dest_zone}"),
        }
    }
}

/// How often a bounty can be taken. `Once` is permanent; `Daily`/`Weekly` come
/// back after the real elapsed time represented by a world day/week.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repeat {
    Once,
    Daily,
    Weekly,
}

/// A friendly NPC the player is currently leading (transient; not persisted).
#[derive(Clone, Debug)]
struct EscortState {
    quest_id: u32,
    name: &'static str,
    dest_zone: &'static str,
    hp: i32,
    max_hp: i32,
}

/// A posted bounty: offered at `board` (a capital square) and tracked per player.
struct BoardQuest {
    id: u32,
    board: RoomId,
    title: &'static str,
    objective: Objective,
    reward_gold: i64,
    reward_title: Option<&'static str>,
    repeat: Repeat,
    blurb: &'static str,
}

/// Ticks/seconds in a world day (four phases) and the escortee's starting health.
const DAY_TICKS: u64 = PHASE_TICKS * 4;
const DAY_SECS: u64 = DAY_TICKS * TICK_SECS;
const ESCORT_HP: i32 = 80;

/// The standing bounties: per capital, themed to its region. Bounties and
/// collections are `Daily` (repeatable hunts); the one-off discoveries and
/// escorts are `Once`.
const BOARD_QUESTS: &[BoardQuest] = &[
    BoardQuest {
        id: 1,
        board: super::world::TASMANIA_SQUARE,
        title: "Still the Restless Dead",
        objective: Objective::Bounty {
            name_contains: "Skeleton",
            count: 5,
        },
        reward_gold: 120,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Skeletons walk the crypt below Tasmania. Put five back to rest.",
    },
    BoardQuest {
        id: 2,
        board: super::world::TASMANIA_SQUARE,
        title: "Grave Relics",
        objective: Objective::Collect {
            item: CATACOMBS_RELIC_ID,
            count: 3,
        },
        reward_gold: 150,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "The chapel will pay for three relics recovered from the Catacombs.",
    },
    BoardQuest {
        id: 3,
        board: super::world::TASMANIA_SQUARE,
        title: "Into the Dark",
        objective: Objective::Reach {
            zone: "The Sunken Catacombs",
        },
        reward_gold: 60,
        reward_title: Some("Crypt-Delver"),
        repeat: Repeat::Once,
        blurb: "No one has mapped the new crypt. Descend, and live to tell of it.",
    },
    BoardQuest {
        id: 4,
        board: super::world::MELVANALA_SQUARE,
        title: "Thin the Pack",
        objective: Objective::Bounty {
            name_contains: "Wolf",
            count: 4,
        },
        reward_gold: 130,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Dire wolves harry the lake road. Cull four from the Thornwood.",
    },
    BoardQuest {
        id: 5,
        board: super::world::MELVANALA_SQUARE,
        title: "Forest Spoils",
        objective: Objective::Collect {
            item: THORNWOOD_RELIC_ID,
            count: 3,
        },
        reward_gold: 160,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Bring back three spoils taken from the Thornwood Hollows.",
    },
    BoardQuest {
        id: 6,
        board: super::world::MELVANALA_SQUARE,
        title: "Walk the Hollows",
        objective: Objective::Reach {
            zone: "The Thornwood Hollows",
        },
        reward_gold: 60,
        reward_title: Some("Wood-Warden"),
        repeat: Repeat::Once,
        blurb: "Step beneath the eaves and find your way to the heart-tree's grove.",
    },
    BoardQuest {
        id: 7,
        board: super::world::MATLATESH_SQUARE,
        title: "Clear the Lurkers",
        objective: Objective::Bounty {
            name_contains: "Lurker",
            count: 4,
        },
        reward_gold: 140,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Things lie in wait in the flooded caves. Clear four of them out.",
    },
    BoardQuest {
        id: 8,
        board: super::world::MATLATESH_SQUARE,
        title: "Cavern Salvage",
        objective: Objective::Collect {
            item: CAVERNS_RELIC_ID,
            count: 3,
        },
        reward_gold: 170,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Salvage three finds from the depths of the Drowned Caverns.",
    },
    BoardQuest {
        id: 9,
        board: super::world::MATLATESH_SQUARE,
        title: "Sound the Deep",
        objective: Objective::Reach {
            zone: "The Drowned Caverns",
        },
        reward_gold: 70,
        reward_title: Some("Deep-Walker"),
        repeat: Repeat::Once,
        blurb: "Find the tide-mouth beneath Matlatesh and enter the drowned dark.",
    },
    BoardQuest {
        id: 10,
        board: super::world::TASMANIA_SQUARE,
        title: "Last Rites",
        objective: Objective::Escort {
            npc: "Brother Aldric",
            dest_zone: "The Sunken Catacombs",
        },
        reward_gold: 220,
        reward_title: Some("Crypt Shepherd"),
        repeat: Repeat::Once,
        blurb: "An old priest must bless the crypt. Keep him alive and see him in.",
    },
    BoardQuest {
        id: 11,
        board: super::world::MELVANALA_SQUARE,
        title: "The Scholar's Folly",
        objective: Objective::Escort {
            npc: "Mira the Scholar",
            dest_zone: "The Thornwood Hollows",
        },
        reward_gold: 220,
        reward_title: Some("Wood-Shepherd"),
        repeat: Repeat::Once,
        blurb: "A scholar would study the heart-tree. Guard her through the Hollows.",
    },
    BoardQuest {
        id: 12,
        board: super::world::MATLATESH_SQUARE,
        title: "The Diver's Charge",
        objective: Objective::Escort {
            npc: "Old Pell the Diver",
            dest_zone: "The Drowned Caverns",
        },
        reward_gold: 240,
        reward_title: Some("Tide Shepherd"),
        repeat: Repeat::Once,
        blurb: "Old Pell knows the tides. Bring him safe to the drowned dark.",
    },
];

fn board_quest(id: u32) -> Option<&'static BoardQuest> {
    BOARD_QUESTS.iter().find(|q| q.id == id)
}

struct MobInstance {
    spawn: MobSpawn,
    hp: i32,
    alive: bool,
    respawn_at: Option<Instant>,
    /// What this mob does beyond standing and fighting (from `World::behaviors`).
    behavior: MobBehavior,
    /// Where the mob actually is right now. Roamers move; this drives which room
    /// shows the mob and which mob a player in a room can engage. Starts at home.
    current_room: RoomId,
    /// The mob's home; roamers tether to it and return here on respawn.
    leash_home: RoomId,
    /// Ticks until this mob may take another roaming step.
    move_cooldown: u8,
    /// Ambushers are hidden from the room view until a player enters (then they
    /// reveal and strike first). Always true for every other behavior.
    revealed: bool,
    /// Ticks until a Summoner may call another add.
    summon_cooldown: u8,
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
    /// Next id for a runtime-only summoned add (Summoner behavior). Kept well
    /// clear of authored spawn ids so the two never collide.
    next_summon_id: u32,
    /// The world heartbeat, in ticks. Drives time-of-day and weather.
    world_ticks: u64,
    /// The active wandering world boss, if one currently roams.
    world_boss: Option<u32>,
    /// Tick at which the next world boss may rise.
    next_world_boss_tick: u64,
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
                let behavior = world.behavior_of(spawn.id);
                (
                    spawn.id,
                    MobInstance {
                        hp: spawn.max_hp,
                        alive: true,
                        respawn_at: None,
                        behavior,
                        current_room: spawn.home,
                        leash_home: spawn.home,
                        move_cooldown: 0,
                        revealed: !matches!(behavior, MobBehavior::Ambusher),
                        summon_cooldown: 0,
                        spawn: spawn.clone(),
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
            next_summon_id: SUMMON_ID_START,
            world_ticks: 0,
            world_boss: None,
            next_world_boss_tick: WORLD_BOSS_FIRST_TICK,
        }
    }

    /// The current world clock phase.
    fn time_of_day(&self) -> TimeOfDay {
        TimeOfDay::from_ticks(self.world_ticks)
    }

    /// The current weather.
    fn weather(&self) -> Weather {
        Weather::from_ticks(self.world_ticks)
    }

    /// Push a system line to every player currently in the world (server-wide
    /// announcements like a world boss rising or falling).
    fn log_all(&mut self, text: String) {
        let ids: Vec<Uuid> = self.players.keys().copied().collect();
        for id in ids {
            self.log_to(id, LogKind::System, text.clone());
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
            banked_gold: 0,
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
            board_progress: Vec::new(),
            board_done: Vec::new(),
            quest_cooldowns: Vec::new(),
            escort: None,
            frontier_descent_pending: false,
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
        self.log_to(
            user_id,
            LogKind::System,
            "New adventurers usually leave by the South Gate. Stranger paths from the square lead into much older danger."
                .to_string(),
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
        let xp = saved.xp.max(0);
        let saved_level = saved.level.clamp(1, Class::MAX_LEVEL);
        let level = saved_level.max(level_for_xp(xp)).clamp(1, Class::MAX_LEVEL);
        let stats = class.stats_at(level);
        let room = if self.world.room(saved.room).is_some_and(|r| r.safe) {
            saved.room
        } else {
            self.world.start_room
        };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.class = Some(class);
            p.level = level;
            p.xp = xp;
            p.gold = saved.gold.max(0);
            p.banked_gold = saved.banked_gold.max(0);
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
            p.board_progress = saved.board_progress.clone();
            p.board_done = saved.board_done.clone();
            p.quest_cooldowns = saved.quest_cooldowns.clone();
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
            banked_gold: p.banked_gold,
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
            board_progress: p.board_progress.clone(),
            board_done: p.board_done.clone(),
            quest_cooldowns: p.quest_cooldowns.clone(),
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

    fn clear_frontier_descent_pending(&mut self, user_id: Uuid) {
        if let Some(player) = self.players.get_mut(&user_id) {
            player.frontier_descent_pending = false;
        }
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
            if let Some(player) = self.players.get_mut(&user_id) {
                player.frontier_descent_pending = false;
            }
            self.log_to(
                user_id,
                LogKind::Normal,
                format!("You can't go {}.", dir.label()),
            );
            return;
        };
        let from = self.players.get(&user_id).map(|p| p.room).unwrap_or(dest);
        if !self.can_cross_progression_gate(user_id, from, dest) {
            return;
        }
        if self.is_frontier_gateway(from, dest) {
            let confirmed = self
                .players
                .get(&user_id)
                .is_some_and(|p| p.frontier_descent_pending);
            if !confirmed {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.frontier_descent_pending = true;
                }
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!(
                        "The way {} opens into the Frontier: older, meaner country meant for seasoned adventurers. Press {} again if you truly want to go.",
                        dir.label(),
                        dir_input_hint(dir)
                    ),
                );
                return;
            }
        } else if let Some(player) = self.players.get_mut(&user_id) {
            player.frontier_descent_pending = false;
        }
        if let Some(player) = self.players.get_mut(&user_id) {
            player.frontier_descent_pending = false;
            player.previous_room = Some(from);
            player.room = dest;
            player.visited.insert(dest);
        }
        self.describe_room(user_id);
        self.apply_critter_perks(user_id);
        self.move_followers(user_id, from, dest, dir);
    }

    fn is_frontier_gateway(&self, from: RoomId, dest: RoomId) -> bool {
        from == self.world.start_room && dest == frontier_entrance_room()
    }

    fn can_cross_progression_gate(&mut self, user_id: Uuid, from: RoomId, dest: RoomId) -> bool {
        if from == FIRST_DUNGEON_GATE_FROM
            && dest == FIRST_DUNGEON_GATE_TO
            && !self.player_has_title(user_id, FIRST_DUNGEON_GATE_TITLE)
        {
            self.clear_frontier_descent_pending(user_id);
            self.log_to(
                user_id,
                LogKind::System,
                "The roots clutch the ladder fast. The Elder Treant still keeps the old forest's leave to descend.".to_string(),
            );
            return false;
        }

        if self.is_living_dark_gateway(from, dest)
            && !self.player_has_title(user_id, FRONTIER_GATE_TITLE)
        {
            self.clear_frontier_descent_pending(user_id);
            self.log_to(
                user_id,
                LogKind::System,
                "The way recoils from you. Defeat the Archdemon Mal'gareth before entering the living dark beyond the capitals.".to_string(),
            );
            return false;
        }

        if self.is_frontier_gateway(from, dest)
            && !self.player_has_required_titles(user_id, &FRONTIER_REQUIRED_TITLES)
        {
            let missing = self.frontier_missing_requirement_text(user_id);
            self.clear_frontier_descent_pending(user_id);
            self.log_to(
                user_id,
                LogKind::System,
                format!("The Frontier stair stays cold and shut. {missing}"),
            );
            return false;
        }

        true
    }

    fn is_living_dark_gateway(&self, from: RoomId, dest: RoomId) -> bool {
        matches!(
            (from, self.world.room(dest).map(|r| r.zone)),
            (super::world::TASMANIA_SQUARE, Some("The Sunken Catacombs"))
                | (
                    super::world::MELVANALA_SQUARE,
                    Some("The Thornwood Hollows")
                )
                | (super::world::MATLATESH_SQUARE, Some("The Drowned Caverns"))
        )
    }

    fn player_has_title(&self, user_id: Uuid, title: &str) -> bool {
        self.players
            .get(&user_id)
            .is_some_and(|p| p.titles.iter().any(|owned| owned == title))
    }

    fn player_has_required_titles(&self, user_id: Uuid, required: &[&str]) -> bool {
        self.players
            .get(&user_id)
            .is_some_and(|p| titles_include_all(&p.titles, required))
    }

    fn frontier_missing_requirement_text(&self, user_id: Uuid) -> String {
        let Some(player) = self.players.get(&user_id) else {
            return "Earn the Archdemon title and the three living-dark seals first.".to_string();
        };
        if !player
            .titles
            .iter()
            .any(|owned| owned == FRONTIER_GATE_TITLE)
        {
            return "Defeat the Archdemon Mal'gareth, then claim the three living-dark seals before seeking the King beyond it."
                .to_string();
        }
        let missing: Vec<&str> = [
            (CATACOMBS_GATE_TITLE, "Sunken Catacombs"),
            (THORNWOOD_GATE_TITLE, "Thornwood Hollows"),
            (CAVERNS_GATE_TITLE, "Drowned Caverns"),
        ]
        .into_iter()
        .filter_map(|(title, label)| {
            (!player.titles.iter().any(|owned| owned == title)).then_some(label)
        })
        .collect();
        if missing.is_empty() {
            "The old warning holds for one more breath.".to_string()
        } else {
            format!(
                "Claim the remaining living-dark seals: {}.",
                missing.join(", ")
            )
        }
    }

    fn exit_label(&self, from: RoomId, dir: Dir, dest: RoomId) -> String {
        if self.is_frontier_gateway(from, dest) {
            format!("{} (dangerous Frontier)", dir.label())
        } else {
            dir.label().to_string()
        }
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
                if !self.can_cross_progression_gate(f, from, dest) {
                    if let Some(p) = self.players.get_mut(&f) {
                        p.following = None;
                    }
                    continue;
                }
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

    /// Reveal any Ambushers lurking in the player's room: they spring out and
    /// land a free first strike. Once revealed they behave like any other foe.
    fn reveal_ambushers(&mut self, user_id: Uuid) {
        let room = match self.players.get(&user_id) {
            Some(p) if p.respawn_at.is_none() => p.room,
            _ => return,
        };
        let lurkers: Vec<(u32, i32, DamageType, String)> = self
            .mobs
            .values()
            .filter(|m| {
                m.alive
                    && !m.revealed
                    && matches!(m.behavior, MobBehavior::Ambusher)
                    && m.current_room == room
            })
            .map(|m| {
                (
                    m.spawn.id,
                    m.spawn.damage,
                    m.spawn.profile.attack_type,
                    m.spawn.name.to_string(),
                )
            })
            .collect();
        if lurkers.is_empty() {
            return;
        }
        // Fog hides them better: the ambush lands half again as hard.
        let fog = self.weather() == Weather::Fog;
        for (id, dmg, dt, name) in lurkers {
            if let Some(m) = self.mobs.get_mut(&id) {
                m.revealed = true;
            }
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{name} lunges from the shadows and strikes first!"),
            );
            let dmg = if fog { dmg * 3 / 2 } else { dmg };
            if !self.strike_player(user_id, dmg, dt, &name) {
                break;
            }
        }
        self.dirty = true;
        self.mark_world_dirty();
    }

    fn describe_room(&mut self, user_id: Uuid) {
        self.describe_room_context(user_id, true);
    }

    fn describe_room_context(&mut self, user_id: Uuid, announce_travel: bool) {
        self.reveal_ambushers(user_id);
        if !matches!(self.players.get(&user_id), Some(p) if p.respawn_at.is_none()) {
            return;
        }
        // Exploration bounties: arriving in a zone can complete a "reach" quest.
        let here_zone = self
            .players
            .get(&user_id)
            .and_then(|p| self.world.room(p.room))
            .map(|r| r.zone);
        if let Some(here_zone) = here_zone {
            self.bump_quests(user_id, |o| {
                u32::from(matches!(o, Objective::Reach { zone } if zone == here_zone))
            });
            self.check_escort_arrival(user_id, here_zone);
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        let room_id = player.room;
        let Some(room) = self.world.room(room_id) else {
            return;
        };
        let name = room.name.to_string();
        let desc = room.desc.to_string();
        let mut exits: Vec<String> = room
            .exits
            .iter()
            .map(|(dir, dest)| self.exit_label(room_id, *dir, *dest))
            .collect();
        exits.sort_unstable();
        let exit_text = if exits.is_empty() {
            "none".to_string()
        } else {
            exits.join(", ")
        };
        let mob_names: Vec<String> = self
            .mobs
            .values()
            .filter(|m| m.alive && m.revealed && m.current_room == room_id)
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
        } else if feat.kind == FeatureKind::Bank {
            let safe = self.world.room(room_id).is_some_and(|r| r.safe);
            if safe {
                self.use_bank(user_id);
            }
        } else if feat.kind == FeatureKind::Board {
            self.use_board(user_id, room_id);
        }
        self.dirty = true;
    }

    fn board_quest_available(&self, p: &PlayerState, q: &BoardQuest) -> bool {
        self.board_quest_available_at(p, q, now_unix_secs())
    }

    /// Whether `q` can be taken now: not already in progress, not the active
    /// escort, and either never-done (`Once`) or off cooldown (`Daily`/`Weekly`).
    fn board_quest_available_at(&self, p: &PlayerState, q: &BoardQuest, now_secs: u64) -> bool {
        if p.board_progress.iter().any(|(id, _)| *id == q.id) {
            return false;
        }
        if p.escort.as_ref().is_some_and(|e| e.quest_id == q.id) {
            return false;
        }
        match q.repeat {
            Repeat::Once => !p.board_done.contains(&q.id),
            Repeat::Daily | Repeat::Weekly => {
                let period = if q.repeat == Repeat::Weekly {
                    DAY_SECS * 7
                } else {
                    DAY_SECS
                };
                match p.quest_cooldowns.iter().find(|(id, _)| *id == q.id) {
                    None => true,
                    Some((_, at)) => now_secs.saturating_sub(*at) >= period,
                }
            }
        }
    }

    /// Examine a quest board: claim a finished bounty if one is ready here,
    /// otherwise take up the next available posting for this capital's region.
    fn use_board(&mut self, user_id: Uuid, board_room: RoomId) {
        let (progress, level) = match self.players.get(&user_id) {
            Some(p) => (p.board_progress.clone(), p.level),
            None => return,
        };
        // 1) A finished counter-bounty for this board takes priority - claim it.
        let claimable = progress.iter().find_map(|(id, prog)| {
            board_quest(*id).filter(|q| q.board == board_room && *prog >= q.objective.target())
        });
        if let Some(q) = claimable {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.board_progress.retain(|(qid, _)| *qid != q.id);
                p.gold += q.reward_gold;
                // Repeatable bounties go on cooldown; one-offs are done for good.
                if q.repeat == Repeat::Once {
                    p.board_done.push(q.id);
                } else {
                    p.quest_cooldowns.retain(|(id, _)| *id != q.id);
                    p.quest_cooldowns.push((q.id, now_unix_secs()));
                }
            }
            self.log_to(
                user_id,
                LogKind::Loot,
                format!("Bounty claimed: {} (+{} gold).", q.title, q.reward_gold),
            );
            if let Some(title) = q.reward_title {
                self.award_title(user_id, title.to_string(), level);
            }
            self.dirty = true;
            return;
        }
        // 2) Otherwise post the next available bounty for this board.
        let next = match self.players.get(&user_id) {
            Some(p) => BOARD_QUESTS
                .iter()
                .find(|q| q.board == board_room && self.board_quest_available(p, q)),
            None => None,
        };
        let Some(q) = next else {
            let pending = progress
                .iter()
                .any(|(id, _)| board_quest(*id).is_some_and(|qq| qq.board == board_room));
            let msg = if pending {
                "Every bounty here is already in your hands - go and finish them."
            } else {
                "The board has no new bounties for you. Come back when more are posted."
            };
            self.log_to(user_id, LogKind::Normal, msg.to_string());
            return;
        };
        if let Objective::Escort { npc, dest_zone } = q.objective {
            if self
                .players
                .get(&user_id)
                .is_some_and(|p| p.escort.is_some())
            {
                self.log_to(
                    user_id,
                    LogKind::Normal,
                    "You are already leading someone - see them safe first.".to_string(),
                );
                return;
            }
            if let Some(p) = self.players.get_mut(&user_id) {
                p.escort = Some(EscortState {
                    quest_id: q.id,
                    name: npc,
                    dest_zone,
                    hp: ESCORT_HP,
                    max_hp: ESCORT_HP,
                });
            }
            self.log_to(
                user_id,
                LogKind::System,
                format!("{npc} falls in beside you. Lead them, alive, into {dest_zone}."),
            );
        } else {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.board_progress.push((q.id, 0));
            }
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "Bounty accepted - {}: {} ({}).",
                    q.title,
                    q.blurb,
                    q.objective.describe()
                ),
            );
        }
        self.dirty = true;
    }

    /// Wound the player's escortee with some chance when the player is struck;
    /// if it falls, the escort is lost. Called from the combat round.
    fn wound_escort(&mut self, user_id: Uuid, raw: i32) {
        let roll = (self.generation as usize).wrapping_add(raw as usize) % 100;
        let mut fallen: Option<&'static str> = None;
        if let Some(p) = self.players.get_mut(&user_id)
            && let Some(esc) = p.escort.as_mut()
            && roll < 35
        {
            esc.hp -= (raw / 2).max(1);
            if esc.hp <= 0 {
                fallen = Some(esc.name);
            }
        }
        if let Some(name) = fallen {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.escort = None;
            }
            self.log_to(
                user_id,
                LogKind::System,
                format!("{name} falls! The escort is lost - take the charge again from the board."),
            );
            self.dirty = true;
        }
    }

    /// Complete an active escort if the player has reached its destination zone.
    fn check_escort_arrival(&mut self, user_id: Uuid, here_zone: &str) {
        let arrived = self
            .players
            .get(&user_id)
            .and_then(|p| p.escort.as_ref())
            .filter(|e| e.dest_zone == here_zone)
            .map(|e| e.quest_id);
        let Some(quest_id) = arrived else { return };
        let Some(q) = board_quest(quest_id) else {
            return;
        };
        let level = self.players.get(&user_id).map(|p| p.level).unwrap_or(1);
        let npc = match q.objective {
            Objective::Escort { npc, .. } => npc,
            _ => "your charge",
        };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.escort = None;
            p.board_done.push(quest_id);
            p.gold += q.reward_gold;
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "{npc} is safe. Escort complete: {} (+{} gold).",
                q.title, q.reward_gold
            ),
        );
        if let Some(title) = q.reward_title {
            self.award_title(user_id, title.to_string(), level);
        }
        self.dirty = true;
    }

    /// Advance any accepted bounty whose objective `inc` reports progress for.
    /// `inc` returns how much a given objective advanced this event (0 if none).
    fn bump_quests(&mut self, user_id: Uuid, inc: impl Fn(Objective) -> u32) {
        let mut newly_met: Vec<&'static str> = Vec::new();
        if let Some(p) = self.players.get_mut(&user_id) {
            for (id, prog) in p.board_progress.iter_mut() {
                let Some(q) = board_quest(*id) else { continue };
                let need = q.objective.target();
                if *prog >= need {
                    continue;
                }
                let step = inc(q.objective);
                if step > 0 {
                    *prog = (*prog + step).min(need);
                    if *prog >= need {
                        newly_met.push(q.title);
                    }
                }
            }
        }
        for title in newly_met {
            self.log_to(
                user_id,
                LogKind::Loot,
                format!("Objective met - {title}. Return to the board to claim your reward."),
            );
            self.dirty = true;
        }
    }

    fn use_bank(&mut self, user_id: Uuid) {
        let Some(p) = self.players.get_mut(&user_id) else {
            return;
        };
        let message = if p.gold > 0 {
            let amount = p.gold;
            p.gold = 0;
            p.banked_gold += amount;
            format!(
                "You deposit {amount} carried gold. The bank now holds {} gold for you.",
                p.banked_gold
            )
        } else if p.banked_gold > 0 {
            let amount = p.banked_gold;
            p.banked_gold = 0;
            p.gold += amount;
            format!("You withdraw {amount} gold. Keep it close, or spend it quickly.")
        } else {
            "The clerk taps the empty ledger. You have no gold to bank.".to_string()
        };
        self.log_to(user_id, LogKind::Loot, message);
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
            .find(|m| m.alive && m.revealed && m.current_room == room_id)
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
        let gold = gold_for_kill(xp, boss);
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
        // Bounty bounties: tick any accepted "slay N of X" board quest.
        self.bump_quests(user_id, |o| {
            u32::from(matches!(o, Objective::Bounty { name_contains, .. } if mob_name.contains(name_contains)))
        });
        if boss && let Some(zone) = super::world::frontier_zone_of_boss(&mob_name) {
            self.complete_quest(user_id, zone, mob_level);
        }
        let achievement = boss_achievement_for(&mob_name);
        if let Some(achievement) = achievement {
            self.log_to(
                user_id,
                LogKind::Loot,
                format!(
                    "First defeat of {} can award chips and badge {} once per account.",
                    achievement.mob_name,
                    award_badge(achievement.award_category, 1)
                ),
            );
        }
        self.check_level_up(user_id);
        self.pending_kills.push(KillOutcome {
            user_id,
            mob_name,
            achievement,
        });
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
        let bonus_xp = (80 + boss_level * 24) as i64;
        let bonus_gold = (35 + boss_level * 6) as i64;
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
        // Collection bounties: tick any "recover N of this item" board quest.
        self.bump_quests(user_id, |o| {
            u32::from(matches!(o, Objective::Collect { item, .. } if item == pick))
        });
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

        // Advance the world clock (drives time-of-day and weather).
        self.world_ticks = self.world_ticks.wrapping_add(1);

        // World boss lifecycle: note when the reigning boss has fallen (before
        // the reaper sweeps its corpse), then raise a new one when due.
        if let Some(id) = self.world_boss
            && !self.mobs.get(&id).is_some_and(|m| m.alive)
        {
            self.world_boss = None;
            self.next_world_boss_tick = self.world_ticks + WORLD_BOSS_INTERVAL;
        }

        // Reap runtime-only summoned adds (and the dead world boss) once gone.
        let before = self.mobs.len();
        self.mobs.retain(|id, m| *id < SUMMON_ID_START || m.alive);
        if self.mobs.len() != before {
            self.mark_world_dirty();
        }

        if self.world_boss.is_none() && self.world_ticks >= self.next_world_boss_tick {
            self.spawn_world_boss();
        }

        let mut world_changed = false;
        for mob in self.mobs.values_mut() {
            if !mob.alive
                && let Some(at) = mob.respawn_at
                && now >= at
            {
                mob.alive = true;
                mob.hp = mob.spawn.max_hp;
                mob.respawn_at = None;
                // A respawned roamer returns home and re-hides if it ambushes.
                mob.current_room = mob.leash_home;
                mob.move_cooldown = 0;
                mob.summon_cooldown = 0;
                mob.revealed = !matches!(mob.behavior, MobBehavior::Ambusher);
                self.dirty = true;
                world_changed = true;
            }
        }
        if world_changed {
            self.mark_world_dirty();
        }

        // Roaming: move wanderers/patrollers/hunters that no one is fighting.
        self.move_roamers();

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
            let lost_escort = self
                .players
                .get(&user_id)
                .and_then(|p| p.escort.as_ref())
                .map(|e| e.name);
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
                // A fallen escort can't be led from beyond the temple.
                player.escort = None;
            }
            self.log_to(
                user_id,
                LogKind::System,
                "You wake at the Temple of the Dawn, restored.".to_string(),
            );
            if let Some(name) = lost_escort {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("You lost {name} when you fell - the escort must be taken anew."),
                );
            }
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
                    // Brute: the closer to death, the harder it swings.
                    let enraged =
                        matches!(m.behavior, MobBehavior::Brute) && m.hp * 3 < m.spawn.max_hp;
                    let dmg = if enraged {
                        m.spawn.damage * 3 / 2
                    } else {
                        m.spawn.damage
                    };
                    (dmg, m.spawn.profile.attack_type, m.spawn.name.to_string())
                })
                .unwrap_or((0, DamageType::Physical, String::new()));
            if !self.strike_player(user_id, mob_damage, mob_dtype, &mob_name) {
                continue;
            }
            // Resolve the rest of the mob's behavior this round (cast/pack/
            // summon/steal/flee). No-op for plain Sentinels.
            self.resolve_mob_behavior(user_id, mob_id);
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

    /// Raise the lone wandering world boss after the Frontier seals are claimed.
    /// It hunts as a roaming boss across the living-dark and Frontier regions.
    fn spawn_world_boss(&mut self) {
        if !self
            .players
            .values()
            .any(|p| titles_include_all(&p.titles, &FRONTIER_REQUIRED_TITLES))
        {
            self.next_world_boss_tick = self.world_ticks + WORLD_BOSS_INTERVAL;
            return;
        }
        let rooms: Vec<RoomId> = self
            .world
            .rooms
            .values()
            .filter(|r| !r.safe && (is_frontier_room(r.id) || is_living_dark_zone(r.zone)))
            .map(|r| r.id)
            .collect();
        if rooms.is_empty() {
            self.next_world_boss_tick = self.world_ticks + WORLD_BOSS_INTERVAL;
            return;
        }
        let room = rooms[(self.world_ticks as usize) % rooms.len()];
        const NAMES: [&str; 4] = [
            "Gravelord Yorth",
            "the Hollow Sovereign",
            "Malrik the Unburied",
            "Vaultwarden Sceth",
        ];
        let name = NAMES[(self.world_ticks / WORLD_BOSS_INTERVAL.max(1)) as usize % NAMES.len()];
        let spawn = MobSpawn {
            id: WORLD_BOSS_ID,
            name,
            home: room,
            max_hp: 7200,
            damage: 145,
            xp: 1600,
            respawn_secs: 0,
            loot: super::items::frontier_loot(6),
            boss: true,
            profile: DamageProfile::new(
                DamageType::Shadow,
                Some(DamageType::Physical),
                Some(DamageType::Holy),
            ),
        };
        self.mobs.insert(
            WORLD_BOSS_ID,
            MobInstance {
                hp: spawn.max_hp,
                alive: true,
                respawn_at: None,
                behavior: MobBehavior::Hunter,
                current_room: room,
                leash_home: room,
                move_cooldown: 0,
                revealed: true,
                summon_cooldown: 0,
                spawn,
            },
        );
        self.world_boss = Some(WORLD_BOSS_ID);
        let zone = self
            .world
            .room(room)
            .map(|r| r.zone)
            .unwrap_or("the deep world");
        self.log_all(format!(
            "A chill grips Lateania: {name} rises in {zone} and begins to hunt."
        ));
        self.dirty = true;
        self.mark_world_dirty();
    }

    /// Step roaming mobs (Wanderer/Patroller/Hunter) that no player is fighting,
    /// keeping them inside their own zone and out of safe rooms. Hunters prefer a
    /// neighbour that holds a player so they close the distance. Ordinary Hunters
    /// only prowl after dark; the world boss roams its endgame regions at any hour.
    fn move_roamers(&mut self) {
        let dark = self.time_of_day().is_dark();
        let world_boss = self.world_boss;
        let engaged: Vec<u32> = self.players.values().filter_map(|p| p.target).collect();
        let player_rooms: Vec<RoomId> = self
            .players
            .values()
            .filter(|p| p.respawn_at.is_none())
            .map(|p| p.room)
            .collect();

        let mut plan: Vec<(u32, RoomId)> = Vec::new();
        let mut ticking: Vec<u32> = Vec::new();
        for (id, m) in self.mobs.iter() {
            let is_boss = Some(*id) == world_boss;
            if !m.alive
                || !m.revealed
                || engaged.contains(id)
                || !matches!(
                    m.behavior,
                    MobBehavior::Wanderer | MobBehavior::Patroller | MobBehavior::Hunter
                )
            {
                continue;
            }
            // Ordinary Hunters keep to their lair by day and only prowl in the dark.
            if matches!(m.behavior, MobBehavior::Hunter) && !is_boss && !dark {
                ticking.push(*id);
                continue;
            }
            if m.move_cooldown > 0 {
                ticking.push(*id);
                continue;
            }
            let Some(room) = self.world.room(m.current_room) else {
                continue;
            };
            let zone = room.zone;
            let dests: Vec<RoomId> = room
                .exits
                .values()
                .copied()
                .filter(|to| {
                    self.world
                        .room(*to)
                        // The world boss may leave its spawn zone; others keep to their zone.
                        .is_some_and(|d| !d.safe && (is_boss || d.zone == zone))
                })
                .collect();
            if dests.is_empty() {
                ticking.push(*id);
                continue;
            }
            let pick = (m.spawn.id as usize).wrapping_add(self.generation as usize) % dests.len();
            let dest = if matches!(m.behavior, MobBehavior::Hunter) {
                dests
                    .iter()
                    .copied()
                    .find(|d| player_rooms.contains(d))
                    .unwrap_or(dests[pick])
            } else {
                dests[pick]
            };
            plan.push((*id, dest));
        }

        for id in ticking {
            if let Some(m) = self.mobs.get_mut(&id) {
                m.move_cooldown = m.move_cooldown.saturating_sub(1);
            }
        }
        let mut moved = false;
        for (id, dest) in plan {
            if let Some(m) = self.mobs.get_mut(&id) {
                m.current_room = dest;
                m.move_cooldown = MOB_MOVE_COOLDOWN;
                moved = true;
            }
        }
        if moved {
            self.dirty = true;
            self.mark_world_dirty();
        }
    }

    /// Behaviors that fire during a mob's combat turn: casters bolt, pack hunters
    /// gang up, summoners call adds, thieves rob and run, skirmishers flee when
    /// hurt. Called right after the mob's normal strike; a no-op for Sentinels,
    /// Brutes (handled in the strike), and roamers.
    fn resolve_mob_behavior(&mut self, user_id: Uuid, mob_id: u32) {
        let (behavior, room, name, bite, hp, max_hp, summon_ready) = {
            let Some(m) = self.mobs.get(&mob_id) else {
                return;
            };
            if !m.alive {
                return;
            }
            (
                m.behavior,
                m.current_room,
                m.spawn.name.to_string(),
                m.spawn.damage,
                m.hp,
                m.spawn.max_hp,
                m.summon_cooldown == 0,
            )
        };
        // A cheap per-round roll without threading RNG state through combat.
        let roll = (self.generation as usize).wrapping_add(mob_id as usize) % 100;

        match behavior {
            MobBehavior::Caster(school) if roll < 40 => {
                // A storm charges the air, so spell-bolts land half again as hard.
                let mut bolt = bite + bite / 2;
                if self.weather() == Weather::Storm {
                    bolt = bolt * 3 / 2;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{name} channels a bolt of {}!", school.label()),
                );
                let _ = self.strike_player(user_id, bolt, school, &name);
            }
            MobBehavior::PackHunter => {
                let allies: Vec<(i32, DamageType, String)> = self
                    .mobs
                    .values()
                    .filter(|o| {
                        o.alive && o.revealed && o.current_room == room && o.spawn.id != mob_id
                    })
                    .map(|o| {
                        (
                            o.spawn.damage,
                            o.spawn.profile.attack_type,
                            o.spawn.name.to_string(),
                        )
                    })
                    .collect();
                if !allies.is_empty() {
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{name} howls - the pack closes in!"),
                    );
                    for (dmg, dt, an) in allies {
                        if !self.strike_player(user_id, dmg, dt, &an) {
                            break;
                        }
                    }
                }
            }
            MobBehavior::Summoner => {
                if summon_ready {
                    self.summon_add(mob_id, room);
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{name} calls a servant from the dark!"),
                    );
                    if let Some(mm) = self.mobs.get_mut(&mob_id) {
                        mm.summon_cooldown = 6;
                    }
                } else if let Some(mm) = self.mobs.get_mut(&mob_id) {
                    mm.summon_cooldown = mm.summon_cooldown.saturating_sub(1);
                }
            }
            MobBehavior::Thief if roll < 35 => {
                let stolen = self
                    .players
                    .get(&user_id)
                    .map(|p| (p.gold / 10).clamp(5, 50).min(p.gold))
                    .unwrap_or(0);
                if stolen > 0 {
                    if let Some(p) = self.players.get_mut(&user_id) {
                        p.gold -= stolen;
                    }
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{name} snatches {stolen} gold and bolts!"),
                    );
                    self.flee_mob(user_id, mob_id);
                }
            }
            MobBehavior::Skirmisher if hp * 3 < max_hp => {
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{name} breaks away and flees into the dark!"),
                );
                self.flee_mob(user_id, mob_id);
            }
            _ => {}
        }
    }

    /// Spawn a short-lived add for a Summoner. The add is a runtime-only mob
    /// (id >= `SUMMON_ID_START`) that simply dies for good when killed.
    fn summon_add(&mut self, parent_id: u32, room: RoomId) {
        let (max_hp, damage, profile) = {
            let Some(parent) = self.mobs.get(&parent_id) else {
                return;
            };
            (
                parent.spawn.max_hp / 3 + 20,
                (parent.spawn.damage / 2).max(3),
                parent.spawn.profile,
            )
        };
        let id = self.next_summon_id;
        self.next_summon_id = self.next_summon_id.wrapping_add(1);
        let spawn = MobSpawn {
            id,
            name: "a Risen Servant",
            home: room,
            max_hp,
            damage,
            xp: 5,
            respawn_secs: 0,
            loot: &[],
            boss: false,
            profile,
        };
        self.mobs.insert(
            id,
            MobInstance {
                hp: spawn.max_hp,
                alive: true,
                respawn_at: None,
                behavior: MobBehavior::Sentinel,
                current_room: room,
                leash_home: room,
                move_cooldown: 0,
                revealed: true,
                summon_cooldown: 0,
                spawn,
            },
        );
        self.dirty = true;
        self.mark_world_dirty();
    }

    /// Move a mob to a random same-zone, non-safe neighbour and drop the player's
    /// lock on it (Skirmisher/Thief flight). No-op if there is nowhere to run.
    fn flee_mob(&mut self, user_id: Uuid, mob_id: u32) {
        let dest = {
            let Some(m) = self.mobs.get(&mob_id) else {
                return;
            };
            let Some(room) = self.world.room(m.current_room) else {
                return;
            };
            let zone = room.zone;
            let dests: Vec<RoomId> = room
                .exits
                .values()
                .copied()
                .filter(|to| {
                    self.world
                        .room(*to)
                        .is_some_and(|d| d.zone == zone && !d.safe)
                })
                .collect();
            if dests.is_empty() {
                None
            } else {
                Some(dests[(self.generation as usize).wrapping_add(mob_id as usize) % dests.len()])
            }
        };
        let Some(to) = dest else { return };
        if let Some(m) = self.mobs.get_mut(&mob_id) {
            m.current_room = to;
            m.move_cooldown = MOB_MOVE_COOLDOWN;
        }
        if let Some(p) = self.players.get_mut(&user_id)
            && p.target == Some(mob_id)
        {
            p.target = None;
        }
        self.dirty = true;
        self.mark_world_dirty();
    }

    /// Strike a player and return whether this mob's current attack sequence
    /// should continue. A lethal blow, Warrior death-save, or veteran
    /// resurrection ends the sequence so extra behavior cannot immediately hit
    /// the same life again.
    fn strike_player(
        &mut self,
        user_id: Uuid,
        raw: i32,
        dtype: DamageType,
        mob_name: &str,
    ) -> bool {
        let now = Instant::now();
        let escort_raw = raw;
        // The dark emboldens the dead: every mob blow hits harder after dusk.
        let raw = raw * self.time_of_day().mob_damage_pct() / 100;
        let Some(p) = self.players.get_mut(&user_id) else {
            return false;
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
                self.wound_escort(user_id, escort_raw);
                return false;
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
                self.wound_escort(user_id, escort_raw);
                return false;
            }
            p.hp = 0;
            p.target = None;
            p.respawn_at = Some(now + Duration::from_secs(PLAYER_RESPAWN_SECS));
            let lost_escort = p.escort.take().map(|e| e.name);
            let lost_gold = carried_gold_death_loss(p.gold);
            if lost_gold > 0 {
                p.gold -= lost_gold;
            }
            let death_message = if lost_gold > 0 {
                format!("You have fallen! Darkness takes you... You lose {lost_gold} carried gold.")
            } else {
                "You have fallen! Darkness takes you...".to_string()
            };
            self.log_to(user_id, LogKind::System, death_message);
            if let Some(name) = lost_escort {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("You lost {name} when you fell - the escort must be taken anew."),
                );
            }
            false
        } else {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{mob_name} {verb} you for {dmg}."),
            );
            self.wound_escort(user_id, escort_raw);
            true
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
        let time_of_day = self.time_of_day().label();
        let weather = self.weather().label();
        for (user_id, player) in &self.players {
            let room = self.world.room(player.room);
            let (room_name, room_desc, zone, safe, exits) = match room {
                Some(room) => {
                    let mut exits: Vec<(Dir, String)> = room
                        .exits
                        .iter()
                        .map(|(dir, dest)| (*dir, self.exit_label(player.room, *dir, *dest)))
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
                .filter(|m| m.alive && m.revealed && m.current_room == player.room)
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
            let mut quests: Vec<QuestView> = (0..super::world::frontier_zone_count())
                .filter_map(|z| {
                    super::world::frontier_zone_info(z).map(|(zname, boss)| QuestView {
                        name: format!("{zname} - slay {boss}"),
                        done: player.completed_quests.contains(&z),
                        reward: format!("title: Champion of the {zname}"),
                        frontier: true,
                    })
                })
                .collect();
            // Accepted board bounties, with live progress and a claim hint.
            for (id, prog) in &player.board_progress {
                if let Some(q) = board_quest(*id) {
                    let need = q.objective.target();
                    let ready = *prog >= need;
                    quests.push(QuestView {
                        name: if ready {
                            format!("{} - READY to claim", q.title)
                        } else {
                            format!("{} ({}/{})", q.title, prog, need)
                        },
                        done: ready,
                        reward: format!(
                            "{} gold{}",
                            q.reward_gold,
                            match q.reward_title {
                                Some(t) => format!(" + title: {t}"),
                                None => String::new(),
                            }
                        ),
                        frontier: false,
                    });
                }
            }

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
                    banked_gold: player.banked_gold,
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
                    time_of_day,
                    weather,
                    escort: player
                        .escort
                        .as_ref()
                        .map(|e| (e.name.to_string(), e.hp, e.max_hp, e.dest_zone.to_string())),
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

fn dir_input_hint(dir: Dir) -> &'static str {
    match dir {
        Dir::North => "w",
        Dir::South => "s",
        Dir::East => "d",
        Dir::West => "a",
        Dir::Up => "<",
        Dir::Down => ">",
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

fn titles_include_all(titles: &[String], required: &[&str]) -> bool {
    required
        .iter()
        .all(|needed| titles.iter().any(|owned| owned == *needed))
}

fn is_living_dark_zone(zone: &str) -> bool {
    matches!(
        zone,
        "The Sunken Catacombs" | "The Thornwood Hollows" | "The Drowned Caverns"
    )
}

fn boss_achievement_for(mob_name: &str) -> Option<BossAchievement> {
    match mob_name {
        "the Archdemon Mal'gareth" => Some(ARCHDEMON_ACHIEVEMENT),
        "the King Who Was Promised Nothing" => Some(FRONTIER_KING_ACHIEVEMENT),
        _ => None,
    }
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

fn gold_for_kill(xp: i32, boss: bool) -> i32 {
    let base = if boss { 10 } else { 3 };
    base + xp.max(0) / 5
}

fn carried_gold_death_loss(gold: i64) -> i64 {
    if gold <= 0 {
        return 0;
    }
    let loss = gold
        .saturating_mul(DEATH_GOLD_LOSS_PERCENT)
        .saturating_add(99)
        / 100;
    loss.min(gold)
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

    fn grant_frontier_unlock_titles(s: &mut WorldState, user_id: Uuid) {
        let p = s.players.get_mut(&user_id).expect("player exists");
        for title in FRONTIER_REQUIRED_TITLES {
            if !p.titles.iter().any(|owned| owned == title) {
                p.titles.push(title.to_string());
            }
        }
    }

    fn dir_to_zone(s: &WorldState, from: RoomId, zone: &str) -> Dir {
        s.world
            .room(from)
            .expect("room exists")
            .exits
            .iter()
            .find_map(|(dir, dest)| {
                s.world
                    .room(*dest)
                    .is_some_and(|room| room.zone == zone)
                    .then_some(*dir)
            })
            .expect("exit to zone exists")
    }

    /// Put a classed player and a single controlled mob (with `behavior`) into a
    /// non-safe Frontier room that has same-zone neighbours to flee to, engage
    /// it, and return (state, mob_id). The mob is given a big HP pool so the
    /// player's opening strike can't kill it before its behavior resolves.
    fn engaged_with(behavior: MobBehavior) -> (WorldState, u32) {
        const ROOM: RoomId = 2001; // Frontier zone 0, interior (non-safe, has exits)
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let mob_id = *s.mobs.keys().next().expect("world has mobs");
        {
            let m = s.mobs.get_mut(&mob_id).unwrap();
            m.behavior = behavior;
            m.alive = true;
            m.revealed = true;
            m.current_room = ROOM;
            m.leash_home = ROOM;
            m.hp = 200;
            m.spawn.max_hp = 1000;
            m.spawn.damage = 1; // can't kill the player while we observe
        }
        s.players.get_mut(&uid(1)).unwrap().room = ROOM;
        s.engage(uid(1));
        assert_eq!(s.players[&uid(1)].target, Some(mob_id), "engaged the mob");
        (s, mob_id)
    }

    #[test]
    fn skirmisher_flees_when_wounded_and_breaks_the_lock() {
        let (mut s, mob_id) = engaged_with(MobBehavior::Skirmisher);
        let start = s.mobs[&mob_id].current_room;
        // Wound it below a third so the flee condition trips.
        s.mobs.get_mut(&mob_id).unwrap().hp = 100; // < 1000/3
        s.tick();
        assert_ne!(
            s.mobs[&mob_id].current_room, start,
            "a wounded skirmisher should flee to another room"
        );
        assert_eq!(
            s.players[&uid(1)].target,
            None,
            "fleeing breaks the player's target lock"
        );
    }

    #[test]
    fn summoner_calls_an_add_into_the_fight() {
        let (mut s, _mob_id) = engaged_with(MobBehavior::Summoner);
        let before = s.mobs.len();
        s.tick();
        assert!(
            s.mobs.keys().any(|id| *id >= SUMMON_ID_START),
            "summoner should have spawned a runtime add"
        );
        assert!(s.mobs.len() > before, "the add joins the mob roster");
    }

    #[test]
    fn world_clock_cycles_through_day_phases_and_weather() {
        assert_eq!(TimeOfDay::from_ticks(0), TimeOfDay::Dawn);
        assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS), TimeOfDay::Day);
        assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 2), TimeOfDay::Dusk);
        assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 3), TimeOfDay::Night);
        assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 4), TimeOfDay::Dawn);
        // The dark hits harder than the day.
        assert_eq!(TimeOfDay::Day.mob_damage_pct(), 100);
        assert!(TimeOfDay::Night.mob_damage_pct() > 100);
        // Weather rolls over as the clock advances.
        assert_ne!(
            Weather::from_ticks(0),
            Weather::from_ticks(WEATHER_TICKS * 2)
        );
    }

    #[test]
    fn world_boss_waits_for_frontier_unlock_titles() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Ranger);
        s.world_ticks = WORLD_BOSS_FIRST_TICK - 1;
        s.next_world_boss_tick = WORLD_BOSS_FIRST_TICK;
        s.tick();
        assert_eq!(
            s.world_boss, None,
            "world boss should not wake before the living-dark seals"
        );
        assert!(
            s.next_world_boss_tick > WORLD_BOSS_FIRST_TICK,
            "failed wake should reschedule instead of retrying every tick"
        );
    }

    #[test]
    fn world_boss_rises_on_schedule_and_is_announced() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Ranger);
        grant_frontier_unlock_titles(&mut s, uid(1));
        s.world_ticks = WORLD_BOSS_FIRST_TICK - 1;
        s.next_world_boss_tick = WORLD_BOSS_FIRST_TICK;
        s.tick();
        assert_eq!(
            s.world_boss,
            Some(WORLD_BOSS_ID),
            "a world boss should rise"
        );
        let boss = s
            .mobs
            .get(&WORLD_BOSS_ID)
            .expect("world boss joins the roster");
        assert!(boss.spawn.boss, "it is a boss");
        assert!(matches!(boss.behavior, MobBehavior::Hunter), "it hunts");
        assert!(
            boss.spawn.loot.iter().any(|id| (3000..3200).contains(id)),
            "post-unlock world boss should drop Frontier catalog loot"
        );
        assert!(
            is_frontier_room(boss.current_room)
                || s.world
                    .room(boss.current_room)
                    .is_some_and(|room| is_living_dark_zone(room.zone)),
            "world boss should spawn in endgame regions"
        );
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|l| l.text.contains("rises")),
            "the rising is announced server-wide"
        );
    }

    #[test]
    fn board_bounty_accepts_then_pays_out_on_claim() {
        use super::super::world::{TASMANIA_SQUARE, features_at};
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().room = TASMANIA_SQUARE;
        let board = features_at(TASMANIA_SQUARE)
            .iter()
            .position(|f| f.kind == FeatureKind::Board)
            .expect("a board stands in the Tasmania square");

        // First examine accepts the next bounty (id 1).
        s.interact(uid(1), board);
        assert!(
            s.players[&uid(1)]
                .board_progress
                .iter()
                .any(|(id, _)| *id == 1),
            "examining the board accepts the next bounty"
        );

        // Force it complete, then claim on the next examine.
        for e in s
            .players
            .get_mut(&uid(1))
            .unwrap()
            .board_progress
            .iter_mut()
        {
            if e.0 == 1 {
                e.1 = 99;
            }
        }
        let gold_before = s.players[&uid(1)].gold;
        s.interact(uid(1), board);
        // Quest 1 is a Daily, so a claim records a cooldown rather than a
        // permanent done-flag.
        assert!(
            s.players[&uid(1)]
                .quest_cooldowns
                .iter()
                .any(|(id, _)| *id == 1),
            "claiming the daily records its cooldown"
        );
        assert_eq!(
            s.players[&uid(1)].gold,
            gold_before + 120,
            "the reward is paid on claim"
        );
        assert!(
            !s.players[&uid(1)]
                .board_progress
                .iter()
                .any(|(id, _)| *id == 1),
            "a claimed bounty leaves the active list"
        );
    }

    #[test]
    fn reach_bounty_completes_on_entering_the_zone() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Ranger);
        // Hold the "Into the Dark" reach bounty (id 3 -> The Sunken Catacombs).
        s.players
            .get_mut(&uid(1))
            .unwrap()
            .board_progress
            .push((3, 0));
        s.players.get_mut(&uid(1)).unwrap().room = 5001; // a Catacombs room
        s.describe_room(uid(1));
        let prog = s.players[&uid(1)]
            .board_progress
            .iter()
            .find(|(id, _)| *id == 3)
            .map(|(_, p)| *p)
            .expect("reach bounty still tracked");
        assert!(
            prog >= 1,
            "entering the catacombs completes the reach bounty"
        );
    }

    #[test]
    fn escort_completes_on_reaching_its_destination_zone() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().escort = Some(EscortState {
            quest_id: 10,
            name: "Brother Aldric",
            dest_zone: "The Sunken Catacombs",
            hp: 80,
            max_hp: 80,
        });
        let gold_before = s.players[&uid(1)].gold;
        s.players.get_mut(&uid(1)).unwrap().room = 5001; // a Catacombs room
        s.describe_room(uid(1));
        assert!(
            s.players[&uid(1)].escort.is_none(),
            "the escort completes on arrival"
        );
        assert!(
            s.players[&uid(1)].board_done.contains(&10),
            "quest 10 is done"
        );
        assert_eq!(
            s.players[&uid(1)].gold,
            gold_before + 220,
            "the escort reward is paid"
        );
    }

    #[test]
    fn escort_is_lost_when_the_escortee_is_slain() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().escort = Some(EscortState {
            quest_id: 10,
            name: "Brother Aldric",
            dest_zone: "The Sunken Catacombs",
            hp: 3,
            max_hp: 80,
        });
        // generation is 0, so roll = raw % 100; raw=10 -> 10 < 35 -> a hit lands.
        s.wound_escort(uid(1), 10);
        assert!(
            s.players[&uid(1)].escort.is_none(),
            "a slain escortee ends the escort"
        );
    }

    #[test]
    fn daily_bounty_goes_on_cooldown_then_returns_after_a_day() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().room = super::super::world::TASMANIA_SQUARE;
        let board = super::super::world::features_at(super::super::world::TASMANIA_SQUARE)
            .iter()
            .position(|f| f.kind == FeatureKind::Board)
            .expect("board in the square");
        // Take and finish the daily bounty (id 1), then claim it.
        s.players
            .get_mut(&uid(1))
            .unwrap()
            .board_progress
            .push((1, 99));
        s.interact(uid(1), board);
        assert!(
            s.players[&uid(1)]
                .quest_cooldowns
                .iter()
                .any(|(id, _)| *id == 1),
            "claiming a daily records its cooldown"
        );
        assert!(
            !s.players[&uid(1)].board_done.contains(&1),
            "a daily is never permanently done"
        );
        let q1 = board_quest(1).unwrap();
        let claimed_at = s.players[&uid(1)]
            .quest_cooldowns
            .iter()
            .find_map(|(id, at)| (*id == 1).then_some(*at))
            .expect("daily claim timestamp");
        assert!(
            !s.board_quest_available_at(&s.players[&uid(1)], q1, claimed_at),
            "a freshly-claimed daily is unavailable"
        );
        assert!(
            s.board_quest_available_at(&s.players[&uid(1)], q1, claimed_at + DAY_SECS),
            "the daily returns once a day has passed"
        );
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
    fn first_dungeon_descent_requires_elder_treant_title() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().room = FIRST_DUNGEON_GATE_FROM;

        s.move_player(uid(1), Dir::Down);
        assert_eq!(s.players[&uid(1)].room, FIRST_DUNGEON_GATE_FROM);
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("Elder Treant")),
            "gate should point the player at the first boss"
        );

        s.players
            .get_mut(&uid(1))
            .unwrap()
            .titles
            .push(FIRST_DUNGEON_GATE_TITLE.to_string());
        s.move_player(uid(1), Dir::Down);
        assert_eq!(s.players[&uid(1)].room, FIRST_DUNGEON_GATE_TO);
    }

    #[test]
    fn living_dark_regions_require_archdemon_title() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().room = super::super::world::TASMANIA_SQUARE;
        let dir = dir_to_zone(
            &s,
            super::super::world::TASMANIA_SQUARE,
            "The Sunken Catacombs",
        );

        s.move_player(uid(1), dir);
        assert_eq!(
            s.players[&uid(1)].room,
            super::super::world::TASMANIA_SQUARE
        );
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("Archdemon Mal'gareth")),
            "gate should point players at the Archdemon first"
        );

        s.players
            .get_mut(&uid(1))
            .unwrap()
            .titles
            .push(FRONTIER_GATE_TITLE.to_string());
        s.move_player(uid(1), dir);
        assert_eq!(
            s.world.room(s.players[&uid(1)].room).map(|room| room.zone),
            Some("The Sunken Catacombs")
        );
    }

    #[test]
    fn frontier_entrance_requires_archdemon_title_then_confirming_move() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let home = s.world.start_room;

        s.move_player(uid(1), Dir::Down);
        assert_eq!(
            s.players[&uid(1)].room,
            home,
            "Frontier should be locked before the Archdemon falls"
        );
        assert!(!s.players[&uid(1)].frontier_descent_pending);
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("Archdemon Mal'gareth")),
            "gate should point the player at the authored final boss"
        );
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("three living-dark seals")),
            "gate should mention the full Frontier unlock chain"
        );

        s.players
            .get_mut(&uid(1))
            .unwrap()
            .titles
            .push(FRONTIER_GATE_TITLE.to_string());
        s.move_player(uid(1), Dir::Down);
        assert_eq!(
            s.players[&uid(1)].room,
            home,
            "Frontier should still be locked before the living-dark bosses fall"
        );
        assert!(!s.players[&uid(1)].frontier_descent_pending);
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("living-dark seals")),
            "gate should point the player at the three side regions"
        );

        grant_frontier_unlock_titles(&mut s, uid(1));
        s.move_player(uid(1), Dir::Down);
        assert_eq!(
            s.players[&uid(1)].room,
            home,
            "first descent should warn without moving"
        );
        assert!(s.players[&uid(1)].frontier_descent_pending);
        assert!(
            s.players[&uid(1)]
                .log
                .iter()
                .any(|line| line.text.contains("older, meaner country")),
            "warning should explain the Frontier danger"
        );

        s.move_player(uid(1), Dir::Down);
        assert_eq!(s.players[&uid(1)].room, frontier_entrance_room());
        assert!(!s.players[&uid(1)].frontier_descent_pending);
    }

    #[test]
    fn frontier_warning_clears_when_moving_elsewhere() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        grant_frontier_unlock_titles(&mut s, uid(1));

        s.move_player(uid(1), Dir::Down);
        assert!(s.players[&uid(1)].frontier_descent_pending);
        s.move_player(uid(1), Dir::South);
        assert_eq!(s.players[&uid(1)].room, 5);
        assert!(!s.players[&uid(1)].frontier_descent_pending);
    }

    #[test]
    fn town_square_exit_labels_mark_frontier_as_dangerous() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);

        let snap = s.snapshot();
        let view = snap.players.get(&uid(1)).expect("player view");
        assert!(
            view.exits.iter().any(|(dir, label)| {
                *dir == Dir::Down && label.as_str() == "down (dangerous Frontier)"
            }),
            "Town Square should visibly mark the Frontier exit"
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
    fn bank_toggles_between_deposit_and_withdraw_all_gold() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);

        s.interact(uid(1), 1); // feature 1 in the square is the banker's grille
        let p = &s.players[&uid(1)];
        assert_eq!(p.gold, 0);
        assert_eq!(p.banked_gold, STARTING_GOLD);

        s.interact(uid(1), 1);
        let p = &s.players[&uid(1)];
        assert_eq!(p.gold, STARTING_GOLD);
        assert_eq!(p.banked_gold, 0);
    }

    #[test]
    fn normal_death_loses_carried_gold_but_not_banked_gold() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Mage);
        if let Some(p) = s.players.get_mut(&uid(1)) {
            p.gold = 1000;
            p.banked_gold = 500;
        }

        s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");

        let p = &s.players[&uid(1)];
        assert_eq!(p.gold, 800);
        assert_eq!(p.banked_gold, 500);
        assert!(p.respawn_at.is_some());
        assert!(
            p.log
                .iter()
                .any(|line| line.text.contains("lose 200 carried gold")),
            "death log should explain the gold loss"
        );
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
    fn final_bosses_map_to_lifetime_achievements() {
        let archdemon = boss_achievement_for("the Archdemon Mal'gareth")
            .expect("authored final boss should grant an achievement");
        assert_eq!(archdemon.reward_key, LATEANIA_ARCHDEMON_REWARD_KEY);
        assert_eq!(archdemon.ledger_reason, LATEANIA_ARCHDEMON_LEDGER_REASON);
        assert_eq!(archdemon.award_category, LATEANIA_ARCHDEMON_AWARD_CATEGORY);

        let frontier_king = boss_achievement_for("the King Who Was Promised Nothing")
            .expect("last Frontier boss should grant an achievement");
        assert_eq!(frontier_king.reward_key, LATEANIA_FRONTIER_KING_REWARD_KEY);
        assert_eq!(
            frontier_king.ledger_reason,
            LATEANIA_FRONTIER_KING_LEDGER_REASON
        );
        assert_eq!(
            frontier_king.award_category,
            LATEANIA_FRONTIER_KING_AWARD_CATEGORY
        );

        assert!(boss_achievement_for("the Elder Treant").is_none());
    }

    #[test]
    fn loading_saved_character_reconciles_level_from_xp() {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Mage);
        let mut saved = s.export_saved(uid(1)).expect("character saves");
        saved.level = 1;
        saved.xp = xp_for_level(5);

        s.hydrate(uid(1), &saved);
        let p = &s.players[&uid(1)];
        assert_eq!(p.level, 5, "saved xp should drive restored level");
        assert_eq!(p.base_attack, Class::Mage.stats_at(5).attack);

        let snap = s.snapshot();
        let view = snap.players.get(&uid(1)).expect("player view");
        assert_eq!(view.level, 5);
        assert!(
            view.abilities.iter().any(|a| a.name == "Frost Nova"),
            "restored level should update unlocked skills"
        );
    }

    #[test]
    fn gold_math_keeps_rewards_and_death_loss_predictable() {
        assert_eq!(gold_for_kill(80, false), 19);
        assert_eq!(gold_for_kill(352, true), 80);
        assert_eq!(carried_gold_death_loss(0), 0);
        assert_eq!(carried_gold_death_loss(1), 1);
        assert_eq!(carried_gold_death_loss(1000), 200);
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
        let fountain = super::super::world::features_at(620)
            .iter()
            .position(|f| f.kind == FeatureKind::Fountain)
            .expect("the square has a fountain");
        s.interact(uid(1), fountain);
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
        if let Some(p) = s.players.get_mut(&uid(1)) {
            p.scores.strength = 10;
            p.scores.constitution = 10;
        }
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
