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
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

use chrono::Utc;
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chips::ChipMove,
        mud_character::MudCharacter,
        mud_world_state::MudWorldState,
        profile_award::{
            LATEANIA_ARCHDEMON_AWARD_CATEGORY, LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
            LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY, LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
            award_badge, grant_unique_milestone_award,
        },
        reward::{
            LATEANIA_ARCHDEMON_REWARD_KEY, LATEANIA_FRONTIER_KING_REWARD_KEY,
            LATEANIA_KAETHYR_ASCENDANT_REWARD_KEY, LATEANIA_SUNDERING_DEEP_REWARD_KEY,
        },
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
use super::appearance;
use super::classes::{ARCHETYPE_LEVEL, ArchetypeDef, Class, level_for_xp, xp_for_level};
use super::crafting::{recipe, recipe_indices_for};
use super::damage::{DamageProfile, DamageType, Defense};
use super::housing::{self, furniture_by_key, plot_of_room};
use super::items::{
    CATACOMBS_RELIC_ID, CAVERNS_RELIC_ID, Item, ItemKind, Slot, THORNWOOD_RELIC_ID, item, shop_at,
};
use super::persist::{
    SavedCharacter, SavedCharacterInit, SavedMob, SavedMobDot, SavedMobStun, SavedWorld,
};
use super::pets::{Pet, pet_species_by_key};
use super::skills::{CraftSkill, GatherSkill, TamingSkill, skill_level_for_xp, skill_progress};
use super::stats::{
    AbilityScores, CritOutcome, SCORE_CAP, Score, ScoreOfferView, crit_outcome, modifier,
    points_earned,
};
use super::taming::{PetSkillEffect, beast_species, beasts_at, tame_chance, tame_xp};
use super::world::{
    CritterKind, Dir, FeatureKind, MiniMap, MobBehavior, MobSpawn, Perk, RegionProgress,
    ResourceNode, RoomId, World, craft_stations_at, critter_index, critters_at, features_at,
    frontier_entrance_room, is_frontier_room, node_index, nodes_at, seed_world,
    tutorial_start_room,
};

// ---- Tuning: tick rate, timers, gate titles, boss achievements -----------

/// World heartbeat. One combat round resolves per tick.
const TICK_SECS: u64 = 2;
/// First id handed out to runtime-only summoned adds, kept far clear of the
/// authored spawn-id ranges (base game, Catacombs 800k+, Frontier 900k+).
const SUMMON_ID_START: u32 = 990_000_000;
/// A roamer takes a step at most this often (in ticks); at 2s/tick that is ~8s.
const MOB_MOVE_COOLDOWN: u8 = 4;
/// Ticks a wounded, stunned, or festering mob may go with nobody targeting it
/// before it recovers in full (health, stuns, DoTs). The grace (~6s) absorbs
/// a dropped connection that comes straight back and a target switched for a
/// moment, not a death and the walk back from the temple; it is far shorter
/// than any ability cooldown, so a foe can never be whittled down across
/// engagements. Fleeing skips the grace: the foe you turn your back on
/// recovers on the spot.
const MOB_RESET_TICKS: u8 = 3;
/// Ticks between two gulps of any heal/restore consumable. Draughts used to
/// be spammable inside a fight, bounded by gold alone; a breath between them
/// makes a potion a decision instead of a second health bar.
const QUAFF_COOLDOWN_TICKS: u8 = 5;
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

/// A short "Xh Ym" (or "Ym") countdown to the next UTC midnight, for
/// once-a-real-day mechanics (currently just stray adoption). Spelled out in
/// player-facing messages rather than a bare "come back tomorrow", since the
/// day boundary here is a real calendar day at UTC midnight - easy to
/// confuse with the visible in-game Dawn/Day/Dusk/Night clock, which is a
/// completely different, much faster (~16 real minutes) cycle.
fn time_until_next_utc_day() -> String {
    let remaining = 86_400 - (now_unix_secs() % 86_400);
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
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
    /// A phase-of-the-sun glyph (same `●○` dot family the character sheet's
    /// ability scores already use, so it reads as an existing house style,
    /// not a new one), so the world clock is legible at a glance instead of
    /// blending into the rest of the room panel's dim text.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Dawn => "\u{25D0}",  // ◐
            Self::Day => "\u{25CB}",   // ○
            Self::Dusk => "\u{25D1}",  // ◑
            Self::Night => "\u{25CF}", // ●
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
/// How long a fallen player's spirit lingers by their corpse, waiting for a
/// resurrection, before it is drawn back to the temple automatically. The
/// player may also release early. (Was an 8s rest before the dead state.)
const CORPSE_LINGER_SECS: u64 = 90;
/// Fraction of max HP (and resource) a resurrected player rises with.
const RESURRECT_HP_PCT: i32 = 40;
/// Gold to feed (heal, revive, and raise the loyalty of) a companion.
const PET_FEED_COST: i64 = 20;
/// Consecutive days a wild adoptable critter must be fed to win it over as a
/// stray companion (Genesys). Free (no gold cost) - the price is patience.
const STRAY_ADOPTION_DAYS: u32 = 5;
/// Fraction of a blow that splashes onto a fighting companion when its owner is
/// struck (the pet wades in and shares the punishment).
const PET_WOUND_PCT: i32 = 30;
/// Resource a caster spends to perform the Resurrection rite.
const RESURRECT_COST: i32 = 30;
/// Gold to warp to a marked personal waypoint. Word of recall (to Embergate)
/// stays free; a warp to your own chosen spot costs something, so a portable
/// teleporter stays a real convenience rather than trivialising distance.
const WAYPOINT_WARP_COST: i64 = 250;
/// Monk "Iron Body": percent reduction to incoming physical blows.
const IRON_BODY_PCT: i32 = 15;
/// Beastlord "Pack Bond": percent bonus to a companion's attack (and, via the
/// same fraction, its effective toughness against wounds) plus a share knocked
/// off its auto-skill cooldowns.
const BEASTLORD_PET_PCT: i32 = 30;
/// Percent of the owner's attack rating a companion adds to its own bite
/// (`Pet::attack`). The same shape as an ability (a flat floor plus a share
/// of the rating), so the pet multiplies the build instead of replacing it:
/// tuned in the arena to keep a band-appropriate companion at 12-30% of a
/// character's output (`a_companion_is_a_share_of_the_fight_not_the_fight`).
const PET_COEF_PCT: i32 = 20;
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
/// The Sundered Reaches open only to whoever has unmade the Frontier's crown.
const REACHES_GATE_TITLE: &str = "Bane of the King Who Was Promised Nothing";
/// Kaelmyr, the Ashen Reach, opens only to whoever has drowned the deepest crown
/// of the Reaches - the Bane of Yssgar. It is the deepest end-game gate.
const KAELMYR_GATE_TITLE: &str = "Bane of Yssgar, the Sundering Deep";

/// How often the world autosaves every present character's progress.
const AUTOSAVE_SECS: u64 = 60;
/// How often the shared world runtime snapshot is persisted.
const WORLD_AUTOSAVE_SECS: u64 = 15;
const LATEANIA_WORLD_KEY: &str = "lateania";

#[derive(Clone, Copy)]
struct BossAchievement {
    mob_name: &'static str,
    award_category: &'static str,
    /// Chip payout via a reward template: once per character, and at most once
    /// every 7 days per account (SHOP.md Phase 6). `None` means the profile
    /// badge is the whole prize.
    payout: Option<BossPayout>,
}

#[derive(Clone, Copy)]
struct BossPayout {
    reward_key: &'static str,
    chip_move: ChipMove,
}

const ARCHDEMON_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "the Archdemon Mal'gareth",
    award_category: LATEANIA_ARCHDEMON_AWARD_CATEGORY,
    payout: Some(BossPayout {
        reward_key: LATEANIA_ARCHDEMON_REWARD_KEY,
        chip_move: ChipMove::LateaniaArchdemonDefeat,
    }),
};

const FRONTIER_KING_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "the King Who Was Promised Nothing",
    award_category: LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
    payout: Some(BossPayout {
        reward_key: LATEANIA_FRONTIER_KING_REWARD_KEY,
        chip_move: ChipMove::LateaniaFrontierKingDefeat,
    }),
};

const SUNDERING_DEEP_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "Yssgar, the Sundering Deep",
    award_category: LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
    payout: Some(BossPayout {
        reward_key: LATEANIA_SUNDERING_DEEP_REWARD_KEY,
        chip_move: ChipMove::LateaniaSunderingDeepDefeat,
    }),
};

const KAETHYR_ASCENDANT_ACHIEVEMENT: BossAchievement = BossAchievement {
    mob_name: "Kaethyr Ascendant, Who Sang the God Awake",
    award_category: LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY,
    payout: Some(BossPayout {
        reward_key: LATEANIA_KAETHYR_ASCENDANT_REWARD_KEY,
        chip_move: ChipMove::LateaniaKaethyrAscendantDefeat,
    }),
};

/// Account age (in days) at which an adventurer is a "citizen" of Lateania and
/// earns extra resurrections.
const VETERAN_DAYS: i64 = 20;
/// In-place resurrections a veteran gets per adventure (refreshed at a capital
/// fountain). Newer accounts get none and respawn at the temple as before.
const VETERAN_RESURRECTIONS: u8 = 2;

/// A character within an account: which slot of whose saves.
type CharKey = (Uuid, i16);

#[derive(Clone)]
pub struct LateaniaService {
    activity: ActivityPublisher,
    chip_svc: ChipService,
    db: Db,
    snapshot_tx: watch::Sender<MudSnapshot>,
    snapshot_rx: watch::Receiver<MudSnapshot>,
    state: Arc<Mutex<WorldState>>,
    active_sessions: Arc<StdMutex<HashMap<Uuid, HashSet<Uuid>>>>,
    // Keyed by (account, slot): a save in flight for one slot must never be
    // mistaken for another slot's, or a fast slot switch could hydrate a join
    // from the wrong character's still-in-flight blob.
    persist_versions: Arc<StdMutex<HashMap<CharKey, u64>>>,
    persist_locks: Arc<StdMutex<HashMap<CharKey, Arc<Mutex<()>>>>>,
    prepared_saves: Arc<StdMutex<HashMap<CharKey, (u64, SavedCharacter)>>>,
    character_resets: Arc<StdMutex<HashSet<CharKey>>>,
    character_reset_versions: Arc<StdMutex<HashMap<CharKey, u64>>>,
    /// Which character slot the landing last *asked* to play, set by
    /// `select_slot` before `join_task` fires. Absent means slot 0, so accounts
    /// that never see the slot picker (or predate it) keep loading their one
    /// existing character. This is intent only: it is read exactly once, by the
    /// `join_task` that creates the world player, and never by a save.
    active_slot: Arc<StdMutex<HashMap<Uuid, i16>>>,
    /// Which slot the character *currently in the world* was loaded from, bound
    /// the moment `join` creates that player and released when it leaves. Every
    /// save resolves its slot from here (see `prepare_persist`), never from
    /// `active_slot`: the picker is account-wide and a second session selecting
    /// a different slot would otherwise redirect the live character's saves on
    /// top of the character saved there.
    live_slot: Arc<StdMutex<HashMap<Uuid, i16>>>,
    /// Cached slot summaries for the character-select landing, refreshed by
    /// `character_slots_task` and read synchronously by the render path.
    slot_summaries: Arc<StdMutex<HashMap<Uuid, Vec<SlotSummary>>>>,
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

/// How a room description came about, deciding the Travel line in the Recent
/// feed. In field mode (rpg on) the moving `@` and the Here panel already tell
/// the story of a step through known land, so only a discovery earns a line;
/// classic mode keeps its per-step breadcrumb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrival {
    /// Re-describing in place (a look around, an ambient refresh): no line.
    Silent,
    /// Travel into a room already on the player's map.
    Revisit,
    /// First footfall in this room.
    Discovery,
}

/// Who hears a spoken line: the room (the default, unchanged), everyone in
/// the same named zone, or every adventurer currently in Lateania. Chosen per
/// message with a leading `/z`/`/zone` or `/w`/`/world` marker; see `say`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChatScope {
    Room,
    Zone,
    World,
}

#[derive(Clone, Debug)]
pub struct MobView {
    /// Spawn id, so a click on this foe's row can target this exact foe.
    pub id: u32,
    pub name: String,
    pub hp: i32,
    pub max_hp: i32,
    pub level: i32,
    /// Rarity rank for colouring the name: common/uncommon/rare/epic/legendary.
    pub rank: String,
    pub boss: bool,
    /// True when this is the foe you're currently locked onto.
    pub targeted: bool,
    /// The damage school this foe strikes with (e.g. "fire").
    pub school: &'static str,
    /// The school this foe is weak to, if any - the tactical opening.
    pub weak: Option<&'static str>,
    /// The school this foe shrugs off, if any.
    pub resist: Option<&'static str>,
    /// Damage-over-time stacks currently ticking on this foe.
    pub dot_stacks: u8,
    /// True while this foe is stunned (skipping its actions).
    pub stunned: bool,
}

/// Which kind of quest a journal row is; closed so the panel matches
/// exhaustively when grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestKind {
    /// The auto-granted new-player chain (one active step at a time).
    Starter,
    /// An accepted board bounty.
    Board,
    /// A Frontier zone-clear quest (only listed once the Frontier is open).
    Frontier,
}

/// One quest row in the journal.
#[derive(Clone, Debug)]
pub struct QuestView {
    pub name: String,
    /// What the quest actually asks for, so it's still readable long after
    /// the one-time accept-time log line has scrolled off - a board bounty's
    /// blurb plus its mechanical objective, or a Frontier quest's plain
    /// "slay X" restated here for the same reason.
    pub desc: String,
    pub done: bool,
    pub reward: String,
    pub kind: QuestKind,
    /// A room this quest points at, for tracking it on the world map (Enter in
    /// the journal). None when no single meaningful place exists.
    pub target: Option<RoomId>,
}

/// One milestone on the Long Road - the realm's spine of great bosses whose
/// titles gate the next land. Derived purely from the player's titles.
#[derive(Clone, Debug)]
pub struct RoadStepView {
    /// The boss to bring down, as named in the world.
    pub boss: String,
    /// Where the fight happens.
    pub place: &'static str,
    /// What falls open once it's done ("" when it is glory alone).
    pub unlocks: &'static str,
    pub done: bool,
    /// The first undone milestone - the one the player walks toward now.
    pub current: bool,
    /// The boss's lair, for tracking the crown on the compass/map (Enter in
    /// the journal). Resolved from the spawn table at world build.
    pub target: Option<RoomId>,
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
    /// Out of legend rather than the mundane world (Genesys).
    pub mythical: bool,
    /// Can be won over as a stray companion by feeding it daily (Genesys).
    pub adoptable: bool,
}

/// One harvestable resource node in the room, for the Resources list.
#[derive(Clone, Debug)]
pub struct NodeView {
    pub name: String,
    pub note: String,
    /// The gathering skill it belongs to, e.g. "Woodcutting".
    pub skill: String,
    /// True when the player can work it right now (off cooldown and skilled enough).
    pub gatherable: bool,
    /// Why it can't be worked, when `gatherable` is false: "needs Mining 16" or
    /// "regrowing"; empty when it can.
    pub reason: String,
}

/// One gathering skill's progress, for the character sheet Skills block.
#[derive(Clone, Debug)]
pub struct SkillView {
    pub name: String,
    pub level: i32,
    pub xp_into: i64,
    pub xp_next: i64,
}

/// One recipe row in the crafting panel.
#[derive(Clone, Debug)]
pub struct CraftEntryView {
    /// Global recipe index, passed back to `craft`.
    pub recipe: usize,
    pub name: String,
    /// The craft skill it trains, e.g. "Smithing".
    pub skill: String,
    /// Compact ingredient list, e.g. "3x Copper Ingot, 1x Oak Plank".
    pub inputs: String,
    /// True when it can be made right now (station here, skilled enough, have
    /// the materials).
    pub craftable: bool,
    /// Why it can't be made, when `craftable` is false; empty when it can.
    pub reason: String,
}

/// The crafting panel, present when the player stands at any craft station. Lists
/// every recipe worked at the stations in this room.
#[derive(Clone, Debug)]
pub struct CraftView {
    /// The stations standing here, e.g. "forge, alchemy lab".
    pub stations: String,
    pub entries: Vec<CraftEntryView>,
}

/// One navigable row of a collapsible list panel (crafting / inventory / shop):
/// a category header, or an item beneath an expanded header. The cursor moves
/// over these rows so a long list can be folded down to just its headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionRow {
    /// A category header. `key` is the stable collapse key (panel-prefixed, e.g.
    /// `"craft:Cooking"`); `label` is what's shown; `count` is the items it holds.
    Header {
        key: String,
        label: String,
        count: usize,
        collapsed: bool,
    },
    /// An item row; `index` indexes into the panel's underlying list.
    Item { index: usize },
}

/// Group `count` items into collapsible sections. `category(i)` returns the
/// `(collapse-key, display label)` for item `i`; sections appear in first-seen
/// order. A section whose key is in `collapsed` shows only its header. The row
/// list is exactly what the cursor navigates and the panel draws.
pub fn section_rows(
    count: usize,
    category: impl Fn(usize) -> (String, String),
    collapsed: &std::collections::HashSet<String>,
) -> Vec<SectionRow> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (String, Vec<usize>)> =
        std::collections::HashMap::new();
    for i in 0..count {
        let (key, label) = category(i);
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key.clone());
                (label, Vec::new())
            })
            .1
            .push(i);
    }
    let mut rows = Vec::new();
    for key in order {
        let (label, items) = &groups[&key];
        let is_collapsed = collapsed.contains(&key);
        rows.push(SectionRow::Header {
            key: key.clone(),
            label: label.clone(),
            count: items.len(),
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            rows.extend(items.iter().map(|&index| SectionRow::Item { index }));
        }
    }
    rows
}

impl CraftView {
    /// Rows grouped under collapsible skill headers (keys `"craft:<skill>"`).
    pub fn rows(&self, collapsed: &std::collections::HashSet<String>) -> Vec<SectionRow> {
        section_rows(
            self.entries.len(),
            |i| {
                let skill = &self.entries[i].skill;
                (format!("craft:{skill}"), skill.clone())
            },
            collapsed,
        )
    }
}

#[derive(Clone, Debug)]
pub struct OccupantView {
    pub user_id: Uuid,
    pub hp: i32,
    pub max_hp: i32,
    pub in_combat: bool,
    /// False when this adventurer is a corpse awaiting resurrection or release.
    pub alive: bool,
    /// The adventurer's composed bio, shown when you profile them.
    pub bio: String,
    /// This adventurer's stable class key (empty if unclassed), for their portrait.
    pub class_key: String,
    /// This adventurer's character level, shown alongside their name.
    pub level: i32,
    /// This adventurer's raw appearance selections, for composing their portrait.
    pub appearance_idx: Vec<u8>,
    /// True when this room is a `pvp` zone and this adventurer is a valid
    /// target: alive, classed, and not you. Drives the clickable roster row
    /// and the hostile marker (see `engage_player`).
    pub attackable: bool,
    /// True when this adventurer is who you're currently duelling.
    pub targeted: bool,
}

/// One row of a leaderboard: who, their level and class (for the portrait
/// glyph/colour), and the ranked value itself (meaning depends on which
/// board it's in - level, pvp kills, or total gold).
#[derive(Clone, Debug)]
pub struct LeaderboardEntry {
    pub user_id: Uuid,
    pub level: i32,
    pub class_key: String,
    pub value: i64,
}

/// The top ten currently-connected, classed adventurers by three measures.
/// Identical for every player this tick (nothing here depends on who's
/// asking), so `WorldState::snapshot` computes it once and shares it via
/// `Arc` rather than rebuilding/cloning it per player.
#[derive(Clone, Debug, Default)]
pub struct LeaderboardView {
    pub by_level: Vec<LeaderboardEntry>,
    pub by_pvp_kills: Vec<LeaderboardEntry>,
    pub by_gold: Vec<LeaderboardEntry>,
}

/// How many characters an account may keep, so trying another class doesn't
/// mean wiping the one you already have.
pub const CHARACTER_SLOTS: i16 = 5;

/// One row of the character-select landing: a slot is either empty (never
/// saved, or just reset) or shows enough of the saved character to recognize
/// it at a glance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotSummary {
    pub slot: i16,
    pub occupied: bool,
    pub class: Option<Class>,
    pub level: i32,
}

impl SlotSummary {
    fn empty(slot: i16) -> Self {
        Self {
            slot,
            occupied: false,
            class: None,
            level: 0,
        }
    }

    fn from_saved(slot: i16, saved: &SavedCharacter) -> Self {
        Self {
            slot,
            occupied: true,
            class: saved.class.as_deref().and_then(Class::from_key),
            level: saved.level,
        }
    }
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
    /// How this gear compares to what's worn in its slot, e.g. "vs worn: +3 atk
    /// -2 hp", "new slot", or "" for non-gear / the worn item itself.
    pub compare: String,
    /// The same comparison as a percent power change (positive = an upgrade,
    /// shown green; negative = worse, red). None for non-gear or the item
    /// already equipped. Drives the batch-sell "non-upgrades" filter.
    pub compare_pct: Option<i32>,
    /// The collapsible category this item groups under (Weapons / Armor /
    /// Consumables / Valuables).
    pub category: &'static str,
    /// The item's flavor/description text.
    pub desc: &'static str,
}

/// A batch-sell request at a merchant. Consumables and equipped gear are never
/// touched; only loose inventory is sold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SellBatch {
    /// Every loose piece of gear and every valuable.
    All,
    /// Only common-rarity gear (plus valuables).
    Common,
    /// Gear that wouldn't improve the character (not an upgrade), plus valuables.
    NonUpgrades,
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
    /// How this gear compares to what's worn in its slot (see `InvView::compare`).
    pub compare: String,
    /// The same comparison as a percent power change (see `InvView::compare_pct`).
    pub compare_pct: Option<i32>,
    /// The collapsible category this item groups under (Weapons / Armor /
    /// Consumables / Valuables).
    pub category: &'static str,
    /// The item's flavor/description text.
    pub desc: &'static str,
}

/// The collapsible-panel category an item belongs to. Split from a single
/// "Consumables" bucket so a batch-sell of loose gear/valuables never risks
/// a buff item that happened to be lumped in with them: "Heals" is anything
/// that actually restores HP/resource, "Consumables" is everything else you
/// use from the pack (poisons and future non-heal effect items) - a player
/// can bulk-sell Valuables without checking every item for a hidden buff.
pub(super) fn item_category(kind: &super::items::ItemKind) -> &'static str {
    use super::items::{ItemKind, Slot};
    match kind {
        ItemKind::Equipment(Slot::Weapon) => "Weapons",
        ItemKind::Equipment(_) => "Armor",
        ItemKind::Consumable { heal, restore } if *heal > 0 || *restore > 0 => "Heals",
        ItemKind::Consumable { .. } | ItemKind::Utility => "Consumables",
        ItemKind::Valuable => "Valuables",
    }
}

/// The player's live companion, for the room/character panels.
#[derive(Clone, Debug)]
pub struct PetView {
    pub name: String,
    pub glyph: String,
    pub level: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub attack: i32,
    pub downed: bool,
    /// Loyalty toward the next level, 0-100.
    pub loyalty_pct: i32,
    /// Auto-skills the pet has unlocked at its level: (name, unlock level). Fire
    /// automatically in combat.
    pub skills: Vec<(String, i32)>,
}

/// One tameable wild beast present in the room, for the Taming panel.
#[derive(Clone, Debug)]
pub struct TameEntryView {
    /// Index into the room's tameable list (passed back to `tame`).
    pub idx: usize,
    pub name: String,
    pub glyph: String,
    /// Animal Taming level this beast requires.
    pub req_level: i32,
    /// The player's success odds right now, 0-100 (0 = under-level or spooked).
    pub odds: u32,
    /// A short status: "" when tamable, else "needs Taming N" / "spooked".
    pub reason: String,
    pub desc: String,
}

/// The Animal Taming panel: the tameable beasts roaming this room, with the
/// player's taming level and odds. Present when a tameable beast is here.
#[derive(Clone, Debug)]
pub struct TamingView {
    /// The player's current Animal Taming level.
    pub taming_level: i32,
    pub entries: Vec<TameEntryView>,
}

/// One companion offered at a Stable.
#[derive(Clone, Debug)]
pub struct StableEntryView {
    pub key: String,
    pub name: String,
    pub glyph: String,
    pub price: i64,
    pub hp: i32,
    pub attack: i32,
    pub desc: String,
    pub affordable: bool,
}

/// The companion vendor, present when the player stands at a Stable.
#[derive(Clone, Debug)]
pub struct StableView {
    pub entries: Vec<StableEntryView>,
    /// Gold to feed the current companion (shown as the panel's tend action).
    pub feed_cost: i64,
}

/// One row in the housing ledger: a deed (at the clerk) or a furnishing (inside
/// a home you own).
#[derive(Clone, Debug)]
pub struct HousingEntryView {
    pub key: String,
    pub name: String,
    pub price: i64,
    /// Compact detail, e.g. "4 rooms" for a deed or the furnishing's flavour.
    pub detail: String,
    pub affordable: bool,
    /// For deeds: already claimed by someone else (and not buyable).
    pub taken: bool,
    /// For deeds: this is the viewing player's own plot.
    pub owned: bool,
}

/// The housing ledger panel: deeds at the clerk, or furnishings inside an owned
/// home. `furnish` distinguishes the two modes.
#[derive(Clone, Debug)]
pub struct HousingView {
    pub title: String,
    /// False = buying deeds at the clerk; true = furnishing a home you own.
    pub furnish: bool,
    pub entries: Vec<HousingEntryView>,
}

/// The waystone fast-travel menu, present when standing on a portal.
#[derive(Clone, Debug)]
pub struct PortalView {
    /// Each offered destination: `(label, room id, is_here)`. A mainland gate
    /// the player has never stood in is absent entirely rather than dimmed;
    /// the archipelago is always listed (it has no walking route in).
    pub entries: Vec<(String, RoomId, bool)>,
    /// How many leading entries are mainland gates, so the panel can head its
    /// three blocks (gates, villages, islands) without index arithmetic over a
    /// list whose first block varies in length.
    pub known_gates: usize,
    /// Mainland gates not yet found, so the panel can say the network is larger
    /// than what it lists without naming what is missing.
    pub unknown_gates: usize,
}

/// A quest board's postings, present whenever the player stands where a
/// board feature is. Replaces the old "examine auto-assigns the next bounty"
/// flow: every ready-to-claim and still-open bounty for this board is listed
/// so taking one is an explicit choice, not the luck of whatever was first in
/// the static list - that's what let a low-level player get handed a bounty
/// for a foe they'd never even seen yet.
#[derive(Clone, Debug)]
pub struct BoardView {
    pub entries: Vec<BoardEntryView>,
}

/// One posting on a board: either a finished counter-bounty ready to turn in
/// (`ready`), or one still open to accept.
#[derive(Clone, Debug)]
pub struct BoardEntryView {
    pub quest_id: u32,
    pub title: String,
    pub blurb: String,
    pub objective: String,
    pub reward: String,
    pub ready: bool,
    /// Where the work is and how to walk there, in plain words.
    pub hint: String,
    /// A rough level at which the bounty is a fair fight.
    pub suggested_level: i32,
    /// True when the bounty's hunting ground sits behind a progression gate
    /// the player has not opened; shown sealed and refused on accept.
    pub locked: bool,
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
    pub reset_versions: HashMap<Uuid, u64>,
}

