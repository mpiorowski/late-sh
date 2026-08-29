//! The test battle arena.
//!
//! A harness that drives the *real* engine (`WorldState::tick`, `engage_mob`,
//! `use_ability`, `use_item`, `flee`) with a scripted character against a real
//! spawn standing in its real home room, and reports who survives and where
//! every point of damage came from. Test-only; it never ships.
//!
//! Why it exists: every balance number in CONTEXT.md §7 that was "modelled"
//! rather than measured has drifted from the engine at least once. The arena
//! measures. A [`Recipe`] is explicit about everything that moves a fight
//! (class, level, archetype, gear, companion, coat, potions, policy, and the
//! ability-score [`Build`]) and the arena pins what must not move: the world
//! clock at a clear day, one fresh character per fight, the foe at full
//! health. Every yardstick and crown contract runs the `Neutral` build (flat
//! 10s, every score hook at zero); the other builds measure what a roll and
//! the placed points do to the same character.
//!
//! Lives as a child module of `svc` (declared there beside `svc_test`) so it
//! can reach the private combat entry points without widening them.
//!
//! What is not modelled, on purpose: the walk. Exploit policies teleport back
//! into the boss room after a flee; mobs never chase, so the walk is time and
//! nothing else. What the foe does in that time (recover, see `MOB_RESET_TICKS`
//! and `flee`) is the engine's, and exactly what those policies measure.

use uuid::Uuid;

use super::super::abilities::{Ability, AbilityEffect, unlocked_for};
use super::super::classes::{ArchetypeDef, Class, Role, archetypes_for, xp_for_level};
use super::super::damage::{DamageProfile, DamageType};
use super::super::items::{
    OIL_SCHOOLS, Rarity, Slot, item, oil_id, poison_id, smith_armor_id, smith_weapon_id,
};
use super::super::pets::{LOYALTY_PER_LEVEL, PET_MAX_LEVEL, PET_SPECIES, Pet, PetSpecies};
use super::super::stats::{AbilityScores, Score};
use super::super::taming::{AELUNOR_TAMEABLE, TAMEABLE};
use super::super::world::{RoomId, seed_world};
use super::{DotSource, SUMMON_ID_START, WorldState};

/// World tick the clock is pinned to before every fight. Ticks `990..=1169`
/// are the one stretch that is neither dark (`TimeOfDay::mob_damage_pct`,
/// +25% mob damage) nor foggy/stormy (ambush and caster-bolt boosts), so an
/// honest fight of up to `HONEST_MAX_TICKS` sees flat multipliers.
/// `the_arena_clock_is_a_clear_day` (arena_test) pins this against the real
/// `TimeOfDay`/`Weather` tables; re-derive it if `PHASE_TICKS`/`WEATHER_TICKS`
/// move.
pub(super) const ARENA_CLOCK: u64 = 989;
/// Ticks an honest fight may run before it is called a stalemate. Equal to the
/// clear-day window above, so an honest fight never crosses into dusk.
pub(super) const HONEST_MAX_TICKS: u32 = 180;
/// Ticks an exploit policy may run: enough to take a crown when the loop
/// works and to prove it stalls when it does not (a fled foe recovers, so a
/// stalled loop makes no progress at all). These cross into dusk and night,
/// where mobs hit +25% harder, which only makes the verdict conservative.
pub(super) const EXPLOIT_MAX_TICKS: u32 = 400;
/// The standard drinking rule: a potion goes down under this health percent.
const DRINK_UNDER_PCT: i32 = 40;
/// Self-heal thresholds for the honest policy, health percent.
const HEAL_UNDER_PCT: i32 = 45;
const HOT_UNDER_PCT: i32 = 70;
const WARD_UNDER_PCT: i32 = 95;
/// Abilities an active player presses between two ticks (two seconds).
const CASTS_PER_TICK: u32 = 2;
/// Hit-and-run: retreat under this health percent, spend this many ticks on
/// the road to a fountain and back (the heal itself is free in the live game).
const RETREAT_UNDER_PCT: i32 = 50;
const RETREAT_TICKS: u32 = 10;
/// Vials in the bag for a coated recipe; the policy re-coats when one runs dry.
const COAT_SUPPLY: u32 = 20;

/// Generated-catalog bases, mirrored from `items.rs` (private there). The
/// slot layout is asserted at use, so a moved catalog crashes here, loudly.
const FRONTIER_ITEM_BASE: u32 = 3000;
const REACHES_ITEM_BASE: u32 = 3200;
const KAELMYR_ITEM_BASE: u32 = 3400;
const REALM_SLOTS: [Slot; 8] = [
    Slot::Weapon,
    Slot::Head,
    Slot::Chest,
    Slot::Legs,
    Slot::Hands,
    Slot::Feet,
    Slot::Ring,
    Slot::Trinket,
];
/// The authored shop/boss-drop band (`items.rs` ids `1000..1300`).
const AUTHORED_GEAR_IDS: std::ops::Range<u32> = 1000..1300;

// ---- Recipes --------------------------------------------------------------