#[derive(Clone, Debug)]
pub struct PlayerView {
    pub joined: bool,
    pub classed: bool,
    pub class_name: String,
    /// Stable class key (e.g. "warrior"), for the composed portrait.
    pub class_key: String,
    pub trait_name: String,
    pub trait_desc: String,
    pub resource_name: String,
    pub resource: i32,
    pub max_resource: i32,
    pub alive: bool,
    pub hp: i32,
    pub max_hp: i32,
    pub attack: i32,
    /// What the Physical auto-attack lands for (the attack by the calling's
    /// `auto_pct`).
    pub swing: i32,
    /// Spell power: what every ability adds to its base (by effect).
    pub spell_power: i32,
    pub armor: i32,
    pub xp: i64,
    pub xp_into_level: i64,
    pub xp_for_next: i64,
    pub level: i32,
    pub gold: i64,
    pub banked_gold: i64,
    /// The player's current room id, for centring the overhead world map.
    /// `None` only in the empty view (no character in the world yet).
    pub room: Option<RoomId>,
    /// Rooms the player has visited, for the overhead map's fog of war. Shared,
    /// not copied: `view()` clones a PlayerView on every keystroke and every
    /// frame, and a well-travelled character's explored set runs to thousands
    /// of rooms.
    pub visited: Arc<HashSet<RoomId>>,
    pub room_name: String,
    pub room_desc: String,
    pub zone: String,
    /// The (min, max) mob level of the zone the player stands in, so the zone
    /// line reads "King's Road · Lv 2-5". None where nothing hostile lives.
    pub zone_band: Option<(i32, i32)>,
    pub safe: bool,
    /// True in a Wildbound-style contested zone (see `Room::pvp`), where the
    /// "Adventurers here" roster shows hostile marks and is clickable to duel.
    pub pvp: bool,
    /// Lifetime adventurers this character has slain in pvp combat.
    pub pvp_kills: i64,
    /// Top-ten currently-connected adventurers by level/pvp kills/gold.
    /// Shared (not per-player data), see `LeaderboardView`. Opened with `?`.
    pub leaderboard: Arc<LeaderboardView>,
    pub exits: Vec<(Dir, String)>,
    pub mobs: Vec<MobView>,
    /// Rooms near you that hold a living, revealed foe, so the live field can
    /// mark where danger is without you having to step into it. Same-level and
    /// within the field's reach only; fog still hides rooms you've never seen.
    pub nearby_foes: Vec<RoomId>,
    /// Rooms near you that hold another adventurer, so the field shows where
    /// other players are. Same window as `nearby_foes`.
    pub nearby_players: Vec<RoomId>,
    /// The live-map RPG view preference, persisted with the character.
    pub rpg_mode: bool,
    /// What you're riding right now (Wildbound mounts), with its stride,
    /// e.g. "Moonlit Unicorn (stride 4)". None on foot.
    pub riding: Option<String>,
    /// Whether a personal waypoint is set (see `set_waypoint`/`warp_to_waypoint`).
    pub waypoint_set: bool,
    pub occupants: Vec<OccupantView>,
    /// The companion this player is auto-following, if any (for the UI tag).
    pub following: Option<Uuid>,
    /// Wild creatures sharing the room.
    pub wildlife: Vec<WildlifeView>,
    /// Harvestable resource nodes in the room (trees, veins, fishing spots...).
    pub nodes: Vec<NodeView>,
    /// The player's gathering skills and their progress, for the Skills block.
    pub skills: Vec<SkillView>,
    pub in_combat_with: Option<String>,
    /// Absorb shield remaining on the player, for the battle frame.
    pub shield: i32,
    /// Outgoing-damage buff magnitude currently active (0 when none).
    pub empower: i32,
    /// True while the player is stunned (skipping their actions).
    pub stunned: bool,
    /// The active weapon coat as a display line ("fire coat x8"), if any.
    pub coat: Option<String>,
    pub abilities: Vec<AbilityView>,
    pub inventory: Vec<InvView>,
    pub shop: Option<ShopView>,
    /// The player's live combat companion, if any.
    pub pet: Option<PetView>,
    /// A won-over stray companion's name, if any (Genesys) - lives alongside
    /// `pet` rather than replacing it.
    pub stray: Option<String>,
    /// The companion vendor, present when standing at a capital Stable.
    pub stable: Option<StableView>,
    /// The Animal Taming panel, present when a tameable wild beast roams here.
    pub taming: Option<TamingView>,
    /// The housing ledger, present at the clerk or inside a home you own.
    pub housing: Option<HousingView>,
    /// The crafting panel, present when standing at any craft station.
    pub crafting: Option<CraftView>,
    /// The waystone fast-travel menu, present when standing on a portal.
    pub portal: Option<PortalView>,
    /// The quest board's postings, present when standing where a board is.
    pub board: Option<BoardView>,
    /// The composed character bio (from the appearance choices).
    pub bio: String,
    /// The appearance/bio builder rows: (field label, chosen option).
    pub appearance: Vec<(String, String)>,
    /// The raw appearance selection indices, for composing the portrait.
    pub appearance_idx: Vec<u8>,
    pub log: Vec<LogLine>,
    pub respawning: bool,
    /// True while this player is a corpse (fallen, awaiting rez or release).
    pub dead: bool,
    /// Whether this player's class commands the Resurrection rite.
    pub can_resurrect: bool,
    /// Whether a resurrectable corpse (another fallen player) is in this room.
    pub corpse_here: bool,
    /// Rolled D&D ability scores (shown on the select screen and sheet).
    pub scores: AbilityScores,
    /// Titles earned by slaying notable foes.
    pub titles: Vec<String>,
    /// Level for each title (parallel to `titles`).
    pub title_levels: Vec<i32>,
    /// Index of the displayed title, if one is chosen.
    pub active_title: Option<usize>,
    /// The journal's quest rows: the active starter step, accepted board
    /// bounties, and (once the Frontier is open) its zone quests.
    pub quests: Vec<QuestView>,
    /// The Long Road: the realm's great-boss spine, derived from titles.
    pub road: Vec<RoadStepView>,
    /// True once the player holds every title the Frontier stair demands (the
    /// journal shows the 20 zone quests only then; sealed reads as one line).
    pub frontier_open: bool,
    /// Veteran in-place resurrections remaining / total this adventure.
    pub resurrections_left: u8,
    pub resurrection_cap: u8,
    /// Lookable things in the current room (Examine panel).
    pub features: Vec<FeatureView>,
    /// Overhead map of the explored neighbourhood around the player.
    pub minimap: MiniMap,
    /// The whole-world atlas: exploration progress per major region (Map panel).
    pub atlas: Vec<RegionProgress>,
    /// The world clock phase, e.g. "dawn"/"day"/"dusk"/"night".
    pub time_of_day: &'static str,
    /// A phase-of-the-sun glyph for `time_of_day` (see `TimeOfDay::glyph`),
    /// so the clock reads at a glance rather than blending into dim text.
    pub time_of_day_glyph: &'static str,
    /// True during dusk/night, when mobs hit 25% harder (`TimeOfDay::is_dark`).
    /// Surfaced so the UI can colour the clock as a real danger cue, not
    /// just flavour text - the day/night cycle otherwise reads as
    /// decoration even though it has a real mechanical effect.
    pub time_of_day_dark: bool,
    /// The current weather, e.g. "clear"/"rain"/"fog"/"storm".
    pub weather: &'static str,
    /// An active escort, if any: (name, hp, max_hp, destination zone).
    pub escort: Option<(String, i32, i32, String)>,
    /// The chosen archetype path, as (name, role label), once selected at L10.
    pub archetype: Option<(String, String)>,
    /// When eligible to pick an archetype but not yet chosen, the offered paths
    /// as (name, role label, description); empty otherwise. Drives the select UI.
    pub archetype_choices: Vec<(String, String, String)>,
    /// Attribute points earned and not yet placed.
    pub score_points: i32,
    /// While a point waits to be placed (and no archetype crossroads is open),
    /// one row per score for the point screen; empty otherwise.
    pub score_offer: Vec<ScoreOfferView>,
}

impl PlayerView {
    fn empty() -> Self {
        Self {
            joined: false,
            room: None,
            visited: Arc::new(HashSet::new()),
            classed: false,
            class_name: String::new(),
            class_key: String::new(),
            trait_name: String::new(),
            trait_desc: String::new(),
            resource_name: String::new(),
            resource: 0,
            max_resource: 0,
            alive: false,
            hp: 0,
            max_hp: 0,
            attack: 0,
            swing: 0,
            spell_power: 0,
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
            zone_band: None,
            safe: true,
            pvp: false,
            pvp_kills: 0,
            leaderboard: Arc::new(LeaderboardView::default()),
            exits: Vec::new(),
            mobs: Vec::new(),
            nearby_foes: Vec::new(),
            nearby_players: Vec::new(),
            rpg_mode: true,
            riding: None,
            waypoint_set: false,
            occupants: Vec::new(),
            following: None,
            wildlife: Vec::new(),
            nodes: Vec::new(),
            skills: Vec::new(),
            in_combat_with: None,
            shield: 0,
            empower: 0,
            stunned: false,
            coat: None,
            abilities: Vec::new(),
            inventory: Vec::new(),
            shop: None,
            pet: None,
            stray: None,
            stable: None,
            taming: None,
            housing: None,
            crafting: None,
            portal: None,
            board: None,
            bio: String::new(),
            appearance: Vec::new(),
            appearance_idx: Vec::new(),
            log: Vec::new(),
            respawning: false,
            dead: false,
            can_resurrect: false,
            corpse_here: false,
            scores: AbilityScores::default(),
            titles: Vec::new(),
            title_levels: Vec::new(),
            active_title: None,
            quests: Vec::new(),
            road: Vec::new(),
            frontier_open: false,
            resurrections_left: 0,
            resurrection_cap: 0,
            features: Vec::new(),
            minimap: MiniMap::default(),
            atlas: Vec::new(),
            time_of_day: "day",
            time_of_day_glyph: "\u{25CB}",
            time_of_day_dark: false,
            weather: "clear",
            escort: None,
            archetype: None,
            archetype_choices: Vec::new(),
            score_points: 0,
            score_offer: Vec::new(),
        }
    }
}

pub fn empty_player_view() -> PlayerView {
    PlayerView::empty()
}

/// A compact comparison of a piece of gear against whatever the player currently
/// wears in that slot, for the inventory and shop panels. Returns "" for
/// non-gear and for the worn item itself; "new slot" when nothing is worn there;
/// otherwise the stat deltas, e.g. "vs worn: +3 atk -2 hp".
fn compare_to_worn(equipped: &HashMap<Slot, u32>, it: &Item) -> String {
    let Some(slot) = it.slot() else {
        return String::new();
    };
    match equipped.get(&slot).and_then(|id| item(*id)) {
        None => "new slot".to_string(),
        Some(worn) if worn.id == it.id => String::new(),
        Some(worn) => {
            let deltas = [
                (it.mods.attack - worn.mods.attack, "atk"),
                (it.mods.max_hp - worn.mods.max_hp, "hp"),
                (it.mods.armor - worn.mods.armor, "arm"),
            ];
            let parts: Vec<String> = deltas
                .iter()
                .filter(|(d, _)| *d != 0)
                .map(|(d, label)| format!("{d:+} {label}"))
                .collect();
            if parts.is_empty() {
                "same as worn".to_string()
            } else {
                format!("vs worn: {}", parts.join(" "))
            }
        }
    }
}