/// What the character wears. Tiers are 0-based catalog tiers, as in `items.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gear {
    Naked,
    /// The highest-power authored piece per slot (shop stock and boss drops,
    /// `1000..1300`). Nothing is class-gated at `equip`, so this is what any
    /// class can wear before the Frontier.
    ShopBest,
    /// What a prepared character of a crafting tier actually wears: the
    /// smithed weapon and plate of tier `t` plus, in every other slot, the
    /// best authored piece at or under the tier's rarity cap (Common at tier
    /// 0 up to Legendary at tier 5), so an L12 is never modelled in a 2600g
    /// Mythril sword.
    Kit(usize),
    /// The full eight-piece set of a Frontier zone tier (`0..20`).
    Frontier(usize),
    /// The full set of a Sundered Reaches tier (`0..20`).
    Reaches(usize),
    /// The full set of a Kaelmyr tier (`0..20`).
    Kaelmyr(usize),
}

impl Gear {
    pub(super) fn label(self) -> String {
        match self {
            Self::Naked => "naked".to_string(),
            Self::ShopBest => "shop".to_string(),
            Self::Kit(t) => format!("kit{}", t + 1),
            Self::Frontier(t) => format!("front{}", t + 1),
            Self::Reaches(t) => format!("reach{}", t + 1),
            Self::Kaelmyr(t) => format!("kael{}", t + 1),
        }
    }

    /// Item ids to equip. Every id is asserted to be gear.
    fn pieces(self) -> Vec<u32> {
        match self {
            Self::Naked => Vec::new(),
            Self::ShopBest => {
                let mut best: Vec<(Slot, u32, i32)> = Vec::new();
                for id in AUTHORED_GEAR_IDS {
                    let Some(it) = item(id) else { continue };
                    let Some(slot) = it.slot() else { continue };
                    match best.iter_mut().find(|(s, _, _)| *s == slot) {
                        Some(entry) if entry.2 >= it.power() => {}
                        Some(entry) => *entry = (slot, id, it.power()),
                        None => best.push((slot, id, it.power())),
                    }
                }
                best.into_iter().map(|(_, id, _)| id).collect()
            }
            Self::Kit(t) => {
                let cap = match t {
                    0 => Rarity::Common,
                    1 | 2 => Rarity::Uncommon,
                    3 => Rarity::Rare,
                    4 => Rarity::Epic,
                    _ => Rarity::Legendary,
                };
                let mut out = vec![smith_weapon_id(t as u32), smith_armor_id(t as u32)];
                let mut best: Vec<(Slot, u32, i32)> = Vec::new();
                for id in AUTHORED_GEAR_IDS {
                    let Some(it) = item(id) else { continue };
                    let Some(slot) = it.slot() else { continue };
                    if matches!(slot, Slot::Weapon | Slot::Chest)
                        || rarity_rank(it.rarity) > rarity_rank(cap)
                    {
                        continue;
                    }
                    match best.iter_mut().find(|(s, _, _)| *s == slot) {
                        Some(entry) if entry.2 >= it.power() => {}
                        Some(entry) => *entry = (slot, id, it.power()),
                        None => best.push((slot, id, it.power())),
                    }
                }
                out.extend(best.into_iter().map(|(_, id, _)| id));
                out
            }
            Self::Frontier(t) => realm_set(FRONTIER_ITEM_BASE, t),
            Self::Reaches(t) => realm_set(REACHES_ITEM_BASE, t),
            Self::Kaelmyr(t) => realm_set(KAELMYR_ITEM_BASE, t),
        }
    }
}

fn rarity_rank(r: Rarity) -> u8 {
    match r {
        Rarity::Common => 0,
        Rarity::Uncommon => 1,
        Rarity::Rare => 2,
        Rarity::Epic => 3,
        Rarity::Legendary => 4,
    }
}

fn realm_set(base: u32, tier: usize) -> Vec<u32> {
    REALM_SLOTS
        .iter()
        .enumerate()
        .map(|(i, want)| {
            let id = base + tier as u32 * 10 + i as u32;
            let got = slot_of(id);
            assert_eq!(
                got, *want,
                "arena: generated catalog layout moved, item {id} is {got:?} not {want:?}"
            );
            id
        })
        .collect()
}

fn slot_of(id: u32) -> Slot {
    match item(id).and_then(|it| it.slot()) {
        Some(slot) => slot,
        None => panic!("arena: item {id} is not gear"),
    }
}

/// The combat companion, always at `PET_MAX_LEVEL` (a maxed pet costs 720 gold
/// of feed, trivially affordable in the band the arena studies).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Companion {
    None,
    /// The hardest-biting Stable species.
    ShopBest,
    /// The hardest-biting beast in either wild pool.
    TameBest,
}

impl Companion {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::None => "no pet",
            Self::ShopBest => "shop pet",
            Self::TameBest => "tame pet",
        }
    }

    fn species(self) -> Option<&'static PetSpecies> {
        let strongest = |pool: &'static [PetSpecies]| pool.iter().max_by_key(|s| s.base_attack);
        match self {
            Self::None => None,
            Self::ShopBest => strongest(PET_SPECIES),
            Self::TameBest => {
                let wild = strongest(TAMEABLE);
                let fae = strongest(AELUNOR_TAMEABLE);
                match (wild, fae) {
                    (Some(a), Some(b)) => Some(if b.base_attack > a.base_attack { b } else { a }),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            }
        }
    }
}

/// What is on the weapon. Tiers are 0-based alchemy tiers (`0..6`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Coat {
    None,
    /// The oil a prepared player brings: the foe's weakness if an oil covers
    /// it, else the first oil school the foe does not resist.
    BestOil(usize),
    Poison(usize),
}

impl Coat {
    pub(super) fn label(self) -> String {
        match self {
            Self::None => "no coat".to_string(),
            Self::BestOil(t) => format!("oil{}", t + 1),
            Self::Poison(t) => format!("poison{}", t + 1),
        }
    }

    fn vial_for(self, foe: &DamageProfile) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Poison(t) => Some(poison_id(t as u32)),
            Self::BestOil(t) => {
                let weak = OIL_SCHOOLS.iter().position(|s| Some(*s) == foe.weak);
                let neutral = OIL_SCHOOLS.iter().position(|s| Some(*s) != foe.resist);
                let school = match (weak, neutral) {
                    (Some(i), _) => i,
                    (None, Some(i)) => i,
                    (None, None) => 0,
                };
                Some(oil_id(school as u32, t as u32))
            }
        }
    }
}

/// The six ability scores. `Neutral` is the reference every other number in
/// the arena is measured on; the rest are the shapes a roll and 25 placed
/// points can take, including the ones nobody sensible would choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Build {
    /// Flat 10s: every score hook at zero.
    Neutral,
    /// 18 in one score, 10 in the rest: a lucky roll and a few points on one axis.
    Peak(Score),
    /// 20 in the class's primary score and 20 CON, 10 elsewhere: the way most
    /// players will actually spend.
    Focused,
    /// Every score at 18: the whole roll going the character's way.
    Blessed,
    /// Every score at 3: the worst 4d6 can do, six times over.
    Cursed,
    /// STR, DEX, and INT at 20, the rest at 3: all edge and no body.
    GlassCannon,
    /// CON and WIS at 20, STR, DEX, and INT at 3: outlasts everything, kills nothing.
    Tortoise,
    /// CHA at 20, the rest at 3: the merchant who wandered into a fight.
    Merchant,
}

impl Build {
    pub(super) fn label(self) -> String {
        match self {
            Self::Neutral => "neutral".to_string(),
            Self::Peak(score) => format!("peak {}", score.label()),
            Self::Focused => "focused".to_string(),
            Self::Blessed => "blessed".to_string(),
            Self::Cursed => "cursed".to_string(),
            Self::GlassCannon => "glass cannon".to_string(),
            Self::Tortoise => "tortoise".to_string(),
            Self::Merchant => "merchant".to_string(),
        }
    }

    pub(super) fn scores(self, class: Class) -> AbilityScores {
        let flat = |v: i32| AbilityScores {
            strength: v,
            dexterity: v,
            constitution: v,
            intelligence: v,
            wisdom: v,
            charisma: v,
        };
        let mut scores = match self {
            Self::Neutral | Self::Peak(_) | Self::Focused => flat(10),
            Self::Blessed => flat(18),
            Self::Cursed | Self::GlassCannon | Self::Tortoise | Self::Merchant => flat(3),
        };
        let mut set = |which: Score, v: i32| match which {
            Score::Strength => scores.strength = v,
            Score::Dexterity => scores.dexterity = v,
            Score::Constitution => scores.constitution = v,
            Score::Intelligence => scores.intelligence = v,
            Score::Wisdom => scores.wisdom = v,
            Score::Charisma => scores.charisma = v,
        };
        match self {
            Self::Neutral | Self::Blessed | Self::Cursed => {}
            Self::Peak(which) => set(which, 18),
            Self::Focused => {
                set(class.primary_score(), 20);
                set(Score::Constitution, 20);
            }
            Self::GlassCannon => {
                set(Score::Strength, 20);
                set(Score::Dexterity, 20);
                set(Score::Intelligence, 20);
            }
            Self::Tortoise => {
                set(Score::Constitution, 20);
                set(Score::Wisdom, 20);
            }
            Self::Merchant => set(Score::Charisma, 20),
        }
        scores
    }
}

/// How the character fights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Policy {
    /// Stand and fight: auto-attack, rotate the unlocked roster by value
    /// (heal/ward/empower under thresholds, then the best ready offensive),
    /// drink under `DRINK_UNDER_PCT`, re-coat when the coat runs dry.
    Honest,
    /// Honest, plus reading the foe: every offensive pick is weighed by the
    /// foe's resist/weak multiplier for that ability's school, so a Mage
    /// throws Frost at the Ashen and Holy at the Undead. The same player,
    /// looking at the traits line. The ceiling of the school game.
    Routed,
    /// Engage, trade one exchange, flee, step straight back in. Retreat to a
    /// free fountain heal under `RETREAT_UNDER_PCT`. The foe never heals.
    HitAndRun,
    /// Engage only while the foe is stunned or a stun is ready; stun, land the
    /// free exchange, flee before the stun wears off, wait out the cooldown.
    /// The foe never swings. Needs a stun in the roster (`has_stun`).
    StunAndFlee,
}

impl Policy {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Honest => "honest",
            Self::Routed => "routed",
            Self::HitAndRun => "hit-and-run",
            Self::StunAndFlee => "stun-and-flee",
        }
    }

    fn max_ticks(self) -> u32 {
        match self {
            Self::Honest | Self::Routed => HONEST_MAX_TICKS,
            Self::HitAndRun | Self::StunAndFlee => EXPLOIT_MAX_TICKS,
        }
    }
}

/// One character, fully specified. Every field is required: the arena has no
/// opinion about what a "normal" character is, the caller states it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Recipe {
    pub class: Class,
    pub level: i32,
    /// `None` below `ARCHETYPE_LEVEL` or to measure the bare class.
    pub archetype: Option<&'static ArchetypeDef>,
    pub gear: Gear,
    pub companion: Companion,
    pub coat: Coat,
    /// Level-appropriate healing draughts in the bag (`potion_for_level`).
    pub potions: u32,
    pub policy: Policy,
    pub build: Build,
}