// ---- The service: command tasks, autosave loops, and snapshots -----------

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
            character_reset_versions: Arc::new(StdMutex::new(HashMap::new())),
            active_slot: Arc::new(StdMutex::new(HashMap::new())),
            live_slot: Arc::new(StdMutex::new(HashMap::new())),
            slot_summaries: Arc::new(StdMutex::new(HashMap::new())),
        };
        // Build the overhead map's coordinate field and POI index now. Both are
        // lazy statics costing a world-gen apiece, and their first caller is
        // `draw_world_map`, which runs on the render task under the app mutex.
        // A panic in here would poison the lazies and then panic every later
        // map render on a server that looked healthy at boot, so it is fatal
        // instead: the world data itself already proved sound (`seed_world`
        // ran synchronously above), which makes a warm-up panic a code bug.
        let warm = tokio::task::spawn_blocking(super::worldmap::warm);
        tokio::spawn(async move {
            if let Err(error) = warm.await {
                tracing::error!(?error, "world map warm-up panicked, exiting");
                std::process::exit(1);
            }
        });
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

    // ---- Character slots ---------------------------------------------------
    //
    // An account can keep up to `CHARACTER_SLOTS` saved characters, but the
    // world only ever holds one player per account, so only one of those
    // characters is live at a time. Two different questions therefore need two
    // different answers, and conflating them loses saves:
    //
    //   `active_slot` - which slot the landing last asked for. Account-wide,
    //     changes on every Enter from any connection, read only by the
    //     `join_task` that actually creates the world player.
    //   `live_slot`   - which slot the character in the world came from. Bound
    //     at that same join and released at leave; the only thing a save is
    //     ever allowed to consult.
    //
    // Everything downstream of join still keys off the account's own `user_id`,
    // unchanged.

    /// The slot the landing last asked to play for this account. Defaults to 0
    /// so accounts that never touch the slot picker keep their one character.
    fn active_slot(&self, user_id: Uuid) -> i16 {
        self.active_slot
            .lock_recover()
            .get(&user_id)
            .copied()
            .unwrap_or(0)
    }

    /// Which slot the account's live character was loaded from, if one is in
    /// the world at all.
    fn live_slot(&self, user_id: Uuid) -> Option<i16> {
        self.live_slot.lock_recover().get(&user_id).copied()
    }

    /// Bind the account's live character to the slot it was just loaded from.
    /// Called only where `join` creates the world player.
    fn bind_live_slot(&self, user_id: Uuid, slot: i16) {
        self.live_slot.lock_recover().insert(user_id, slot);
    }

    /// Release the binding once the character has left the world. Called only
    /// where the world player is removed.
    fn unbind_live_slot(&self, user_id: Uuid) {
        self.live_slot.lock_recover().remove(&user_id);
    }

    /// Pick which character slot the next `join_task` for this account loads.
    pub fn select_slot(&self, user_id: Uuid, slot: i16) {
        self.active_slot.lock_recover().insert(user_id, slot);
    }

    /// Cached slot summaries for the character-select landing; empty until
    /// `character_slots_task` resolves at least once (the landing then just
    /// shows every slot as empty for a frame or two).
    pub fn character_slots(&self, user_id: Uuid) -> Vec<SlotSummary> {
        self.slot_summaries
            .lock_recover()
            .get(&user_id)
            .cloned()
            .unwrap_or_else(|| (0..CHARACTER_SLOTS).map(SlotSummary::empty).collect())
    }

    /// Refresh the cached slot summaries for the landing. Safe to call often;
    /// it's a handful of small-blob reads, not the world lock.
    pub fn character_slots_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let Ok(client) = svc.db.get().await else {
                return;
            };
            let rows = match MudCharacter::list(&client, user_id).await {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%user_id, ?error, "failed to list mud character slots");
                    return;
                }
            };
            let mut by_slot: HashMap<i16, SavedCharacter> = rows
                .into_iter()
                .filter_map(|(slot, blob)| SavedCharacter::from_json(&blob).map(|s| (slot, s)))
                .collect();
            let summaries = (0..CHARACTER_SLOTS)
                .map(|slot| match by_slot.remove(&slot) {
                    Some(saved) => SlotSummary::from_saved(slot, &saved),
                    None => SlotSummary::empty(slot),
                })
                .collect();
            svc.slot_summaries.lock_recover().insert(user_id, summaries);
        });
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
            svc.publish(&state);
        });
    }

    pub fn join_task(&self, user_id: Uuid, session_id: Uuid) {
        self.mark_session_joined(user_id, session_id);
        let svc = self.clone();
        tokio::spawn(async move {
            let slot = svc.active_slot(user_id);
            if !svc.has_active_session(user_id) {
                return;
            }
            if svc.character_reset_in_progress(user_id, slot) {
                return;
            }
            let load_version = svc.current_persist_version(user_id, slot);

            // Load any saved character before exposing a fresh player. A DB
            // failure must not become "no save", otherwise later autosave or
            // logout can overwrite an existing character with a starter one.
            let saved = if let Some(saved) = svc.prepared_saved(user_id, slot) {
                Some(saved)
            } else {
                match svc.db.get().await {
                    Ok(client) => match MudCharacter::load(&client, user_id, slot).await {
                        Ok(Some(blob)) => SavedCharacter::from_json(&blob),
                        Ok(None) => None,
                        Err(error) => {
                            tracing::warn!(%user_id, slot, ?error, "failed to load mud character");
                            return;
                        }
                    },
                    Err(error) => {
                        tracing::warn!(%user_id, slot, ?error, "no db client for mud character load");
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
            if svc.character_reset_in_progress(user_id, slot) {
                return;
            }
            let saved = if svc.current_persist_version(user_id, slot) == load_version {
                saved
            } else {
                svc.prepared_saved(user_id, slot)
            };
            match state.players.contains_key(&user_id) {
                false => {
                    state.join(user_id);
                    // Bind before hydrating: from here until this character
                    // leaves, every save for the account goes to `slot` and
                    // nowhere else, whatever the landing is later asked for.
                    svc.bind_live_slot(user_id, slot);
                    state.set_veteran(user_id, veteran);
                    if let Some(saved) = saved {
                        state.hydrate(user_id, &saved);
                    }
                    // A player materialized in the world (fresh join, not an
                    // already-present session). The lounge feed's repeat window
                    // absorbs quick leave/rejoin ping-pong.
                    svc.activity.game_started_task(user_id, ActivityGame::Mud);
                }
                // Already in the world: a second connection for the same
                // account attaches to the character that is already playing.
                // One world identity per account means the slot it asked for
                // simply loses, and it must be told so, or it looks like the
                // pick silently failed.
                true => {
                    if svc.live_slot(user_id).is_some_and(|live| live != slot) {
                        state.log_to(
                            user_id,
                            LogKind::System,
                            "You're already adventuring on another connection. \
                             Both are playing that character; close the other \
                             session first to switch."
                                .to_string(),
                        );
                    }
                }
            }
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
                // Stage the save while the character is still live (that is
                // what resolves its slot), then remove it and release the
                // binding, so nothing that runs later can save it again.
                let saved = state
                    .export_saved(user_id)
                    .and_then(|saved| svc.prepare_persist(user_id, saved));
                state.leave(user_id);
                svc.unbind_live_slot(user_id);
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

    fn begin_character_reset(&self, user_id: Uuid, slot: i16) {
        let key = (user_id, slot);
        self.character_resets.lock_recover().insert(key);
        self.character_reset_versions
            .lock_recover()
            .entry(key)
            .and_modify(|version| *version += 1)
            .or_insert(1);
        let mut versions = self.persist_versions.lock_recover();
        versions
            .entry(key)
            .and_modify(|version| *version += 1)
            .or_insert(1);
        self.prepared_saves.lock_recover().remove(&key);
    }

    fn finish_character_reset(&self, user_id: Uuid, slot: i16) {
        self.character_resets
            .lock_recover()
            .remove(&(user_id, slot));
    }

    fn character_reset_in_progress(&self, user_id: Uuid, slot: i16) -> bool {
        self.character_resets
            .lock_recover()
            .contains(&(user_id, slot))
    }

    fn current_persist_version(&self, user_id: Uuid, slot: i16) -> u64 {
        self.persist_versions
            .lock_recover()
            .get(&(user_id, slot))
            .copied()
            .unwrap_or(0)
    }

    /// Stage one character blob for writing, targeting the slot its character
    /// was loaded from. The slot is resolved here rather than passed in, so no
    /// caller can name the wrong one: a save exists only for a character that
    /// is in the world, and that character has exactly one slot for its whole
    /// stay. Returns None when nothing is live to save (a leave that already
    /// released the binding, or a reset in flight).
    fn prepare_persist(&self, user_id: Uuid, saved: SavedCharacter) -> Option<PendingSave> {
        let slot = self.live_slot(user_id)?;
        let key = (user_id, slot);
        let resets = self.character_resets.lock_recover();
        if resets.contains(&key) {
            return None;
        }
        let mut versions = self.persist_versions.lock_recover();
        let version = versions.entry(key).and_modify(|v| *v += 1).or_insert(1);
        self.prepared_saves
            .lock_recover()
            .insert(key, (*version, saved.clone()));
        Some(PendingSave {
            user_id,
            slot,
            version: *version,
            saved,
        })
    }

    fn prepared_saved(&self, user_id: Uuid, slot: i16) -> Option<SavedCharacter> {
        self.prepared_saves
            .lock_recover()
            .get(&(user_id, slot))
            .map(|(_, saved)| saved.clone())
    }

    fn clear_prepared_save(&self, save: &PendingSave) {
        let key = (save.user_id, save.slot);
        let mut prepared_saves = self.prepared_saves.lock_recover();
        if prepared_saves
            .get(&key)
            .is_some_and(|(version, _)| *version == save.version)
        {
            prepared_saves.remove(&key);
        }
    }

    fn is_latest_persist(&self, save: &PendingSave) -> bool {
        self.persist_versions
            .lock_recover()
            .get(&(save.user_id, save.slot))
            .is_some_and(|version| *version == save.version)
    }

    fn persist_lock(&self, user_id: Uuid, slot: i16) -> Arc<Mutex<()>> {
        self.persist_locks
            .lock_recover()
            .entry((user_id, slot))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Write one character blob to the database (best-effort).
    async fn persist(&self, save: PendingSave) {
        if self.character_reset_in_progress(save.user_id, save.slot) {
            return;
        }
        if !self.is_latest_persist(&save) {
            return;
        }
        let lock = self.persist_lock(save.user_id, save.slot);
        let _guard = lock.lock().await;
        if self.character_reset_in_progress(save.user_id, save.slot) {
            return;
        }
        if !self.is_latest_persist(&save) {
            return;
        }
        match self.db.get().await {
            Ok(client) => {
                match MudCharacter::save(&client, save.user_id, save.slot, save.saved.to_json())
                    .await
                {
                    Ok(()) => self.clear_prepared_save(&save),
                    Err(error) => {
                        tracing::warn!(user_id = %save.user_id, slot = save.slot, ?error, "failed to save mud character");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(user_id = %save.user_id, slot = save.slot, ?error, "no db client for mud character save");
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
    /// autosaves; mirrors the artboard shutdown flush in main. Saves
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

    /// Commit one of the two offered archetype paths (by 0-based menu index).
    pub fn choose_archetype_task(&self, user_id: Uuid, choice: usize) {
        self.mutate(user_id, move |s| s.choose_archetype(user_id, choice));
    }

    /// Place one earned attribute point on the `choice`-th score (the point
    /// screen's 1-6, in `Score::ALL` order).
    pub fn spend_score_point_task(&self, user_id: Uuid, choice: usize) {
        self.mutate(user_id, move |s| s.spend_score_point(user_id, choice));
    }

    /// Release a lingering spirit to the temple (only when dead).
    pub fn release_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.release_to_temple(user_id));
    }

    /// Perform the Resurrection rite on the nearest corpse in the room.
    pub fn resurrect_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.resurrect_nearest(user_id));
    }

    /// Buy a companion of the given species key at the room's Stable.
    pub fn buy_pet_task(&self, user_id: Uuid, species_key: String) {
        self.mutate(user_id, move |s| s.buy_pet(user_id, &species_key));
    }

    /// Feed and tend the player's companion, wherever they stand.
    pub fn feed_pet_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.feed_pet(user_id));
    }

    /// Attempt to tame the wild beast at index `idx` in the current room's
    /// tameable list into the player's active companion.
    pub fn tame_task(&self, user_id: Uuid, idx: usize) {
        self.mutate(user_id, move |s| s.tame(user_id, idx));
    }

    /// Buy the deed to a housing plot (tier index) at the clerk.
    pub fn buy_deed_task(&self, user_id: Uuid, plot: usize) {
        self.mutate(user_id, move |s| s.buy_deed(user_id, plot));
    }

    /// Buy a furnishing and place it in the home room the player stands in.
    pub fn buy_furniture_task(&self, user_id: Uuid, key: String) {
        self.mutate(user_id, move |s| s.buy_furniture(user_id, &key));
    }

    /// Cycle appearance field `field` by `delta` (+1 / -1) on the bio builder.
    pub fn cycle_appearance_task(&self, user_id: Uuid, field: usize, delta: i8) {
        self.mutate(user_id, move |s| s.cycle_appearance(user_id, field, delta));
    }

    pub fn toggle_mount_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.toggle_mount(user_id));
    }

    pub fn move_task(&self, user_id: Uuid, dir: Dir) {
        self.mutate_preserving_frontier_warning(user_id, move |s| s.move_player(user_id, dir));
    }

    pub fn recall_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.recall(user_id));
    }

    pub fn set_waypoint_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.set_waypoint(user_id));
    }

    pub fn warp_to_waypoint_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.warp_to_waypoint(user_id));
    }

    pub fn retreat_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.retreat_to_haven(user_id));
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

    /// Work a resource node in the current room (chop/mine/fish/forage/skin).
    pub fn gather_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.gather(user_id));
    }

    /// Craft the recipe at a global index, if the station/skill/materials allow.
    pub fn craft_task(&self, user_id: Uuid, recipe_index: usize) {
        self.mutate(user_id, move |s| s.craft(user_id, recipe_index));
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

    pub fn engage_mob_task(&self, user_id: Uuid, mob_id: u32) {
        self.mutate(user_id, move |s| s.engage_mob(user_id, mob_id));
    }

    pub fn engage_player_task(&self, user_id: Uuid, target_id: Uuid) {
        self.mutate(user_id, move |s| s.engage_player(user_id, target_id));
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

    pub fn unequip_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.unequip(user_id, item_id));
    }

    pub fn use_item_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.use_item(user_id, item_id));
    }

    pub fn quaff_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| s.quaff_best(user_id));
    }

    pub fn toggle_rpg_mode_task(&self, user_id: Uuid) {
        self.mutate(user_id, move |s| {
            if let Some(p) = s.players.get_mut(&user_id) {
                p.rpg_mode = !p.rpg_mode;
                s.dirty = true; // persist the preference
            }
        });
    }

    pub fn buy_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.buy(user_id, item_id));
    }

    pub fn sell_task(&self, user_id: Uuid, item_id: u32) {
        self.mutate(user_id, move |s| s.sell(user_id, item_id));
    }

    /// Batch-sell loose inventory at a merchant (see `SellBatch`).
    pub fn sell_batch_task(&self, user_id: Uuid, kind: SellBatch) {
        self.mutate(user_id, move |s| s.sell_batch(user_id, kind));
    }

    /// Step through a waystone portal to another landing.
    pub fn travel_task(&self, user_id: Uuid, dest: RoomId) {
        self.mutate(user_id, move |s| s.travel(user_id, dest));
    }

    /// Turn in a finished counter-bounty chosen from the board's picker.
    pub fn claim_board_task(&self, user_id: Uuid, quest_id: u32) {
        self.mutate(user_id, move |s| s.claim_board_quest(user_id, quest_id));
    }

    /// Accept a bounty chosen from the board's picker.
    pub fn accept_board_task(&self, user_id: Uuid, quest_id: u32) {
        self.mutate(user_id, move |s| s.accept_board_quest(user_id, quest_id));
    }

    /// Delete one character slot. Only kicks a live session out (and clears
    /// its sessions/in-memory player) when that slot is the one actually
    /// being played right now - deleting an idle slot from the landing must
    /// never disturb a session mid-adventure on a different one.
    pub fn delete_character_task(&self, user_id: Uuid, slot: i16) {
        let svc = self.clone();
        tokio::spawn(async move {
            svc.begin_character_reset(user_id, slot);
            // "Live" means the character actually in the world came from this
            // slot, not that the landing happens to be pointing at it: a
            // cursor sitting on slot 3 must never evict the slot-0 character
            // someone is mid-fight with.
            if svc.live_slot(user_id) == Some(slot) {
                svc.clear_sessions(user_id);
                let mut state = svc.state.lock().await;
                state.delete_character(user_id);
                svc.unbind_live_slot(user_id);
                svc.publish(&state);
            }

            let lock = svc.persist_lock(user_id, slot);
            let _guard = lock.lock().await;
            match svc.db.get().await {
                Ok(client) => {
                    if let Err(error) = MudCharacter::delete_slot(&client, user_id, slot).await {
                        tracing::warn!(%user_id, slot, ?error, "failed to delete mud character");
                    }
                }
                Err(error) => {
                    tracing::warn!(%user_id, slot, ?error, "no db client for mud character delete");
                }
            }
            svc.prepared_saves.lock_recover().remove(&(user_id, slot));
            svc.character_slots_task(user_id);
            svc.finish_character_reset(user_id, slot);
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
                if state.dirty {
                    svc.publish(&state);
                    state.dirty = false;
                }
                drop(state);
                for outcome in tick.kills {
                    svc.publish_kill_outcome(outcome);
                }
            }
        });
    }

    fn publish_kill_outcome(&self, outcome: KillOutcome) {
        let Some(achievement) = outcome.achievement else {
            // Ordinary mobs *and* the regional/sub-bosses stay dashboard/quest
            // -only noise. Only the three named realm crowns (below) reach
            // #lounge — the regional bosses fall so often they buried the feed.
            self.activity.game_won_task(
                outcome.user_id,
                ActivityGame::Mud,
                Some(format!("slew {}", outcome.mob_name)),
                None,
            );
            return;
        };
        // The named-achievement bosses are the story tier: they ship to #lounge
        // via the structured `BossSlain` kind.
        self.activity
            .boss_slain_task(outcome.user_id, ActivityGame::Mud, outcome.mob_name.clone());

        let chip_svc = self.chip_svc.clone();
        let activity = self.activity.clone();
        let db = self.db.clone();
        // Which character took the crown. The world does not carry the slot,
        // so it is read here, one step after the tick that produced the kill,
        // from the same binding every save resolves against.
        let slot = self
            .live_slot(outcome.user_id)
            .unwrap_or_else(|| self.active_slot(outcome.user_id));
        tokio::spawn(async move {
            // The badge's recorded score is the crown's chip amount; every
            // crown pays now, so the fallback 0 only covers a payout-less
            // achievement, which no current crown is.
            let mut badge_score = 0_i64;
            let mut grant_badge = true;
            if let Some(pay) = achievement.payout {
                match crown_payout(&db, &chip_svc, outcome.user_id, slot, achievement, pay).await {
                    Some(amount) => badge_score = amount,
                    None => grant_badge = false,
                }
            }

            let badge = award_badge(achievement.award_category, 1);
            if grant_badge {
                match db.get().await {
                    Ok(client) => {
                        if let Err(error) = grant_unique_milestone_award(
                            &client,
                            outcome.user_id,
                            achievement.award_category,
                            badge_score,
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
        let mut snapshot = state.snapshot();
        // The "reset elsewhere" signal exists to stop a live session from
        // silently becoming a different character, so it is scoped to the slot
        // that session is actually playing - not the one the landing points at
        // (deleting an idle slot from another tab must not kick anyone). With
        // no live character there is nothing to kick, so fall back to the
        // picker's choice for a session still on its way in.
        snapshot.reset_versions = self
            .character_reset_versions
            .lock_recover()
            .iter()
            .filter(|((user_id, slot), _)| {
                *slot
                    == self
                        .live_slot(*user_id)
                        .unwrap_or_else(|| self.active_slot(*user_id))
            })
            .map(|((user_id, _), version)| (*user_id, *version))
            .collect();
        let _ = self.snapshot_tx.send(snapshot);
    }
}

/// Pay one realm crown, behind two gates at once (SHOP.md Phase 6): the
/// character persists, so a maxed one would take the easy crowns nightly
/// without the 7-day account lockout, and `d` deletes the character, so the
/// lockout alone would be a reroll farm. The character row id is what the
/// per-character half keys on.
///
/// `Some(amount)` is what the profile badge records, whether the gates paid or
/// refused. `None` means the payout could not be attempted at all, which
/// suppresses the badge too: a badge whose payout never ran leaves no way to
/// tell later whether it paid.
async fn crown_payout(
    db: &Db,
    chip_svc: &ChipService,
    user_id: Uuid,
    slot: i16,
    achievement: BossAchievement,
    pay: BossPayout,
) -> Option<i64> {
    let character_id = character_row_id(db, user_id, slot, achievement.mob_name).await?;
    let grant = chip_svc
        .credit_run_cooldown_reward_template(
            user_id,
            pay.reward_key,
            &character_id.to_string(),
            pay.chip_move,
        )
        .await;
    match grant {
        Ok(grant) if grant.credited => Some(grant.amount),
        Ok(grant) => {
            tracing::info!(
                user_id = %user_id,
                payout = grant.amount,
                boss = achievement.mob_name,
                "suppressed Lateania boss chips: this character already took the crown, or the account is inside the lockout"
            );
            Some(grant.amount)
        }
        Err(error) => {
            tracing::error!(
                ?error,
                user_id = %user_id,
                boss = achievement.mob_name,
                "failed to credit Lateania boss chips"
            );
            None
        }
    }
}

/// The character row a crown's payout keys on. `None` means the row is gone or
/// unreadable.
async fn character_row_id(db: &Db, user_id: Uuid, slot: i16, boss: &str) -> Option<Uuid> {
    let client = match db.get().await {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(
                ?error,
                user_id = %user_id,
                boss,
                "no db client for the Lateania crown payout"
            );
            return None;
        }
    };
    match MudCharacter::id_for_slot(&client, user_id, slot).await {
        Ok(Some(id)) => Some(id),
        Ok(None) => {
            tracing::error!(
                user_id = %user_id,
                slot,
                boss,
                "no Lateania character row to key the crown payout on"
            );
            None
        }
        Err(error) => {
            tracing::error!(
                ?error,
                user_id = %user_id,
                slot,
                boss,
                "failed to read the Lateania character id for the crown payout"
            );
            None
        }
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
}

struct PendingSave {
    user_id: Uuid,
    slot: i16,
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
    /// A personal waypoint the player has marked (see `set_waypoint`), warped
    /// to with `warp_to_waypoint` - the far run back from the Frontier's deep
    /// levels to Embergate (and back again) for healing/resurrecting is a real
    /// pain point without one. Persists across sessions.
    waypoint: Option<RoomId>,
    /// Every room this character has stood in, for the overhead map. Shared
    /// with the published views (`Arc::make_mut` on entry), so a snapshot costs
    /// a refcount instead of a deep copy per player.
    visited: Arc<HashSet<RoomId>>,
    target: Option<u32>,
    /// Another adventurer this character is trading blows with, in a `pvp`
    /// room (see `Room::pvp`). Distinct from `target` (mobs) so a fight with
    /// a mob and a duel with a player never collide. Cleared on death, flee,
    /// or leaving the room.
    pvp_target: Option<Uuid>,
    /// Adventurers slain in `pvp` combat, lifetime. Drives the Wildbound
    /// reaver title track (see `pvp_title_for`) and is persisted.
    pvp_kills: i64,
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
    /// Ticks until the next heal/restore consumable may be used
    /// (`QUAFF_COOLDOWN_TICKS`). Transient.
    quaff_cd: u8,
    /// Healing-over-time on self.
    self_effects: Vec<ActiveEffect>,
    /// Per-ability cooldowns: ability id -> ticks remaining.
    cooldowns: HashMap<u32, u32>,
    inventory: Vec<u32>,
    equipped: HashMap<Slot, u32>,
    /// True once the class trait's death-save has been spent this life (Warrior).
    death_save_used: bool,
    /// Rolled D&D ability scores, grown by placed points; every score feeds
    /// one mechanic, see `stats::Score::rule`.
    scores: AbilityScores,
    /// Attribute points placed so far; what is left to place is
    /// `points_earned(level)` less this, so a save can never drift.
    score_points_spent: i32,
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
    /// Index of the next uncompleted starter-chain quest; equal to
    /// `STARTER_QUESTS.len()` once the chain is done. Persisted.
    starter_stage: u8,
    /// Kills counted toward the current starter stage, when it is a slay
    /// stage. Persisted alongside.
    starter_kills: u32,
    /// The chosen archetype path (from `ARCHETYPES`), once level 10 is reached.
    archetype: Option<&'static ArchetypeDef>,
    /// The combat companion bought from a Stable; travels with and fights for
    /// the player. At most one at a time.
    pet: Option<Pet>,
    /// A stray companion won over by feeding it daily (Genesys) - lives on top
    /// of the pet above rather than replacing it; a WILDLIFE index.
    stray: Option<usize>,
    /// In-progress courting of a wild adoptable critter: (WILDLIFE index,
    /// consecutive days fed, the last day fed as a Unix day number). Reset if
    /// a day is missed; promoted to `stray` once it reaches the streak needed.
    stray_bond: Option<(usize, u32, u64)>,
    /// Chosen appearance/bio trait indices (see `appearance::FIELDS`).
    appearance: [u8; appearance::N_FIELDS],
    /// Gathering-skill xp, keyed by trade; the level is a pure function of xp.
    /// A missing entry means the trade is untrained (level 1, 0 xp).
    skills: HashMap<GatherSkill, i64>,
    /// Crafting-skill xp, keyed by trade (same shape and curve as `skills`).
    craft_skills: HashMap<CraftSkill, i64>,
    /// Total Animal Taming xp (the beastmaster trade). Its level is a pure
    /// function of this, on the same skill curve (1..=SKILL_MAX_LEVEL).
    /// Persisted (schema v14).
    taming_xp: i64,
    /// Whether the live walk-around field (RPG mode) is on for this character.
    /// A rendering preference, but persisted so it survives across sessions.
    rpg_mode: bool,
    /// When this player last spoke on a zone/world scope, for the anti-spam
    /// broadcast cooldown. Session-only.
    last_broadcast: Option<Instant>,
    /// Riding the companion (Wildbound mounts). Session-only.
    mounted: bool,
    /// A coated weapon: the coat's school, damage per tick, and strikes
    /// remaining. The poison vials and the four alchemy oils share this one
    /// slot, so applying any coat replaces the last. Each landed melee hit
    /// leaves a DoT of the coat's school (through the foe's resist/weak
    /// profile) and spends one charge. Transient.
    weapon_coat: Option<(DamageType, i32, u8)>,
    /// The friendly NPC the player is currently escorting, if any (transient).
    escort: Option<EscortState>,
    /// Transient warning gate for the start-room Frontier entrance.
    frontier_descent_pending: bool,
    /// Veteran in-place resurrections: total this adventure and how many remain.
    resurrection_cap: u8,
    resurrections_left: u8,
    /// While dead, this is the deadline at which the corpse is auto-released to
    /// the temple if no one resurrects the player and they don't release first.
    respawn_at: Option<Instant>,
    /// True while the player is a corpse awaiting resurrection or release.
    dead: bool,
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

    /// Compare a piece of gear against what is worn in its slot, as a percent
    /// power change (positive = upgrade). `None` for non-gear, an unslotted item,
    /// or the very item already equipped in that slot.
    fn compare_gear(&self, it: &super::items::Item) -> Option<i32> {
        let slot = it.slot()?;
        if self.equipped.get(&slot) == Some(&it.id) {
            return None; // this is the equipped item itself
        }
        let worn_power = self
            .equipped
            .get(&slot)
            .and_then(|id| item(*id))
            .map(|w| w.power())
            .unwrap_or(0);
        let new_power = it.power();
        if worn_power == 0 {
            // Nothing worn: any positive-power gear is a straight gain.
            return (new_power > 0).then_some(100);
        }
        Some((new_power - worn_power) * 100 / worn_power.max(1))
    }

    /// Whether a piece of gear would improve the character over what is worn.
    fn is_upgrade(&self, it: &super::items::Item) -> bool {
        self.compare_gear(it).is_some_and(|pct| pct > 0)
    }

    /// The chosen archetype's tuning percentages, or all-zero if none is picked.
    /// Returns `(attack_pct, mitigation_pct, heal_pct, max_hp_pct)`.
    fn archetype_mods(&self) -> (i32, i32, i32, i32) {
        match self.archetype {
            Some(a) => (a.attack_pct, a.mitigation_pct, a.heal_pct, a.max_hp_pct),
            None => (0, 0, 0, 0),
        }
    }

    fn max_hp(&self) -> i32 {
        let (_, hp, _) = self.equipment_mods();
        let base = self.base_max_hp
            + hp
            + self.scores.hp_bonus(self.level)
            + super::classes::milestone_hp_bonus(self.level);
        let (_, _, _, hp_pct) = self.archetype_mods();
        (base + base * hp_pct / 100).max(1)
    }

    /// The attack rating before the archetype: class curve, gear, and an
    /// active empower. Both the swing and spell power derive from it; the
    /// archetype's `attack_pct` is applied once, downstream, and the scores
    /// act on the two branches (Strength on the swing, Intelligence on spell
    /// power), never on the rating itself.
    fn attack_rating(&self) -> i32 {
        let (atk, _, _) = self.equipment_mods();
        (self.base_attack + atk + self.empower).max(1)
    }

    /// The sheet's attack: the rating with the archetype applied.
    fn attack(&self) -> i32 {
        let base = self.attack_rating();
        let (atk_pct, _, _, _) = self.archetype_mods();
        (base + base * atk_pct / 100).max(1)
    }

    /// What the Physical auto-attack lands for: the attack scaled by the
    /// calling's `auto_pct` (a Mage swings at half, a Warrior in full), then
    /// by Strength. An unclassed character cannot engage, so its swing is
    /// never read.
    fn swing(&self) -> i32 {
        let auto_pct = match self.class {
            Some(c) => c.damage_weights().auto_pct,
            None => 100,
        };
        let base = self.attack() * auto_pct / 100;
        (base + base * self.scores.swing_pct() / 100).max(1)
    }

    /// Spell power: the share of the attack rating an ability adds on top of
    /// its table magnitude (times `ability_coef_pct`). The archetype is left
    /// out here and applied once to the whole hit in `ability_damage`.
    fn spell_power(&self) -> i32 {
        self.spell_power_of(self.attack_rating())
    }

    /// Spell power for an arbitrary rating (the Empower arm feeds the rating
    /// *without* the running empower, so a buff never compounds itself),
    /// then Intelligence on top.
    fn spell_power_of(&self, rating: i32) -> i32 {
        let spell_pct = match self.class {
            Some(c) => c.damage_weights().spell_pct,
            None => 0,
        };
        let power = rating * spell_pct / 100;
        power + power * self.scores.spell_power_pct() / 100
    }

    /// Resource regained per tick: the class regen plus Wisdom, never below 1.
    fn regen(&self) -> i32 {
        (self.resource_regen + self.scores.regen_bonus()).max(1)
    }

    /// What a shop charges this character for `it`: the list price less the
    /// Charisma discount (or plus its markup), never below 1 gold.
    fn buy_price(&self, it: &Item) -> i64 {
        let pct = self.scores.price_pct() as i64;
        (it.price - it.price * pct / 100).max(1)
    }

    /// What a merchant pays this character for `it`: half the list price plus
    /// the Charisma premium (or less its penalty), never below 1 gold.
    fn sell_price(&self, it: &Item) -> i64 {
        let base = it.sell_price();
        let pct = self.scores.price_pct() as i64;
        (base + base * pct / 100).max(1)
    }

    /// Attribute points earned by level and not yet placed, never more than
    /// the scores can still take (`AbilityScores::headroom`): a point with no
    /// slot to go in is not owed, so the point screen, which holds every key
    /// until the point is placed, can always be satisfied.
    fn score_points(&self) -> i32 {
        (points_earned(self.level) - self.score_points_spent)
            .max(0)
            .min(self.scores.headroom())
    }

    /// The point screen's rows while a point waits to be placed: every score
    /// with what it does now and what it would do after the point. Empty when
    /// there is nothing to place, while the archetype crossroads is open (so
    /// the two screens never fight for the keys), and while dead (a corpse
    /// sees the corpse view and its release key, and places the point once
    /// it rises).
    fn score_offer(&self) -> Vec<ScoreOfferView> {
        let crossroads = self.archetype.is_none() && self.level >= ARCHETYPE_LEVEL;
        if self.class.is_none() || crossroads || self.dead || self.score_points() <= 0 {
            return Vec::new();
        }
        Score::ALL
            .iter()
            .map(|&which| {
                let value = self.scores.score(which);
                let mut raised = self.scores;
                let after = raised
                    .raise(which)
                    .then(|| raised.effect(which, self.level));
                ScoreOfferView {
                    label: which.label().to_string(),
                    name: which.name().to_string(),
                    value,
                    modifier: modifier(value),
                    now: self.scores.effect(which, self.level),
                    after,
                    rule: which.rule().to_string(),
                }
            })
            .collect()
    }

    fn armor(&self) -> i32 {
        let (_, _, armor) = self.equipment_mods();
        armor
    }

    /// True while trading blows with a mob or another adventurer. Movement,
    /// recall, mounting, and waypoints all gate on this, same as a plain mob
    /// fight - a pvp duel holds you in place exactly like combat always has.
    fn in_combat(&self) -> bool {
        self.target.is_some() || self.pvp_target.is_some()
    }

    /// Total xp trained in a gathering skill (0 if untrained).
    fn skill_xp(&self, skill: GatherSkill) -> i64 {
        self.skills.get(&skill).copied().unwrap_or(0)
    }

    /// Total xp trained in a crafting skill (0 if untrained).
    fn craft_xp(&self, skill: CraftSkill) -> i64 {
        self.craft_skills.get(&skill).copied().unwrap_or(0)
    }

    /// Current Animal Taming level (1 if untrained).
    fn taming_level(&self) -> i32 {
        skill_level_for_xp(self.taming_xp)
    }

    /// How many of an item id sit in the pack.
    fn item_count(&self, id: u32) -> u32 {
        self.inventory.iter().filter(|&&i| i == id).count() as u32
    }

    /// Remove up to `n` copies of an item id from the pack.
    fn consume(&mut self, id: u32, mut n: u32) {
        self.inventory.retain(|&x| {
            if n > 0 && x == id {
                n -= 1;
                false
            } else {
                true
            }
        });
    }
}

// ---- Board quests: objectives, repeats, escorts, the bounty table --------

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
    /// Where the work is and how to walk there, in plain words. The blurb sets
    /// the scene; this answers "so where do I actually go".
    hint: &'static str,
    /// A rough level at which the bounty is a fair fight, shown on the board
    /// so a fresh adventurer can tell a chore from a death sentence.
    suggested_level: i32,
    /// Gate titles the bounty's hunting ground sits behind (empty when the
    /// ground is open country). A player missing any of them sees the posting
    /// sealed and cannot accept it.
    requires: &'static [&'static str],
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
        hint: "The crypt mouth opens from Tasmania's own square - but the living dark needs the Archdemon's fall before it will let you in.",
        suggested_level: 32,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Relics drop from the dead of the Sunken Catacombs, entered from Tasmania's square once the Archdemon has fallen.",
        suggested_level: 32,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "The way down lies in Tasmania's square itself; it opens only to a Bane of the Archdemon.",
        suggested_level: 30,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "The Thornwood opens from Melvanala's lakeside square - post-Archdemon country; its packs are no roadside wolves.",
        suggested_level: 32,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Spoils drop from the beasts and fae of the Thornwood Hollows, below Melvanala's square, once the Archdemon has fallen.",
        suggested_level: 32,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "The Hollows open from Melvanala's square, to a Bane of the Archdemon.",
        suggested_level: 30,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "The flooded caves open from Matlatesh's square - post-Archdemon country, the hardest of the three living darks.",
        suggested_level: 34,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Salvage drops from the aberrations of the Drowned Caverns, below Matlatesh's square, once the Archdemon has fallen.",
        suggested_level: 34,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "The Caverns open from Matlatesh's square, to a Bane of the Archdemon.",
        suggested_level: 30,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Brother Aldric waits by this board; the Catacombs he must reach open from Tasmania's square, past the Archdemon's gate.",
        suggested_level: 33,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Mira waits by this board; the Hollows she studies open from Melvanala's square, past the Archdemon's gate.",
        suggested_level: 33,
        requires: &[FRONTIER_GATE_TITLE],
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
        hint: "Old Pell waits by this board; the Caverns he would dive open from Matlatesh's square, past the Archdemon's gate.",
        suggested_level: 35,
        requires: &[FRONTIER_GATE_TITLE],
    },
    // ---- The Sundered Reaches (off Matlatesh) ----------------------------
    BoardQuest {
        id: 13,
        board: super::world::MATLATESH_SQUARE,
        title: "Stem the Drowned Tide",
        objective: Objective::Bounty {
            name_contains: "drowned",
            count: 6,
        },
        reward_gold: 360,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "The Reaches vomit up their dead onto the shore. Put six of the drowned down again.",
        hint: "The drowned walk the Drowned Crypts on the old road below Duskhollow - and thicker still in the Reaches, for those who hold the sea-gate.",
        suggested_level: 12,
        requires: &[],
    },
    BoardQuest {
        id: 14,
        board: super::world::MATLATESH_SQUARE,
        title: "Lay the Revenants",
        objective: Objective::Bounty {
            name_contains: "revenant",
            count: 5,
        },
        reward_gold: 400,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Restless revenants stalk the sunken cities. Lay five of them to their long rest.",
        hint: "Revenants stalk the Drowned Crypts below Duskhollow and the frozen heights of Frostspire, on the old road east of Embergate.",
        suggested_level: 14,
        requires: &[],
    },
    BoardQuest {
        id: 15,
        board: super::world::MATLATESH_SQUARE,
        title: "The Sea-Gate",
        objective: Objective::Reach {
            zone: "The Saltmarsh Shallows",
        },
        reward_gold: 140,
        reward_title: Some("Reach-Walker"),
        repeat: Repeat::Once,
        blurb: "A drowned realm lies beyond the desert's edge. Pass the sea-gate and set foot in it.",
        hint: "The sea-gate stands in Matlatesh's shallows; it opens only to a Bane of the King Who Was Promised Nothing.",
        suggested_level: 52,
        requires: &[REACHES_GATE_TITLE],
    },
    BoardQuest {
        id: 16,
        board: super::world::MATLATESH_SQUARE,
        title: "Sound the Deepest Dark",
        objective: Objective::Reach {
            zone: "The Sundering Deep",
        },
        reward_gold: 600,
        reward_title: Some("Sounder of the Deep"),
        repeat: Repeat::Once,
        blurb: "Few return from the floor of all seas. Reach the Sundering Deep and prove it can be done.",
        hint: "The Sundering Deep is the floor of the Sundered Reaches - twenty zones down from the sea-gate.",
        suggested_level: 60,
        requires: &[REACHES_GATE_TITLE],
    },
    // ---- Kaelmyr, the Ashen Reach (the ash-cairn board, off Yssgar) -------
    BoardQuest {
        id: 17,
        board: super::world::KAELMYR_BASE,
        title: "Cross the Ash-Gate",
        objective: Objective::Reach {
            zone: "The Cinderfall Shore",
        },
        reward_gold: 300,
        reward_title: Some("Ash-Walker"),
        repeat: Repeat::Once,
        blurb: "A burnt continent lies below the drowned wound. Descend the ash-gate and set foot on Kaelmyr.",
        hint: "The ash-gate descends from Yssgar's drowned chamber at the bottom of the Reaches; it opens only to a Bane of Yssgar.",
        suggested_level: 62,
        requires: &[KAELMYR_GATE_TITLE],
    },
    BoardQuest {
        id: 18,
        board: super::world::KAELMYR_BASE,
        title: "Salt the Cinder-Dead",
        objective: Objective::Bounty {
            name_contains: "revenant",
            count: 6,
        },
        reward_gold: 700,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "The Reaches' dead wash up and rise again on the burnt strand. Put six of the cinder-dead down.",
        hint: "The cinder-dead shamble along Kaelmyr's Cinderfall Shore, just past the ash-gate.",
        suggested_level: 64,
        requires: &[KAELMYR_GATE_TITLE],
    },
    BoardQuest {
        id: 19,
        board: super::world::KAELMYR_BASE,
        title: "Break the Emberkin Rite",
        objective: Objective::Bounty {
            name_contains: "Emberkin",
            count: 4,
        },
        reward_gold: 760,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "The ash-shamans keep their pyres lit with the living. Scatter four of the Emberkin from the terraces.",
        hint: "The Emberkin keep their pyres in the caldera terraces west of the Cinderfall Shore.",
        suggested_level: 66,
        requires: &[KAELMYR_GATE_TITLE],
    },
    BoardQuest {
        id: 20,
        board: super::world::KAELMYR_BASE,
        title: "Ashen Salvage",
        objective: Objective::Collect {
            item: super::items::KAELMYR_SHORE_RELIC_ID,
            count: 3,
        },
        reward_gold: 720,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "Relics of the world's first age wash up on the cinder shore. Bring back three from Kaelmyr.",
        hint: "Shore relics drop from the dead along Kaelmyr's Cinderfall Shore, past the ash-gate.",
        suggested_level: 64,
        requires: &[KAELMYR_GATE_TITLE],
    },
    BoardQuest {
        id: 21,
        board: super::world::KAELMYR_BASE,
        title: "Reach the Ashen King",
        objective: Objective::Reach {
            zone: "The Unquenched Throne",
        },
        reward_gold: 1200,
        reward_title: Some("Throne-Seeker of Kaelmyr"),
        repeat: Repeat::Once,
        blurb: "Kaethyr the Unquenched has ruled the ash since the Sundering. Walk to his burning throne and look upon it.",
        hint: "The Unquenched Throne stands near Kaelmyr's far end - a long march east and down through the ash.",
        suggested_level: 75,
        requires: &[KAELMYR_GATE_TITLE],
    },
    BoardQuest {
        id: 22,
        board: super::world::KAELMYR_BASE,
        title: "Silence the Hollow Choir",
        objective: Objective::Bounty {
            name_contains: "Choir",
            count: 4,
        },
        reward_gold: 820,
        reward_title: None,
        repeat: Repeat::Daily,
        blurb: "The Hollow Choir sings to wake the drowned god beneath the wound. Silence four of the choristers.",
        hint: "The Hollow Choir sings in Kaelmyr's deepest zones, on the way to the Sundering Wound.",
        suggested_level: 78,
        requires: &[KAELMYR_GATE_TITLE],
    },
];

fn board_quest(id: u32) -> Option<&'static BoardQuest> {
    BOARD_QUESTS.iter().find(|q| q.id == id)
}

// ---- The starter chain and the Long Road ---------------------------------

/// A goal for one starter-chain step. Separate from `Objective` because the
/// chain needs "slay anything in a zone", which boards never ask for.
#[derive(Clone, Copy, Debug)]
enum StarterGoal {
    /// Set foot in the named zone.
    Reach { zone: &'static str },
    /// Slay this many foes anywhere in the named zone.
    SlayIn { zone: &'static str, count: u32 },
    /// Slay one foe whose name contains this fragment.
    SlayNamed { name_contains: &'static str },
}

fn starter_goal_target(goal: StarterGoal) -> u32 {
    match goal {
        StarterGoal::Reach { .. } | StarterGoal::SlayNamed { .. } => 1,
        StarterGoal::SlayIn { count, .. } => count,
    }
}

/// One step of the auto-granted new-player chain. Sequential: exactly one is
/// active at a time, finishing it opens the next, and the whole chain hands a
/// fresh character from Wayfarer's Hollow to the first real gate title.
struct StarterQuest {
    title: &'static str,
    goal: StarterGoal,
    /// Where to go and what to do, in plain words - shown in the journal and
    /// as the room panel's "next" line.
    hint: &'static str,
    /// The room the step points at, for tracking on the world map.
    target: RoomId,
    reward_gold: i64,
    reward_xp: i64,
}

const STARTER_QUESTS: &[StarterQuest] = &[
    StarterQuest {
        title: "First Steps",
        goal: StarterGoal::Reach { zone: "Embergate" },
        hint: "Leave Wayfarer's Hollow for Embergate proper: press r to recall to the Town Square, or walk south through the Gilded Flagon.",
        target: 1,
        reward_gold: 25,
        reward_xp: 20,
    },
    StarterQuest {
        title: "The Open Road",
        goal: StarterGoal::SlayIn {
            zone: "King's Road",
            count: 3,
        },
        hint: "Head south past the South Gate. Goblins, bandits and gaunt wolves prowl the King's Road - put down three of them.",
        target: 6,
        reward_gold: 40,
        reward_xp: 40,
    },
    StarterQuest {
        title: "Under the Eaves",
        goal: StarterGoal::Reach {
            zone: "Whisperwood",
        },
        hint: "Follow the King's Road south until the trees close in and the Whisperwood begins.",
        target: 11,
        reward_gold: 40,
        reward_xp: 40,
    },
    StarterQuest {
        title: "The Elder Treant",
        goal: StarterGoal::SlayNamed {
            name_contains: "Elder Treant",
        },
        hint: "Deep in Whisperwood the Elder Treant keeps the way down into Duskhollow. Bring it down, and its leave to descend is yours.",
        target: 28,
        reward_gold: 80,
        reward_xp: 120,
    },
    StarterQuest {
        title: "Into the Dark Below",
        goal: StarterGoal::Reach {
            zone: "Duskhollow Caverns",
        },
        hint: "Descend past the Treant's grove into Duskhollow Caverns. From here the deeps chain onward, boss by boss.",
        target: 31,
        reward_gold: 60,
        reward_xp: 80,
    },
];

fn starter_quest(stage: u8) -> Option<&'static StarterQuest> {
    STARTER_QUESTS.get(stage as usize)
}

/// One milestone of the Long Road: the realm's spine of great bosses. `boss`
/// must match the spawn's name exactly - the view derives each milestone's
/// required title via `title_for`, so the roadmap can never drift from what a
/// kill actually grants (a drift test pins the gate consts to this table).
struct RoadMilestone {
    boss: &'static str,
    place: &'static str,
    unlocks: &'static str,
}

const LONG_ROAD: &[RoadMilestone] = &[
    RoadMilestone {
        boss: "the Elder Treant",
        place: "Whisperwood",
        unlocks: "the descent into Duskhollow",
    },
    RoadMilestone {
        boss: "the Archdemon Mal'gareth",
        place: "the Obsidian Throne, at the authored road's end",
        unlocks: "the living dark below the three capitals",
    },
    RoadMilestone {
        boss: "The Bonewright Lich",
        place: "the Sunken Catacombs, below Tasmania",
        unlocks: "one of the three Frontier seals",
    },
    RoadMilestone {
        boss: "the Elder Dryad",
        place: "the Thornwood Hollows, below Melvanala",
        unlocks: "one of the three Frontier seals",
    },
    RoadMilestone {
        boss: "the Abyss-Thing",
        place: "the Drowned Caverns, below Matlatesh",
        unlocks: "one of the three Frontier seals",
    },
    RoadMilestone {
        boss: "the King Who Was Promised Nothing",
        place: "the Frontier's deepest zone",
        unlocks: "the sea-gate into the Sundered Reaches",
    },
    RoadMilestone {
        boss: "Yssgar, the Sundering Deep",
        place: "the deepest chamber of the Sundered Reaches",
        unlocks: "the ash-gate down into Kaelmyr",
    },
    RoadMilestone {
        boss: "Kaethyr the Unquenched, Ashen King of Kaelmyr",
        place: "the Unquenched Throne",
        unlocks: "",
    },
    RoadMilestone {
        boss: "Kaethyr Ascendant, Who Sang the God Awake",
        place: "the Sundering Wound",
        unlocks: "the last crown of the realm",
    },
];

/// The one line that always answers "where do I go now": the active starter
/// step, else the Long Road's first unconquered milestone. None only once the
/// realm is fully conquered.
fn next_step_for(starter_stage: u8, titles: &[String]) -> Option<String> {
    if let Some(q) = starter_quest(starter_stage) {
        return Some(format!("{}: {}", q.title, q.hint));
    }
    LONG_ROAD
        .iter()
        .find(|m| !titles.iter().any(|t| *t == title_for(m.boss, true)))
        .map(|m| format!("bring down {} in {}", m.boss, m.place))
}

/// The Long Road rows for a set of earned titles: each milestone checked off
/// by its boss title, the first undone one flagged as current. `targets` is
/// the per-milestone lair room (see `road_targets`), parallel to `LONG_ROAD`.
fn road_view(titles: &[String], targets: &[Option<RoomId>]) -> Vec<RoadStepView> {
    let mut current_found = false;
    LONG_ROAD
        .iter()
        .zip(targets.iter().copied().chain(std::iter::repeat(None)))
        .map(|(m, target)| {
            let title = title_for(m.boss, true);
            let done = titles.contains(&title);
            let current = !done && !current_found;
            if current {
                current_found = true;
            }
            RoadStepView {
                boss: m.boss.to_string(),
                place: m.place,
                unlocks: m.unlocks,
                done,
                current,
                target,
            }
        })
        .collect()
}

/// Each Long Road milestone's lair: the home room of the spawn whose name the
/// milestone carries. Computed once at world build; the drift test pins every
/// milestone to a real spawn, so a `None` here means the table rotted.
fn road_targets(world: &World) -> Vec<Option<RoomId>> {
    LONG_ROAD
        .iter()
        .map(|m| {
            world
                .spawns
                .iter()
                .find(|s| s.name == m.boss)
                .map(|s| s.home)
        })
        .collect()
}

/// One board posting's picker-menu row, for a `BoardView`.
fn board_entry(q: &BoardQuest, ready: bool, locked: bool) -> BoardEntryView {
    BoardEntryView {
        quest_id: q.id,
        title: q.title.to_string(),
        blurb: q.blurb.to_string(),
        objective: q.objective.describe(),
        reward: format!(
            "{} gold{}",
            q.reward_gold,
            match q.reward_title {
                Some(t) => format!(" + title: {t}"),
                None => String::new(),
            }
        ),
        ready,
        hint: q.hint.to_string(),
        suggested_level: q.suggested_level,
        locked,
    }
}

/// True when the bounty's hunting ground sits behind a gate title the player
/// does not hold: the posting shows sealed and cannot be accepted, so a fresh
/// adventurer is never handed work in a land that will refuse them the door.
fn board_quest_locked(q: &BoardQuest, titles: &[String]) -> bool {
    !titles_include_all(titles, q.requires)
}

// ---- Live mobs, and the world state they live in -------------------------

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
    /// Consecutive ticks this mob has been wounded, stunned, or festering with
    /// nobody targeting it. At `MOB_RESET_TICKS` it recovers in full. Reset
    /// to zero whenever a player holds it as a target.
    untargeted: u8,
}

/// Where a damage-over-time stack came from. The two behave differently on
/// re-application and the difference is load-bearing: an ability DoT is one
/// wound per cast and stacks, because a cooldown rations how often it can be
/// cast. A weapon coat re-seeds on *every* landed strike, at the same cadence
/// the DoT itself ticks, so stacking it would multiply the coat's rider by its
/// duration (a 3-tick DoT reseeded every tick pays three times over). A coat
/// therefore keeps exactly one stack per attacker and refreshes it in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotSource {
    Ability,
    Coat,
}

/// One live damage-over-time stack on a mob. `per_tick` already has the
/// target's resist/weak baked in (see `seed_mob_dot`).
#[derive(Clone, Copy, Debug)]
struct MobDot {
    owner: Uuid,
    per_tick: i32,
    remaining: u8,
    source: DotSource,
}

/// One live damage-over-time stack on a player. Unlike `MobDot` this carries
/// its school live, since every tick re-applies the victim's armor.
#[derive(Clone, Copy, Debug)]
struct PvpDot {
    owner: Uuid,
    per_tick: i32,
    school: DamageType,
    remaining: u8,
    source: DotSource,
}

struct WorldState {
    room_id: Uuid,
    world: World,
    /// Each Long Road milestone's lair room, parallel to `LONG_ROAD` (see
    /// `road_targets`). Computed once here so snapshots never scan the spawns.
    road_targets: Vec<Option<RoomId>>,
    players: HashMap<Uuid, PlayerState>,
    mobs: HashMap<u32, MobInstance>,
    /// mob id -> stun ticks remaining.
    mob_stuns: HashMap<u32, u8>,
    /// mob id -> active damage-over-time stacks.
    mob_dots: HashMap<u32, Vec<MobDot>>,
    /// Pvp equivalents of `mob_stuns`/`mob_dots`, keyed by the victim's user
    /// id instead of a mob id (see `strike_pvp_target`/`seed_pvp_dot`). Each
    /// dot stack also carries its `DamageType`, since (unlike a mob's baked-in
    /// resist/weak) a player's `strike_player` needs the real school on every
    /// tick to apply the right armor reduction.
    pvp_stuns: HashMap<Uuid, u8>,
    pvp_dots: HashMap<Uuid, Vec<PvpDot>>,
    /// Kills accumulated during a tick, drained for the activity feed.
    pending_kills: Vec<KillOutcome>,
    generation: u64,
    dirty: bool,
    world_dirty: bool,
    world_revision: u64,
    /// Hunt cooldowns for `Game` critters, keyed by global WILDLIFE index.
    hunted: HashMap<usize, Instant>,
    /// Harvest cooldowns for resource nodes, keyed by global NODES index.
    gathered: HashMap<usize, Instant>,
    /// Per-player, per-beast cooldown after a *failed* tame: (user, beast index)
    /// -> when it bolted. A spooked beast won't be approached again for a spell.
    tame_cooldowns: HashMap<(Uuid, usize), Instant>,
    /// Pet auto-skill cooldowns: (user, pet-skill index) -> the `world_ticks`
    /// value at which that skill may next fire. Transient (combat-round timing).
    pet_skill_cd: HashMap<(Uuid, usize), u64>,
    /// Next id for a runtime-only summoned add (Summoner behavior). Kept well
    /// clear of authored spawn ids so the two never collide.
    next_summon_id: u32,
    /// The world heartbeat, in ticks. Drives time-of-day and weather.
    world_ticks: u64,
    /// The active wandering world boss, if one currently roams.
    world_boss: Option<u32>,
    /// Tick at which the next world boss may rise.
    next_world_boss_tick: u64,
    /// Who holds the deed to each housing plot (keyed by tier/plot index).
    plot_owner: HashMap<usize, Uuid>,
    /// Furnishings placed in each home room (keyed by room id).
    house_furniture: HashMap<RoomId, Vec<&'static super::housing::Furniture>>,
}

const LOG_CAP: usize = 60;
const SAVED_HOUSE_FURNITURE_LIMIT: usize = 512;
const TEMPLE_ROOM: RoomId = 4;
/// How long a hunted game critter stays gone before it wanders back.
const GAME_RESPAWN: Duration = Duration::from_secs(40);
/// How long a harvested resource node stays depleted before it regrows.
const NODE_RESPAWN: Duration = Duration::from_secs(45);
/// How long a beast stays spooked (and un-approachable) after a failed tame.
const TAME_COOLDOWN: Duration = Duration::from_secs(30);
/// Minimum gap between one player's zone/world broadcasts. Room speech is
/// self-limiting (only co-located players hear it); a global channel needs a
/// brake or one voice can flood every log in Lateania.
const BROADCAST_COOLDOWN: Duration = Duration::from_secs(10);
/// The auto-attack bar a tier-`t` coat is measured against: `attack()` for a
/// character at that tier's crafting gate wearing that tier's crafted weapon.
/// Measured from the engine, not guessed - `the_attack_bar_still_matches_a_real
/// _character` rebuilds a real player at each gate and pins every entry. The
/// coat curves below and the world pass's grind-rate budget are both written as
/// a share of this, so neither can drift away from what a fight actually looks
/// like (they once did: the oil rider was certified at 15% of output while
/// really running three to six times that).
#[cfg(test)]
pub(super) const TIER_ATTACK_BAR: [i32; 6] = [12, 31, 52, 77, 106, 148];
/// Character level at each crafting tier's gate, the level `TIER_ATTACK_BAR` is
/// measured at (mirrors `crafting::LEVEL_REQ`).
#[cfg(test)]
pub(super) const TIER_GATE_LEVEL: [i32; 6] = [1, 8, 16, 26, 38, 55];
/// Poison damage per tick applied by a coated weapon, by poison tier (0..6).
/// The burst half of the coat family: about 30% of the auto bar but only
/// `POISON_CHARGES` strikes of it, so a vial is roughly three quarters of an
/// oil's damage packed into half the window. Cheap, and the right answer when
/// the fight will be over quickly.
pub(super) const POISON_PER_TICK: [i32; 6] = [3, 9, 15, 23, 32, 45];
/// Strikes a single weapon-coating lasts before the poison is spent.
pub(super) const POISON_CHARGES: u8 = 5;
/// Ticks each coated strike (poison or oil) festers in the foe. A coat re-seeds
/// every landed strike and refreshes rather than stacks (see `DotSource`), so
/// this is the wound's lifetime after the last swing, not a multiplier on it.
pub(super) const POISON_DOT_TICKS: u8 = 3;
/// Oil damage per tick, by oil tier (0..6). The sustain half: about a fifth of
/// the auto bar, held for `OIL_CHARGES` strikes, which is the whole of a boss
/// fight. Sized so a coated character gains roughly 15% of total output, the
/// figure the world pass's routed budget is written against.
pub(super) const OIL_PER_TICK: [i32; 6] = [2, 6, 10, 15, 21, 30];
/// Strikes a single oil coating lasts. Several fights' worth, so choosing an
/// oil is a route decision made at the zone gate, not per-fight busywork.
pub(super) const OIL_CHARGES: u8 = 12;
/// Share of a character's output that comes from the Physical auto-attack at
/// band gear; the rest is abilities in the class's school mix. The routed
/// grind-rate budget in `world_test.rs` splits output this way, and the coat
/// rider converts through it (a rider worth 20% of the auto is worth
/// `0.20 * AUTO_SHARE` of total output).
#[cfg(test)]
pub(super) const AUTO_SHARE: f64 = 0.75;
/// Ticks a cooked meal's well-fed regen lasts.
const WELL_FED_TICKS: u8 = 8;

/// Percent of the caster's spell power an ability adds to its table
/// magnitude, by effect. Instant hits get the most, a finisher more still,
/// control less, and the over-time effects a slice per tick (a 5-tick DoT at
/// 30% lands 150% over its life, rationed by its cooldown). Heals, wards and
/// empowers scale the same way, so a level-55 Mend is not a level-3 Mend
/// with better gear. The table magnitude stays the flat floor every ability
/// keeps at level 1.
const fn ability_coef_pct(effect: AbilityEffect) -> i32 {
    match effect {
        AbilityEffect::Strike => 100,
        AbilityEffect::Finisher => 150,
        AbilityEffect::Stun => 50,
        AbilityEffect::DamageOverTime => 30,
        AbilityEffect::Heal => 50,
        AbilityEffect::HealOverTime => 20,
        AbilityEffect::Ward => 60,
        AbilityEffect::Empower => 25,
    }
}

impl WorldState {
    // ---- Construction, the world clock, and broadcast -------------------

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
                        untargeted: 0,
                        spawn: spawn.clone(),
                    },
                )
            })
            .collect();
        let road_targets = road_targets(&world);
        Self {
            room_id,
            world,
            road_targets,
            players: HashMap::new(),
            mobs,
            mob_stuns: HashMap::new(),
            mob_dots: HashMap::new(),
            pvp_stuns: HashMap::new(),
            pvp_dots: HashMap::new(),
            pending_kills: Vec::new(),
            generation: 0,
            dirty: false,
            world_dirty: false,
            world_revision: 0,
            hunted: HashMap::new(),
            gathered: HashMap::new(),
            tame_cooldowns: HashMap::new(),
            pet_skill_cd: HashMap::new(),
            next_summon_id: SUMMON_ID_START,
            world_ticks: 0,
            world_boss: None,
            next_world_boss_tick: WORLD_BOSS_FIRST_TICK,
            plot_owner: HashMap::new(),
            house_furniture: HashMap::new(),
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

    // ---- Joining, class choice, and character reset ---------------------

    fn join(&mut self, user_id: Uuid) -> bool {
        if self.players.contains_key(&user_id) {
            return false;
        }
        // Brand-new characters land in Wayfarer's Hollow, the tutorial zone -
        // never `World::start_room` directly, which stays Embergate's square
        // so map anchoring, recall, and every "home is room 1" assumption
        // elsewhere is untouched. A returning character's saved room (from
        // `hydrate`) is unaffected by this.
        let start = tutorial_start_room();
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
            waypoint: None,
            visited: Arc::new(HashSet::from([start])),
            target: None,
            pvp_target: None,
            pvp_kills: 0,
            following: None,
            opening_strike: false,
            empower: 0,
            empower_ticks: 0,
            shield: 0,
            shield_ticks: 0,
            stunned: 0,
            quaff_cd: 0,
            self_effects: Vec::new(),
            cooldowns: HashMap::new(),
            inventory: vec![1000, 1300, 1300], // a rusty sword and two minor draughts
            equipped: HashMap::new(),
            death_save_used: false,
            scores: AbilityScores::roll(),
            score_points_spent: 0,
            titles: Vec::new(),
            title_levels: Vec::new(),
            active_title: None,
            completed_quests: Vec::new(),
            board_progress: Vec::new(),
            board_done: Vec::new(),
            quest_cooldowns: Vec::new(),
            starter_stage: 0,
            starter_kills: 0,
            archetype: None,
            pet: None,
            stray: None,
            stray_bond: None,
            appearance: [0; appearance::N_FIELDS],
            skills: HashMap::new(),
            craft_skills: HashMap::new(),
            taming_xp: 0,
            rpg_mode: true,
            last_broadcast: None,
            mounted: false,
            weapon_coat: None,
            escort: None,
            frontier_descent_pending: false,
            resurrection_cap: 0,
            resurrections_left: 0,
            respawn_at: None,
            dead: false,
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
            "Welcome to Wayfarer's Hollow, a safe place to learn your trade before the real world asks anything of you. Explore it at your own pace - press r anytime to leave for Embergate, the real town, whenever you're ready."
                .to_string(),
        );
        // The chain's first step, so a brand-new player has a concrete goal
        // from their very first breath (it also rides the journal and the
        // room panel's Next line from here on).
        if let Some(q) = starter_quest(0) {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Next - {}: {}", q.title, q.hint),
            );
        }
        self.describe_room(user_id);
    }

    /// Commit an archetype path at level 10. `choice` indexes the per-class
    /// offer list (`archetypes_for`); ignored if already chosen, unclassed, or
    /// below the eligibility level. Re-derives HP so the bonus takes effect now.
    fn choose_archetype(&mut self, user_id: Uuid, choice: usize) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        let Some(class) = p.class else { return };
        if p.archetype.is_some() || p.level < ARCHETYPE_LEVEL {
            return;
        }
        let offers = super::classes::archetypes_for(class);
        let Some(def) = offers.get(choice).copied() else {
            return;
        };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.archetype = Some(def);
            // The max-HP bonus may have lifted the ceiling; top up to it.
            p.hp = p.max_hp();
        }
        self.log_to(
            user_id,
            LogKind::System,
            format!(
                "You embrace the path of the {}, a {} calling.",
                def.name,
                def.role.label(),
            ),
        );
        self.describe_room(user_id);
    }

    /// Place one earned attribute point on `Score::ALL[choice]`. Nothing
    /// happens with no point to place, and a score at `SCORE_CAP` says so and
    /// keeps the point.
    fn spend_score_point(&mut self, user_id: Uuid, choice: usize) {
        let Some(which) = Score::ALL.get(choice).copied() else {
            return;
        };
        let placed = match self.players.get_mut(&user_id) {
            Some(p) if p.class.is_some() && p.score_points() > 0 => {
                if p.scores.raise(which) {
                    p.score_points_spent += 1;
                    Some((
                        p.scores.score(which),
                        p.scores.effect(which, p.level),
                        p.score_points(),
                    ))
                } else {
                    None
                }
            }
            _ => return,
        };
        match placed {
            Some((value, effect, left)) => {
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    format!("{} rises to {value}: {effect}.", which.name()),
                );
                if left > 0 {
                    self.log_to(
                        user_id,
                        LogKind::System,
                        format!("{left} attribute point(s) still to place."),
                    );
                }
            }
            None => {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("{} is already at its peak of {SCORE_CAP}.", which.name()),
                );
            }
        }
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
            stacks.retain(|dot| dot.owner != user_id);
        }
        self.mob_dots.retain(|_, stacks| !stacks.is_empty());
        let after: usize = self.mob_dots.values().map(Vec::len).sum();
        if after != before {
            self.mark_world_dirty();
        }
        self.dirty = true;
    }

    // ---- Persistence: hydrate a save, export one, the shared world ------

    /// Apply a saved character onto a freshly-joined player. Restores class,
    /// progression, gold, gear, inventory, and the room they logged out in.
    /// Nothing hostile acts on its own here (every fight is player-started and
    /// no combat state is saved), so coming back where you stood is safe; only
    /// a room that no longer exists falls back to the start room.
    fn hydrate(&mut self, user_id: Uuid, saved: &SavedCharacter) {
        let Some(class) = saved.class() else {
            // No class chosen last time; leave the player at the select screen.
            return;
        };
        let xp = saved.xp.max(0);
        let saved_level = saved.level.clamp(1, Class::MAX_LEVEL);
        let level = saved_level.max(level_for_xp(xp)).clamp(1, Class::MAX_LEVEL);
        let stats = class.stats_at(level);
        let room = match self.world.room(saved.room) {
            Some(_) => saved.room,
            None => self.world.start_room,
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
            // A stale waypoint (a room that no longer exists) is simply dropped.
            p.waypoint = saved.waypoint.filter(|&r| self.world.room(r).is_some());
            p.visited = Arc::new(saved.visited.iter().copied().collect());
            Arc::make_mut(&mut p.visited).insert(room);
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
            // Rolled scores, placed points, and earned titles persist across
            // sessions. Points are bounded by what the level has earned, so a
            // character saved before the points existed simply has them all
            // still to place.
            p.scores = saved.scores;
            p.score_points_spent = saved.score_points_spent.clamp(0, points_earned(level));
            p.titles = saved.titles.clone();
            p.title_levels = saved.title_levels.clone();
            p.title_levels.resize(p.titles.len(), 1);
            p.active_title = saved.active_title.filter(|&i| i < p.titles.len());
            p.completed_quests = saved.completed_quests.clone();
            p.board_progress = saved.board_progress.clone();
            p.board_done = saved.board_done.clone();
            p.quest_cooldowns = saved.quest_cooldowns.clone();
            // Restore gathering-skill xp (unknown keys are dropped, so retiring a
            // trade never breaks a save).
            p.skills = saved
                .skills
                .iter()
                .filter_map(|(key, xp)| GatherSkill::from_key(key).map(|s| (s, *xp)))
                .collect();
            p.craft_skills = saved
                .craft_skills
                .iter()
                .filter_map(|(key, xp)| CraftSkill::from_key(key).map(|s| (s, *xp)))
                .collect();
            // Restore Animal Taming xp (0 for pre-taming saves).
            p.taming_xp = saved.taming_xp.max(0);
            // Restore lifetime PvP kills (0 for pre-Wildbound-Waste saves).
            p.pvp_kills = saved.pvp_kills.max(0);
            // Restore the starter chain. Pre-v19 saves default to stage 0; a
            // character already past level 10 has long outgrown the tutorial
            // chain, so it is marked complete rather than handed to a veteran.
            let chain_len = STARTER_QUESTS.len() as u8;
            p.starter_stage = if saved.version < 19 && level >= 10 {
                chain_len
            } else {
                saved.starter_stage.min(chain_len)
            };
            p.starter_kills = saved.starter_kills;
            p.rpg_mode = saved.rpg_mode;
            // Restore the chosen archetype (ignored if the key is unknown or no
            // longer matches the class, e.g. a respec/rename).
            p.archetype = saved
                .archetype
                .as_deref()
                .and_then(super::classes::archetype_by_key)
                .filter(|a| a.class == class);
            // Restore the companion (full health; loyalty carries its level).
            if let Some(key) = saved.pet.as_deref()
                && pet_species_by_key(key).is_none()
            {
                tracing::warn!(%user_id, key, "dropping saved pet with unknown species key");
            }
            p.pet = saved
                .pet
                .as_deref()
                .and_then(pet_species_by_key)
                .map(|species| Pet::new(species, saved.pet_loyalty));
            // Restore the stray companion and any in-progress courting (Genesys).
            // A stale index (the world's critter roster shrank) is simply dropped.
            p.stray = saved
                .stray
                .map(|i| i as usize)
                .filter(|&i| i < super::world::WILDLIFE.len());
            p.stray_bond = saved
                .stray_bond
                .map(|(i, streak, day)| (i as usize, streak, day))
                .filter(|&(i, ..)| i < super::world::WILDLIFE.len());
            // Restore the appearance/bio choices (clamped to valid options).
            for i in 0..appearance::N_FIELDS {
                let v = saved.appearance.get(i).copied().unwrap_or(0);
                p.appearance[i] = v % appearance::option_count(i).max(1) as u8;
            }
            // Restore vitals last so equipment and CON max-hp are already in effect.
            let max = p.max_hp();
            p.hp = if saved.hp > 0 { saved.hp.min(max) } else { max };
        }
        // Re-register housing ownership + furnishings (service-side side-state).
        if let Some(plot) = saved.owned_plot.map(|p| p as usize) {
            if plot < housing::TIERS.len() {
                self.plot_owner.insert(plot, user_id);
                self.restore_saved_house_furniture(user_id, plot, &saved.house_furniture);
            } else {
                tracing::warn!(
                    %user_id,
                    plot,
                    tiers = housing::TIERS.len(),
                    "dropping saved home: plot index out of range"
                );
            }
        }
        let name = class.name();
        self.log_to(
            user_id,
            LogKind::System,
            format!("Welcome back. Your {name} stands ready (level {level})."),
        );
        // Re-orientation that survives any scrollback: say what the next goal
        // is every time a character comes back to the world.
        let step = self
            .players
            .get(&user_id)
            .and_then(|p| next_step_for(p.starter_stage, &p.titles));
        if let Some(step) = step {
            self.log_to(user_id, LogKind::System, format!("Next - {step}"));
        }
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
            waypoint: p.waypoint,
            visited: {
                let mut rooms: Vec<RoomId> = p.visited.iter().copied().collect();
                rooms.sort_unstable();
                rooms
            },
            inventory: p.inventory.clone(),
            equipped,
            scores: p.scores,
            score_points_spent: p.score_points_spent,
            titles: p.titles.clone(),
            title_levels: p.title_levels.clone(),
            active_title: p.active_title,
            completed_quests: p.completed_quests.clone(),
            board_progress: p.board_progress.clone(),
            board_done: p.board_done.clone(),
            quest_cooldowns: p.quest_cooldowns.clone(),
            archetype: p.archetype.map(|a| a.key.to_string()),
            pet: p.pet.map(|pet| pet.species.key.to_string()),
            pet_loyalty: p.pet.map(|pet| pet.loyalty_xp).unwrap_or(0),
            stray: p.stray.map(|i| i as u32),
            stray_bond: p.stray_bond.map(|(i, streak, day)| (i as u32, streak, day)),
            owned_plot: self.owned_plot(user_id).map(|plot| plot as u32),
            house_furniture: self
                .owned_plot(user_id)
                .map(|plot| self.saved_house_furniture_for_plot(user_id, plot))
                .unwrap_or_default(),
            appearance: p.appearance.to_vec(),
            skills: p
                .skills
                .iter()
                .map(|(s, xp)| (s.key().to_string(), *xp))
                .collect(),
            craft_skills: p
                .craft_skills
                .iter()
                .map(|(s, xp)| (s.key().to_string(), *xp))
                .collect(),
            taming_xp: p.taming_xp,
            rpg_mode: p.rpg_mode,
            pvp_kills: p.pvp_kills,
            starter_stage: p.starter_stage,
            starter_kills: p.starter_kills,
        }))
    }

    fn export_all_saved(&self) -> Vec<(Uuid, SavedCharacter)> {
        self.players
            .keys()
            .filter_map(|uid| self.export_saved(*uid).map(|s| (*uid, s)))
            .collect()
    }

    fn restore_saved_house_furniture(
        &mut self,
        user_id: Uuid,
        plot: usize,
        saved_furniture: &[(RoomId, String)],
    ) {
        let base = housing::plot_base(plot);
        let end = base + housing::TIERS[plot].rooms() as RoomId;
        for room in base..end {
            self.house_furniture.remove(&room);
        }

        let mut seen = HashSet::new();
        let mut restored = 0usize;
        for (room, key) in saved_furniture {
            if plot_of_room(*room) != Some(plot) {
                continue;
            }
            if !seen.insert((*room, key.clone())) {
                continue;
            }
            if restored >= SAVED_HOUSE_FURNITURE_LIMIT {
                tracing::warn!(
                    %user_id,
                    plot,
                    limit = SAVED_HOUSE_FURNITURE_LIMIT,
                    "dropping excess saved house furniture"
                );
                break;
            }
            if let Some(furn) = furniture_by_key(key) {
                self.house_furniture.entry(*room).or_default().push(furn);
                restored += 1;
            } else {
                tracing::warn!(%user_id, key, "dropping saved furniture with unknown key");
            }
        }
    }

    fn saved_house_furniture_for_plot(&self, user_id: Uuid, plot: usize) -> Vec<(RoomId, String)> {
        let base = housing::plot_base(plot);
        let end = base + housing::TIERS[plot].rooms() as RoomId;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for room in base..end {
            let Some(furniture) = self.house_furniture.get(&room) else {
                continue;
            };
            for furn in furniture {
                if !seen.insert((room, furn.key)) {
                    continue;
                }
                if out.len() >= SAVED_HOUSE_FURNITURE_LIMIT {
                    tracing::warn!(
                        %user_id,
                        plot,
                        limit = SAVED_HOUSE_FURNITURE_LIMIT,
                        "dropping excess house furniture during save"
                    );
                    return out;
                }
                out.push((room, furn.key.to_string()));
            }
        }
        out
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
                stacks.iter().filter_map(|dot| {
                    (dot.remaining > 0).then_some(SavedMobDot {
                        mob_id: *mob_id,
                        owner: dot.owner,
                        damage: dot.per_tick,
                        remaining_ticks: dot.remaining,
                        from_coat: dot.source == DotSource::Coat,
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
                self.mob_dots.entry(dot.mob_id).or_default().push(MobDot {
                    owner: dot.owner,
                    per_tick: dot.damage,
                    remaining: dot.remaining_ticks,
                    source: if dot.from_coat {
                        DotSource::Coat
                    } else {
                        DotSource::Ability
                    },
                });
            }
        }

        self.dirty = true;
        self.world_dirty = false;
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

    // ---- Movement, and the gates a road may not cross -------------------

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
        if player.in_combat() {
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
        let descent_warning = if self.is_frontier_gateway(from, dest) {
            Some(format!(
                "The way {} opens into the Frontier: older, meaner country meant for seasoned adventurers. Press {} again if you truly want to go.",
                dir.label(),
                dir_input_hint(dir)
            ))
        } else if self.is_reaches_gateway(from, dest) {
            Some(format!(
                "Beyond the sea-gate lie the Sundered Reaches: a drowned realm crueller than any Frontier mile. Press {} again if you truly mean to pass.",
                dir_input_hint(dir)
            ))
        } else if self.is_kaelmyr_gateway(from, dest) {
            Some(format!(
                "Below Yssgar's chamber gapes the wound the seas fled into, and beyond it lies Kaelmyr, the Ashen Reach: a burnt continent older than the world's drowning. Nothing you have faced compares. Press {} again if you truly mean to descend.",
                dir_input_hint(dir)
            ))
        } else {
            None
        };
        if let Some(warning) = descent_warning {
            let confirmed = self
                .players
                .get(&user_id)
                .is_some_and(|p| p.frontier_descent_pending);
            if !confirmed {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.frontier_descent_pending = true;
                }
                self.log_to(user_id, LogKind::System, warning);
                return;
            }
        } else if let Some(player) = self.players.get_mut(&user_id) {
            player.frontier_descent_pending = false;
        }
        let mut first_visit = false;
        if let Some(player) = self.players.get_mut(&user_id) {
            player.frontier_descent_pending = false;
            player.previous_room = Some(from);
            player.room = dest;
            first_visit = Arc::make_mut(&mut player.visited).insert(dest);
        }
        let arrival = if first_visit {
            Arrival::Discovery
        } else {
            Arrival::Revisit
        };
        self.describe_room_context(user_id, arrival);
        self.apply_critter_perks(user_id);
        self.move_followers(user_id, from, dest, dir);
        self.continue_ride(user_id, dir);
    }

    /// Wildbound mounts: while riding, one keypress strides several rooms.
    /// After a successful step, keep walking the same direction until the
    /// mount's stride is spent, the way runs out, a fight starts, or a
    /// gateway asks for its own confirmation (its early-return handles that).
    fn continue_ride(&mut self, user_id: Uuid, dir: Dir) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if !player.mounted || player.in_combat() {
            return;
        }
        let stride = player
            .pet
            .as_ref()
            .and_then(|pet| super::taming::mount_stride(pet.species.key))
            .unwrap_or(1);
        if stride <= 1 {
            return;
        }
        for _ in 1..stride {
            let Some(player) = self.players.get(&user_id) else {
                return;
            };
            if player.in_combat() || player.respawn_at.is_some() {
                return;
            }
            let has_way = self
                .world
                .room(player.room)
                .is_some_and(|room| room.exits.contains_key(&dir));
            if !has_way {
                return;
            }
            // Temporarily dismount for the inner step so it doesn't recurse
            // into its own ride-continuation; remount after.
            if let Some(p) = self.players.get_mut(&user_id) {
                p.mounted = false;
            }
            let before = self.players.get(&user_id).map(|p| p.room);
            self.move_player(user_id, dir);
            if let Some(p) = self.players.get_mut(&user_id) {
                p.mounted = true;
            }
            // A gateway prompt or gate refusal leaves the room unchanged - stop.
            if self.players.get(&user_id).map(|p| p.room) == before {
                return;
            }
        }
    }

    /// Swing up onto the companion's back (or down off it). Needs a tamed
    /// beast that can actually be ridden, and both feet out of combat.
    fn toggle_mount(&mut self, user_id: Uuid) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.in_combat() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "Not in the middle of a fight - flee (z) first.".to_string(),
            );
            return;
        }
        if player.mounted {
            let name = player
                .pet
                .as_ref()
                .map(|p| p.species.name)
                .unwrap_or("your mount");
            if let Some(p) = self.players.get_mut(&user_id) {
                p.mounted = false;
            }
            self.log_to(user_id, LogKind::Normal, format!("You dismount {name}."));
            return;
        }
        let Some(pet) = player.pet.as_ref() else {
            self.log_to(
                user_id,
                LogKind::System,
                "You have no companion to ride - tame one of the great beasts of Broceliande."
                    .to_string(),
            );
            return;
        };
        let Some(stride) = super::taming::mount_stride(pet.species.key) else {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "{} is no riding beast. The rideable kind roam the deep Greenwood.",
                    pet.species.name
                ),
            );
            return;
        };
        let name = pet.species.name;
        if let Some(p) = self.players.get_mut(&user_id) {
            p.mounted = true;
        }
        self.log_to(
            user_id,
            LogKind::Normal,
            format!("You swing up onto {name}'s back - each step now carries you {stride} rooms."),
        );
    }

    fn is_frontier_gateway(&self, from: RoomId, dest: RoomId) -> bool {
        from == self.world.start_room && dest == frontier_entrance_room()
    }

    /// The sea-gate: stepping from Matlatesh's square into the Sundered Reaches.
    fn is_reaches_gateway(&self, from: RoomId, dest: RoomId) -> bool {
        from == super::world::MATLATESH_SQUARE && super::world::is_reaches_room(dest)
    }

    /// The ash-gate: stepping from Yssgar's Reaches chamber down into Kaelmyr.
    fn is_kaelmyr_gateway(&self, from: RoomId, dest: RoomId) -> bool {
        super::world::is_reaches_room(from) && super::world::is_kaelmyr_room(dest)
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

        if self.is_reaches_gateway(from, dest)
            && !self.player_has_title(user_id, REACHES_GATE_TITLE)
        {
            self.clear_frontier_descent_pending(user_id);
            self.log_to(
                user_id,
                LogKind::System,
                "The sea-gate stands sealed. Only one crowned Bane of the King Who Was Promised Nothing may pass into the Sundered Reaches.".to_string(),
            );
            return false;
        }

        if self.is_kaelmyr_gateway(from, dest)
            && !self.player_has_title(user_id, KAELMYR_GATE_TITLE)
        {
            self.clear_frontier_descent_pending(user_id);
            self.log_to(
                user_id,
                LogKind::System,
                "The wound stays shut against you. Only one who has drowned Yssgar - a crowned Bane of Yssgar, the Sundering Deep - may descend into Kaelmyr.".to_string(),
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
        } else if self.is_reaches_gateway(from, dest) {
            format!("{} (the Sundered Reaches)", dir.label())
        } else if self.is_kaelmyr_gateway(from, dest) {
            format!("{} (Kaelmyr, the Ashen Reach)", dir.label())
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
                    Arc::make_mut(&mut p.visited).insert(dest);
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

    // ---- Recall, waypoints, retreat, and following ----------------------

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
        if player.in_combat() {
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
            Arc::make_mut(&mut p.visited).insert(home);
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

    /// Mark the current room as a personal waypoint, warped back to with
    /// `warp_to_waypoint` - a portable answer to the far run between
    /// Embergate and the Frontier's deep levels for healing and resurrecting.
    /// Free to set; out of combat only, like recall.
    fn set_waypoint(&mut self, user_id: Uuid) {
        if !self.is_classed(user_id) {
            return;
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.in_combat() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You can't fix a waypoint in the thick of combat - flee (z) first.".to_string(),
            );
            return;
        }
        let room = player.room;
        if let Some(p) = self.players.get_mut(&user_id) {
            p.waypoint = Some(room);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            "You fix a waypoint here. You'll find your way back to this spot.".to_string(),
        );
        self.dirty = true;
    }

    /// Warp to the marked personal waypoint, from anywhere. Costs
    /// `WAYPOINT_WARP_COST` gold and works only out of combat - recall (to
    /// Embergate) stays free, so a warp to your own chosen spot costs
    /// something instead of trivialising distance entirely.
    fn warp_to_waypoint(&mut self, user_id: Uuid) {
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
        if player.in_combat() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You can't warp in the thick of combat - flee (z) first.".to_string(),
            );
            return;
        }
        let Some(dest) = player.waypoint else {
            self.log_to(
                user_id,
                LogKind::System,
                "You have no waypoint set - fix one first.".to_string(),
            );
            return;
        };
        if player.room == dest {
            self.log_to(
                user_id,
                LogKind::Normal,
                "You're already standing at your waypoint.".to_string(),
            );
            return;
        }
        if player.gold < WAYPOINT_WARP_COST {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Warping to your waypoint costs {WAYPOINT_WARP_COST} gold."),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= WAYPOINT_WARP_COST;
            p.previous_room = Some(p.room);
            p.room = dest;
            Arc::make_mut(&mut p.visited).insert(dest);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            "You warp to your waypoint. The world folds soft as cloth, and it rises around you."
                .to_string(),
        );
        self.describe_room(user_id);
        self.apply_critter_perks(user_id);
        self.dirty = true;
    }

    /// Whether a walking progression gate would refuse this single step. The
    /// silent twin of `can_cross_progression_gate`, used by the haven retreat
    /// so its pathing can never slip through a sealed gate.
    fn gate_blocks(&self, user_id: Uuid, from: RoomId, dest: RoomId) -> bool {
        (from == FIRST_DUNGEON_GATE_FROM
            && dest == FIRST_DUNGEON_GATE_TO
            && !self.player_has_title(user_id, FIRST_DUNGEON_GATE_TITLE))
            || (self.is_living_dark_gateway(from, dest)
                && !self.player_has_title(user_id, FRONTIER_GATE_TITLE))
            || (self.is_frontier_gateway(from, dest)
                && !self.player_has_required_titles(user_id, &FRONTIER_REQUIRED_TITLES))
            || (self.is_reaches_gateway(from, dest)
                && !self.player_has_title(user_id, REACHES_GATE_TITLE))
            || (self.is_kaelmyr_gateway(from, dest)
                && !self.player_has_title(user_id, KAELMYR_GATE_TITLE))
    }

    /// Retreat to the nearest haven: a breadth-first walk over the exits the
    /// player could take on foot, ending at the closest safe room. The
    /// maze-country answer to being lost - deep in a briar maze it reads as
    /// "back to this zone's gate" without any per-zone bookkeeping. Out of
    /// combat only, and it never crosses a gate walking would refuse.
    fn retreat_to_haven(&mut self, user_id: Uuid) {
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
        if player.in_combat() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You can't slip away in the thick of combat - flee (z) first.".to_string(),
            );
            return;
        }
        let start = player.room;
        if self.world.room(start).is_some_and(|r| r.safe) {
            self.log_to(
                user_id,
                LogKind::Normal,
                "You already stand in a haven.".to_string(),
            );
            return;
        }
        let mut queue = VecDeque::from([start]);
        let mut seen = HashSet::from([start]);
        let mut haven = None;
        while let Some(room) = queue.pop_front() {
            let Some(r) = self.world.room(room) else {
                continue;
            };
            if r.safe {
                haven = Some(room);
                break;
            }
            for next in r.exits.values() {
                if !self.gate_blocks(user_id, room, *next) && seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        let Some(haven) = haven else {
            self.log_to(
                user_id,
                LogKind::System,
                "No haven answers from here.".to_string(),
            );
            return;
        };
        if let Some(p) = self.players.get_mut(&user_id) {
            p.previous_room = Some(p.room);
            p.room = haven;
            Arc::make_mut(&mut p.visited).insert(haven);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            "You retrace your turnings in a rush, and the quiet of the nearest haven closes around you."
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

    // ---- Gathering, hunting, and crafting -------------------------------

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
                    // A full heal, not a small top-up: the old partial-heal
                    // amount meant walking in and out of the room over and
                    // over just to fully mend, which reads as tedious rather
                    // than as a real rest stop.
                    Perk::Mend => {
                        p.hp = p.max_hp();
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

    /// Work a resource node in the current room: harvest the highest-tier node
    /// the player qualifies for, granting its raw material and skill xp. Nodes
    /// don't need a safe/unsafe room and never involve combat.
    fn gather(&mut self, user_id: Uuid) {
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
        if nodes_at(room_id).is_empty() {
            self.log_to(
                user_id,
                LogKind::Normal,
                "There's nothing to gather here.".to_string(),
            );
            return;
        }
        // `try_gather` logs its own reason when a node is present but unworkable.
        self.try_gather(user_id, room_id);
    }

    /// Harvest the best node in the room the player can work right now. Returns
    /// true if a material was taken. When a node is present but out of reach
    /// (under-skilled) or still regrowing, it logs why and returns false.
    fn try_gather(&mut self, user_id: Uuid, room_id: RoomId) -> bool {
        let now = Instant::now();
        let Some(player) = self.players.get(&user_id) else {
            return false;
        };
        let nodes = nodes_at(room_id);

        // Pick the highest-tier node the player qualifies for and that is off
        // cooldown. Also remember the toughest node they're too unskilled for,
        // and whether anything here is merely regrowing, for a helpful message.
        let mut choice: Option<(usize, &'static ResourceNode)> = None;
        let mut under_skilled: Option<(&'static ResourceNode, i32)> = None;
        let mut regrowing = false;
        for &n in &nodes {
            let Some(ni) = node_index(n) else {
                continue;
            };
            let level = skill_level_for_xp(player.skill_xp(n.skill));
            if level < n.level_req {
                if under_skilled.is_none_or(|(u, _)| n.tier > u.tier) {
                    under_skilled = Some((n, level));
                }
                continue;
            }
            let ready = match self.gathered.get(&ni) {
                Some(t) => now.duration_since(*t) >= NODE_RESPAWN,
                None => true,
            };
            if !ready {
                regrowing = true;
                continue;
            }
            if choice.is_none_or(|(_, c)| n.tier > c.tier) {
                choice = Some((ni, n));
            }
        }

        let Some((ni, node)) = choice else {
            if let Some((n, level)) = under_skilled {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!(
                        "You can't work {} yet - it needs {} level {} (yours is {level}).",
                        n.name,
                        n.skill.label(),
                        n.level_req,
                    ),
                );
            } else if regrowing {
                self.log_to(
                    user_id,
                    LogKind::Normal,
                    "The resources here need time to recover.".to_string(),
                );
            }
            return false;
        };

        self.gathered.insert(ni, now);
        let skill = node.skill;
        let yield_item = node.yield_item;
        let gained = node.xp;
        let node_name = node.name;
        let item_name = item(yield_item)
            .map(|i| i.name.to_string())
            .unwrap_or_else(|| "something".to_string());
        let (before, after) = if let Some(p) = self.players.get_mut(&user_id) {
            p.inventory.push(yield_item);
            let cur = p.skill_xp(skill);
            let before = skill_level_for_xp(cur);
            let new_xp = cur + gained as i64;
            p.skills.insert(skill, new_xp);
            (before, skill_level_for_xp(new_xp))
        } else {
            return false;
        };
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "You {} {node_name} and take {item_name}. (+{gained} {} xp)",
                skill.verb(),
                skill.label(),
            ),
        );
        if after > before {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Your {} rises to level {after}!", skill.label()),
            );
        }
        self.dirty = true;
        true
    }

    /// Craft the recipe at `recipe_index`: requires the matching station in the
    /// room, enough craft-skill level, and all input materials. Consumes the
    /// inputs, adds the output, and trains the craft skill.
    fn craft(&mut self, user_id: Uuid, recipe_index: usize) {
        if !self.is_classed(user_id) {
            return;
        }
        let Some(rc) = recipe(recipe_index) else {
            return;
        };
        // Gather everything decidable under a read borrow, then drop it.
        let room_id;
        let level;
        let missing: Option<(u32, u32)>;
        {
            let Some(player) = self.players.get(&user_id) else {
                return;
            };
            if player.respawn_at.is_some() {
                return;
            }
            room_id = player.room;
            level = skill_level_for_xp(player.craft_xp(rc.skill));
            missing = rc
                .inputs
                .iter()
                .find(|ing| player.item_count(ing.item) < ing.qty)
                .map(|ing| (ing.item, ing.qty));
        }
        if !craft_stations_at(room_id).contains(&rc.skill) {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You need a {} to make that.", rc.skill.station()),
            );
            return;
        }
        if level < rc.level_req {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "Your {} ({level}) isn't skilled enough - that needs level {}.",
                    rc.skill.label(),
                    rc.level_req,
                ),
            );
            return;
        }
        if let Some((item_id, qty)) = missing {
            let name = item(item_id).map(|i| i.name).unwrap_or("materials");
            self.log_to(
                user_id,
                LogKind::System,
                format!("You don't have the materials ({qty}x {name})."),
            );
            return;
        }

        let out_name = item(rc.output)
            .map(|i| i.name.to_string())
            .unwrap_or_else(|| "something".to_string());
        let (before, after) = {
            let p = self.players.get_mut(&user_id).expect("player present");
            for ing in &rc.inputs {
                p.consume(ing.item, ing.qty);
            }
            for _ in 0..rc.output_qty {
                p.inventory.push(rc.output);
            }
            let cur = p.craft_xp(rc.skill);
            p.craft_skills.insert(rc.skill, cur + rc.xp as i64);
            (
                skill_level_for_xp(cur),
                skill_level_for_xp(cur + rc.xp as i64),
            )
        };
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "You {} {out_name}. (+{} {} xp)",
                rc.skill.verb(),
                rc.xp,
                rc.skill.label(),
            ),
        );
        if after > before {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Your {} rises to level {after}!", rc.skill.label()),
            );
        }
        self.dirty = true;
    }

    // ---- Looking at a room, examining it, and the Ways ------------------

    fn look(&mut self, user_id: Uuid) {
        self.describe_room_context(user_id, Arrival::Silent);
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
        self.describe_room_context(user_id, Arrival::Revisit);
    }

    fn describe_room_context(&mut self, user_id: Uuid, arrival: Arrival) {
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
            self.bump_starter_reach(user_id, here_zone);
            self.check_escort_arrival(user_id, here_zone);
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        let room_id = player.room;
        let Some(room) = self.world.room(room_id) else {
            return;
        };
        let rpg_mode = player.rpg_mode;
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
        match arrival {
            Arrival::Silent => {}
            // A discovery in field mode carries the room's prose too: the
            // field layout has no Now panel, so the feed is the one place a
            // newly found room gets to describe itself.
            Arrival::Discovery if rpg_mode => {
                self.log_to(user_id, LogKind::Travel, format!("You find {name}."));
                self.log_to(user_id, LogKind::Travel, desc.clone());
            }
            // Field-mode steps through known land say nothing: the @ moved and
            // the Here panel names the room. Classic mode keeps its breadcrumb.
            Arrival::Revisit if rpg_mode => {}
            Arrival::Discovery | Arrival::Revisit => {
                self.log_to(user_id, LogKind::Travel, format!("Arrived at {name}."));
            }
        }
        self.log_to(user_id, LogKind::Room, format!("== {name} =="));
        self.log_to(user_id, LogKind::Room, desc);
        // Furnishings set down in a home are part of the room for everyone here.
        if let Some(furn) = self.house_furniture.get(&room_id)
            && !furn.is_empty()
        {
            let listed = furn.iter().map(|f| f.name).collect::<Vec<_>>().join(", ");
            self.log_to(user_id, LogKind::Room, format!("Here stands {listed}."));
        }
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
        let villagers: Vec<_> = features
            .iter()
            .filter(|f| f.kind == FeatureKind::Villager)
            .collect();
        let other: Vec<_> = features
            .iter()
            .filter(|f| f.kind != FeatureKind::Villager)
            .collect();
        // A villager is always announced up front, never hidden behind a menu -
        // that's the whole point of standing there.
        for v in &villagers {
            self.log_to(
                user_id,
                LogKind::Room,
                format!(
                    "{} stands here, waiting for a question. Press o to ask.",
                    v.name
                ),
            );
        }
        if !other.is_empty() {
            let names: Vec<&str> = other.iter().map(|f| f.name).collect();
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
        if feat.kind == FeatureKind::Villager {
            // No "you ask X for a moment" preamble: that phrasing implied an
            // exchange was starting when the line *is* the whole interaction,
            // which read as "...and? that's it?" A villager's dialogue is the
            // payoff, not a placeholder for one - present it directly, same
            // as the "look at" default below does for its own `desc`.
            self.log_to(
                user_id,
                LogKind::Room,
                format!("{} says: \"{}\"", feat.name, feat.desc),
            );
            self.dirty = true;
            return;
        }
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
        } else if feat.kind == FeatureKind::Housing {
            self.log_to(
                user_id,
                LogKind::System,
                "Press n to open the housing ledger: buy a deed here, or furnish a home you own from inside it.".to_string(),
            );
        } else if feat.kind == FeatureKind::Portal {
            self.log_to(
                user_id,
                LogKind::System,
                "Press i to open the ways: step through to any waystone you know of.".to_string(),
            );
        }
        self.dirty = true;
    }

    /// Step through a waystone to another. Only works when the player stands on a
    /// portal, is out of combat, and the destination is a real portal landing.
    fn travel(&mut self, user_id: Uuid, dest: RoomId) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        if p.in_combat() {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You can't step through while fighting.".to_string(),
            );
            return;
        }
        let on_portal = features_at(p.room)
            .iter()
            .any(|f| f.kind == FeatureKind::Portal);
        if !on_portal {
            self.log_to(
                user_id,
                LogKind::System,
                "There is no waystone here to step through.".to_string(),
            );
            return;
        }
        let Some((label, _)) = super::world::waystone_destinations()
            .into_iter()
            .find(|(_, r)| *r == dest)
        else {
            return;
        };
        if dest == p.room {
            return;
        }
        // The Ways carry no progression rules of their own; they only shorten a
        // road the player has already walked. Titles are checked where you walk
        // in, in `can_cross_progression_gate`.
        if !super::world::waystone_is_known(dest, &p.visited) {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "The waystone hums against your palm, then stills. The Ways carry you only where your own feet have already gone, and you have never stood at {label}."
                ),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.previous_room = Some(p.room);
            p.room = dest;
            Arc::make_mut(&mut p.visited).insert(dest);
        }
        self.log_to(
            user_id,
            LogKind::Travel,
            "The waystone takes you in a breath of blue light...".to_string(),
        );
        self.describe_room(user_id);
    }

    // ---- Board quests, escorts, and the starter chain -------------------

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

    /// Every posting for a board in the player's room: ready-to-claim
    /// counter-bounties first, then bounties still open to accept. Backs the
    /// picker menu (`Panel::Board`) - the player chooses, rather than
    /// `examine` silently auto-assigning whatever came first in the static
    /// list (which is how a fresh adventurer could get handed a bounty for a
    /// foe several zones above them with no way to preview or decline it).
    fn board_entries(&self, user_id: Uuid, board_room: RoomId) -> Vec<BoardEntryView> {
        let Some(p) = self.players.get(&user_id) else {
            return Vec::new();
        };
        let mut entries: Vec<BoardEntryView> = p
            .board_progress
            .iter()
            .filter_map(|(id, prog)| {
                let q = board_quest(*id)?;
                (q.board == board_room && *prog >= q.objective.target())
                    .then(|| board_entry(q, true, false))
            })
            .collect();
        entries.extend(
            BOARD_QUESTS
                .iter()
                .filter(|q| q.board == board_room && self.board_quest_available(p, q))
                .map(|q| board_entry(q, false, board_quest_locked(q, &p.titles))),
        );
        entries
    }

    /// Turn in a finished counter-bounty chosen from the board's picker. A
    /// stale selection (already claimed elsewhere, or not actually ready) is
    /// silently a no-op rather than an error the player has to parse.
    fn claim_board_quest(&mut self, user_id: Uuid, quest_id: u32) {
        let Some(q) = board_quest(quest_id) else {
            return;
        };
        let ready = self.players.get(&user_id).is_some_and(|p| {
            p.board_progress
                .iter()
                .any(|(id, prog)| *id == quest_id && *prog >= q.objective.target())
        });
        if !ready {
            return;
        }
        let level = self.players[&user_id].level;
        if let Some(p) = self.players.get_mut(&user_id) {
            p.board_progress.retain(|(id, _)| *id != quest_id);
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
    }

    /// Accept a bounty explicitly chosen from the board's picker.
    fn accept_board_quest(&mut self, user_id: Uuid, quest_id: u32) {
        let Some(q) = board_quest(quest_id) else {
            return;
        };
        let available = self
            .players
            .get(&user_id)
            .is_some_and(|p| self.board_quest_available(p, q));
        if !available {
            return;
        }
        // A sealed posting cannot be taken: its hunting ground refuses the
        // player at the door, so accepting it would only hand out dead weight.
        let locked = self
            .players
            .get(&user_id)
            .is_some_and(|p| board_quest_locked(q, &p.titles));
        if locked {
            let missing = q
                .requires
                .iter()
                .filter(|t| {
                    !self
                        .players
                        .get(&user_id)
                        .is_some_and(|p| p.titles.iter().any(|owned| owned == **t))
                })
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            self.log_to(
                user_id,
                LogKind::System,
                format!("The posting is sealed to you - that ground opens only to: {missing}."),
            );
            return;
        }
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

    /// Advance the starter chain when `inc` reports progress for its active
    /// goal. Completing a step pays out and announces the next; completing the
    /// last hands the player over to the Long Road and the capital boards.
    fn bump_starter(&mut self, user_id: Uuid, inc: impl Fn(StarterGoal) -> u32) {
        enum Outcome {
            Progress(&'static StarterQuest, u32, u32),
            Complete(&'static StarterQuest),
        }
        let outcome = {
            let Some(p) = self.players.get_mut(&user_id) else {
                return;
            };
            let Some(q) = starter_quest(p.starter_stage) else {
                return;
            };
            let step = inc(q.goal);
            if step == 0 {
                return;
            }
            let need = starter_goal_target(q.goal);
            p.starter_kills = (p.starter_kills + step).min(need);
            if p.starter_kills < need {
                Outcome::Progress(q, p.starter_kills, need)
            } else {
                p.starter_stage += 1;
                p.starter_kills = 0;
                p.gold += q.reward_gold;
                p.xp += q.reward_xp;
                Outcome::Complete(q)
            }
        };
        match outcome {
            Outcome::Progress(q, done, need) => {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("{} - {done}/{need}.", q.title),
                );
            }
            Outcome::Complete(q) => {
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    format!(
                        "{} - done (+{} xp, +{} gold).",
                        q.title, q.reward_xp, q.reward_gold
                    ),
                );
                let next_stage = self
                    .players
                    .get(&user_id)
                    .map(|p| p.starter_stage)
                    .unwrap_or(0);
                match starter_quest(next_stage) {
                    Some(next) => self.log_to(
                        user_id,
                        LogKind::System,
                        format!("Next - {}: {}", next.title, next.hint),
                    ),
                    None => self.log_to(
                        user_id,
                        LogKind::System,
                        "You know the land now. The Long Road in your journal (j) names every crown between you and the realm's end; the capital boards post the daily work."
                            .to_string(),
                    ),
                }
                self.check_level_up(user_id);
            }
        }
        self.dirty = true;
    }

    /// Room-enter half of the starter chain: Reach goals.
    fn bump_starter_reach(&mut self, user_id: Uuid, here_zone: &'static str) {
        self.bump_starter(user_id, |g| {
            u32::from(matches!(g, StarterGoal::Reach { zone } if zone == here_zone))
        });
    }

    /// Kill half of the starter chain: SlayIn (by the zone the fight happened
    /// in) and SlayNamed (by the slain foe's name).
    fn bump_starter_kill(&mut self, user_id: Uuid, mob_name: &str, here_zone: &str) {
        self.bump_starter(user_id, |g| match g {
            StarterGoal::SlayIn { zone, .. } => u32::from(zone == here_zone),
            StarterGoal::SlayNamed { name_contains } => u32::from(mob_name.contains(name_contains)),
            StarterGoal::Reach { .. } => 0,
        });
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

    // ---- Combat: targeting, abilities, damage, and the kill -------------

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
            Some(mob_id) => self.set_target(user_id, mob_id),
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

    /// Lock onto a specific foe (a click on its roster row), then let the combat
    /// tick trade blows with it. Falls back to [`Self::engage`] if the clicked
    /// foe is already gone - slain, fled, or a stale row - so a click never dead-
    /// ends when there's still something else to fight.
    fn engage_mob(&mut self, user_id: Uuid, mob_id: u32) {
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
        let valid = self
            .mobs
            .get(&mob_id)
            .is_some_and(|m| m.alive && m.revealed && m.current_room == room_id);
        if valid {
            self.set_target(user_id, mob_id);
        } else {
            self.engage(user_id);
        }
    }

    /// Point the player at `mob_id` and announce it. Shared by the auto-target
    /// [`Self::engage`] and the click-to-target [`Self::engage_mob`].
    fn set_target(&mut self, user_id: Uuid, mob_id: u32) {
        let mob_name = self
            .mobs
            .get(&mob_id)
            .map(|m| m.spawn.name.to_string())
            .unwrap_or_default();
        if self.players.get(&user_id).is_some_and(|p| p.mounted) {
            if let Some(p) = self.players.get_mut(&user_id) {
                p.mounted = false;
            }
            self.log_to(
                user_id,
                LogKind::Combat,
                "You slide from the saddle - this is foot work.".to_string(),
            );
        }
        // Taking a mob target breaks off any duel. `target` and `pvp_target`
        // are mutually exclusive by contract - `damage_target` and the `Stun`
        // arm both resolve pvp first, so a player holding two targets at once
        // would have their abilities damage the rival while the stun landed on
        // the mob. `engage_player` clears `target`; this is the other half.
        let dropped_duel = self
            .players
            .get_mut(&user_id)
            .and_then(|p| p.pvp_target.take())
            .is_some();
        if dropped_duel {
            self.log_to(
                user_id,
                LogKind::Combat,
                "You break off the duel.".to_string(),
            );
        }
        if let Some(player) = self.players.get_mut(&user_id) {
            player.target = Some(mob_id);
            // Opportunist: the Rogue's first strike of a fight always crits.
            player.opening_strike = player.class == Some(Class::Rogue);
        }
        // A named boss is an event, not another roster row - open with a bark.
        let boss = self
            .mobs
            .get(&mob_id)
            .is_some_and(|m| m.spawn.boss && m.alive);
        if boss {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!(
                    "{mob_name} turns its full attention on you. The air itself seems to brace."
                ),
            );
        }
        self.log_to(
            user_id,
            LogKind::Combat,
            format!("You close with {mob_name}!"),
        );
    }

    /// Lock onto another adventurer in a `pvp` room (a click on their roster
    /// row in the "Adventurers here" list). Mirrors [`Self::engage_mob`] but
    /// keeps a separate `pvp_target` so a mob fight and a duel never collide;
    /// the victim auto-retaliates if they weren't already fighting anything.
    fn engage_player(&mut self, user_id: Uuid, target_id: Uuid) {
        if !self.is_classed(user_id) || user_id == target_id {
            return;
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.respawn_at.is_some() {
            return;
        }
        let room_id = player.room;
        if !self.world.room(room_id).is_some_and(|r| r.pvp) {
            self.log_to(
                user_id,
                LogKind::System,
                "There's no dueling ground here.".to_string(),
            );
            return;
        }
        let valid = self
            .players
            .get(&target_id)
            .is_some_and(|t| t.room == room_id && t.class.is_some() && t.respawn_at.is_none());
        if !valid {
            self.log_to(
                user_id,
                LogKind::System,
                "That adventurer is no longer here to fight.".to_string(),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            if p.mounted {
                p.mounted = false;
            }
            p.pvp_target = Some(target_id);
            p.target = None;
            p.opening_strike = p.class == Some(Class::Rogue);
        }
        self.log_to(
            user_id,
            LogKind::Combat,
            "You draw on a fellow adventurer!".to_string(),
        );
        // The victim rounds on their attacker at once, unless they were
        // already mid-fight with someone or something else.
        let victim_free = self
            .players
            .get(&target_id)
            .is_some_and(|t| t.pvp_target.is_none() && t.target.is_none());
        if victim_free {
            if let Some(t) = self.players.get_mut(&target_id) {
                t.pvp_target = Some(user_id);
            }
            self.log_to(
                target_id,
                LogKind::Combat,
                "You are set upon by a fellow adventurer!".to_string(),
            );
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
        if needs_target && player.target.is_none() && player.pvp_target.is_none() {
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
                let power = self.ability_power(ability, user_id);
                let amount = self.amplified_heal(class, power);
                self.heal_player(user_id, amount);
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{} restores {} health.", ability.name, amount),
                );
            }
            AbilityEffect::HealOverTime => {
                let power = self.ability_power(ability, user_id);
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.self_effects.push(ActiveEffect {
                        kind: AbilityEffect::HealOverTime,
                        magnitude: power,
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
                // Fed the rating without the running empower, so recasting
                // never stacks a buff on top of itself.
                let power = self
                    .players
                    .get(&user_id)
                    .map(|p| {
                        let sp = p.spell_power_of(p.attack_rating() - p.empower);
                        ability.magnitude + sp * ability_coef_pct(ability.effect) / 100
                    })
                    .unwrap_or(ability.magnitude);
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.empower = power;
                    p.empower_ticks = ability.duration;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{} surges through you (+{} damage).", ability.name, power),
                );
            }
            AbilityEffect::Ward => {
                let power = self.ability_power(ability, user_id);
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.shield = power;
                    p.shield_ticks = ability.duration;
                }
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{} shields you ({} absorb).", ability.name, power),
                );
            }
            AbilityEffect::Strike => {
                let dmg = self.ability_damage(class, ability, user_id);
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
            }
            AbilityEffect::Finisher => {
                let dmg = self.ability_damage(class, ability, user_id);
                if let Some(p) = self.players.get_mut(&user_id) {
                    p.empower = p.empower.max(dmg / 8);
                    p.empower_ticks = p.empower_ticks.max(ability.duration);
                }
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
            }
            AbilityEffect::DamageOverTime => {
                let tick = self.ability_damage(class, ability, user_id);
                if self
                    .players
                    .get(&user_id)
                    .is_some_and(|p| p.pvp_target.is_some())
                {
                    self.seed_pvp_dot(
                        user_id,
                        tick,
                        ability.damage_type,
                        ability.duration,
                        DotSource::Ability,
                        ability.name,
                    );
                } else {
                    self.seed_mob_dot(
                        user_id,
                        tick,
                        ability.damage_type,
                        ability.duration,
                        DotSource::Ability,
                        ability.name,
                    );
                }
            }
            AbilityEffect::Stun => {
                let target = self.players.get(&user_id).and_then(|p| p.target);
                let pvp_target = self.players.get(&user_id).and_then(|p| p.pvp_target);
                let dmg = self.ability_damage(class, ability, user_id);
                self.damage_target(user_id, dmg, ability.damage_type, ability.name);
                // Only stun if the target survived the hit.
                if let Some(mob_id) = target
                    && self.mobs.get(&mob_id).is_some_and(|m| m.alive)
                {
                    // A fresh daze never cuts a longer one short.
                    self.mob_stuns
                        .entry(mob_id)
                        .and_modify(|t| *t = (*t).max(ability.duration))
                        .or_insert(ability.duration);
                    self.mark_world_dirty();
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{} leaves the foe reeling!", ability.name),
                    );
                } else if let Some(victim_id) = pvp_target
                    && self.players.get(&victim_id).is_some_and(|v| !v.dead)
                {
                    self.pvp_stuns
                        .entry(victim_id)
                        .and_modify(|t| *t = (*t).max(ability.duration))
                        .or_insert(ability.duration);
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("{} leaves your rival reeling!", ability.name),
                    );
                }
            }
        }
    }

    /// An ability's power: its table magnitude plus the caster's spell power
    /// weighted by the effect (see `ability_coef_pct`). No class traits, no
    /// archetype: the raw number every effect arm starts from.
    fn ability_power(&self, ability: &Ability, user_id: Uuid) -> i32 {
        let sp = self
            .players
            .get(&user_id)
            .map(|p| p.spell_power())
            .unwrap_or(0);
        ability.magnitude + sp * ability_coef_pct(ability.effect) / 100
    }

    fn amplified_heal(&self, class: Class, base: i32) -> i32 {
        if class == Class::Cleric {
            base + base / 4 // Light of the Dawn
        } else {
            base
        }
    }

    fn ability_damage(&self, class: Class, ability: &Ability, user_id: Uuid) -> i32 {
        let mut dmg = self.ability_power(ability, user_id);
        if class == Class::Mage || class == Class::Runemaster {
            dmg += dmg / 5; // Arcane Mastery / Runic Overflow
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
        // DPS-archetype amplification applies to every ability hit.
        if let Some(p) = self.players.get(&user_id) {
            let (atk_pct, _, _, _) = p.archetype_mods();
            dmg += dmg * atk_pct / 100;
        }
        dmg
    }

    fn heal_player(&mut self, user_id: Uuid, amount: i32) {
        if let Some(p) = self.players.get_mut(&user_id) {
            // Healer-archetype amplification applies to every heal they receive
            // (heals are self-targeted today, so caster == recipient).
            let (_, _, heal_pct, _) = p.archetype_mods();
            let amount = amount + amount * heal_pct / 100;
            let max = p.max_hp();
            p.hp = (p.hp + amount).min(max);
            self.dirty = true;
        }
    }

    fn damage_target(&mut self, user_id: Uuid, raw: i32, dtype: DamageType, source: &str) {
        // A pvp duel takes priority. The two targets never coexist: taking a
        // duel clears the mob target (`engage_player`) and taking a mob target
        // breaks off the duel (`set_target`). Checking pvp first keeps that
        // invariant explicit here.
        if let Some(victim_id) = self.players.get(&user_id).and_then(|p| p.pvp_target) {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{source} hits your rival for {raw} {}.", dtype.label()),
            );
            self.strike_pvp_target(user_id, victim_id, raw, dtype, source);
            return;
        }
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
        origin: DotSource,
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
        let stacks = self.mob_dots.entry(mob_id).or_default();
        // A coat refreshes its own single wound; an ability opens a new one.
        let existing = match origin {
            DotSource::Coat => stacks
                .iter_mut()
                .find(|d| d.owner == user_id && d.source == DotSource::Coat),
            DotSource::Ability => None,
        };
        let opened = match existing {
            Some(dot) => {
                dot.per_tick = scaled;
                dot.remaining = duration;
                false
            }
            None => {
                stacks.push(MobDot {
                    owner: user_id,
                    per_tick: scaled,
                    remaining: duration,
                    source: origin,
                });
                true
            }
        };
        self.mark_world_dirty();
        // Only the wound opening is worth a line. A coat re-seeds every swing,
        // so logging refreshes would bury the fight in its own upkeep.
        if opened {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{source} festers in the foe ({} damage).", dtype.label()),
            );
        }
        self.dirty = true;
    }

    /// Pvp counterpart of `seed_mob_dot`: seeds a damage-over-time on the
    /// caster's `pvp_target`. Unlike a mob dot, the resist/weak multiplier is
    /// *not* baked in up front - each tick goes through `strike_pvp_target`
    /// (`strike_player`), which needs the real `DamageType` to apply the
    /// victim's armor correctly every time.
    fn seed_pvp_dot(
        &mut self,
        user_id: Uuid,
        per_tick: i32,
        dtype: DamageType,
        duration: u8,
        origin: DotSource,
        source: &str,
    ) {
        let Some(victim_id) = self.players.get(&user_id).and_then(|p| p.pvp_target) else {
            return;
        };
        let stacks = self.pvp_dots.entry(victim_id).or_default();
        // Same one-wound-per-coat rule as `seed_mob_dot`.
        let existing = match origin {
            DotSource::Coat => stacks
                .iter_mut()
                .find(|d| d.owner == user_id && d.source == DotSource::Coat),
            DotSource::Ability => None,
        };
        let opened = match existing {
            Some(dot) => {
                dot.per_tick = per_tick;
                dot.school = dtype;
                dot.remaining = duration;
                false
            }
            None => {
                stacks.push(PvpDot {
                    owner: user_id,
                    per_tick,
                    school: dtype,
                    remaining: duration,
                    source: origin,
                });
                true
            }
        };
        if opened {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{source} festers in your rival ({} damage).", dtype.label()),
            );
        }
        self.dirty = true;
    }

    /// Deal `raw` pvp damage from `attacker_id` to `victim_id` via
    /// `strike_player` (armor, shields, Monk/Tank mitigation, the Warrior
    /// death-save, and veteran in-place resurrection all apply exactly as
    /// they do against a mob), then handle a real kill: the victim's lost
    /// carried gold becomes the killer's spoils, plus a flat xp bonus, a
    /// `pvp_kills` tick, and the reaver title track. Shared by the tick's
    /// auto-attack pass, offensive abilities, pet bites, and pvp dots.
    fn strike_pvp_target(
        &mut self,
        attacker_id: Uuid,
        victim_id: Uuid,
        raw: i32,
        dtype: DamageType,
        source: &str,
    ) -> bool {
        let gold_before = self.players.get(&victim_id).map(|v| v.gold).unwrap_or(0);
        let survived = self.strike_player(victim_id, raw, dtype, source);
        self.dirty = true;
        if !survived && self.players.get(&victim_id).is_some_and(|v| v.dead) {
            let gold_gain = (gold_before
                - self
                    .players
                    .get(&victim_id)
                    .map(|v| v.gold)
                    .unwrap_or(gold_before))
            .max(0);
            let victim_level = self.players.get(&victim_id).map(|v| v.level).unwrap_or(1);
            let xp_gain = (15 + victim_level as i64 * 5).max(15);
            let mut new_kill_count = 0;
            let mut atk_level = 1;
            if let Some(a) = self.players.get_mut(&attacker_id) {
                a.pvp_target = None;
                a.gold += gold_gain;
                a.xp += xp_gain;
                a.pvp_kills += 1;
                new_kill_count = a.pvp_kills;
                atk_level = a.level;
            }
            self.log_to(
                attacker_id,
                LogKind::Loot,
                format!("You have slain a rival adventurer! (+{xp_gain} xp, +{gold_gain} gold)"),
            );
            if let Some(title) = pvp_title_for(new_kill_count) {
                self.award_title(attacker_id, title.to_string(), atk_level);
            }
            self.check_level_up(attacker_id);
        }
        survived
    }

    /// Pvp counterpart of `fire_pet_skills`: the owner's companion's unlocked
    /// auto-skills fire against a `pvp_target` instead of a mob. `SavageBite`/
    /// `Pounce` and `Rend` route through `strike_pvp_target`/`seed_pvp_dot` so
    /// they respect the victim's armor exactly like every other pvp blow;
    /// `Roar`/`Guard` are pure self-buffs and work identically either way.
    /// Returns true if the companion's blow finished the victim off.
    #[allow(clippy::too_many_arguments)]
    fn fire_pet_skills_pvp(
        &mut self,
        user_id: Uuid,
        victim_id: Uuid,
        pet_level: i32,
        pet_atk: i32,
        pet_name: &str,
        pet_skills: &'static [super::taming::PetSkill],
        beastlord: bool,
    ) -> bool {
        let now_tick = self.world_ticks;
        for (si, skill) in pet_skills
            .iter()
            .filter(|s| s.level <= pet_level)
            .enumerate()
        {
            let ready = self
                .pet_skill_cd
                .get(&(user_id, si))
                .is_none_or(|&next| now_tick >= next);
            if !ready {
                continue;
            }
            let base_cd = skill.cooldown as u64;
            let cd = if beastlord {
                (base_cd - base_cd * BEASTLORD_PET_PCT as u64 / 100).max(1)
            } else {
                base_cd
            };
            self.pet_skill_cd.insert((user_id, si), now_tick + cd);
            match skill.effect {
                PetSkillEffect::SavageBite | PetSkillEffect::Pounce => {
                    let bonus = skill.power + pet_atk * skill.power / 20;
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name}'s {} rips into your rival!", skill.name),
                    );
                    self.strike_pvp_target(
                        user_id,
                        victim_id,
                        bonus,
                        DamageType::Physical,
                        pet_name,
                    );
                    if self.players.get(&victim_id).is_some_and(|v| v.dead) {
                        return true;
                    }
                }
                PetSkillEffect::Rend => {
                    let per_tick = skill.power + pet_atk / 8;
                    self.seed_pvp_dot(
                        user_id,
                        per_tick,
                        DamageType::Physical,
                        3,
                        DotSource::Ability,
                        &format!("Your {pet_name}'s Rend"),
                    );
                }
                PetSkillEffect::Roar => {
                    let mag = skill.power + pet_atk / 10;
                    if let Some(p) = self.players.get_mut(&user_id) {
                        p.empower = p.empower.max(mag);
                        p.empower_ticks = p.empower_ticks.max(4);
                    }
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!(
                            "Your {pet_name} looses an intimidating roar - you feel emboldened!"
                        ),
                    );
                    self.dirty = true;
                }
                PetSkillEffect::Guard => {
                    let mag = skill.power + pet_atk / 4;
                    if let Some(p) = self.players.get_mut(&user_id) {
                        p.shield = p.shield.max(mag);
                        p.shield_ticks = p.shield_ticks.max(4);
                    }
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name} guards you closely, warding the next blows."),
                    );
                    self.dirty = true;
                }
                PetSkillEffect::Mend => {
                    let mag = skill.power + pet_atk / 6;
                    self.heal_player(user_id, mag);
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name} nuzzles you with a mending glow."),
                    );
                }
            }
        }
        false
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
            // Necromancer "Soul Harvest" and Spiritmaster "Spirit Siphon" take both
            // health and Souls from a kill; Warlock "Pact of Souls" feeds only the
            // pact (Mana).
            if matches!(
                p.class,
                Some(Class::Necromancer) | Some(Class::Spiritmaster)
            ) {
                let life = (p.max_hp() / 12).max(6);
                let souls = (p.max_resource / 8).max(5);
                p.hp = (p.hp + life).min(p.max_hp());
                p.resource = (p.resource + souls).min(p.max_resource);
            } else if p.class == Some(Class::Warlock) {
                let mana = (p.max_resource / 8).max(5);
                p.resource = (p.resource + mana).min(p.max_resource);
            }
        }
        self.roll_loot(user_id, &mob_name, loot, boss);
        // Titles used to mint one per distinct mob NAME on first kill - with 426
        // distinct regular foes in the world, that buried the handful of titles
        // that actually mean something under a wall of "Ratbane"/"Wolfbane"
        // clutter. Only bosses (139 named encounters) grant a themed "Bane of..."
        // title now; the Frontier zone "Champion of..." and final-boss lifetime
        // achievements already sit alongside this and stay untouched.
        if boss {
            self.grant_title(user_id, &mob_name, boss, mob_level);
        }
        // Bounty bounties: tick any accepted "slay N of X" board quest.
        self.bump_quests(user_id, |o| {
            u32::from(matches!(o, Objective::Bounty { name_contains, .. } if mob_name.contains(name_contains)))
        });
        // The starter chain's slay steps: the fight happened in the player's
        // own room, so its zone is the hunting ground.
        let here_zone = self
            .players
            .get(&user_id)
            .and_then(|p| self.world.room(p.room))
            .map(|r| r.zone);
        if let Some(here_zone) = here_zone {
            self.bump_starter_kill(user_id, &mob_name, here_zone);
        }
        if boss && let Some(zone) = super::world::frontier_zone_of_boss(&mob_name) {
            self.complete_quest(user_id, zone);
        }
        let achievement = boss_achievement_for(&mob_name);
        if let Some(achievement) = achievement {
            let line = match achievement.payout.is_some() {
                true => format!(
                    "Defeating {} pays chips once per character, and at most once every 7 days; the {} badge is yours the first time.",
                    achievement.mob_name,
                    award_badge(achievement.award_category, 1)
                ),
                false => format!(
                    "First defeat of {} can award the {} badge, once per account.",
                    achievement.mob_name,
                    award_badge(achievement.award_category, 1)
                ),
            };
            self.log_to(user_id, LogKind::Loot, line);
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

    // ---- What a kill pays: titles, quests, loot, and levels -------------

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
    /// award the "Champion of the ..." title plus an xp/gold bounty, both
    /// keyed to the level the zone is pitched at (`frontier_zone_level`).
    fn complete_quest(&mut self, user_id: Uuid, zone: usize) {
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
        // Never the level over the boss's head: that reads by bite and moves
        // with every retune of the ladder, and a one-time payout must not.
        let reward_level = super::world::frontier_zone_level(zone);
        let bonus_xp = (80 + reward_level * 24) as i64;
        let bonus_gold = (35 + reward_level * 6) as i64;
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
        self.award_title(user_id, format!("Champion of the {zname}"), reward_level);
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
        // Level-up is a moment: lead with a bold banner, then the per-level
        // detail. Full heal + resource already applied above.
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("★═══ LEVEL UP! You are now level {new_level}. ═══★"),
        );
        // Every level is a real reward: announce the concrete stat gains, any
        // ability learned, and the named milestone at every fifth level.
        let res_label = class.resource().label();
        for lvl in (old_level + 1)..=new_level {
            let cur = class.stats_at(lvl);
            let prev = class.stats_at(lvl - 1);
            let d_hp = (cur.max_hp + super::classes::milestone_hp_bonus(lvl))
                - (prev.max_hp + super::classes::milestone_hp_bonus(lvl - 1));
            let d_atk = cur.attack - prev.attack;
            let d_res = cur.max_resource - prev.max_resource;
            let mut gains = format!("+{d_hp} max HP, +{d_atk} attack");
            if d_res > 0 {
                gains.push_str(&format!(", +{d_res} {res_label}"));
            }
            self.log_to(
                user_id,
                LogKind::System,
                format!("Level {lvl} reached - {gains}."),
            );
            if let Some(a) = learned_at(class, lvl) {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("  New ability: {} - {}", a.name, a.desc),
                );
            }
            if let Some(name) = super::classes::level_milestone(lvl) {
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    format!(
                        "  ✦ Milestone - {name}! Hard-won growth toughens you (permanent +HP)."
                    ),
                );
                // Milestones are a big deal: the whole world hears of it.
                self.log_all(format!(
                    "A hero rises: an adventurer has reached the rank of {name}."
                ));
            }
            if lvl % super::stats::POINT_EVERY_LEVELS == 0 {
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    "  ✦ An attribute point is yours to place.".to_string(),
                );
            }
            if lvl == Class::MAX_LEVEL {
                self.log_to(
                    user_id,
                    LogKind::Loot,
                    "  ⚔ You have reached the pinnacle - level 50, the height of your calling. Few ever stand here.".to_string(),
                );
                self.log_all(
                    "The bells of Embergate ring: an adventurer has reached level 50, the pinnacle of their calling!"
                        .to_string(),
                );
            }
        }
    }

    // ---- Fleeing, and world-local chat ----------------------------------

    /// Restore a living mob to full: health, stuns, and every festering wound.
    /// Returns whether there was anything to shed, so callers can announce it
    /// only when it happened.
    fn recover_mob(&mut self, mob_id: u32) -> bool {
        let Some(m) = self.mobs.get_mut(&mob_id) else {
            return false;
        };
        if !m.alive {
            return false;
        }
        let wounded = m.hp < m.spawn.max_hp;
        m.hp = m.spawn.max_hp;
        m.untargeted = 0;
        let stunned = self.mob_stuns.remove(&mob_id).is_some_and(|t| t > 0);
        let festering = self.mob_dots.remove(&mob_id).is_some();
        let shed = wounded || stunned || festering;
        if shed {
            self.dirty = true;
            self.mark_world_dirty();
        }
        shed
    }

    /// The recovery sweep, once per tick after every round has resolved: a
    /// mob that has gone `MOB_RESET_TICKS` with nobody holding it as a target
    /// while wounded, stunned, or festering recovers in full, and everyone in
    /// its room is told. Covers the attacker dying, disconnecting, or walking
    /// off in any way `flee` does not see.
    fn recover_abandoned_mobs(&mut self) {
        let targeted: HashSet<u32> = self.players.values().filter_map(|p| p.target).collect();
        let mut due: Vec<u32> = Vec::new();
        for (id, m) in self.mobs.iter_mut() {
            if !m.alive || targeted.contains(id) {
                m.untargeted = 0;
                continue;
            }
            let afflicted = m.hp < m.spawn.max_hp
                || self.mob_stuns.get(id).is_some_and(|t| *t > 0)
                || self.mob_dots.contains_key(id);
            if !afflicted {
                m.untargeted = 0;
                continue;
            }
            m.untargeted = m.untargeted.saturating_add(1);
            if m.untargeted >= MOB_RESET_TICKS {
                due.push(*id);
            }
        }
        for mob_id in due {
            if !self.recover_mob(mob_id) {
                continue;
            }
            let (room, name) = match self.mobs.get(&mob_id) {
                Some(m) => (m.current_room, m.spawn.name.to_string()),
                None => continue,
            };
            let watchers: Vec<Uuid> = self
                .players
                .iter()
                .filter(|(_, p)| p.room == room)
                .map(|(id, _)| *id)
                .collect();
            for uid in watchers {
                self.log_to(
                    uid,
                    LogKind::Combat,
                    format!("{name} shakes off its wounds."),
                );
            }
        }
    }

    fn flee(&mut self, user_id: Uuid) {
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if !player.in_combat() {
            self.log_to(
                user_id,
                LogKind::Normal,
                "You're not fighting anything.".to_string(),
            );
            return;
        }
        let room_id = player.room;
        let fled_mob = player.target;
        // Turning your back on a foe is not free: it strikes once more as you
        // run, unless it is reeling from a stun. A blow that fells you ends
        // the flight where you stand (a death-save or veteran rising still
        // gets away).
        let parting = fled_mob.and_then(|mob_id| {
            let m = self.mobs.get(&mob_id)?;
            let reeling = self.mob_stuns.get(&mob_id).copied().unwrap_or(0) > 0;
            if !m.alive || m.current_room != room_id || reeling {
                return None;
            }
            Some((
                mob_id,
                m.spawn.damage,
                m.spawn.profile.attack_type,
                m.spawn.name.to_string(),
            ))
        });
        if let Some((_, dmg, dtype, name)) = parting {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("{name} strikes at your back as you run!"),
            );
            self.strike_player(user_id, dmg, dtype, &name);
            if self.players.get(&user_id).is_some_and(|p| p.dead) {
                return;
            }
        }
        let exit = self
            .world
            .room(room_id)
            .and_then(|r| r.exits.iter().next().map(|(dir, dest)| (*dir, *dest)));
        if let Some(player) = self.players.get_mut(&user_id) {
            player.target = None;
            player.pvp_target = None;
        }
        // The foe you leave recovers on the spot, unless someone else is still
        // fighting it: whittling a boss down across engagements is not a
        // strategy, it is the hole this closes.
        if let Some(mob_id) = fled_mob {
            let still_fought = self.players.values().any(|p| p.target == Some(mob_id));
            if !still_fought && self.recover_mob(mob_id) {
                let name = self
                    .mobs
                    .get(&mob_id)
                    .map(|m| m.spawn.name.to_string())
                    .unwrap_or_default();
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{name} shakes off its wounds as you run."),
                );
            }
        }
        match exit {
            Some((dir, dest)) => {
                if let Some(player) = self.players.get_mut(&user_id) {
                    player.previous_room = Some(room_id);
                    player.room = dest;
                    Arc::make_mut(&mut player.visited).insert(dest);
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

    /// Speak a line, scoped by an optional leading channel marker: `/z ` or
    /// `/zone ` for everyone in the same named zone, `/w ` or `/world ` for
    /// every adventurer currently in Lateania. No marker means the room, same
    /// as it always has. Whichever scope, this is still world-local chat - it
    /// never reaches late.sh's own global feed.
    fn say(&mut self, user_id: Uuid, message: &str) {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }
        // A marker only counts if it's the whole word ("/zone"/"/z" followed
        // by whitespace or nothing) - "/zebra" or "/zealous" fall through as
        // plain room speech instead of being mistaken for a scope marker.
        let after_marker = |rest: &str| rest.is_empty() || rest.starts_with(char::is_whitespace);
        let (scope, body) = if let Some(rest) = trimmed
            .strip_prefix("/zone")
            .or_else(|| trimmed.strip_prefix("/z"))
            .filter(|rest| after_marker(rest))
        {
            (ChatScope::Zone, rest.trim())
        } else if let Some(rest) = trimmed
            .strip_prefix("/world")
            .or_else(|| trimmed.strip_prefix("/w"))
            .filter(|rest| after_marker(rest))
        {
            (ChatScope::World, rest.trim())
        } else {
            (ChatScope::Room, trimmed)
        };
        if body.is_empty() {
            return;
        }
        let Some(room_id) = self.players.get(&user_id).map(|p| p.room) else {
            return;
        };
        // Zone/world scopes carry to players who never chose to stand near
        // you; hold each voice to one broadcast per cooldown window.
        if !matches!(scope, ChatScope::Room) {
            let now = Instant::now();
            let held = self
                .players
                .get(&user_id)
                .and_then(|p| p.last_broadcast)
                .is_some_and(|last| now.duration_since(last) < BROADCAST_COOLDOWN);
            if held {
                self.log_to(
                    user_id,
                    LogKind::System,
                    "You've just called out - give the echo a breath before shouting again."
                        .to_string(),
                );
                return;
            }
            if let Some(p) = self.players.get_mut(&user_id) {
                p.last_broadcast = Some(now);
            }
        }
        let recipients: Vec<Uuid> = match scope {
            ChatScope::Room => self
                .players
                .iter()
                .filter(|(_, p)| p.room == room_id)
                .map(|(id, _)| *id)
                .collect(),
            ChatScope::Zone => {
                let Some(zone) = self.world.room(room_id).map(|r| r.zone) else {
                    return;
                };
                self.players
                    .iter()
                    .filter(|(_, p)| self.world.room(p.room).is_some_and(|r| r.zone == zone))
                    .map(|(id, _)| *id)
                    .collect()
            }
            ChatScope::World => self.players.keys().copied().collect(),
        };
        let (self_verb, other_verb) = match scope {
            ChatScope::Room => ("You say", "Someone says"),
            ChatScope::Zone => ("You say to the zone", "Someone says to the zone"),
            ChatScope::World => (
                "You say to all of Lateania",
                "Someone says to all of Lateania",
            ),
        };
        for recipient in recipients {
            let prefix = if recipient == user_id {
                self_verb
            } else {
                other_verb
            };
            self.log_to(recipient, LogKind::Say, format!("{prefix}: {body}"));
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

    /// Take off a worn item and put it back in the pack. The inventory panel
    /// lists equipped gear alongside loose gear, so Enter on a worn row has to
    /// mean "take this off"; without this it fell through to `equip`, which
    /// found the item missing from the pack and returned in silence.
    fn unequip(&mut self, user_id: Uuid, item_id: u32) {
        let Some(it) = item(item_id) else { return };
        let Some(slot) = it.slot() else { return };
        let worn = self
            .players
            .get(&user_id)
            .map(|p| p.equipped.get(&slot) == Some(&item_id))
            .unwrap_or(false);
        if !worn {
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.equipped.remove(&slot);
            p.inventory.push(item_id);
            // Losing the gear can lower max hp, so vitals follow it down.
            let max = p.max_hp();
            p.hp = p.hp.min(max);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You take off {} ({}).", it.name, slot.label()),
        );
    }

    /// Drink the best healing potion in one keystroke, for use mid-fight. Picks
    /// the *smallest* potion that still fills the health you're missing so a big
    /// draught isn't wasted on a scratch; if none is large enough, drinks the
    /// biggest you have. Only healing (heal > 0) consumables count - mana
    /// draughts and poisons are left alone.
    fn quaff_best(&mut self, user_id: Uuid) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        let missing = p.max_hp() - p.hp;
        if missing <= 0 {
            self.log_to(
                user_id,
                LogKind::System,
                "You are already at full health.".to_string(),
            );
            return;
        }
        // Smallest heal that covers the gap, else the largest heal available.
        let mut covering: Option<(u32, i32)> = None;
        let mut biggest: Option<(u32, i32)> = None;
        for &id in &p.inventory {
            let Some(it) = item(id) else { continue };
            let ItemKind::Consumable { heal, .. } = it.kind else {
                continue;
            };
            if heal <= 0 {
                continue;
            }
            if heal >= missing && covering.is_none_or(|(_, h)| heal < h) {
                covering = Some((id, heal));
            }
            if biggest.is_none_or(|(_, h)| heal > h) {
                biggest = Some((id, heal));
            }
        }
        match covering.or(biggest) {
            Some((id, _)) => self.use_item(user_id, id),
            None => self.log_to(
                user_id,
                LogKind::System,
                "You have no potion to drink.".to_string(),
            ),
        }
    }

    fn use_item(&mut self, user_id: Uuid, item_id: u32) {
        let Some(it) = item(item_id) else { return };
        // Poisons and oils aren't drunk - they coat your weapon.
        if let Some(tier) = super::items::poison_tier(item_id) {
            let per_tick = POISON_PER_TICK[(tier as usize).min(POISON_PER_TICK.len() - 1)];
            self.coat_weapon(
                user_id,
                item_id,
                DamageType::Poison,
                per_tick,
                POISON_CHARGES,
            );
            return;
        }
        if let Some((school, tier)) = super::items::oil_school_tier(item_id) {
            let per_tick = OIL_PER_TICK[(tier as usize).min(OIL_PER_TICK.len() - 1)];
            self.coat_weapon(user_id, item_id, school, per_tick, OIL_CHARGES);
            return;
        }
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
        let queasy = self.players.get(&user_id).map(|p| p.quaff_cd).unwrap_or(0);
        if queasy > 0 {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "You are still queasy from the last draught ({}s).",
                    u64::from(queasy) * TICK_SECS
                ),
            );
            return;
        }
        // Cooked food grants a well-fed regen on top of its immediate heal, and
        // so do the rarest Sunderlakes fish (their "special" - see fish_well_fed).
        let well_fed = super::items::food_tier(item_id)
            .map(|t| 2 + t as i32)
            .or_else(|| super::items::fish_well_fed(item_id));
        if let Some(p) = self.players.get_mut(&user_id) {
            if let Some(pos) = p.inventory.iter().position(|i| *i == item_id) {
                p.inventory.remove(pos);
            }
            let max = p.max_hp();
            p.hp = (p.hp + heal).min(max);
            p.resource = (p.resource + restore).min(p.max_resource);
            p.quaff_cd = QUAFF_COOLDOWN_TICKS;
            if let Some(regen) = well_fed {
                p.self_effects.push(ActiveEffect {
                    kind: AbilityEffect::HealOverTime,
                    magnitude: regen,
                    remaining: WELL_FED_TICKS,
                });
            }
        }
        let verb = if well_fed.is_some() { "eat" } else { "use" };
        self.log_to(user_id, LogKind::Loot, format!("You {verb} {}.", it.name));
        self.dirty = true;
    }

    /// Coat the player's weapon with a poison or an oil: each landed melee hit
    /// will leave a DoT of the coat's school until the charges run out. One
    /// coat slot: applying a new coat replaces the old. Consumes the vial.
    fn coat_weapon(
        &mut self,
        user_id: Uuid,
        item_id: u32,
        school: DamageType,
        per_tick: i32,
        charges: u8,
    ) {
        let has = self
            .players
            .get(&user_id)
            .map(|p| p.inventory.contains(&item_id))
            .unwrap_or(false);
        if !has {
            return;
        }
        let name = item(item_id).map(|i| i.name).unwrap_or("coating");
        if let Some(p) = self.players.get_mut(&user_id) {
            if let Some(pos) = p.inventory.iter().position(|i| *i == item_id) {
                p.inventory.remove(pos);
            }
            p.weapon_coat = Some((school, per_tick, charges));
        }
        self.log_to(
            user_id,
            LogKind::Combat,
            format!("You coat your weapon with {name} ({charges} strikes)."),
        );
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
        let (gold, price) = match self.players.get(&user_id) {
            Some(p) => (p.gold, p.buy_price(it)),
            None => return,
        };
        if gold < price {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You can't afford {} ({price}g).", it.name),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= price;
            p.inventory.push(item_id);
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You buy {} for {price}g.", it.name),
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
        let price = match self.players.get(&user_id) {
            Some(p) => p.sell_price(it),
            None => return,
        };
        // Worn gear is listed in the inventory panel but lives in `equipped`,
        // so say why rather than doing nothing. A loose duplicate in the pack
        // is still fair game even while the other copy is worn.
        let worn = self
            .players
            .get(&user_id)
            .map(|p| {
                p.equipped.values().any(|id| *id == item_id) && !p.inventory.contains(&item_id)
            })
            .unwrap_or(false);
        if worn {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You'd have to take off {} first.", it.name),
            );
            return;
        }
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

    /// Which kind of batch-sell was requested.
    fn sell_batch(&mut self, user_id: Uuid, kind: SellBatch) {
        if shop_at(self.players.get(&user_id).map(|p| p.room).unwrap_or(0)).is_none() {
            self.log_to(
                user_id,
                LogKind::System,
                "You need a merchant to sell.".to_string(),
            );
            return;
        }
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        // Decide which pack items to sell. Equipped gear and consumables are
        // always kept; the batch only touches loose inventory.
        let doomed: Vec<u32> = p
            .inventory
            .iter()
            .copied()
            .filter(|id| {
                let Some(it) = item(*id) else { return false };
                match it.kind {
                    ItemKind::Consumable { .. } => false, // never dump potions
                    ItemKind::Utility => false,           // never dump poisons/buff items either
                    ItemKind::Valuable => true,           // pure sell-fodder, always goes
                    ItemKind::Equipment(_) => match kind {
                        SellBatch::All => true,
                        SellBatch::Common => it.rarity == super::items::Rarity::Common,
                        // "won't improve the character": not an upgrade over worn gear.
                        SellBatch::NonUpgrades => !p.is_upgrade(it),
                    },
                }
            })
            .collect();
        if doomed.is_empty() {
            self.log_to(
                user_id,
                LogKind::System,
                "Nothing to sell that way.".to_string(),
            );
            return;
        }
        let mut count = 0;
        let mut total = 0;
        if let Some(p) = self.players.get_mut(&user_id) {
            for id in &doomed {
                if let Some(pos) = p.inventory.iter().position(|i| i == id) {
                    p.inventory.remove(pos);
                    let price = item(*id).map(|it| p.sell_price(it)).unwrap_or(1);
                    p.gold += price;
                    total += price;
                    count += 1;
                }
            }
        }
        let what = match kind {
            SellBatch::All => "loose gear and valuables",
            SellBatch::Common => "common items",
            SellBatch::NonUpgrades => "items that wouldn't improve you",
        };
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You sell {count} {what} for {total}g."),
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
                for dot in stacks.iter_mut() {
                    if dot.remaining > 0 {
                        total += dot.per_tick;
                        dot.remaining -= 1;
                        owner = Some(dot.owner);
                    }
                }
                stacks.retain(|dot| dot.remaining > 0);
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

        // A lingering corpse whose deadline has passed is drawn back to the
        // temple automatically (the player never released and no one revived
        // them in time).
        let auto_released: Vec<Uuid> = self
            .players
            .iter()
            .filter(|(_, p)| p.respawn_at.is_some_and(|at| now >= at))
            .map(|(id, _)| *id)
            .collect();
        for user_id in auto_released {
            self.send_to_temple(
                user_id,
                "Your spirit slips free and you wake at the Temple of the Dawn, restored.",
            );
        }

        // Per-player upkeep: regen, buff/shield/effect timers, cooldowns.
        let player_ids: Vec<Uuid> = self.players.keys().copied().collect();
        for uid in &player_ids {
            let mut hot_heal = 0;
            if let Some(p) = self.players.get_mut(uid) {
                if p.class.is_some() && p.respawn_at.is_none() {
                    p.resource = (p.resource + p.regen()).min(p.max_resource);
                    // Bard "Battle Hymn" and Skald "War-Chant": Tempo keeps perfect
                    // time and returns faster than other resources.
                    if matches!(p.class, Some(Class::Bard) | Some(Class::Skald)) {
                        let beat = 2 + p.level / 10;
                        p.resource = (p.resource + beat).min(p.max_resource);
                    }
                    // Druid "Nature's Renewal" and Paladin "Aura of Devotion" both
                    // mend a little health every tick (the Druid a touch more).
                    let mend = match p.class {
                        Some(Class::Druid) => 2 + p.level / 8,
                        Some(Class::Paladin) => 1 + p.level / 12,
                        _ => 0,
                    };
                    if mend > 0 && p.hp < p.max_hp() {
                        p.hp = (p.hp + mend).min(p.max_hp());
                    }
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
                if p.quaff_cd > 0 {
                    p.quaff_cd -= 1;
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
            let (mob_id, base_atk, opening, frenzy_pct, class, crit_pct) =
                match self.players.get(&user_id) {
                    Some(p) => {
                        // Berserker "Frenzy": the more it bleeds the harder it
                        // swings, half a percent of damage per percent of health
                        // missing, up to +50% at death's door. It used to start
                        // only below half health, which a fighter who drinks under
                        // 40% almost never sees: a trait gated past the point of
                        // use, and the Berserker read as a Warrior with less HP.
                        let frenzy = if p.class == Some(Class::Berserker) {
                            let max = p.max_hp().max(1);
                            let missing = ((max - p.hp).max(0) * 100) / max;
                            (missing / 2).clamp(0, 50)
                        } else {
                            0
                        };
                        (
                            p.target,
                            p.swing(),
                            p.opening_strike,
                            frenzy,
                            p.class,
                            p.scores.crit_pct(),
                        )
                    }
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
            // Ranger "Hunter's Instinct": strikes against a wounded foe (below half
            // health) bite harder, on auto-attacks as well as abilities.
            let ranger_wounded = class == Some(Class::Ranger)
                && self
                    .mobs
                    .get(&mob_id)
                    .is_some_and(|m| m.hp * 2 < m.spawn.max_hp);
            // Opportunist: the Rogue's opening strike of a fight lands as a crit.
            let player_atk = if opening { base_atk * 2 } else { base_atk };
            // Berserker Frenzy scales the blow up as health runs low.
            let player_atk = player_atk * (100 + frenzy_pct) / 100;
            // Hunter's Instinct: extra damage into the wounded foe.
            let player_atk = if ranger_wounded {
                player_atk + player_atk / 4
            } else {
                player_atk
            };
            // Dexterity: the swing may crit for double, or, below 10, glance
            // for half.
            let roll = rand::thread_rng().gen_range(0..100);
            let (player_atk, dex_line) = match crit_outcome(crit_pct, roll) {
                CritOutcome::Plain => (player_atk, None),
                CritOutcome::Critical => (
                    player_atk * 2,
                    Some("Critical hit! Your swing lands for double."),
                ),
                CritOutcome::Glancing => (
                    player_atk / 2,
                    Some("A glancing blow. Your swing lands for half."),
                ),
            };
            if let Some(line) = dex_line {
                self.log_to(user_id, LogKind::Combat, line.to_string());
            }
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
            let (mob_name, dealt, defense, dead, big_hit, staggered) = {
                let Some(mob) = self.mobs.get_mut(&mob_id) else {
                    continue;
                };
                let (dealt, defense) = mob.spawn.profile.apply(player_atk, DamageType::Physical);
                let hp_before = mob.hp;
                mob.hp -= dealt;
                self.dirty = true;
                (
                    mob.spawn.name.to_string(),
                    dealt,
                    defense,
                    mob.hp <= 0,
                    dealt * 4 >= mob.spawn.max_hp,
                    hp_before * 4 > mob.spawn.max_hp
                        && mob.hp * 4 <= mob.spawn.max_hp
                        && mob.hp > 0,
                )
            };
            self.mark_world_dirty();
            let tag = defense_tag(defense, DamageType::Physical);
            // A blow worth a quarter of the foe's whole life deserves a louder
            // sentence than the tick-by-tick chip damage.
            let verb = if big_hit { "crush into" } else { "strike" };
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("You {verb} {mob_name} for {dealt} physical{tag}."),
            );
            if staggered {
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{mob_name} staggers - the fight turns your way!"),
                );
            }
            // Valewalker "Reaping Harvest": each landed melee strike draws a little
            // of the wild's vigour back into the reaper.
            if class == Some(Class::Valewalker) {
                let mend = self
                    .players
                    .get(&user_id)
                    .map(|p| (3 + p.level / 4).max(1))
                    .unwrap_or(0);
                if mend > 0 {
                    self.heal_player(user_id, mend);
                }
            }
            if dead {
                self.kill_mob(user_id, mob_id);
                continue;
            }
            // A coated weapon (poison or oil) leaves a festering DoT of the
            // coat's school in the struck foe, through the foe's resist/weak
            // profile, and spends one charge (the target is the player's
            // current mob).
            let coat = self.players.get(&user_id).and_then(|p| p.weapon_coat);
            if let Some((school, per_tick, charges)) = coat {
                self.seed_mob_dot(
                    user_id,
                    per_tick,
                    school,
                    POISON_DOT_TICKS,
                    DotSource::Coat,
                    coat_source(school),
                );
                if let Some(p) = self.players.get_mut(&user_id) {
                    let left = charges.saturating_sub(1);
                    p.weapon_coat = (left > 0).then_some((school, per_tick, left));
                }
                if charges <= 1 {
                    self.log_to(
                        user_id,
                        LogKind::System,
                        "The last of the coating is spent.".to_string(),
                    );
                }
            }
            // A living, fighting companion piles onto the same target. If its
            // bite finishes the foe, the kill is credited to its owner. A
            // Beastlord's "Pack Bond" empowers that companion (see pet_power_pct).
            let pet_bonus = if class == Some(Class::Beastlord) {
                BEASTLORD_PET_PCT
            } else {
                0
            };
            if let Some((pet_glyph, pet_name, pet_atk, pet_level, pet_skills)) = self
                .players
                .get(&user_id)
                .and_then(|p| p.pet.as_ref().map(|pet| (pet, p.attack_rating())))
                .filter(|(pet, _)| !pet.downed)
                .map(|(pet, rating)| {
                    let bite = pet.attack() + rating * PET_COEF_PCT / 100;
                    (
                        pet.species.glyph,
                        pet.species.name,
                        bite + bite * pet_bonus / 100,
                        pet.level(),
                        pet.species.skills,
                    )
                })
            {
                let (pet_dealt, pet_dead) = {
                    let Some(mob) = self.mobs.get_mut(&mob_id) else {
                        continue;
                    };
                    let (dealt, _) = mob.spawn.profile.apply(pet_atk, DamageType::Physical);
                    mob.hp -= dealt;
                    self.dirty = true;
                    (dealt, mob.hp <= 0)
                };
                self.mark_world_dirty();
                self.log_to(
                    user_id,
                    LogKind::Combat,
                    format!("{pet_glyph} Your {pet_name} tears into {mob_name} for {pet_dealt}."),
                );
                if pet_dead {
                    self.kill_mob(user_id, mob_id);
                    continue;
                }
                // The companion's level-gated auto-skills fire here, each on its
                // own cooldown (savage bite / rend / roar / guard / pounce).
                let beastlord = class == Some(Class::Beastlord);
                if self.fire_pet_skills(
                    user_id, mob_id, pet_level, pet_atk, pet_name, &mob_name, pet_skills, beastlord,
                ) {
                    // A killing pounce may have finished the foe.
                    continue;
                }
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

        // Resolve a combat round for each pvp-engaged player: the same shape
        // as the mob loop above, but the foe is another adventurer. Both
        // sides of a duel carry their own `pvp_target` (set on the victim by
        // `engage_player`'s auto-retaliation), so two duelling players each
        // land a blow this same tick, same as trading blows with a mob.
        let pvp_fighters: Vec<(Uuid, Uuid)> = self
            .players
            .iter()
            .filter(|(_, p)| p.pvp_target.is_some() && p.respawn_at.is_none())
            .filter_map(|(id, p)| p.pvp_target.map(|t| (*id, t)))
            .collect();

        for (attacker_id, victim_id) in pvp_fighters {
            // Snapshot everything needed from the attacker up front so no
            // live immutable borrow survives into the `get_mut` calls below.
            let Some((room_id, atk_class, opening, atk_hp, atk_max_hp, base_atk, crit_pct)) =
                self.players.get(&attacker_id).map(|a| {
                    (
                        a.room,
                        a.class,
                        a.opening_strike,
                        a.hp,
                        a.max_hp(),
                        a.swing(),
                        a.scores.crit_pct(),
                    )
                })
            else {
                continue;
            };
            let room_is_pvp = self.world.room(room_id).is_some_and(|r| r.pvp);
            let valid_victim = self.players.get(&victim_id).is_some_and(|v| {
                room_is_pvp && v.room == room_id && v.respawn_at.is_none() && v.class.is_some()
            });
            if !valid_victim {
                // The foe left, died, changed rooms, or the ground stopped
                // being contested (e.g. dragged into a safe room). Drop the
                // duel quietly, same as a mob fight ending.
                if let Some(a) = self.players.get_mut(&attacker_id) {
                    a.pvp_target = None;
                }
                continue;
            }
            // A stunned adventurer skips their own swing this round, same as
            // a stunned mob does.
            let stunned = self.pvp_stuns.get(&attacker_id).copied().unwrap_or(0) > 0;
            if let Some(v) = self.pvp_stuns.get_mut(&attacker_id)
                && *v > 0
            {
                *v -= 1;
            }
            if stunned {
                self.log_to(
                    attacker_id,
                    LogKind::Combat,
                    "You are stunned and cannot strike.".to_string(),
                );
                continue;
            }
            let ranger_wounded = atk_class == Some(Class::Ranger)
                && self
                    .players
                    .get(&victim_id)
                    .is_some_and(|v| v.hp * 2 < v.max_hp());
            let frenzy_pct = if atk_class == Some(Class::Berserker) {
                let missing = ((atk_max_hp - atk_hp).max(0) * 100) / atk_max_hp.max(1);
                (missing / 2).clamp(0, 50)
            } else {
                0
            };
            let atk = if opening { base_atk * 2 } else { base_atk };
            let atk = atk * (100 + frenzy_pct) / 100;
            let atk = if ranger_wounded { atk + atk / 4 } else { atk };
            let roll = rand::thread_rng().gen_range(0..100);
            let (atk, dex_line) = match crit_outcome(crit_pct, roll) {
                CritOutcome::Plain => (atk, None),
                CritOutcome::Critical => {
                    (atk * 2, Some("Critical hit! Your swing lands for double."))
                }
                CritOutcome::Glancing => {
                    (atk / 2, Some("A glancing blow. Your swing lands for half."))
                }
            };
            if let Some(line) = dex_line {
                self.log_to(attacker_id, LogKind::Combat, line.to_string());
            }
            if opening && let Some(a) = self.players.get_mut(&attacker_id) {
                a.opening_strike = false;
            }
            self.log_to(
                attacker_id,
                LogKind::Combat,
                format!("You strike your rival for {atk} physical."),
            );
            self.strike_pvp_target(attacker_id, victim_id, atk, DamageType::Physical, "a rival");
            if self.players.get(&victim_id).is_some_and(|v| v.dead) {
                continue;
            }
            // A coated weapon works in a duel exactly as against a mob: the
            // landed swing seeds a DoT of the coat's school on the rival
            // (through their armor each tick, via the pvp dot pass) and
            // spends a charge.
            let coat = self.players.get(&attacker_id).and_then(|p| p.weapon_coat);
            if let Some((school, per_tick, charges)) = coat {
                self.seed_pvp_dot(
                    attacker_id,
                    per_tick,
                    school,
                    POISON_DOT_TICKS,
                    DotSource::Coat,
                    coat_source(school),
                );
                if let Some(p) = self.players.get_mut(&attacker_id) {
                    let left = charges.saturating_sub(1);
                    p.weapon_coat = (left > 0).then_some((school, per_tick, left));
                }
                if charges <= 1 {
                    self.log_to(
                        attacker_id,
                        LogKind::System,
                        "The last of the coating is spent.".to_string(),
                    );
                }
            }
            // A living, fighting companion piles onto the same target, same as
            // it does against a mob - biting through `strike_pvp_target` so it
            // respects the victim's armor/shield/death-save exactly like a
            // player's own blow does.
            let pet_bonus = if atk_class == Some(Class::Beastlord) {
                BEASTLORD_PET_PCT
            } else {
                0
            };
            if let Some((pet_glyph, pet_name, pet_atk, pet_level, pet_skills)) = self
                .players
                .get(&attacker_id)
                .and_then(|p| p.pet.as_ref().map(|pet| (pet, p.attack_rating())))
                .filter(|(pet, _)| !pet.downed)
                .map(|(pet, rating)| {
                    let bite = pet.attack() + rating * PET_COEF_PCT / 100;
                    (
                        pet.species.glyph,
                        pet.species.name,
                        bite + bite * pet_bonus / 100,
                        pet.level(),
                        pet.species.skills,
                    )
                })
            {
                self.log_to(
                    attacker_id,
                    LogKind::Combat,
                    format!("{pet_glyph} Your {pet_name} tears into your rival for {pet_atk}."),
                );
                self.strike_pvp_target(
                    attacker_id,
                    victim_id,
                    pet_atk,
                    DamageType::Physical,
                    "your companion",
                );
                if self.players.get(&victim_id).is_some_and(|v| v.dead) {
                    continue;
                }
                let beastlord = atk_class == Some(Class::Beastlord);
                if self.fire_pet_skills_pvp(
                    attacker_id,
                    victim_id,
                    pet_level,
                    pet_atk,
                    pet_name,
                    pet_skills,
                    beastlord,
                ) {
                    continue;
                }
            }
        }

        // Pvp damage-over-time from player abilities (poison, DoT spells).
        // Same shape as the mob DoT pass above, but the victim is a player,
        // so each tick routes through `strike_pvp_target` for full armor/
        // shield/death handling instead of a raw hp subtraction.
        let pvp_dot_victims: Vec<Uuid> = self.pvp_dots.keys().copied().collect();
        for victim_id in pvp_dot_victims {
            let mut ticks: Vec<(Uuid, i32, DamageType)> = Vec::new();
            if let Some(stacks) = self.pvp_dots.get_mut(&victim_id) {
                for dot in stacks.iter_mut() {
                    if dot.remaining > 0 {
                        ticks.push((dot.owner, dot.per_tick, dot.school));
                        dot.remaining -= 1;
                    }
                }
                stacks.retain(|dot| dot.remaining > 0);
                if stacks.is_empty() {
                    self.pvp_dots.remove(&victim_id);
                }
            }
            for (attacker_id, per, dtype) in ticks {
                let alive = self.players.get(&victim_id).is_some_and(|v| !v.dead);
                if !alive {
                    continue;
                }
                self.strike_pvp_target(attacker_id, victim_id, per, dtype, "A lingering wound");
            }
        }

        // Foes nobody is fighting any more shed their wounds (see MOB_RESET_TICKS).
        self.recover_abandoned_mobs();

        // No idle timeout: a player stays put in Lateania for as long as their
        // session is actually open, however long they go without touching a
        // key. Real disconnects/leaving the door are already handled by
        // `leave_task`, which is the genuine cleanup path - an inactivity
        // clock on top of that only ever punished someone reading a long room
        // description or stepping away for a call.
        if self.dirty {
            self.generation = self.generation.wrapping_add(1);
        }
        TickOutput {
            kills: std::mem::take(&mut self.pending_kills),
        }
    }

    // ---- Mobs between rounds: the world boss, roaming, behaviour --------

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
                untargeted: 0,
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
                untargeted: 0,
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
        // Monk "Iron Body": the trained body blunts physical blows.
        if p.class == Some(Class::Monk) && dtype == DamageType::Physical {
            dmg = (dmg - dmg * IRON_BODY_PCT / 100).max(1);
        }
        // Tank-archetype mitigation reduces every incoming blow.
        let (_, mitigation_pct, _, _) = p.archetype_mods();
        if mitigation_pct > 0 {
            dmg = (dmg - dmg * mitigation_pct / 100).max(1);
        }
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
                self.wound_pet(user_id, dmg);
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
                self.wound_pet(user_id, dmg);
                return false;
            }
            // No save and no charge left: the player falls and becomes a corpse
            // where they stand. Their spirit lingers - a healer may resurrect
            // them, or they can release to the temple - until the linger
            // deadline draws them back automatically.
            p.hp = 0;
            p.target = None;
            p.shield = 0;
            p.empower = 0;
            p.dead = true;
            p.respawn_at = Some(now + Duration::from_secs(CORPSE_LINGER_SECS));
            let lost_escort = p.escort.take().map(|e| e.name);
            let lost_gold = carried_gold_death_loss(p.gold);
            if lost_gold > 0 {
                p.gold -= lost_gold;
            }
            let death_message = if lost_gold > 0 {
                format!(
                    "You have fallen! Your spirit lingers by your corpse (you lose {lost_gold} carried gold). Wait for a resurrection, or press r to release to the temple."
                )
            } else {
                "You have fallen! Your spirit lingers by your corpse. Wait for a resurrection, or press r to release to the temple.".to_string()
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
            self.wound_pet(user_id, dmg);
            true
        }
    }

    // ---- Death, the temple, and resurrection ----------------------------

    /// Send a (usually dead) player to the Temple of the Dawn, fully restored,
    /// clearing the corpse state. Shared by the auto-release tick and the manual
    /// release action. A fallen escort cannot be led from beyond the temple.
    fn send_to_temple(&mut self, user_id: Uuid, message: &str) {
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
            player.dead = false;
            player.death_save_used = false;
            player.shield = 0;
            player.empower = 0;
            player.escort = None;
        }
        self.log_to(user_id, LogKind::System, message.to_string());
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

    /// Release a lingering spirit to the temple now, instead of waiting for a
    /// resurrection. No-op unless the player is currently a corpse.
    fn release_to_temple(&mut self, user_id: Uuid) {
        if !self.players.get(&user_id).is_some_and(|p| p.dead) {
            return;
        }
        self.send_to_temple(
            user_id,
            "You release your hold on the world and wake at the Temple of the Dawn, restored.",
        );
    }

    /// Perform the Resurrection rite: a capable, living caster calls the nearest
    /// fallen adventurer in their room back to life where they lie. Costs
    /// resource and revives the target at a fraction of full vitality.
    fn resurrect_nearest(&mut self, user_id: Uuid) {
        // The caster must be alive, classed with the rite, and able to pay.
        let caster = match self.players.get(&user_id) {
            Some(p) if !p.dead => p,
            _ => return,
        };
        let room = caster.room;
        let can = caster.class.is_some_and(|c| c.can_resurrect());
        if !can {
            self.log_to(
                user_id,
                LogKind::System,
                "You do not command the Resurrection rite.".to_string(),
            );
            return;
        }
        if caster.resource < RESURRECT_COST {
            self.log_to(
                user_id,
                LogKind::System,
                format!("You need {RESURRECT_COST} to perform the rite."),
            );
            return;
        }
        // The nearest fallen adventurer in the room (deterministic by id).
        let mut corpses: Vec<Uuid> = self
            .players
            .values()
            .filter(|p| p.dead && p.room == room && p.user_id != user_id)
            .map(|p| p.user_id)
            .collect();
        corpses.sort();
        let Some(target_id) = corpses.first().copied() else {
            self.log_to(
                user_id,
                LogKind::System,
                "No fallen adventurer lies here to resurrect.".to_string(),
            );
            return;
        };
        if let Some(caster) = self.players.get_mut(&user_id) {
            caster.resource -= RESURRECT_COST;
        }
        if let Some(target) = self.players.get_mut(&target_id) {
            target.dead = false;
            target.respawn_at = None;
            target.death_save_used = false;
            target.shield = 0;
            target.empower = 0;
            let max = target.max_hp();
            target.hp = (max * RESURRECT_HP_PCT / 100).max(1);
            target.resource = (target.max_resource * RESURRECT_HP_PCT / 100).max(0);
        }
        self.log_to(
            user_id,
            LogKind::Combat,
            "You speak the Resurrection rite and call a fallen adventurer back to life!"
                .to_string(),
        );
        self.log_to(
            target_id,
            LogKind::System,
            "A surge of holy light pulls you back from death - you live again, where you fell."
                .to_string(),
        );
        self.describe_room(target_id);
        self.dirty = true;
        self.mark_world_dirty();
    }

    // ---- Companions: the stable, feeding, and wounds --------------------

    /// Whether a companion Stable stands in this room.
    fn room_has_stable(&self, room: RoomId) -> bool {
        features_at(room)
            .iter()
            .any(|f| f.kind == FeatureKind::Stable)
    }

    /// Buy a companion of `species_key` at the Stable in the player's room. A new
    /// purchase replaces any current companion (it returns to the wild).
    fn buy_pet(&mut self, user_id: Uuid, species_key: &str) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        if !self.room_has_stable(p.room) {
            self.log_to(
                user_id,
                LogKind::System,
                "You must be at a stable to buy a companion.".to_string(),
            );
            return;
        }
        let Some(species) = pet_species_by_key(species_key) else {
            return;
        };
        if p.gold < species.price {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "The {} costs {} gold - more than you carry.",
                    species.name, species.price
                ),
            );
            return;
        }
        let released = p.pet.map(|old| old.species.name);
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= species.price;
            p.pet = Some(Pet::new(species, 0));
        }
        if let Some(old) = released {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Your {old} is set loose and pads off into the wild."),
            );
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "{} {} answers to you now. Lead it well - feed it here to make it stronger.",
                species.glyph, species.name
            ),
        );
        self.dirty = true;
    }

    /// Feed the player's companion, or a wild adoptable critter sharing the
    /// room if one is here and no stray has been won over yet (Genesys) -
    /// one key, whichever feeding actually matters right now. An owned pet
    /// that's hurt or downed always comes first: courting a stray is a
    /// patient side project, never a reason to leave a real emergency
    /// unfed.
    fn feed_pet(&mut self, user_id: Uuid) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        let room = p.room;
        let pet_needs_care = p
            .pet
            .as_ref()
            .is_some_and(|pet| pet.downed || pet.hp < pet.max_hp());
        if !pet_needs_care && p.stray.is_none() && critters_at(room).iter().any(|c| c.adoptable) {
            self.feed_wild_critter(user_id, room);
            return;
        }
        self.feed_owned_pet(user_id);
    }

    /// Court a wild adoptable critter (Genesys): feed it once a day, several
    /// days running, and it takes to you as a stray companion - on top of
    /// whatever pet you already keep. Miss a day and it grows wary again.
    fn feed_wild_critter(&mut self, user_id: Uuid, room: RoomId) {
        let Some(critter) = critters_at(room).into_iter().find(|c| c.adoptable) else {
            return;
        };
        let Some(idx) = critter_index(critter) else {
            return;
        };
        let name = critter.name;
        let today = now_unix_secs() / 86_400;
        let bond = self.players.get(&user_id).and_then(|p| p.stray_bond);
        let same_critter = matches!(bond, Some((bi, ..)) if bi == idx);
        let already_today = matches!(bond, Some((bi, _, ld)) if bi == idx && ld == today);

        // The streak tracks real calendar days (UTC midnight), not the
        // visible in-game Dawn/Day/Dusk/Night clock (which cycles every
        // ~16 minutes) - the two are easy to conflate, so every message here
        // spells out the concrete real-world countdown rather than just
        // saying "today"/"tomorrow" and leaving the player to guess.
        let until_reset = time_until_next_utc_day();
        if already_today {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "You've already fed {name} today. The day resets at midnight UTC, in {until_reset} - come back after that."
                ),
            );
            return;
        }

        let (new_bond, message, adopted) = match bond {
            Some((_, streak, last_day)) if same_critter && last_day + 1 == today => {
                let new_streak = streak + 1;
                if new_streak >= STRAY_ADOPTION_DAYS {
                    (
                        None,
                        format!(
                            "{name} nuzzles up against you and follows without hesitation - after {STRAY_ADOPTION_DAYS} days of care, you've won it over. A new stray companion, alongside anything else you keep."
                        ),
                        true,
                    )
                } else {
                    (
                        Some((idx, new_streak, today)),
                        format!(
                            "You feed {name} again. It trusts you a little more. ({new_streak}/{STRAY_ADOPTION_DAYS} days; next feed opens at midnight UTC, in {until_reset})"
                        ),
                        false,
                    )
                }
            }
            Some(_) if same_critter => (
                Some((idx, 1, today)),
                format!(
                    "{name} has grown wary again - you'll need to start over. (1/{STRAY_ADOPTION_DAYS} days; next feed opens at midnight UTC, in {until_reset})"
                ),
                false,
            ),
            _ => (
                Some((idx, 1, today)),
                format!(
                    "You offer {name} something to eat. It watches you carefully, but doesn't run. (1/{STRAY_ADOPTION_DAYS} days; next feed opens at midnight UTC, in {until_reset})"
                ),
                false,
            ),
        };

        if let Some(p) = self.players.get_mut(&user_id) {
            p.stray_bond = new_bond;
            if adopted {
                p.stray = Some(idx);
            }
        }
        self.log_to(user_id, LogKind::Loot, message);
        self.dirty = true;
    }

    fn feed_owned_pet(&mut self, user_id: Uuid) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        if p.pet.is_none() {
            self.log_to(
                user_id,
                LogKind::System,
                "You have no companion to feed.".to_string(),
            );
            return;
        }
        if p.gold < PET_FEED_COST {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Feed costs {PET_FEED_COST} gold."),
            );
            return;
        }
        let mut leveled = false;
        let mut name = String::new();
        let mut new_level = 0;
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= PET_FEED_COST;
            if let Some(pet) = p.pet.as_mut() {
                leveled = pet.feed();
                name = pet.species.name.to_string();
                new_level = pet.level();
            }
        }
        self.log_to(
            user_id,
            LogKind::Loot,
            format!("You feed and tend your {name}; it mends and warms to you."),
        );
        if leveled {
            self.log_to(
                user_id,
                LogKind::System,
                format!("Your {name} grows stronger! (companion level {new_level})"),
            );
        }
        self.dirty = true;
    }

    /// Splash a fraction of an incoming blow onto a fighting companion. A pet
    /// that drops to zero is downed and stops fighting until fed.
    fn wound_pet(&mut self, user_id: Uuid, raw: i32) {
        let mut downed_name: Option<String> = None;
        if let Some(p) = self.players.get_mut(&user_id) {
            // Beastlord "Pack Bond" toughens the companion, softening the splash.
            let beastlord = p.class == Some(Class::Beastlord);
            let mut splash = (raw * PET_WOUND_PCT / 100).max(1);
            if beastlord {
                splash = (splash - splash * BEASTLORD_PET_PCT / 100).max(1);
            }
            if let Some(pet) = p.pet.as_mut()
                && !pet.downed
            {
                pet.hp -= splash;
                if pet.hp <= 0 {
                    pet.hp = 0;
                    pet.downed = true;
                    downed_name = Some(pet.species.name.to_string());
                }
            }
        }
        if let Some(name) = downed_name {
            self.log_to(
                user_id,
                LogKind::Combat,
                format!("Your {name} is beaten down! Feed it at a stable to rouse it."),
            );
            self.dirty = true;
        }
    }

    // ---- Pet auto-skills ------------------------------------------------

    /// Fire the companion's level-gated auto-skills against the owner's target,
    /// each on its own cooldown (tracked in `world_ticks`). Returns true if the
    /// foe was slain (by a killing pounce), so the combat step knows to move on.
    /// Damage/DoT scale with the pet's own attack; Roar empowers the owner and
    /// Guard shields them. Lock-free/snapshot-only: only `WorldState` is touched.
    #[allow(clippy::too_many_arguments)]
    fn fire_pet_skills(
        &mut self,
        user_id: Uuid,
        mob_id: u32,
        pet_level: i32,
        pet_atk: i32,
        pet_name: &str,
        mob_name: &str,
        pet_skills: &'static [super::taming::PetSkill],
        beastlord: bool,
    ) -> bool {
        let now_tick = self.world_ticks;
        for (si, skill) in pet_skills
            .iter()
            .filter(|s| s.level <= pet_level)
            .enumerate()
        {
            // Respect the per-skill cooldown.
            let ready = self
                .pet_skill_cd
                .get(&(user_id, si))
                .is_none_or(|&next| now_tick >= next);
            if !ready {
                continue;
            }
            // Beastlord "Pack Bond" shortens the companion's skill cooldowns so it
            // looses them more often (at least one tick off, never below one).
            let base_cd = skill.cooldown as u64;
            let cd = if beastlord {
                (base_cd - base_cd * BEASTLORD_PET_PCT as u64 / 100).max(1)
            } else {
                base_cd
            };
            self.pet_skill_cd.insert((user_id, si), now_tick + cd);
            match skill.effect {
                PetSkillEffect::SavageBite | PetSkillEffect::Pounce => {
                    // Bonus burst damage, scaled by the pet's bite.
                    let bonus = skill.power + pet_atk * skill.power / 20;
                    let dead = {
                        let Some(mob) = self.mobs.get_mut(&mob_id) else {
                            return false;
                        };
                        let (dealt, _) = mob.spawn.profile.apply(bonus, DamageType::Physical);
                        mob.hp -= dealt;
                        mob.hp <= 0
                    };
                    self.dirty = true;
                    self.mark_world_dirty();
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name}'s {} rips into {mob_name}!", skill.name),
                    );
                    if dead {
                        self.kill_mob(user_id, mob_id);
                        return true;
                    }
                }
                PetSkillEffect::Rend => {
                    let per_tick = skill.power + pet_atk / 8;
                    self.seed_mob_dot(
                        user_id,
                        per_tick,
                        DamageType::Physical,
                        3,
                        DotSource::Ability,
                        &format!("Your {pet_name}'s Rend"),
                    );
                }
                PetSkillEffect::Roar => {
                    let mag = skill.power + pet_atk / 10;
                    if let Some(p) = self.players.get_mut(&user_id) {
                        p.empower = p.empower.max(mag);
                        p.empower_ticks = p.empower_ticks.max(4);
                    }
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!(
                            "Your {pet_name} looses an intimidating roar - you feel emboldened!"
                        ),
                    );
                    self.dirty = true;
                }
                PetSkillEffect::Guard => {
                    let mag = skill.power + pet_atk / 4;
                    if let Some(p) = self.players.get_mut(&user_id) {
                        p.shield = p.shield.max(mag);
                        p.shield_ticks = p.shield_ticks.max(4);
                    }
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name} guards you closely, warding the next blows."),
                    );
                    self.dirty = true;
                }
                PetSkillEffect::Mend => {
                    let mag = skill.power + pet_atk / 6;
                    self.heal_player(user_id, mag);
                    self.log_to(
                        user_id,
                        LogKind::Combat,
                        format!("Your {pet_name} nuzzles you with a mending glow."),
                    );
                }
            }
        }
        false
    }

    // ---- Animal Taming --------------------------------------------------

    /// Attempt to tame the wild beast identified by its index in the room's
    /// tameable list. Driven by the player's Animal Taming level versus the
    /// beast's required level: a clear success chance, a spooked cooldown on
    /// failure, and on success the beast becomes the player's active companion
    /// (replacing any current one, like `buy_pet`) and trains the trade.
    fn tame(&mut self, user_id: Uuid, idx: usize) {
        if !self.is_classed(user_id) {
            return;
        }
        let Some(player) = self.players.get(&user_id) else {
            return;
        };
        if player.dead || player.respawn_at.is_some() {
            return;
        }
        let room = player.room;
        let here = beasts_at(room);
        let Some(wb) = here.get(idx).copied() else {
            self.log_to(
                user_id,
                LogKind::System,
                "There is no such beast here to tame.".to_string(),
            );
            return;
        };
        let species = beast_species(wb.species);
        let bi = wb.species;
        let now = Instant::now();
        // A spooked beast will not be approached again until it settles.
        if let Some(t) = self.tame_cooldowns.get(&(user_id, bi))
            && now.duration_since(*t) < TAME_COOLDOWN
        {
            self.log_to(
                user_id,
                LogKind::Normal,
                format!("The {} is still wary of you. Give it time.", species.name),
            );
            return;
        }
        let taming_xp = player.taming_xp;
        let cha_pct = player.scores.tame_pct();
        let level = skill_level_for_xp(taming_xp);
        // Under-level: refused outright, with a clear reason.
        if level < species.tame_level {
            self.log_to(
                user_id,
                LogKind::System,
                format!(
                    "The {} is beyond your skill - taming it needs {} level {} (yours is {level}).",
                    species.name,
                    TamingSkill::label(),
                    species.tame_level,
                ),
            );
            return;
        }
        let chance = tame_chance(taming_xp, species, cha_pct);
        // The approach: a beat of warily-earned trust before the roll.
        self.log_to(
            user_id,
            LogKind::Normal,
            format!(
                "The {} eyes you warily as you step close, hand open and low...",
                species.name
            ),
        );
        let roll = rand::thread_rng().gen_range(0..100);
        if roll < chance {
            // Success: it becomes the active companion, and the trade trains.
            let released = self
                .players
                .get(&user_id)
                .and_then(|p| p.pet.map(|o| o.species.name));
            let gained = tame_xp(species);
            let (before, after) = if let Some(p) = self.players.get_mut(&user_id) {
                p.pet = Some(Pet::new(species, 0));
                let b = skill_level_for_xp(p.taming_xp);
                p.taming_xp += gained as i64;
                (b, skill_level_for_xp(p.taming_xp))
            } else {
                return;
            };
            if let Some(old) = released {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("Your {old} is set loose to make room, and pads off into the green."),
                );
            }
            self.log_to(
                user_id,
                LogKind::Loot,
                format!(
                    "{} You've earned its trust! The {} is yours now. (+{gained} {} xp)",
                    species.glyph,
                    species.name,
                    TamingSkill::label()
                ),
            );
            if after > before {
                self.log_to(
                    user_id,
                    LogKind::System,
                    format!("Your {} rises to level {after}!", TamingSkill::label()),
                );
            }
            self.tame_cooldowns.remove(&(user_id, bi));
        } else {
            // Failure: it bolts, and stays spooked for a spell.
            self.tame_cooldowns.insert((user_id, bi), now);
            self.log_to(
                user_id,
                LogKind::Normal,
                format!(
                    "The {} shies, then bolts into the briars. Not this time.",
                    species.name
                ),
            );
        }
        self.dirty = true;
    }

    // ---- Player housing -------------------------------------------------

    /// Whether a housing clerk stands in this room.
    fn room_has_housing_clerk(&self, room: RoomId) -> bool {
        features_at(room)
            .iter()
            .any(|f| f.kind == FeatureKind::Housing)
    }

    /// The plot (tier index) this player holds the deed to, if any.
    fn owned_plot(&self, user_id: Uuid) -> Option<usize> {
        self.plot_owner
            .iter()
            .find(|(_, owner)| **owner == user_id)
            .map(|(plot, _)| *plot)
    }

    /// Buy the deed to tier `plot` and claim its home. Must be at the clerk, own
    /// no home already, and the plot must be unclaimed and affordable.
    fn buy_deed(&mut self, user_id: Uuid, plot: usize) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        if !self.room_has_housing_clerk(p.room) {
            self.log_to(
                user_id,
                LogKind::System,
                "You can only buy a deed from the housing clerk at Hearthward Close.".to_string(),
            );
            return;
        }
        let Some(tier) = housing::TIERS.get(plot) else {
            return;
        };
        if let Some(existing) = self.owned_plot(user_id) {
            let name = housing::TIERS[existing].label;
            self.log_to(
                user_id,
                LogKind::System,
                format!("You already hold the deed to a {name}. One home to a name."),
            );
            return;
        }
        if self.plot_owner.contains_key(&plot) {
            self.log_to(
                user_id,
                LogKind::System,
                format!("The {} is already spoken for. Try another.", tier.label),
            );
            return;
        }
        if p.gold < tier.price {
            self.log_to(
                user_id,
                LogKind::System,
                format!("The {} deed costs {} gold.", tier.label, tier.price),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= tier.price;
        }
        self.plot_owner.insert(plot, user_id);
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "The deed is yours - the {} at Hearthward Close is now your home. Step inside and furnish it from the clerk's catalogue.",
                tier.label
            ),
        );
        self.dirty = true;
    }

    /// Buy a furnishing and set it down in the home room the player is standing
    /// in. Must be inside a home this player owns.
    fn buy_furniture(&mut self, user_id: Uuid, key: &str) {
        let Some(p) = self.players.get(&user_id) else {
            return;
        };
        let room = p.room;
        let Some(plot) = plot_of_room(room) else {
            self.log_to(
                user_id,
                LogKind::System,
                "You can only place furniture inside your own home.".to_string(),
            );
            return;
        };
        if self.plot_owner.get(&plot) != Some(&user_id) {
            self.log_to(
                user_id,
                LogKind::System,
                "This is not your home to furnish.".to_string(),
            );
            return;
        }
        let Some(furn) = furniture_by_key(key) else {
            return;
        };
        if p.gold < furn.price {
            self.log_to(
                user_id,
                LogKind::System,
                format!("{} costs {} gold.", furn.name, furn.price),
            );
            return;
        }
        if let Some(p) = self.players.get_mut(&user_id) {
            p.gold -= furn.price;
        }
        self.house_furniture.entry(room).or_default().push(furn);
        self.log_to(
            user_id,
            LogKind::Loot,
            format!(
                "You set down {} - the room feels more like home.",
                furn.name
            ),
        );
        self.dirty = true;
    }

    // ---- Appearance, per-player logging, and the snapshot ---------------

    /// Cycle one appearance/bio field forward (+1) or back (-1), wrapping.
    fn cycle_appearance(&mut self, user_id: Uuid, field: usize, delta: i8) {
        if field >= appearance::N_FIELDS {
            return;
        }
        let count = appearance::option_count(field) as i32;
        if let Some(p) = self.players.get_mut(&user_id) {
            let cur = p.appearance[field] as i32;
            p.appearance[field] = (cur + delta as i32).rem_euclid(count) as u8;
            self.dirty = true;
        }
    }

    fn log_to(&mut self, user_id: Uuid, kind: LogKind, text: String) {
        if let Some(player) = self.players.get_mut(&user_id) {
            push_log(&mut player.log, kind, text);
            self.dirty = true;
        }
    }

    /// Top-ten currently-connected, classed adventurers by level, lifetime
    /// pvp kills, and total gold (carried + banked). See `LeaderboardView`.
    fn build_leaderboard(&self) -> LeaderboardView {
        const TOP_N: usize = 10;
        fn entry(p: &PlayerState, value: i64) -> LeaderboardEntry {
            LeaderboardEntry {
                user_id: p.user_id,
                level: p.level,
                class_key: p.class.map(|c| c.as_key().to_string()).unwrap_or_default(),
                value,
            }
        }
        let classed: Vec<&PlayerState> = self
            .players
            .values()
            .filter(|p| p.class.is_some())
            .collect();

        let mut by_level = classed.clone();
        by_level.sort_by_key(|p| std::cmp::Reverse(p.level));
        by_level.truncate(TOP_N);

        let mut by_pvp_kills = classed.clone();
        by_pvp_kills.sort_by_key(|p| std::cmp::Reverse(p.pvp_kills));
        by_pvp_kills.truncate(TOP_N);

        let mut by_gold = classed;
        by_gold.sort_by_key(|p| std::cmp::Reverse(p.gold + p.banked_gold));
        by_gold.truncate(TOP_N);

        LeaderboardView {
            by_level: by_level.iter().map(|p| entry(p, p.level as i64)).collect(),
            by_pvp_kills: by_pvp_kills.iter().map(|p| entry(p, p.pvp_kills)).collect(),
            by_gold: by_gold
                .iter()
                .map(|p| entry(p, p.gold + p.banked_gold))
                .collect(),
        }
    }

    fn snapshot(&self) -> MudSnapshot {
        let mut players = HashMap::new();
        let time_of_day_now = self.time_of_day();
        let time_of_day = time_of_day_now.label();
        let time_of_day_glyph = time_of_day_now.glyph();
        let time_of_day_dark = time_of_day_now.is_dark();
        let weather = self.weather().label();
        // ONE pass over the world's mobs and players per snapshot, shared by
        // every player's view below. Snapshots run on every publish inside the
        // global lock, and the world holds thousands of spawns: sweeping them
        // once per PLAYER made every keystroke O(players x mobs).
        let coords = super::worldmap::world_coords();
        let mut mobs_by_room: HashMap<RoomId, Vec<&MobInstance>> = HashMap::new();
        let mut foe_rooms: Vec<(RoomId, super::worldmap::Coord)> = Vec::new();
        for m in self.mobs.values() {
            if !m.alive || !m.revealed {
                continue;
            }
            let seen = mobs_by_room.entry(m.current_room).or_default();
            if seen.is_empty()
                && let Some(&c) = coords.get(&m.current_room)
            {
                foe_rooms.push((m.current_room, c));
            }
            seen.push(m);
        }
        // Rooms holding at least one adventurer. A viewer's own room is
        // filtered out per player below, so "another adventurer" needs no
        // identity here, only occupancy.
        let mut occupied_rooms: Vec<(RoomId, super::worldmap::Coord)> = Vec::new();
        {
            let mut seen: HashSet<RoomId> = HashSet::new();
            for other in self.players.values() {
                if seen.insert(other.room)
                    && let Some(&c) = coords.get(&other.room)
                {
                    occupied_rooms.push((other.room, c));
                }
            }
        }
        // Computed once for every player this snapshot, not per-player: the
        // three top-ten boards only depend on who's classed and online right
        // now, never on who's asking.
        let leaderboard = Arc::new(self.build_leaderboard());
        for (user_id, player) in &self.players {
            let room = self.world.room(player.room);
            let (room_name, room_desc, zone, safe, pvp, exits) = match room {
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
                        room.pvp,
                        exits,
                    )
                }
                None => (
                    String::new(),
                    String::new(),
                    String::new(),
                    true,
                    false,
                    Vec::new(),
                ),
            };
            let zone_band = room.and_then(|r| self.world.zone_band(r.zone));
            let mobs: Vec<MobView> = mobs_by_room
                .get(&player.room)
                .into_iter()
                .flatten()
                .map(|m| MobView {
                    id: m.spawn.id,
                    name: m.spawn.name.to_string(),
                    hp: m.hp,
                    max_hp: m.spawn.max_hp,
                    level: m.spawn.level(),
                    rank: m.spawn.rank().to_string(),
                    boss: m.spawn.boss,
                    targeted: player.target == Some(m.spawn.id),
                    school: m.spawn.profile.attack_type.label(),
                    weak: m.spawn.profile.weak.map(|d| d.label()),
                    resist: m.spawn.profile.resist.map(|d| d.label()),
                    dot_stacks: self
                        .mob_dots
                        .get(&m.spawn.id)
                        .map(|stacks| stacks.len().min(u8::MAX as usize) as u8)
                        .unwrap_or(0),
                    stunned: self.mob_stuns.get(&m.spawn.id).is_some_and(|t| *t > 0),
                })
                .collect();
            // Foes lairing in nearby rooms (not this one) and other adventurers
            // in the same window, so the live field can mark where danger and
            // company sit. Bounded to a window around the player on the same
            // level; the field's own fog still hides rooms never seen. Only the
            // field draws these, so a session without it pays nothing. The
            // cell window is an honest "near me" ever since the coordinate
            // field stopped folding unrelated zones together (worldmap's
            // `zone_interleaves` pin keeps it that way): what sits within a
            // few cells really is a few moves away.
            let (nearby_foes, nearby_players): (Vec<RoomId>, Vec<RoomId>) =
                match coords.get(&player.room) {
                    Some(&pc) if player.rpg_mode => {
                        let near = |c: &super::worldmap::Coord| {
                            c.z == pc.z && (c.x - pc.x).abs() <= 16 && (c.y - pc.y).abs() <= 12
                        };
                        (
                            foe_rooms
                                .iter()
                                .filter(|(r, c)| *r != player.room && near(c))
                                .map(|(r, _)| *r)
                                .collect(),
                            occupied_rooms
                                .iter()
                                .filter(|(r, c)| *r != player.room && near(c))
                                .map(|(r, _)| *r)
                                .collect(),
                        )
                    }
                    _ => (Vec::new(), Vec::new()),
                };
            let occupants: Vec<OccupantView> = self
                .players
                .values()
                .filter(|other| other.user_id != *user_id && other.room == player.room)
                .map(|other| OccupantView {
                    user_id: other.user_id,
                    hp: other.hp,
                    max_hp: other.max_hp(),
                    in_combat: other.in_combat(),
                    alive: !other.dead,
                    bio: appearance::compose_bio(&other.appearance),
                    class_key: other
                        .class
                        .map(|c| c.as_key().to_string())
                        .unwrap_or_default(),
                    level: other.level,
                    appearance_idx: other.appearance.to_vec(),
                    attackable: pvp && !other.dead && other.class.is_some(),
                    targeted: player.pvp_target == Some(other.user_id),
                })
                .collect();
            let corpse_here = occupants.iter().any(|o| !o.alive);
            let now = Instant::now();
            // Birds with a perch alternative toggle every few real minutes, so
            // the same creature reads as aloft one visit and grounded the next.
            let moment_bucket = now_unix_secs() / 300;
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
                    note: c.display_note(moment_bucket).to_string(),
                    kind: match c.kind {
                        CritterKind::Game => "huntable".to_string(),
                        CritterKind::Boon(_) => "boon".to_string(),
                        CritterKind::Skittish => String::new(),
                    },
                    perk: match c.kind {
                        CritterKind::Boon(p) => p.label().to_string(),
                        _ => String::new(),
                    },
                    mythical: c.mythical,
                    adoptable: c.adoptable,
                })
                .collect();
            // Harvestable nodes in the room, each flagged with whether the player
            // can work it now and, if not, why (under-skilled or regrowing).
            let nodes: Vec<NodeView> = nodes_at(player.room)
                .into_iter()
                .map(|n| {
                    let level = skill_level_for_xp(player.skill_xp(n.skill));
                    let ready = match node_index(n).and_then(|ni| self.gathered.get(&ni)) {
                        Some(t) => now.duration_since(*t) >= NODE_RESPAWN,
                        None => true,
                    };
                    let (gatherable, reason) = if level < n.level_req {
                        (false, format!("needs {} {}", n.skill.label(), n.level_req))
                    } else if !ready {
                        (false, "regrowing".to_string())
                    } else {
                        (true, String::new())
                    };
                    NodeView {
                        name: n.name.to_string(),
                        note: n.note.to_string(),
                        skill: n.skill.label().to_string(),
                        gatherable,
                        reason,
                    }
                })
                .collect();
            // Every gathering trade, in a stable order, with its live progress.
            let mut skills: Vec<SkillView> = GatherSkill::ALL
                .iter()
                .map(|&s| {
                    let xp = player.skill_xp(s);
                    let (xp_into, xp_next) = skill_progress(xp);
                    SkillView {
                        name: s.label().to_string(),
                        level: skill_level_for_xp(xp),
                        xp_into,
                        xp_next,
                    }
                })
                .collect();
            // The maker's trades follow the gatherer's in the same Trades block.
            skills.extend(CraftSkill::ALL.iter().map(|&s| {
                let xp = player.craft_xp(s);
                let (xp_into, xp_next) = skill_progress(xp);
                SkillView {
                    name: s.label().to_string(),
                    level: skill_level_for_xp(xp),
                    xp_into,
                    xp_next,
                }
            }));
            // The beastmaster's trade, Animal Taming, closes out the Trades block.
            {
                let (xp_into, xp_next) = skill_progress(player.taming_xp);
                skills.push(SkillView {
                    name: TamingSkill::label().to_string(),
                    level: player.taming_level(),
                    xp_into,
                    xp_next,
                });
            }
            // The crafting panel: every recipe worked at the stations in this room.
            let crafting = {
                let stations = craft_stations_at(player.room);
                if stations.is_empty() {
                    None
                } else {
                    let mut entries = Vec::new();
                    for &st in &stations {
                        let clevel = skill_level_for_xp(player.craft_xp(st));
                        for ri in recipe_indices_for(st) {
                            let Some(rc) = recipe(ri) else {
                                continue;
                            };
                            let inputs = rc
                                .inputs
                                .iter()
                                .map(|ing| {
                                    let n = item(ing.item).map(|i| i.name).unwrap_or("?");
                                    format!("{}x {n}", ing.qty)
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            let have_mats = rc
                                .inputs
                                .iter()
                                .all(|ing| player.item_count(ing.item) >= ing.qty);
                            let (craftable, reason) = if clevel < rc.level_req {
                                (false, format!("needs {} {}", st.label(), rc.level_req))
                            } else if !have_mats {
                                (false, "need materials".to_string())
                            } else {
                                (true, String::new())
                            };
                            entries.push(CraftEntryView {
                                recipe: ri,
                                name: item(rc.output)
                                    .map(|i| i.name.to_string())
                                    .unwrap_or_default(),
                                skill: st.label().to_string(),
                                inputs,
                                craftable,
                                reason,
                            });
                        }
                    }
                    Some(CraftView {
                        stations: stations
                            .iter()
                            .map(|s| s.station())
                            .collect::<Vec<_>>()
                            .join(", "),
                        entries,
                    })
                }
            };
            let in_combat_with = player.target.and_then(|mob_id| {
                self.mobs
                    .get(&mob_id)
                    .filter(|m| m.alive)
                    .map(|m| m.spawn.name.to_string())
            });

            let (classed, class_name, class_key, trait_name, trait_desc, resource_name) =
                match player.class {
                    Some(c) => (
                        true,
                        c.name().to_string(),
                        c.as_key().to_string(),
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
                    sell_price: player.sell_price(it),
                    stats: it.stat_summary(),
                    compare: compare_to_worn(&player.equipped, it),
                    compare_pct: player.compare_gear(it),
                    category: item_category(&it.kind),
                    desc: it.desc,
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
                            sell_price: player.sell_price(it),
                            stats: it.stat_summary(),
                            compare: String::new(),
                            compare_pct: None,
                            category: item_category(&it.kind),
                            desc: it.desc,
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
                        price: player.buy_price(it),
                        affordable: player.gold >= player.buy_price(it),
                        stats: it.stat_summary(),
                        compare: compare_to_worn(&player.equipped, it),
                        compare_pct: player.compare_gear(it),
                        category: item_category(&it.kind),
                        desc: it.desc,
                    })
                    .collect(),
            });

            let owner_rating = player.attack_rating();
            let pet = player.pet.as_ref().map(|pet| PetView {
                name: pet.species.name.to_string(),
                glyph: pet.species.glyph.to_string(),
                level: pet.level(),
                hp: pet.hp,
                max_hp: pet.max_hp(),
                attack: pet.attack() + owner_rating * PET_COEF_PCT / 100,
                downed: pet.downed,
                loyalty_pct: pet.loyalty_pct(),
                skills: pet
                    .species
                    .skills
                    .iter()
                    .filter(|s| s.level <= pet.level())
                    .map(|s| (s.name.to_string(), s.level))
                    .collect(),
            });
            let stray = player
                .stray
                .and_then(|idx| super::world::WILDLIFE.get(idx))
                .map(|c| c.name.to_string());
            let stable = self.room_has_stable(player.room).then(|| StableView {
                feed_cost: PET_FEED_COST,
                entries: super::pets::PET_SPECIES
                    .iter()
                    .filter(|s| !s.is_tameable())
                    .map(|s| StableEntryView {
                        key: s.key.to_string(),
                        name: s.name.to_string(),
                        glyph: s.glyph.to_string(),
                        price: s.price,
                        hp: s.base_hp,
                        attack: s.base_attack,
                        desc: s.desc.to_string(),
                        affordable: player.gold >= s.price,
                    })
                    .collect(),
            });

            // The Animal Taming panel: every tameable beast roaming this room,
            // with the player's odds against each (0 = under-level or spooked).
            let taming = {
                let beasts = beasts_at(player.room);
                if beasts.is_empty() {
                    None
                } else {
                    let taming_level = player.taming_level();
                    let entries = beasts
                        .iter()
                        .enumerate()
                        .map(|(i, wb)| {
                            let sp = beast_species(wb.species);
                            let spooked = self
                                .tame_cooldowns
                                .get(&(*user_id, wb.species))
                                .is_some_and(|t| now.duration_since(*t) < TAME_COOLDOWN);
                            let odds = if spooked {
                                0
                            } else {
                                tame_chance(player.taming_xp, sp, player.scores.tame_pct())
                            };
                            let reason = if taming_level < sp.tame_level {
                                format!("needs Taming {}", sp.tame_level)
                            } else if spooked {
                                "spooked".to_string()
                            } else {
                                String::new()
                            };
                            TameEntryView {
                                idx: i,
                                name: sp.name.to_string(),
                                glyph: sp.glyph.to_string(),
                                req_level: sp.tame_level,
                                odds,
                                reason,
                                desc: sp.desc.to_string(),
                            }
                        })
                        .collect();
                    Some(TamingView {
                        taming_level,
                        entries,
                    })
                }
            };

            // The housing ledger: deeds at the clerk, furnishings inside your home.
            let housing = if self.room_has_housing_clerk(player.room) {
                Some(HousingView {
                    title: "Deeds of Hearthward Close".to_string(),
                    furnish: false,
                    entries: housing::TIERS
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            let owner = self.plot_owner.get(&i);
                            HousingEntryView {
                                key: t.key.to_string(),
                                name: t.label.to_string(),
                                price: t.price,
                                detail: format!("{} rooms - {}", t.rooms(), t.blurb),
                                affordable: player.gold >= t.price,
                                taken: owner.is_some_and(|o| *o != *user_id),
                                owned: owner == Some(user_id),
                            }
                        })
                        .collect(),
                })
            } else if plot_of_room(player.room)
                .is_some_and(|plot| self.plot_owner.get(&plot) == Some(user_id))
            {
                Some(HousingView {
                    title: "Furnish your home".to_string(),
                    furnish: true,
                    entries: housing::FURNITURE
                        .iter()
                        .map(|f| HousingEntryView {
                            key: f.key.to_string(),
                            name: f.name.to_string(),
                            price: f.price,
                            detail: f.desc.to_string(),
                            affordable: player.gold >= f.price,
                            taken: false,
                            owned: false,
                        })
                        .collect(),
                })
            } else {
                None
            };

            // The waystone menu is present whenever the room holds a portal.
            let portal = features_at(player.room)
                .iter()
                .any(|f| f.kind == FeatureKind::Portal)
                .then(|| {
                    let known_gates = super::world::CONTINENT_WAYSTONES
                        .iter()
                        .filter(|(_, room)| player.visited.contains(room))
                        .count();
                    PortalView {
                        entries: super::world::waystone_destinations()
                            .into_iter()
                            .filter(|(_, room)| {
                                super::world::waystone_is_known(*room, &player.visited)
                            })
                            .map(|(label, room)| (label.to_string(), room, room == player.room))
                            .collect(),
                        known_gates,
                        unknown_gates: super::world::CONTINENT_WAYSTONES.len() - known_gates,
                    }
                });

            let board = features_at(player.room)
                .iter()
                .any(|f| f.kind == FeatureKind::Board)
                .then(|| BoardView {
                    entries: self.board_entries(*user_id, player.room),
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
            let atlas = self.world.region_progress(&player.visited, player.room);
            // The journal, in reading order: the active starter step first,
            // then accepted board bounties, then - only once the Frontier's
            // gate titles are held - its twenty zone quests. A locked Frontier
            // used to dump all twenty endgame rows on a level-2 character and
            // drown everything that actually applied to them.
            let mut quests: Vec<QuestView> = Vec::new();
            if let Some(q) = starter_quest(player.starter_stage) {
                let need = starter_goal_target(q.goal);
                quests.push(QuestView {
                    name: if need > 1 {
                        format!("{} ({}/{})", q.title, player.starter_kills, need)
                    } else {
                        q.title.to_string()
                    },
                    desc: q.hint.to_string(),
                    done: false,
                    reward: format!("{} gold + {} xp", q.reward_gold, q.reward_xp),
                    kind: QuestKind::Starter,
                    target: Some(q.target),
                });
            }
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
                        desc: format!("{} ({}) {}", q.blurb, q.objective.describe(), q.hint),
                        done: ready,
                        reward: format!(
                            "{} gold{}",
                            q.reward_gold,
                            match q.reward_title {
                                Some(t) => format!(" + title: {t}"),
                                None => String::new(),
                            }
                        ),
                        kind: QuestKind::Board,
                        target: None,
                    });
                }
            }
            let frontier_open = titles_include_all(&player.titles, &FRONTIER_REQUIRED_TITLES);
            if frontier_open {
                quests.extend((0..super::world::frontier_zone_count()).filter_map(|z| {
                    super::world::frontier_zone_info(z).map(|(zname, boss)| QuestView {
                        name: format!("{zname} - slay {boss}"),
                        desc: format!("Hunt down and slay {boss}, {zname}'s zone boss."),
                        done: player.completed_quests.contains(&z),
                        reward: format!("title: Champion of the {zname}"),
                        kind: QuestKind::Frontier,
                        target: Some(super::world::frontier_zone_entrance(z)),
                    })
                }));
            }
            let road = road_view(&player.titles, &self.road_targets);

            players.insert(
                *user_id,
                PlayerView {
                    joined: true,
                    room: Some(player.room),
                    visited: Arc::clone(&player.visited),
                    classed,
                    class_name,
                    class_key,
                    trait_name,
                    trait_desc,
                    resource_name,
                    resource: player.resource,
                    max_resource: player.max_resource,
                    alive: player.respawn_at.is_none(),
                    hp: player.hp,
                    max_hp: player.max_hp(),
                    attack: player.attack(),
                    swing: player.swing(),
                    spell_power: player.spell_power(),
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
                    zone_band,
                    safe,
                    pvp,
                    pvp_kills: player.pvp_kills,
                    leaderboard: leaderboard.clone(),
                    exits,
                    mobs,
                    nearby_foes,
                    nearby_players,
                    rpg_mode: player.rpg_mode,
                    riding: if player.mounted {
                        player.pet.as_ref().and_then(|pet| {
                            super::taming::mount_stride(pet.species.key)
                                .map(|st| format!("{} (stride {st})", pet.species.name))
                        })
                    } else {
                        None
                    },
                    waypoint_set: player.waypoint.is_some(),
                    occupants,
                    following: player.following,
                    wildlife,
                    nodes,
                    skills,
                    in_combat_with,
                    shield: player.shield,
                    empower: player.empower,
                    stunned: player.stunned > 0,
                    coat: player
                        .weapon_coat
                        .map(|(school, _, charges)| format!("{} coat x{charges}", school.label())),
                    abilities,
                    inventory,
                    shop,
                    pet,
                    stray,
                    stable,
                    taming,
                    housing,
                    crafting,
                    portal,
                    board,
                    bio: appearance::compose_bio(&player.appearance),
                    appearance: (0..appearance::N_FIELDS)
                        .map(|i| {
                            (
                                appearance::field_label(i).to_string(),
                                appearance::option(i, player.appearance[i]).to_string(),
                            )
                        })
                        .collect(),
                    appearance_idx: player.appearance.to_vec(),
                    log: player.log.clone(),
                    respawning: player.respawn_at.is_some(),
                    dead: player.dead,
                    can_resurrect: player.class.is_some_and(|c| c.can_resurrect()),
                    corpse_here,
                    scores: player.scores,
                    titles: player.titles.clone(),
                    title_levels: player.title_levels.clone(),
                    active_title: player.active_title,
                    quests,
                    road,
                    frontier_open,
                    resurrections_left: player.resurrections_left,
                    resurrection_cap: player.resurrection_cap,
                    features,
                    minimap,
                    atlas,
                    time_of_day,
                    time_of_day_glyph,
                    time_of_day_dark,
                    weather,
                    escort: player
                        .escort
                        .as_ref()
                        .map(|e| (e.name.to_string(), e.hp, e.max_hp, e.dest_zone.to_string())),
                    archetype: player
                        .archetype
                        .map(|a| (a.name.to_string(), a.role.label().to_string())),
                    archetype_choices: if player.archetype.is_none()
                        && player.class.is_some()
                        && player.level >= ARCHETYPE_LEVEL
                    {
                        player
                            .class
                            .map(|c| {
                                super::classes::archetypes_for(c)
                                    .into_iter()
                                    .map(|a| {
                                        (
                                            a.name.to_string(),
                                            a.role.label().to_string(),
                                            a.desc.to_string(),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    },
                    score_points: player.score_points(),
                    score_offer: player.score_offer(),
                },
            );
        }
        MudSnapshot {
            room_id: self.room_id,
            generation: self.generation,
            players,
            reset_versions: HashMap::new(),
        }
    }
}

// ---- Free helpers: titles, tags, gold, and the log buffer ----------------

/// The combat-log voice of a weapon coat's school ("Your burning oil ...").
fn coat_source(school: DamageType) -> &'static str {
    match school {
        DamageType::Poison => "Your poison",
        DamageType::Fire => "Your burning oil",
        DamageType::Frost => "Your freezing oil",
        DamageType::Holy => "Your blessed oil",
        DamageType::Lightning => "Your crackling oil",
        DamageType::Shadow => "Your darkened oil",
        DamageType::Arcane => "Your humming oil",
        DamageType::Physical => "Your coating",
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

/// The Wildbound Waste's reaver title track: awarded the tick a lifetime pvp
/// kill count first crosses a threshold (see the pvp-fighters tick loop).
fn pvp_title_for(kills: i64) -> Option<&'static str> {
    match kills {
        1 => Some("Blooded"),
        10 => Some("Reaver of the Waste"),
        50 => Some("Dread of the Wildbound"),
        150 => Some("Warlord of the Waste"),
        500 => Some("Deathless Sovereign of the Waste"),
        _ => None,
    }
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
        "Yssgar, the Sundering Deep" => Some(SUNDERING_DEEP_ACHIEVEMENT),
        "Kaethyr Ascendant, Who Sang the God Awake" => Some(KAETHYR_ASCENDANT_ACHIEVEMENT),
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
#[path = "svc_test.rs"]
mod svc_test;

/// The test battle arena (see `arena.rs`): drives the real engine with
/// scripted characters against real spawns and reports who survives.
#[cfg(test)]
#[path = "arena.rs"]
mod arena;