impl Recipe {
    pub(super) fn label(&self) -> String {
        let arch = match self.archetype {
            Some(a) => a.name,
            None => "no path",
        };
        format!(
            "{:?} L{} {} {} {} {} {}x{} {} {}",
            self.class,
            self.level,
            arch,
            self.gear.label(),
            self.companion.label(),
            self.coat.label(),
            self.potions,
            item(potion_for_level(self.level))
                .map(|i| i.name)
                .unwrap_or("?"),
            self.policy.label(),
            self.build.label()
        )
    }
}

/// The DPS path if the class offers one, else its first path. What a player
/// chasing damage picks.
pub(super) fn dps_or_first(class: Class) -> &'static ArchetypeDef {
    let paths = archetypes_for(class);
    match paths.iter().find(|a| a.role == Role::Dps) {
        Some(a) => a,
        None => match paths.first() {
            Some(a) => a,
            None => panic!("arena: {class:?} has no archetype paths"),
        },
    }
}

/// The healing draught a character of this level would carry (Apothecary
/// stock, `items.rs` ids `1300..1306`).
pub(super) fn potion_for_level(level: i32) -> u32 {
    match level {
        ..=9 => 1300,    // Minor Healing Draught, 40
        10..=24 => 1301, // Healing Potion, 90
        25..=39 => 1302, // Greater Healing Elixir, 210
        40..=59 => 1304, // Elixir of Renewal, 180 + 120 resource
        _ => 1305,       // Phoenix Tonic, 420 + 220 resource
    }
}

/// Whether the class has a `Stun` unlocked at this level (StunAndFlee needs one).
pub(super) fn has_stun(class: Class, level: i32) -> bool {
    unlocked_for(class, level)
        .iter()
        .any(|a| a.effect == AbilityEffect::Stun)
}

// ---- Results --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Won,
    Died,
    /// The tick budget ran out with both alive.
    Stalemate,
    /// The foe fled the room and never came back (Skirmisher/Thief).
    Escaped,
}

impl Outcome {
    pub(super) fn glyph(self) -> &'static str {
        match self {
            Self::Won => "W",
            Self::Died => "D",
            Self::Stalemate => "S",
            Self::Escaped => "E",
        }
    }
}

/// Damage the character dealt, by source, measured from the foe's health.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Damage {
    /// The Physical auto-attack (parsed from the strike line).
    pub auto: i32,
    /// Instant ability hits (Strike/Finisher/Stun), the health drop across the
    /// casts made before a tick.
    pub ability: i32,
    /// Ability damage-over-time ticks.
    pub dot: i32,
    /// Weapon-coat ticks.
    pub coat: i32,
    /// The companion's bite and auto-skills (everything a tick took that the
    /// dots and the auto do not account for).
    pub pet: i32,
}

impl Damage {
    pub(super) fn total(&self) -> i32 {
        self.auto + self.ability + self.dot + self.coat + self.pet
    }

    fn pct(&self, part: i32) -> i32 {
        let total = self.total();
        if total <= 0 { 0 } else { part * 100 / total }
    }

    /// `auto/ability/dot/coat/pet` as percents of the total.
    pub(super) fn shares(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            self.pct(self.auto),
            self.pct(self.ability),
            self.pct(self.dot),
            self.pct(self.coat),
            self.pct(self.pet)
        )
    }
}

/// The foe as the engine actually fields it (post `tune_spawn_balance`).
#[derive(Clone, Copy, Debug)]
pub(super) struct FoeCard {
    pub name: &'static str,
    pub level: i32,
    pub max_hp: i32,
    pub damage: i32,
    pub attack_type: DamageType,
    pub weak: Option<DamageType>,
    pub resist: Option<DamageType>,
}

impl FoeCard {
    pub(super) fn label(&self) -> String {
        let school = |s: Option<DamageType>| s.map(|d| d.label()).unwrap_or("nothing");
        format!(
            "{} (Lv{}, {} hp, {} dmg, strikes {}, weak to {}, resists {})",
            self.name,
            self.level,
            self.max_hp,
            self.damage,
            self.attack_type.label(),
            school(self.weak),
            school(self.resist)
        )
    }
}

#[derive(Clone, Debug)]
pub(super) struct FightResult {
    pub outcome: Outcome,
    pub ticks: u32,
    /// Health left at the end, percent of max (0 when dead).
    pub hp_left_pct: i32,
    pub potions_used: u32,
    /// Net health lost across exchanges (after armor, shields, in-tick heals).
    pub taken: i32,
    pub dealt: Damage,
    /// The character's sheet numbers at the bell, for the report.
    pub attack: i32,
    pub swing: i32,
    pub spell_power: i32,
    pub max_hp: i32,
    pub foe: FoeCard,
}

impl FightResult {
    /// One report cell: outcome, ticks, health left, potions drunk, damage shares.
    pub(super) fn cell(&self) -> String {
        format!(
            "{} {}t {}% {}p {}",
            self.outcome.glyph(),
            self.ticks,
            self.hp_left_pct,
            self.potions_used,
            self.dealt.shares()
        )
    }
}

// ---- The arena -----------------------------------------------------------

#[derive(Default)]
struct Accum {
    ticks: u32,
    taken: i32,
    potions: u32,
    dealt: Damage,
}

enum FoeStatus {
    Here,
    Dead,
    Away,
}

/// One seeded world, reused across fights. `seed_world` leaks every generated
/// string to `'static`, so it must run once per process, not once per fight.
pub(super) struct Arena {
    s: WorldState,
    seq: u128,
}

impl Arena {
    pub(super) fn new() -> Self {
        Self {
            s: WorldState::new(Uuid::from_u128(0xA1E7A), seed_world()),
            seq: 1,
        }
    }

    /// Every spawn flagged `boss`, in world order.
    pub(super) fn bosses(&self) -> Vec<&'static str> {
        self.s
            .world
            .spawns
            .iter()
            .filter(|sp| sp.boss)
            .map(|sp| sp.name)
            .collect()
    }

    fn spawn_id(&self, name: &str) -> u32 {
        match self.s.world.spawns.iter().find(|sp| sp.name == name) {
            Some(sp) => sp.id,
            None => panic!("arena: no spawn named {name:?}"),
        }
    }

    /// The regulars homed in the same zone as `boss`: the trash on a crown's
    /// doorstep, as the engine fields it.
    pub(super) fn doorstep(&self, boss: &str) -> Vec<FoeCard> {
        let home = self.s.mobs[&self.spawn_id(boss)].spawn.home;
        let zone = match self.s.world.room(home) {
            Some(r) => r.zone,
            None => panic!("arena: crown {boss:?} is homed in a room that does not exist"),
        };
        self.s
            .world
            .spawns
            .iter()
            .filter(|sp| !sp.boss && self.s.world.room(sp.home).is_some_and(|r| r.zone == zone))
            .map(|sp| self.foe(sp.name))
            .collect()
    }

    pub(super) fn foe(&self, name: &str) -> FoeCard {
        let sp = &self.s.mobs[&self.spawn_id(name)].spawn;
        FoeCard {
            name: sp.name,
            level: sp.level(),
            max_hp: sp.max_hp,
            damage: sp.damage,
            attack_type: sp.profile.attack_type,
            weak: sp.profile.weak,
            resist: sp.profile.resist,
        }
    }

    /// Run one fight: a fresh character from `recipe` against `foe_name` in
    /// its home room, every mob in that room at full health.
    pub(super) fn fight(&mut self, recipe: Recipe, foe_name: &str) -> FightResult {
        let mob_id = self.spawn_id(foe_name);
        let home = self.s.mobs[&mob_id].spawn.home;
        let profile = self.s.mobs[&mob_id].spawn.profile;
        self.reset_room(home);
        self.s.world_ticks = ARENA_CLOCK;
        let uid = self.spawn_character(&recipe, home, &profile);
        let (attack, swing, spell_power, max_hp) = {
            let p = &self.s.players[&uid];
            (p.attack(), p.swing(), p.spell_power(), p.max_hp())
        };
        let mut acc = Accum::default();
        let outcome = match recipe.policy {
            Policy::Honest | Policy::Routed => {
                self.run_honest(uid, mob_id, home, &recipe, &mut acc)
            }
            Policy::HitAndRun => self.run_hit_and_run(uid, mob_id, home, &recipe, &mut acc),
            Policy::StunAndFlee => self.run_stun_and_flee(uid, mob_id, home, &recipe, &mut acc),
        };
        let hp_left_pct = {
            let p = &self.s.players[&uid];
            if p.dead {
                0
            } else {
                p.hp * 100 / p.max_hp().max(1)
            }
        };
        self.s.players.remove(&uid);
        self.s.mob_dots.remove(&mob_id);
        self.s.pet_skill_cd.retain(|(owner, _), _| *owner != uid);
        FightResult {
            outcome,
            ticks: acc.ticks,
            hp_left_pct,
            potions_used: acc.potions,
            taken: acc.taken,
            dealt: acc.dealt,
            attack,
            swing,
            spell_power,
            max_hp,
            foe: self.foe(foe_name),
        }
    }

    /// The character a recipe builds, as numbers: (attack, swing, spell
    /// power, max hp, armor). For deriving what a crown has to be to stand
    /// up to it.
    pub(super) fn sheet(&mut self, recipe: Recipe) -> (i32, i32, i32, i32, i32) {
        let dummy = self.spawn_id("a straw training dummy");
        let home = self.s.mobs[&dummy].spawn.home;
        let profile = self.s.mobs[&dummy].spawn.profile;
        let uid = self.spawn_character(&recipe, home, &profile);
        let p = &self.s.players[&uid];
        let out = (
            p.attack(),
            p.swing(),
            p.spell_power(),
            p.max_hp(),
            p.armor(),
        );
        self.s.players.remove(&uid);
        out
    }

    /// Damage per tick over `ticks` ticks of honest fighting against the
    /// neutral straw training dummy (Physical, no resist, no weakness, a
    /// 1-damage swing), its pool raised so it cannot die. The one number
    /// that compares callings independent of their shape: what a character
    /// puts out per tick with a full rotation and nothing to dodge.
    pub(super) fn measure_dps(&mut self, recipe: Recipe, ticks: u32) -> i32 {
        self.measure(recipe, ticks).total() / ticks as i32
    }

    /// The damage behind `measure_dps`, by source, over the whole window.
    pub(super) fn measure(&mut self, recipe: Recipe, ticks: u32) -> Damage {
        const DUMMY: &str = "a straw training dummy";
        assert!(
            matches!(recipe.policy, Policy::Honest | Policy::Routed),
            "dps is an honest number"
        );
        let mob_id = self.spawn_id(DUMMY);
        let home = self.s.mobs[&mob_id].spawn.home;
        let real_pool = self.s.mobs[&mob_id].spawn.max_hp;
        self.s.mobs.get_mut(&mob_id).expect("dummy").spawn.max_hp = 10_000_000;
        self.reset_room(home);
        self.s.world_ticks = ARENA_CLOCK;
        let profile = self.s.mobs[&mob_id].spawn.profile;
        let uid = self.spawn_character(&recipe, home, &profile);
        let known = unlocked_for(recipe.class, recipe.level);
        let mut acc = Accum::default();
        for _ in 0..ticks {
            self.engage(uid, mob_id, home);
            self.prep(uid, mob_id, &recipe, &known, &mut acc);
            self.exchange(uid, mob_id, &mut acc);
        }
        self.s.players.remove(&uid);
        self.s.mob_dots.remove(&mob_id);
        self.s.pet_skill_cd.retain(|(owner, _), _| *owner != uid);
        self.s.mobs.get_mut(&mob_id).expect("dummy").spawn.max_hp = real_pool;
        self.reset_room(home);
        acc.dealt
    }

    /// Full health, alive, revealed, at home, unstunned, unwounded, for every
    /// mob that lives in or has wandered into `room`; summoned adds reaped.
    fn reset_room(&mut self, room: RoomId) {
        self.s.mobs.retain(|id, _| *id < SUMMON_ID_START);
        let ids: Vec<u32> = self
            .s
            .mobs
            .values()
            .filter(|m| m.leash_home == room || m.current_room == room)
            .map(|m| m.spawn.id)
            .collect();
        for id in ids {
            let m = self.s.mobs.get_mut(&id).expect("listed above");
            m.alive = true;
            m.hp = m.spawn.max_hp;
            m.respawn_at = None;
            m.current_room = m.leash_home;
            m.move_cooldown = 0;
            m.summon_cooldown = 0;
            m.untargeted = 0;
            m.revealed = true;
            self.s.mob_stuns.remove(&id);
            self.s.mob_dots.remove(&id);
        }
    }

    fn spawn_character(&mut self, recipe: &Recipe, room: RoomId, foe: &DamageProfile) -> Uuid {
        let uid = Uuid::from_u128(0x000A_1E7A_0000_0000 + self.seq);
        self.seq += 1;
        assert!(self.s.join(uid), "arena: character uuid collided");
        self.s.choose_class(uid, recipe.class);
        let stats = recipe.class.stats_at(recipe.level);
        let p = self.s.players.get_mut(&uid).expect("joined above");
        p.level = recipe.level;
        // Pinned so a kill's xp can never level the character mid-report.
        p.xp = xp_for_level(recipe.level);
        p.base_max_hp = stats.max_hp;
        p.max_resource = stats.max_resource;
        p.resource_regen = stats.resource_regen;
        p.base_attack = stats.attack;
        p.scores = recipe.build.scores(recipe.class);
        p.archetype = recipe.archetype;
        p.inventory.clear();
        p.equipped.clear();
        for id in recipe.gear.pieces() {
            p.equipped.insert(slot_of(id), id);
        }
        p.pet = recipe
            .companion
            .species()
            .map(|sp| Pet::new(sp, (PET_MAX_LEVEL as i64 - 1) * LOYALTY_PER_LEVEL));
        let potion = potion_for_level(recipe.level);
        p.inventory
            .extend(std::iter::repeat_n(potion, recipe.potions as usize));
        if let Some(vial) = recipe.coat.vial_for(foe) {
            p.inventory
                .extend(std::iter::repeat_n(vial, COAT_SUPPLY as usize));
        }
        p.gold = 0;
        p.room = room;
        std::sync::Arc::make_mut(&mut p.visited).insert(room);
        p.hp = p.max_hp();
        p.resource = p.max_resource;
        uid
    }

    fn foe_status(&self, mob_id: u32, home: RoomId) -> FoeStatus {
        let m = &self.s.mobs[&mob_id];
        if !m.alive {
            FoeStatus::Dead
        } else if m.current_room != home {
            FoeStatus::Away
        } else {
            FoeStatus::Here
        }
    }

    fn engaged(&self, uid: Uuid, mob_id: u32) -> bool {
        self.s.players[&uid].target == Some(mob_id)
    }

    fn engage(&mut self, uid: Uuid, mob_id: u32, home: RoomId) {
        if self.engaged(uid, mob_id) {
            return;
        }
        self.s.engage_mob(uid, mob_id);
        assert!(
            self.engaged(uid, mob_id),
            "arena: could not engage mob {mob_id} in room {home}"
        );
    }

    fn run_honest(
        &mut self,
        uid: Uuid,
        mob_id: u32,
        home: RoomId,
        recipe: &Recipe,
        acc: &mut Accum,
    ) -> Outcome {
        let known = unlocked_for(recipe.class, recipe.level);
        for _ in 0..recipe.policy.max_ticks() {
            match self.foe_status(mob_id, home) {
                FoeStatus::Dead => return Outcome::Won,
                FoeStatus::Away => return Outcome::Escaped,
                FoeStatus::Here => {}
            }
            self.engage(uid, mob_id, home);
            self.prep(uid, mob_id, recipe, &known, acc);
            self.exchange(uid, mob_id, acc);
            if self.s.players[&uid].dead {
                return Outcome::Died;
            }
        }
        Outcome::Stalemate
    }

    fn run_hit_and_run(
        &mut self,
        uid: Uuid,
        mob_id: u32,
        home: RoomId,
        recipe: &Recipe,
        acc: &mut Accum,
    ) -> Outcome {
        let known = unlocked_for(recipe.class, recipe.level);
        while acc.ticks < recipe.policy.max_ticks() {
            match self.foe_status(mob_id, home) {
                FoeStatus::Dead => return Outcome::Won,
                FoeStatus::Away => return Outcome::Escaped,
                FoeStatus::Here => {}
            }
            let low = {
                let p = &self.s.players[&uid];
                p.hp * 100 / p.max_hp().max(1) < RETREAT_UNDER_PCT
            };
            if low {
                // The road to a fountain and back: the heal is free, the walk
                // is ticks, and nothing in the engine touches the foe meanwhile.
                if self.engaged(uid, mob_id) {
                    self.s.flee(uid);
                    if self.s.players[&uid].dead {
                        return Outcome::Died;
                    }
                }
                for _ in 0..RETREAT_TICKS {
                    self.exchange(uid, mob_id, acc);
                }
                let p = self.s.players.get_mut(&uid).expect("present");
                p.hp = p.max_hp();
                p.room = home;
                continue;
            }
            self.engage(uid, mob_id, home);
            self.prep(uid, mob_id, recipe, &known, acc);
            self.exchange(uid, mob_id, acc);
            if self.s.players[&uid].dead {
                return Outcome::Died;
            }
            if self.engaged(uid, mob_id) {
                self.s.flee(uid);
                if self.s.players[&uid].dead {
                    return Outcome::Died;
                }
                self.s.players.get_mut(&uid).expect("present").room = home;
            }
        }
        Outcome::Stalemate
    }

    fn run_stun_and_flee(
        &mut self,
        uid: Uuid,
        mob_id: u32,
        home: RoomId,
        recipe: &Recipe,
        acc: &mut Accum,
    ) -> Outcome {
        let known = unlocked_for(recipe.class, recipe.level);
        assert!(
            known.iter().any(|a| a.effect == AbilityEffect::Stun),
            "arena: StunAndFlee needs a stun; {:?} at L{} has none",
            recipe.class,
            recipe.level
        );
        while acc.ticks < recipe.policy.max_ticks() {
            match self.foe_status(mob_id, home) {
                FoeStatus::Dead => return Outcome::Won,
                FoeStatus::Away => return Outcome::Escaped,
                FoeStatus::Here => {}
            }
            let foe_stunned = self.s.mob_stuns.get(&mob_id).copied().unwrap_or(0) > 0;
            let stun_ready = {
                let p = &self.s.players[&uid];
                known.iter().any(|a| {
                    a.effect == AbilityEffect::Stun
                        && p.cooldowns.get(&a.id).copied().unwrap_or(0) == 0
                        && p.resource >= a.cost
                })
            };
            if !(foe_stunned || stun_ready) {
                // Wait out the cooldown, out of reach.
                self.exchange(uid, mob_id, acc);
                continue;
            }
            self.engage(uid, mob_id, home);
            self.prep(uid, mob_id, recipe, &known, acc);
            self.exchange(uid, mob_id, acc);
            if self.s.players[&uid].dead {
                return Outcome::Died;
            }
            if self.engaged(uid, mob_id) {
                self.s.flee(uid);
                if self.s.players[&uid].dead {
                    return Outcome::Died;
                }
                self.s.players.get_mut(&uid).expect("present").room = home;
            }
        }
        Outcome::Stalemate
    }

    /// Everything a player does between two ticks: drink, re-coat, cast.
    /// Instant ability damage is measured here from the foe's health.
    fn prep(
        &mut self,
        uid: Uuid,
        mob_id: u32,
        recipe: &Recipe,
        known: &[&'static Ability],
        acc: &mut Accum,
    ) {
        let (hp_pct, coat_dry, has_potion, has_vial) = {
            let p = &self.s.players[&uid];
            let potion = potion_for_level(recipe.level);
            let vial = recipe.coat.vial_for(&self.s.mobs[&mob_id].spawn.profile);
            (
                p.hp * 100 / p.max_hp().max(1),
                p.weapon_coat.is_none(),
                p.quaff_cd == 0 && p.inventory.contains(&potion),
                vial.filter(|v| p.inventory.contains(v)),
            )
        };
        if hp_pct < DRINK_UNDER_PCT && has_potion {
            self.s.use_item(uid, potion_for_level(recipe.level));
            acc.potions += 1;
        }
        if coat_dry && let Some(vial) = has_vial {
            self.s.use_item(uid, vial);
        }
        let before = self.s.mobs[&mob_id].hp;
        let foe_damage = self.s.mobs[&mob_id].spawn.damage;
        let prefer_stun = recipe.policy == Policy::StunAndFlee;
        let routed = recipe.policy == Policy::Routed;
        for _ in 0..CASTS_PER_TICK {
            match self.choose_cast(uid, mob_id, known, foe_damage, prefer_stun, routed) {
                Some(slot) => self.s.use_ability(uid, slot),
                None => break,
            }
        }
        let after = self.s.mobs[&mob_id].hp.max(0);
        acc.dealt.ability += before - after;
    }

    /// One world tick, with the foe's health drop attributed to its sources.
    /// Order inside the tick is dots, then the auto, then the pet, so dots are
    /// charged first (what was due), the auto is read off its log line, and
    /// the remainder is the companion.
    fn exchange(&mut self, uid: Uuid, mob_id: u32, acc: &mut Accum) {
        let before = self.s.mobs[&mob_id].hp;
        let (dot_due, coat_due) = self
            .s
            .mob_dots
            .get(&mob_id)
            .map(|stacks| {
                stacks
                    .iter()
                    .filter(|d| d.remaining > 0)
                    .fold((0, 0), |(a, c), d| match d.source {
                        DotSource::Ability => (a + d.per_tick, c),
                        DotSource::Coat => (a, c + d.per_tick),
                    })
            })
            .unwrap_or((0, 0));
        let hp_before = {
            let p = self.s.players.get_mut(&uid).expect("present");
            p.log.clear();
            p.hp
        };
        self.s.tick();
        acc.ticks += 1;
        let after = self.s.mobs[&mob_id].hp.max(0);
        let delta = (before - after).max(0);
        let due = dot_due + coat_due;
        let dots_applied = due.min(delta);
        let coat_applied = if due > 0 {
            dots_applied * coat_due / due
        } else {
            0
        };
        let rest = delta - dots_applied;
        let auto = self.s.players[&uid]
            .log
            .iter()
            .filter_map(|l| strike_amount(&l.text))
            .sum::<i32>()
            .min(rest);
        acc.dealt.dot += dots_applied - coat_applied;
        acc.dealt.coat += coat_applied;
        acc.dealt.auto += auto;
        acc.dealt.pet += rest - auto;
        let p = &self.s.players[&uid];
        acc.taken += (hp_before - p.hp).max(0);
    }

    /// The honest rotation. Defensive needs first, then the best ready
    /// offensive by expected damage; `routed` weighs that by the foe's
    /// resist/weak multiplier for the ability's school. Returns a 1-based
    /// hotbar slot.
    fn choose_cast(
        &self,
        uid: Uuid,
        mob_id: u32,
        known: &[&'static Ability],
        foe_damage: i32,
        prefer_stun: bool,
        routed: bool,
    ) -> Option<u8> {
        let p = &self.s.players[&uid];
        let profile = self.s.mobs[&mob_id].spawn.profile;
        let school = |a: &Ability| -> i32 {
            if routed {
                profile.defense_against(a.damage_type).multiplier_pct()
            } else {
                100
            }
        };
        let hp_pct = p.hp * 100 / p.max_hp().max(1);
        let has_hot = p
            .self_effects
            .iter()
            .any(|e| e.kind == AbilityEffect::HealOverTime && e.remaining > 0);
        let live_dots = self
            .s
            .mob_dots
            .get(&mob_id)
            .map(|st| {
                st.iter()
                    .filter(|d| d.owner == uid && d.source == DotSource::Ability && d.remaining > 0)
                    .count()
            })
            .unwrap_or(0);
        let dot_abilities = known
            .iter()
            .filter(|a| a.effect == AbilityEffect::DamageOverTime)
            .count();
        let ready =
            |a: &Ability| p.cooldowns.get(&a.id).copied().unwrap_or(0) == 0 && p.resource >= a.cost;
        let best = |value: &dyn Fn(&Ability) -> Option<i32>| -> Option<u8> {
            known
                .iter()
                .enumerate()
                .filter(|(_, a)| ready(a))
                .filter_map(|(i, a)| value(a).map(|v| (v, (i + 1) as u8)))
                .max_by_key(|(v, _)| *v)
                .map(|(_, slot)| slot)
        };
        let of =
            |effect: AbilityEffect| move |a: &Ability| (a.effect == effect).then_some(a.magnitude);
        if prefer_stun
            && let Some(slot) =
                best(&|a| (a.effect == AbilityEffect::Stun).then_some(a.duration as i32))
        {
            return Some(slot);
        }
        if hp_pct < HEAL_UNDER_PCT
            && let Some(slot) = best(&of(AbilityEffect::Heal))
        {
            return Some(slot);
        }
        if hp_pct < HOT_UNDER_PCT
            && !has_hot
            && let Some(slot) = best(&of(AbilityEffect::HealOverTime))
        {
            return Some(slot);
        }
        if p.shield == 0
            && hp_pct < WARD_UNDER_PCT
            && let Some(slot) = best(&of(AbilityEffect::Ward))
        {
            return Some(slot);
        }
        if p.empower == 0
            && let Some(slot) = best(&of(AbilityEffect::Empower))
        {
            return Some(slot);
        }
        best(&|a: &Ability| match a.effect {
            AbilityEffect::Strike | AbilityEffect::Finisher => Some(a.magnitude * school(a) / 100),
            AbilityEffect::Stun => Some(a.magnitude * school(a) / 100 + foe_damage),
            AbilityEffect::DamageOverTime => (live_dots < dot_abilities)
                .then_some(a.magnitude * a.duration as i32 * school(a) / 100),
            AbilityEffect::Heal
            | AbilityEffect::HealOverTime
            | AbilityEffect::Empower
            | AbilityEffect::Ward => None,
        })
    }
}

/// The damage on a `You strike X for N physical...` / `You crush into X for N
/// physical...` line, the only two shapes the auto-attack logs.
fn strike_amount(line: &str) -> Option<i32> {
    if !(line.starts_with("You strike ") || line.starts_with("You crush into ")) {
        return None;
    }
    let rest = line.rsplit(" for ").next()?;
    rest.split(' ').next()?.parse().ok()
}

#[cfg(test)]
#[path = "arena_test.rs"]
mod arena_test;
