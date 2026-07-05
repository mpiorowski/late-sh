//! The persistent Legend of the Green Dragon character and the pure rules that
//! act on it: stat derivation, leveling, shop pricing, healing, banking, and
//! the win/lose outcomes. All authentic LoGD numbers (see [`super::data`]).
//!
//! This module is pure and serde-able: no DB, no RNG except where a fight is
//! resolved through [`super::combat`]. Tests assert the transcribed formulas.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::combat::{Combatant, Companion};
use super::data;

/// Starting on-hand gold for a fresh character (`newplayerstartgold`).
pub const START_GOLD: u64 = 50;
/// Forest fights granted per day (`turns`).
pub const TURNS_PER_DAY: u32 = 10;
/// Hitpoints per level — max HP is a flat `HP_PER_LEVEL * level`.
pub const HP_PER_LEVEL: u32 = 10;
/// Fraction of experience kept after a forest death (`1 - forestexploss`).
pub const EXP_KEEP_ON_DEATH: f64 = 0.90;
/// Forest turns you may leave unused and still earn bank interest (LoGD
/// `fightsforinterest`). Leave more than this unused and you didn't work for it.
pub const FIGHTS_FOR_INTEREST: u32 = 4;
/// Bank balance at/above which no interest is paid (LoGD `maxgoldforinterest`).
pub const MAX_GOLD_FOR_INTEREST: i64 = 100_000;
/// Daily bank interest is a random percent in this inclusive range, rolled fresh
/// each new day (LoGD `mininterest`/`maxinterest` defaults).
pub const MIN_INTEREST_PERCENT: u32 = 1;
pub const MAX_INTEREST_PERCENT: u32 = 10;
/// Gold carried into a fresh run after a dragon kill, before the flawless
/// bonus (LoGD `maxrestartgold`): [`START_GOLD`] plus [`START_GOLD`] per kill,
/// capped here. On-hand gold is *not* retained — the run reset wipes it.
pub const DRAGON_RUN_GOLD_CAP: u64 = 300;
/// Gem ceiling carried into a fresh run after a dragon kill (LoGD
/// `maxrestartgems`).
pub const MAX_RESTART_GEMS: u32 = 10;
/// Max HP granted per dragon point spent on `hp` (LoGD `dragonpointspend`).
pub const HP_PER_DRAGON_POINT: u32 = 5;
/// Gold the bank will lend per character level (LoGD `borrowperlevel`). Debt is
/// a negative balance and accrues interest daily.
pub const BORROW_PER_LEVEL: i64 = 20;
/// One-in-this chance of a gem on a forest victory under level 15 (LoGD
/// `forestgemchance`).
pub const FOREST_GEM_CHANCE: u32 = 25;
/// Charm gained per dragon kill (LoGD `charm += 5`).
pub const CHARM_PER_DRAGON_KILL: u32 = 5;
/// Bonus gold (3x [`START_GOLD`]) and a gem for a flawless, no-damage dragon
/// kill (LoGD's flawless bonus), added on top of the gold cap.
pub const FLAWLESS_GOLD_BONUS: u64 = START_GOLD * 3;
/// Soulpoints awarded for beating a master (LoGD `train.php`).
pub const SOULPOINTS_PER_MASTER: u32 = 5;
/// Forest turns docked by the *paid* resurrection's immediate new day (LoGD
/// `resurrectionturns`, default -6; `newday.php` applies it only when
/// `resurrection=true`). Waiting out the night for free costs nothing extra.
pub const RESURRECTION_TURNS: i32 = -6;
/// Torment fights granted per day in the graveyard (LoGD `gravefightsperday`).
pub const GRAVE_FIGHTS_PER_DAY: u32 = 10;
/// Favor the death overlord charges to resurrect you on the spot
/// (`lib/graveyard/case_resurrection.php`).
pub const RESURRECTION_FAVOR_COST: u32 = 100;
/// Favor at which the overlord starts granting favors (`case_question.php`).
/// The 25-favor haunt itself is PvP and lands in phase 4; the tier messaging
/// exists now.
pub const HAUNT_FAVOR_THRESHOLD: u32 = 25;
/// Extra daily forest fights for the Plainsborn (LoGD `racehuman.php`'s
/// `bonus` setting, default 2). Like upstream's `newday` hook it applies to
/// every new day, including the paid resurrection's.
pub const PLAINSBORN_FOREST_BONUS: u32 = 2;
/// Percent chance of dying under a goldmine cave-in (`raceminedeath`):
/// the default for most races, and the Deepfolk's miner's instinct
/// (`racedwarf.php`'s `minedeathchance`, default 5).
pub const MINE_DEATH_PERCENT: u32 = 90;
pub const DEEPFOLK_MINE_DEATH_PERCENT: u32 = 5;

/// The forest hunting intensities. LoGD offers easier/harder pickings that
/// shift the creature level relative to the player's own level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForestHunt {
    /// "Go Slumming" — weaker creatures (player level - 2).
    Slumming,
    /// "Look for Something to Kill" — creatures at the player's level.
    Hunt,
    /// "Go Thrillseeking" — tougher creatures (player level + 2).
    Thrillseeking,
}

impl ForestHunt {
    /// The creature level this hunt produces for a given player level. LoGD
    /// shifts the target level by ±1 for slumming/thrillseeking (a small random
    /// jitter is layered on at the call site).
    pub fn creature_level(self, player_level: u8) -> u8 {
        let delta: i16 = match self {
            ForestHunt::Slumming => -1,
            ForestHunt::Hunt => 0,
            ForestHunt::Thrillseeking => 1,
        };
        (player_level as i16 + delta).clamp(1, 16) as u8
    }
}

/// One slain foe's contribution to a forest victory settlement.
#[derive(Clone, Copy, Debug)]
pub struct SlainFoe {
    pub level: u8,
    pub gold: u32,
    pub exp: u32,
}

/// What a settled forest victory paid out, for logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForestVictory {
    pub gold: u64,
    pub exp: u64,
    pub gem: bool,
    pub flawless: bool,
    pub turn_refunded: bool,
}

/// A persistent Green Dragon character. One per user, stored as a JSON blob.
///
/// Stats that are fully derivable (attack, defense, max HP) are *not* stored —
/// they come from `level` + equipped tiers, matching how LoGD recomputes them.
/// Every field carries a serde default so old saves load without a migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Character {
    /// Display name (the player's late.sh username).
    pub name: String,
    pub level: u8,
    pub experience: u64,
    /// Current hitpoints. Max is derived via [`Character::max_hitpoints`].
    pub hitpoints: u32,
    /// Equipped weapon tier, 0 (Fists) ..= 15.
    pub weapon_tier: u8,
    /// Equipped armor tier, 0 (none) ..= 15.
    pub armor_tier: u8,
    pub gold: u64,
    /// Banked gold. **Signed**: a negative balance is a live loan (LoGD lets
    /// you borrow up to `level * BORROW_PER_LEVEL`), and debt accrues interest.
    pub gold_in_bank: i64,
    /// Forest fights remaining today.
    pub turns: u32,
    /// False after a forest death; revived on the next new day.
    pub alive: bool,
    /// Whether the player has sought the dragon this run (resets per run).
    pub seen_dragon: bool,
    /// Lifetime dragon kills.
    pub dragon_kills: u32,
    /// Permanent max-HP bought with `hp` dragon points (+5 each).
    pub dragon_hp_bonus: u32,
    /// Permanent attack bought with `at` dragon points (+1 each).
    pub dragon_attack_bonus: u32,
    /// Permanent defense bought with `de` dragon points (+1 each).
    pub dragon_defense_bonus: u32,
    /// Permanent extra daily forest fights bought with `ff` dragon points
    /// (+1/day each, LoGD's `dkff`).
    pub dragon_ff_bonus: u32,
    /// Dragon points earned (one per kill) but not yet allocated. While any are
    /// unspent the spend gate blocks play, exactly like LoGD's new-day gate.
    pub dragon_points_unspent: u32,
    /// Gems: the second currency, found in the forest and spent advancing your
    /// specialty (LoGD's gem economy). Distinct from gold.
    pub gems: u64,
    /// Charm: LoGD's social stat, gained on dragon kills (`+5`). Feeds the
    /// not-yet-built flirting/marriage systems; tracked for parity.
    pub charm: u32,
    /// Soulpoints: the dead-realm HP pool (see [`Character::max_soulpoints`]).
    /// Refilled each new day to `50 + 5*level`, `+5` per master beaten; while
    /// dead, torment fights spend it and damage persists between them.
    pub soulpoints: u32,
    /// Favor with the death overlord (LoGD `deathpower`): earned tormenting
    /// souls while dead, spent restoring the soul pool, fleeing torments, and
    /// buying the paid resurrection. Persists across days and revivals.
    pub favor: u32,
    /// Torment fights remaining today in the graveyard (LoGD `gravefights`).
    /// Refilled on a normal new day, *not* by the paid resurrection.
    pub grave_fights: u32,
    /// Persistent combat companions (e.g. a Bonecall skeleton). They fight
    /// alongside you across battles until destroyed (LoGD `apply_companion`).
    pub companions: Vec<Companion>,
    /// Chosen ancestry (LoGD's race). `None` arms the forced choice gate on
    /// load; permanent once picked (until phase 3's transmutation potion).
    pub race: Race,
    /// The current dragon-kill title, shown before the name. Assigned from the
    /// title ladder at first load and re-rolled on every dragon kill
    /// (`dragon.php` + `lib/titles.php`). Empty only on a never-titled save.
    pub title: String,
    /// Which title column (and later, phase-3 flavor) this character uses.
    pub style: AddressStyle,
    /// Chosen combat specialty, picked once and largely permanent. `None` until
    /// the player decides (LoGD sets it on the first new day).
    pub specialty: Specialty,
    /// Lifetime skill points in the chosen specialty. Advanced by training
    /// (gems) and by certain forest events. Every 3rd point grants a use.
    pub specialty_skill: u32,
    /// Spendable specialty "uses" for today: `floor(skill/3)` refreshed each new
    /// day, +1 bonus for the specialty you actually chose.
    pub specialty_uses: u32,
    /// UTC day-number of the last new-day reset, for turn/heal regeneration.
    pub last_day: i64,
}

/// The four permanent upgrades a dragon point can buy (LoGD `dragonpointspend`:
/// `hp`/`ff`/`at`/`de`). One point is earned per dragon kill and must be
/// allocated before the next day's play begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonPointKind {
    /// +5 permanent max hitpoints.
    Hp,
    /// +1 permanent daily forest fight.
    ForestFights,
    /// +1 permanent attack.
    Attack,
    /// +1 permanent defense.
    Defense,
}

impl DragonPointKind {
    /// Short display label for the spend menu.
    pub fn label(self) -> &'static str {
        match self {
            DragonPointKind::Hp => "+5 max hitpoints",
            DragonPointKind::ForestFights => "+1 forest fight per day",
            DragonPointKind::Attack => "+1 attack",
            DragonPointKind::Defense => "+1 defense",
        }
    }
}

/// The four ancestries, mirroring LoGD's stock race modules
/// (`racehuman`/`raceelf`/`racedwarf`/`racetroll`). A forced one-time choice at
/// the new-day gate; `None` re-arms it (fresh characters, and phase 3's
/// transmutation potion). **Effect numbers are upstream's exactly; the race
/// names are original to late.sh** (the generic analogs are noted per variant).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Race {
    /// Unchosen; the race gate arms on load.
    #[default]
    None,
    /// The human-analog: tireless plains stock, +2 forest fights per day.
    Plainsborn,
    /// The elf-analog: wary forest folk, `+1 + level/5` defense.
    Wealdkin,
    /// The dwarf-analog: delvers of the under-roads, creature gold x1.2 and
    /// a near-immunity to mine cave-ins.
    Deepfolk,
    /// The troll-analog: crag-born brutes, `+1 + level/5` attack.
    Cragborn,
}

/// The four choosable ancestries, in menu order.
pub const RACES: [Race; 4] = [
    Race::Plainsborn,
    Race::Wealdkin,
    Race::Deepfolk,
    Race::Cragborn,
];

impl Race {
    /// Short display label for the stat rail.
    pub fn name(self) -> &'static str {
        match self {
            Race::None => "Unchosen",
            Race::Plainsborn => "Plainsborn",
            Race::Wealdkin => "Wealdkin",
            Race::Deepfolk => "Deepfolk",
            Race::Cragborn => "Cragborn",
        }
    }

    /// The level-scaled stat bonus the elf/troll analogs share:
    /// `1 + floor(level / 5)` (`raceelf.php`/`racetroll.php` `adjuststats`).
    fn scaling_bonus(level: u8) -> u32 {
        1 + level as u32 / 5
    }

    /// Permanent attack bonus (the Cragborn's brawn). Upstream implements this
    /// as a `racialbenefit` buff recomputed each new day; a flat add into the
    /// attack stat is numerically identical. Buffs don't follow the dead, and
    /// neither does this: [`Character::dead_combatant`] ignores it.
    pub fn attack_bonus(self, level: u8) -> u32 {
        match self {
            Race::Cragborn => Self::scaling_bonus(level),
            _ => 0,
        }
    }

    /// Permanent defense bonus (the Wealdkin's wariness); see
    /// [`Race::attack_bonus`] for the shape.
    pub fn defense_bonus(self, level: u8) -> u32 {
        match self {
            Race::Wealdkin => Self::scaling_bonus(level),
            _ => 0,
        }
    }

    /// Extra daily forest fights (`racehuman.php`'s `newday` hook). Applies to
    /// the paid resurrection's docked day too, exactly like the upstream hook.
    pub fn daily_forest_bonus(self) -> u32 {
        match self {
            Race::Plainsborn => PLAINSBORN_FOREST_BONUS,
            _ => 0,
        }
    }

    /// Scale a forest creature's gold drop (`racedwarf.php`'s
    /// `creatureencounter` hook: `round(gold * 1.2)`). Fires where upstream's
    /// hook does: after `buff_foe`, before the thrillseeking x1.1.
    pub fn creature_gold(self, gold: u32) -> u32 {
        match self {
            Race::Deepfolk => (gold as f64 * 1.2).round() as u32,
            _ => gold,
        }
    }

    /// Percent chance a goldmine cave-in kills (`raceminedeath`): rolled as
    /// `e_rand(1,100) < chance`, so the Deepfolk almost always dig free.
    pub fn mine_death_percent(self) -> u32 {
        match self {
            Race::Deepfolk => DEEPFOLK_MINE_DEATH_PERCENT,
            _ => MINE_DEATH_PERCENT,
        }
    }
}

/// The two address styles. Upstream keys DK titles (and, come phase 3, the
/// romance partner and one bard outcome) off a binary `sex` field; our
/// adaptation is a flavor choice that picks the title column. The chooser
/// itself lands with phase 3's character-creation flow; until then everyone
/// defaults to the first style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressStyle {
    #[default]
    First,
    Second,
}

/// The three combat specialties, mirroring LoGD's `MP`/`DA`/`TS`. The in-fight
/// skills each unlocks live in [`super::combat`]; `None` is the undecided state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Specialty {
    /// Undecided — no specialty chosen yet.
    #[default]
    None,
    /// Mystical Powers: regeneration, earth fist, life siphon, lightning aura.
    Mystical,
    /// Dark Arts: skeleton minions, voodoo, the foe-weakening curse, wither.
    DarkArts,
    /// Thief skills: insult, poison blade, hidden attack, backstab.
    Thief,
}

impl Specialty {
    /// Short display label.
    pub fn name(self) -> &'static str {
        match self {
            Specialty::None => "None",
            Specialty::Mystical => "Mystical Powers",
            Specialty::DarkArts => "Dark Arts",
            Specialty::Thief => "Thief Skills",
        }
    }
}

impl Default for Character {
    fn default() -> Self {
        Character {
            name: String::new(),
            level: 1,
            experience: 0,
            hitpoints: HP_PER_LEVEL,
            weapon_tier: 0,
            armor_tier: 0,
            gold: START_GOLD,
            gold_in_bank: 0,
            turns: TURNS_PER_DAY,
            alive: true,
            seen_dragon: false,
            dragon_kills: 0,
            dragon_hp_bonus: 0,
            dragon_attack_bonus: 0,
            dragon_defense_bonus: 0,
            dragon_ff_bonus: 0,
            dragon_points_unspent: 0,
            gems: 0,
            charm: 0,
            // Fresh level-1 soulpoints: 50 + 5*1 (LoGD new-day formula).
            soulpoints: 55,
            favor: 0,
            // Upstream rolls a new day at a fresh account's first login, which
            // fills the daily torment pool; seed it directly.
            grave_fights: GRAVE_FIGHTS_PER_DAY,
            companions: Vec::new(),
            race: Race::None,
            title: String::new(),
            style: AddressStyle::First,
            specialty: Specialty::None,
            specialty_skill: 0,
            specialty_uses: 0,
            last_day: 0,
        }
    }
}

impl Character {
    /// A brand-new level-1 character for `name`, stamped with the current day.
    pub fn new(name: impl Into<String>, today: i64) -> Self {
        Character {
            name: name.into(),
            last_day: today,
            ..Character::default()
        }
    }

    /// Maximum hitpoints: `10 * level` plus any retained dragon-kill bonus.
    pub fn max_hitpoints(&self) -> u32 {
        HP_PER_LEVEL * self.level as u32 + self.dragon_hp_bonus
    }

    /// Attack stat fed to the combat roll: `level + weapon_tier` plus dragon
    /// boons and any level-scaled ancestry bonus.
    pub fn attack(&self) -> u32 {
        self.level as u32
            + self.weapon_tier as u32
            + self.dragon_attack_bonus
            + self.race.attack_bonus(self.level)
    }

    /// Defense stat fed to the combat roll: `level + armor_tier` plus dragon
    /// boons and any level-scaled ancestry bonus.
    pub fn defense(&self) -> u32 {
        self.level as u32
            + self.armor_tier as u32
            + self.dragon_defense_bonus
            + self.race.defense_bonus(self.level)
    }

    /// The player as a [`Combatant`] for [`super::combat::resolve_round`].
    pub fn combatant(&self) -> Combatant {
        Combatant {
            attack: self.attack(),
            defense: self.defense(),
        }
    }

    /// Maximum soulpoints, the dead-realm HP pool ceiling: `5*level + 50`.
    /// LoGD computes this fresh everywhere and never stores it.
    pub fn max_soulpoints(&self) -> u32 {
        5 * self.level as u32 + 50
    }

    /// The player as a combatant while dead (`graveyard.php`): gear and boons
    /// mean nothing beyond the grave — attack and defense are both
    /// `10 + round((level - 1) * 1.5)`.
    pub fn dead_combatant(&self) -> Combatant {
        let stat = 10 + ((self.level as f64 - 1.0) * 1.5).round() as u32;
        Combatant {
            attack: stat,
            defense: stat,
        }
    }

    /// Favor the mausoleum charges to restore the soul pool to max:
    /// `round(10 * missing / max)` (`lib/graveyard/case_restore.php`), 0..=10
    /// scaling with depletion.
    pub fn soul_restore_cost(&self) -> u32 {
        let max = self.max_soulpoints();
        let missing = max.saturating_sub(self.soulpoints);
        (10.0 * missing as f64 / max as f64).round() as u32
    }

    /// Pay favor to restore soulpoints to max. Returns the favor spent, or
    /// `None` if already whole or the favor can't cover the price.
    pub fn restore_soul(&mut self) -> Option<u32> {
        if self.soulpoints >= self.max_soulpoints() {
            return None;
        }
        let cost = self.soul_restore_cost();
        if self.favor < cost {
            return None;
        }
        self.favor -= cost;
        self.soulpoints = self.max_soulpoints();
        Some(cost)
    }

    /// The paid resurrection (`case_resurrection.php` + `newday.php` with
    /// `resurrection=true`): [`RESURRECTION_FAVOR_COST`] favor buys an
    /// immediate extra new day — revive, heal to full, and take
    /// `base + ff + `[`RESURRECTION_TURNS`] turns for what's left of today.
    /// Like upstream's flagged new day it settles bank interest and refreshes
    /// specialty uses, but does **not** refill soulpoints or grave fights, and
    /// leaves `last_day` alone so the real next dawn still rolls a full day.
    /// Returns false (spending nothing) if alive or short on favor.
    pub fn resurrect(&mut self, interest_percent: u32) -> bool {
        if self.alive || self.favor < RESURRECTION_FAVOR_COST {
            return false;
        }
        self.favor -= RESURRECTION_FAVOR_COST;
        self.apply_new_day_interest(interest_percent);
        // The race's `newday` hook fires on the resurrection day too
        // (`newday.php` runs `modulehook("newday")` regardless of the flag),
        // so a Plainsborn's bonus fights soften the -6 dock.
        let turns = TURNS_PER_DAY as i32
            + self.dragon_ff_bonus as i32
            + self.race.daily_forest_bonus() as i32
            + RESURRECTION_TURNS;
        self.turns = turns.max(0) as u32;
        self.refresh_specialty_uses();
        self.alive = true;
        self.seen_dragon = false;
        self.hitpoints = self.max_hitpoints();
        true
    }

    /// Re-roll the dragon-kill title off the ladder for the current kill
    /// count (`dragon.php` re-titles on every kill; the load path uses this to
    /// stamp never-titled saves). The address style picks the column.
    pub fn reroll_title(&mut self, rng: &mut impl Rng) {
        let (first, second) = data::dk_title_pair(self.dragon_kills, rng);
        self.title = match self.style {
            AddressStyle::First => first,
            AddressStyle::Second => second,
        }
        .to_string();
    }

    /// Experience required to advance to the next level (with DK scaling).
    pub fn exp_for_next_level(&self) -> u64 {
        data::exp_to_advance(self.level, self.dragon_kills)
    }

    /// Whether the player has banked enough experience to challenge their
    /// master. (Beating the master is what actually advances the level.)
    pub fn can_challenge_master(&self) -> bool {
        self.level < data::MAX_LEVEL && self.experience >= self.exp_for_next_level()
    }

    /// Whether the Seek-the-Dragon option is available: level 15, not yet
    /// sought this run.
    pub fn can_seek_dragon(&self) -> bool {
        self.level >= data::MAX_LEVEL && !self.seen_dragon
    }

    /// Advance one level after beating the master: +1 level (so +10 max HP, +1
    /// attack, +1 defense via derivation), +5 soulpoints, then heal to full.
    pub fn advance_level(&mut self) {
        if self.level < data::MAX_LEVEL {
            self.level += 1;
            self.soulpoints = self.soulpoints.saturating_add(SOULPOINTS_PER_MASTER);
            self.hitpoints = self.max_hitpoints();
        }
    }

    /// The master fought to advance from the current level, as a combatant.
    pub fn current_master(&self) -> Option<(data::Master, Combatant, u32)> {
        if self.level >= data::MAX_LEVEL {
            return None;
        }
        let master = data::MASTERS[(self.level - 1) as usize];
        let (atk, def, hp) = data::master_stats(self.level);
        Some((
            master,
            Combatant {
                attack: atk,
                defense: def,
            },
            hp,
        ))
    }

    // --- endgame investment scaling (LoGD `dragon.php` / `train.php`) --------
    //
    // The dragon and your master grow with how much *permanent* power you've
    // banked, so buying boons makes those fights keep pace instead of trivially
    // out-gearing a fixed foe. Without this, enough Gypsy purchases make you
    // undefeatable; this is LoGD's fix, transcribed.

    /// Banked permanent power the endgame scales against: attack + defense boons,
    /// plus earned HP over the level-15 base (each 5 HP = 1 point).
    fn investment_points(&self) -> u32 {
        self.dragon_attack_bonus + self.dragon_defense_bonus + self.dragon_hp_bonus / 5
    }

    /// Randomly split `points` into (attack, defense, hp) flux: +1 attack or
    /// defense per point, +5 HP per leftover point, with attack and defense each
    /// capped at `cap`. Mirrors the buff roll shared by the dragon and masters.
    fn partition_flux(points: u32, cap: u32, rng: &mut impl Rng) -> (u32, u32, u32) {
        let cap = cap.min(points);
        let atk = rng.gen_range(0..=cap);
        let def = rng.gen_range(0..=cap.min(points - atk));
        let hp = (points - atk - def) * 5;
        (atk, def, hp)
    }

    /// The Green Dragon's effective stats for this fight (`dragon.php`): base
    /// 45/25/300 plus a random flux over `round(investment * 0.75)` points.
    pub fn scaled_dragon(&self, rng: &mut impl Rng) -> (u32, u32, u32) {
        let points = (self.investment_points() as f64 * 0.75).round() as u32;
        let (a, d, h) = Self::partition_flux(points, points, rng);
        (
            data::DRAGON_ATTACK + a,
            data::DRAGON_DEFENSE + d,
            data::DRAGON_HP + h,
        )
    }

    /// The current master scaled by investment (`train.php`): base stats plus a
    /// flux over `round(investment * 0.33)` points, attack/defense each capped at
    /// a quarter of that. Returns `None` past the max level (no master).
    pub fn scaled_master(&self, rng: &mut impl Rng) -> Option<(data::Master, Combatant, u32)> {
        let (master, base, hp) = self.current_master()?;
        let points = (self.investment_points() as f64 * 0.33).round() as u32;
        let cap = (points as f64 * 0.25).round() as u32;
        let (a, d, h) = Self::partition_flux(points, cap, rng);
        Some((
            master,
            Combatant {
                attack: base.attack + a,
                defense: base.defense + d,
            },
            hp + h,
        ))
    }

    /// Cost in gold to upgrade to `target_tier`, crediting a 75% trade-in on the
    /// currently equipped item of `current_tier`. Returns `None` if the target
    /// is not a strict upgrade or is out of range.
    fn upgrade_cost(current_tier: u8, target_tier: u8) -> Option<u64> {
        if target_tier == 0 || target_tier as usize > data::COST_LADDER.len() {
            return None;
        }
        if target_tier <= current_tier {
            return None;
        }
        let cost = data::COST_LADDER[(target_tier - 1) as usize] as f64;
        let trade_in = if current_tier == 0 {
            0.0
        } else {
            data::COST_LADDER[(current_tier - 1) as usize] as f64 * data::TRADE_IN_FRACTION as f64
        };
        Some((cost - trade_in).max(0.0).round() as u64)
    }

    /// Cost to upgrade the weapon to `tier`, or `None` if not a valid upgrade.
    pub fn weapon_upgrade_cost(&self, tier: u8) -> Option<u64> {
        Self::upgrade_cost(self.weapon_tier, tier)
    }

    /// Cost to upgrade the armor to `tier`, or `None` if not a valid upgrade.
    pub fn armor_upgrade_cost(&self, tier: u8) -> Option<u64> {
        Self::upgrade_cost(self.armor_tier, tier)
    }

    /// Attempt to buy weapon `tier`, spending on-hand gold. Returns true on
    /// success.
    pub fn buy_weapon(&mut self, tier: u8) -> bool {
        match self.weapon_upgrade_cost(tier) {
            Some(cost) if self.gold >= cost => {
                self.gold -= cost;
                self.weapon_tier = tier;
                true
            }
            _ => false,
        }
    }

    /// Attempt to buy armor `tier`, spending on-hand gold. Returns true on
    /// success.
    pub fn buy_armor(&mut self, tier: u8) -> bool {
        match self.armor_upgrade_cost(tier) {
            Some(cost) if self.gold >= cost => {
                self.gold -= cost;
                self.armor_tier = tier;
                true
            }
            _ => false,
        }
    }

    /// Gold cost to fully heal: `round(ln(level) * (damage_taken + 10))`. Free
    /// at level 1 (`ln(1) == 0`).
    pub fn full_heal_cost(&self) -> u64 {
        let missing = self.max_hitpoints().saturating_sub(self.hitpoints);
        if missing == 0 {
            return 0;
        }
        ((self.level as f64).ln() * (missing as f64 + 10.0))
            .round()
            .max(0.0) as u64
    }

    /// Price of a partial heal of `pct` percent of the damage taken:
    /// `round(cost * pct / 100)` off the rounded full-heal price (`healer.php`
    /// sells 100% down to 10% in steps of ten).
    pub fn heal_cost(&self, pct: u32) -> u64 {
        (self.full_heal_cost() as f64 * pct as f64 / 100.0).round() as u64
    }

    /// Pay for a heal of `pct` percent of the missing HP (`round(missing *
    /// pct / 100)`). Returns the HP restored, or `None` if unaffordable.
    pub fn buy_heal(&mut self, pct: u32) -> Option<u32> {
        let cost = self.heal_cost(pct);
        if self.gold < cost {
            return None;
        }
        self.gold -= cost;
        let missing = self.max_hitpoints().saturating_sub(self.hitpoints);
        let healed = (missing as f64 * pct as f64 / 100.0).round() as u32;
        self.hitpoints += healed;
        Some(healed)
    }

    /// Pay to fully heal if affordable. Returns true on success (including the
    /// free level-1 case).
    pub fn buy_full_heal(&mut self) -> bool {
        self.buy_heal(100).is_some()
    }

    /// The healer's free forced normalize: HP above max (a lapsed overheal) is
    /// clipped back down, no charge (`healer.php`'s over-max branch). Returns
    /// true if anything was clipped.
    pub fn normalize_overheal(&mut self) -> bool {
        if self.hitpoints > self.max_hitpoints() {
            self.hitpoints = self.max_hitpoints();
            return true;
        }
        false
    }

    /// Deposit on-hand gold into the bank (clamped to what's on hand). Paying
    /// down debt is the same move: a deposit into a negative balance.
    pub fn deposit(&mut self, amount: u64) {
        let amount = amount.min(self.gold);
        self.gold -= amount;
        self.gold_in_bank = self.gold_in_bank.saturating_add(amount as i64);
    }

    /// Withdraw banked gold to hand (clamped to the positive balance). Going
    /// below zero is a loan — see [`Character::borrow`].
    pub fn withdraw(&mut self, amount: u64) {
        let amount = (amount as i64).min(self.gold_in_bank).max(0);
        self.gold_in_bank -= amount;
        self.gold = self.gold.saturating_add(amount as u64);
    }

    /// The bank's lending ceiling: debt may reach `-level * BORROW_PER_LEVEL`
    /// (`bank.php` `borrowperlevel`).
    pub fn max_borrow(&self) -> i64 {
        self.level as i64 * BORROW_PER_LEVEL
    }

    /// Gold still borrowable before the balance hits the lending floor.
    pub fn borrow_available(&self) -> u64 {
        (self.gold_in_bank + self.max_borrow()).max(0) as u64
    }

    /// Take a loan of `amount` gold (clamped to [`Character::borrow_available`]):
    /// the balance goes negative and the gold lands on hand. Returns the amount
    /// actually borrowed.
    pub fn borrow(&mut self, amount: u64) -> u64 {
        let amount = amount.min(self.borrow_available());
        self.gold_in_bank -= amount as i64;
        self.gold = self.gold.saturating_add(amount);
        amount
    }

    /// Perturb a creature's stat block by the player's banked investment —
    /// LoGD's `buffbadguy` (`lib/forestoutcomes.php`), applied to every forest
    /// creature at spawn:
    /// - scaling pool `dk = round(investment * (0.25 + 0.05 * kills / 100))`
    ///   (creatures harden as dragon kills accumulate),
    /// - experience flux of `±round(exp / 10)`,
    /// - the pool split randomly into +attack / +defense / +5 HP per point,
    /// - gold/exp compensated by `1 + .03*(atk+def) + .001*hp`.
    pub fn buff_foe(&self, base: data::CreatureTier, rng: &mut impl Rng) -> data::CreatureTier {
        let add = (self.dragon_kills as f64 / 100.0) * 0.05;
        let dk = (self.investment_points() as f64 * (0.25 + add)).round() as u32;

        let mut foe = base;
        let expflux = (foe.exp as f64 / 10.0).round() as i32;
        let exp = foe.exp as i64 + rng.gen_range(-expflux..=expflux) as i64;
        foe.exp = exp.max(0) as u32;

        let atkflux = rng.gen_range(0..=dk);
        let defflux = rng.gen_range(0..=(dk - atkflux));
        let hpflux = (dk - atkflux - defflux) * 5;
        foe.attack += atkflux;
        foe.defense += defflux;
        foe.hp += hpflux;

        let bonus = 1.0 + 0.03 * (atkflux + defflux) as f64 + 0.001 * hpflux as f64;
        foe.gold = (foe.gold as f64 * bonus).round() as u32;
        foe.exp = (foe.exp as f64 * bonus).round() as u32;
        foe
    }

    /// Settle a won forest fight — LoGD's `forestvictory`
    /// (`lib/forestoutcomes.php`), covering single kills and multi-fights:
    /// - each foe's gold is rolled `e_rand(0, gold)`, then the total re-rolled
    ///   `e_rand(avg, avg * round((n+1) * 1.2^(n-1)))` (a single kill pays
    ///   `e_rand(g, 2g)` of the first roll; packs multiply),
    /// - experience is the per-foe average plus a level-difference bonus of
    ///   `round(exp * (1 + .25*(foe_level - level)) - exp)` per foe (plus
    ///   `kills * level` on multi-fights), floored at `-exp+1`, a positive
    ///   bonus scaled by `1.05^(n-1)`,
    /// - under level 15, a 1-in-[`FOREST_GEM_CHANCE`] gem,
    /// - a flawless fight refunds the turn if `level <= max_foe_level +
    ///   0.5*(n-1)`,
    /// - a player at 0 HP on a victory is saved at 1 (the mushroom clamp).
    pub fn forest_victory(
        &mut self,
        foes: &[SlainFoe],
        flawless: bool,
        rng: &mut impl Rng,
    ) -> ForestVictory {
        let n = foes.len().max(1) as u32;
        let mut gold_sum: u64 = 0;
        let mut exp_sum: u64 = 0;
        let mut exp_bonus: i64 = 0;
        let mut max_foe_level: u8 = 0;
        for foe in foes {
            gold_sum += rng.gen_range(0..=foe.gold) as u64;
            exp_sum += foe.exp as u64;
            let scaled =
                foe.exp as f64 * (1.0 + 0.25 * (foe.level as f64 - self.level as f64));
            exp_bonus += (scaled - foe.exp as f64).round() as i64;
            max_foe_level = max_foe_level.max(foe.level);
        }
        if n > 1 {
            exp_bonus += (self.dragon_kills as u64 * self.level as u64) as i64;
        }

        let exp = (exp_sum as f64 / n as f64).round() as i64;
        let avg_gold = (gold_sum as f64 / n as f64).round() as u64;
        let gold_hi = avg_gold * ((n as f64 + 1.0) * 1.2f64.powi(n as i32 - 1)).round() as u64;
        let gold = rng.gen_range(avg_gold..=gold_hi.max(avg_gold));

        let mut exp_bonus = (exp_bonus as f64 / n as f64).round() as i64;
        if exp + exp_bonus < 0 {
            exp_bonus = -exp + 1;
        }
        if exp_bonus > 0 {
            exp_bonus = (exp_bonus as f64 * 1.05f64.powi(n as i32 - 1)).round() as i64;
        }
        let exp_won = (exp + exp_bonus).max(0) as u64;

        self.gold = self.gold.saturating_add(gold);
        self.experience = self.experience.saturating_add(exp_won);

        let gem = self.level < data::MAX_LEVEL && rng.gen_range(1..=FOREST_GEM_CHANCE) == 1;
        if gem {
            self.gems += 1;
        }

        // Flawless fights refund the turn, but only when the foes were a real
        // match; packs count for half a level each past the first.
        let effective_level = max_foe_level as f64 + 0.5 * (n as f64 - 1.0);
        let turn_refunded = flawless && self.level as f64 <= effective_level;
        if turn_refunded {
            self.turns += 1;
        }

        // The mushroom save: a victory never leaves you dead on the ground.
        if self.hitpoints == 0 {
            self.hitpoints = 1;
        }

        ForestVictory {
            gold,
            exp: exp_won,
            gem,
            flawless,
            turn_refunded,
        }
    }

    /// Resolve a forest/PvE death: all on-hand gold lost, 10% experience lost,
    /// sent to the graveyard (revived on the next new day).
    pub fn die(&mut self) {
        self.gold = 0;
        self.experience = (self.experience as f64 * EXP_KEEP_ON_DEATH).round() as u64;
        self.alive = false;
        self.hitpoints = 0;
        // Your companions don't follow you past the grave.
        self.companions.clear();
    }

    /// Extra daily forest fights bought with `ff` dragon points (LoGD `dkff`).
    pub fn dk_forest_bonus(&self) -> u32 {
        self.dragon_ff_bonus
    }

    /// Spend one unspent dragon point on `kind`. Returns false (spending
    /// nothing) if none are unspent. An `ff` point also grows *today's* pool by
    /// one, since LoGD spends points before the new day's turns are assembled;
    /// an `hp` point raises current HP alongside the max for the same reason.
    pub fn spend_dragon_point(&mut self, kind: DragonPointKind) -> bool {
        if self.dragon_points_unspent == 0 {
            return false;
        }
        self.dragon_points_unspent -= 1;
        match kind {
            DragonPointKind::Hp => {
                self.dragon_hp_bonus += HP_PER_DRAGON_POINT;
                self.hitpoints += HP_PER_DRAGON_POINT;
            }
            DragonPointKind::ForestFights => {
                self.dragon_ff_bonus += 1;
                self.turns += 1;
            }
            DragonPointKind::Attack => self.dragon_attack_bonus += 1,
            DragonPointKind::Defense => self.dragon_defense_bonus += 1,
        }
        true
    }

    /// Reward a Green Dragon kill (`dragon.php`), then reset to a fresh,
    /// fully-healed run. `flawless` is true if no damage was taken in the fight.
    ///
    /// Faithful to upstream: the run's gold is wiped and restarted at
    /// [`START_GOLD`] plus [`START_GOLD`] per kill (capped at
    /// [`DRAGON_RUN_GOLD_CAP`]); gems accrue `max(0, kills-7)` (capped); charm
    /// `+5`; companions are wiped; and the kill banks **one dragon point** to
    /// spend at the gate ([`Character::spend_dragon_point`]). A flawless kill
    /// adds [`FLAWLESS_GOLD_BONUS`] gold (over the cap) and a gem. The
    /// specialty skill/uses restart at zero.
    pub fn slay_dragon(&mut self, flawless: bool) {
        self.dragon_kills = self.dragon_kills.saturating_add(1);
        self.dragon_points_unspent = self.dragon_points_unspent.saturating_add(1);
        self.charm = self.charm.saturating_add(CHARM_PER_DRAGON_KILL);
        let restart_gems = self.dragon_kills.saturating_sub(7).min(MAX_RESTART_GEMS);
        self.gems = self.gems.saturating_add(restart_gems as u64);
        // The reset wipes on-hand gold: you restart with 50 + 50/kill, capped.
        self.gold =
            (START_GOLD + START_GOLD * self.dragon_kills as u64).min(DRAGON_RUN_GOLD_CAP);
        if flawless {
            // The flawless bonus lands on top of the cap.
            self.gold = self.gold.saturating_add(FLAWLESS_GOLD_BONUS);
            self.gems = self.gems.saturating_add(1);
        }
        // Reset the run.
        self.level = 1;
        self.experience = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.seen_dragon = false;
        self.alive = true;
        // The specialty path is kept, but its skill/uses restart (LoGD's
        // per-module dragonkill hook).
        self.specialty_skill = 0;
        self.specialty_uses = 0;
        self.companions.clear();
        self.hitpoints = self.max_hitpoints();
    }

    /// Run the daily reset if `today` is past the stored day: pay bank interest,
    /// refill forest turns and grave fights, fully heal, revive, and refresh
    /// soulpoints. `interest_percent` is the day's rolled rate and `spirits` is
    /// the day's `e_rand(-1,1)+e_rand(-1,1)` (-2..+2) turn jitter — both
    /// supplied by the caller's RNG. Returns true if a reset happened.
    ///
    /// A dawn revives the dead at no extra cost: upstream's `-6` turn dock and
    /// skipped soulpoint/grave-fight refills apply only to the *paid*
    /// resurrection ([`Character::resurrect`]), never this passive path.
    pub fn roll_new_day(&mut self, today: i64, interest_percent: u32, spirits: i32) -> bool {
        if today <= self.last_day {
            return false;
        }
        self.last_day = today;
        // Interest is settled before turns refill, so it can read how many of
        // yesterday's turns went unused (LoGD's "work for it" gate).
        self.apply_new_day_interest(interest_percent);
        let turns = TURNS_PER_DAY as i32
            + self.dragon_ff_bonus as i32
            + self.race.daily_forest_bonus() as i32
            + spirits;
        self.turns = turns.max(0) as u32;
        self.refresh_specialty_uses();
        self.alive = true;
        // The dragon may be sought once per day (`newday.php` clears
        // `seendragon` daily): a fled attempt doesn't lock out the run.
        self.seen_dragon = false;
        self.soulpoints = self.max_soulpoints();
        self.grave_fights = GRAVE_FIGHTS_PER_DAY;
        self.hitpoints = self.max_hitpoints();
        true
    }

    /// Refill the day's specialty uses: `floor(skill/3)`, plus 1 for having
    /// chosen a specialty at all (LoGD's `specialtybonus`). No-op while undecided.
    pub fn refresh_specialty_uses(&mut self) {
        if self.specialty == Specialty::None {
            self.specialty_uses = 0;
            return;
        }
        self.specialty_uses = self.specialty_skill / 3 + 1;
    }

    /// Pick a specialty (LoGD chooses on the first new day; here it's a one-time
    /// choice). Seeds the first day's uses immediately.
    pub fn choose_specialty(&mut self, specialty: Specialty) {
        self.specialty = specialty;
        self.refresh_specialty_uses();
    }

    /// Advance the chosen specialty by one skill point. Every third point also
    /// grants an immediate use (mirrors `incrementspecialty`). Returns the new
    /// skill level, or `None` if the player has no specialty to advance.
    pub fn increment_specialty(&mut self) -> Option<u32> {
        if self.specialty == Specialty::None {
            return None;
        }
        self.specialty_skill += 1;
        if self.specialty_skill.is_multiple_of(3) {
            self.specialty_uses += 1;
        }
        Some(self.specialty_skill)
    }

    /// Spend `cost` specialty uses to fire an in-fight skill. Returns false (and
    /// spends nothing) if the pool can't cover it.
    pub fn spend_specialty_uses(&mut self, cost: u32) -> bool {
        if self.specialty_uses < cost {
            return false;
        }
        self.specialty_uses -= cost;
        true
    }

    /// Pay the day's bank interest, gated exactly like LoGD: a *positive*
    /// balance earns nothing if more than [`FIGHTS_FOR_INTEREST`] of
    /// yesterday's turns went unused, or if it is at/above
    /// [`MAX_GOLD_FOR_INTEREST`]. **Debt always compounds** — the "work for
    /// it" gate only skips positive balances (`newday.php`). Must be called
    /// before turns are refilled so `self.turns` still holds yesterday's
    /// leftover.
    fn apply_new_day_interest(&mut self, interest_percent: u32) {
        if self.turns > FIGHTS_FOR_INTEREST && self.gold_in_bank >= 0 {
            return;
        }
        if self.gold_in_bank >= MAX_GOLD_FOR_INTEREST {
            return;
        }
        self.apply_bank_interest(interest_percent);
    }

    /// Apply a daily bank interest multiplier (percent, e.g. 7 for 7%) to the
    /// signed balance — growth when positive, compounding debt when negative.
    pub fn apply_bank_interest(&mut self, percent: u32) {
        let factor = 1.0 + percent as f64 / 100.0;
        self.gold_in_bank = (self.gold_in_bank as f64 * factor).round() as i64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_character_matches_seed_defaults() {
        let c = Character::new("hero", 100);
        assert_eq!(c.level, 1);
        assert_eq!(c.experience, 0);
        assert_eq!(c.hitpoints, 10);
        assert_eq!(c.max_hitpoints(), 10);
        assert_eq!(c.attack(), 1); // level 1 + fists 0
        assert_eq!(c.defense(), 1);
        assert_eq!(c.gold, 50);
        assert_eq!(c.turns, 10);
        assert!(c.alive);
    }

    #[test]
    fn stats_track_level_and_gear() {
        let mut c = Character::new("hero", 0);
        c.level = 8;
        c.weapon_tier = 10;
        c.armor_tier = 7;
        assert_eq!(c.max_hitpoints(), 80);
        assert_eq!(c.attack(), 18); // 8 + 10
        assert_eq!(c.defense(), 15); // 8 + 7
    }

    #[test]
    fn specialty_skill_grants_a_use_every_three() {
        let mut c = Character::new("hero", 0);
        c.choose_specialty(Specialty::Thief);
        // Choosing seeds the +1 bonus use.
        assert_eq!(c.specialty_uses, 1);
        // Two increments: still floor(2/3)=0 from skill, the seeded use remains.
        c.increment_specialty();
        c.increment_specialty();
        assert_eq!(c.specialty_skill, 2);
        assert_eq!(c.specialty_uses, 1);
        // The third increment crosses a multiple of 3 and grants a use.
        c.increment_specialty();
        assert_eq!(c.specialty_skill, 3);
        assert_eq!(c.specialty_uses, 2);
    }

    #[test]
    fn specialty_uses_refresh_on_new_day() {
        let mut c = Character::new("hero", 0);
        c.choose_specialty(Specialty::Mystical);
        c.specialty_skill = 9; // floor(9/3) = 3, plus the +1 chosen bonus
        c.specialty_uses = 0; // spent down during the day
        c.roll_new_day(1, 0, 0);
        assert_eq!(c.specialty_uses, 4);
    }

    #[test]
    fn increment_without_specialty_is_a_noop() {
        let mut c = Character::new("hero", 0);
        assert_eq!(c.increment_specialty(), None);
        assert_eq!(c.specialty_skill, 0);
        assert_eq!(c.specialty_uses, 0);
    }

    #[test]
    fn advancing_levels_adds_hp_and_full_heals() {
        let mut c = Character::new("hero", 0);
        c.hitpoints = 3;
        c.advance_level();
        assert_eq!(c.level, 2);
        assert_eq!(c.max_hitpoints(), 20);
        assert_eq!(c.hitpoints, 20);
    }

    #[test]
    fn weapon_trade_in_is_credited() {
        let mut c = Character::new("hero", 0);
        // First weapon, no trade-in: tier 1 costs 48.
        assert_eq!(c.weapon_upgrade_cost(1), Some(48));
        assert!(c.buy_weapon(1));
        assert_eq!(c.weapon_tier, 1);
        assert_eq!(c.gold, 2); // 50 - 48
        // Can't "upgrade" to a lower/equal tier.
        assert_eq!(c.weapon_upgrade_cost(1), None);
        // Tier 2 costs 225 minus 75% of tier-1's 48 = 225 - 36 = 189.
        assert_eq!(c.weapon_upgrade_cost(2), Some(189));
    }

    #[test]
    fn healing_is_free_at_level_one_and_scales_after() {
        let mut c = Character::new("hero", 0);
        c.hitpoints = 1;
        assert_eq!(c.full_heal_cost(), 0); // ln(1) = 0
        assert!(c.buy_full_heal());
        assert_eq!(c.hitpoints, 10);

        c.level = 5;
        c.hitpoints = c.max_hitpoints() - 20; // 20 missing
        // round(ln(5) * (20 + 10)) = round(1.609 * 30) = 48
        assert_eq!(c.full_heal_cost(), 48);
    }

    #[test]
    fn death_zeroes_gold_and_clips_exp() {
        let mut c = Character::new("hero", 0);
        c.gold = 500;
        c.experience = 1000;
        c.die();
        assert_eq!(c.gold, 0);
        assert_eq!(c.experience, 900);
        assert!(!c.alive);
        assert_eq!(c.hitpoints, 0);
    }

    #[test]
    fn banked_gold_survives_death() {
        let mut c = Character::new("hero", 0);
        c.gold = 500;
        c.deposit(400);
        assert_eq!(c.gold, 100);
        assert_eq!(c.gold_in_bank, 400);
        c.die();
        assert_eq!(c.gold, 0);
        assert_eq!(c.gold_in_bank, 400);
    }

    #[test]
    fn new_day_refills_and_revives() {
        let mut c = Character::new("hero", 10);
        c.turns = 0;
        c.level = 3;
        c.grave_fights = 0;
        c.seen_dragon = true;
        c.die();
        // The free path: wait for the dawn and rise with a *full* day — the
        // -6 dock belongs to the paid resurrection only (newday.php applies
        // resurrectionturns only when resurrection=true).
        assert!(c.roll_new_day(11, 0, 0));
        assert_eq!(c.turns, TURNS_PER_DAY);
        assert!(c.alive);
        assert_eq!(c.hitpoints, c.max_hitpoints());
        // Soulpoints refill to 50 + 5*level; grave fights to the daily pool;
        // the dragon may be sought again.
        assert_eq!(c.soulpoints, 50 + 5 * 3);
        assert_eq!(c.grave_fights, GRAVE_FIGHTS_PER_DAY);
        assert!(!c.seen_dragon);
        // Same day again: no reset.
        c.turns = 3;
        assert!(!c.roll_new_day(11, 0, 0));
        assert_eq!(c.turns, 3);
    }

    #[test]
    fn dead_stats_ignore_gear_and_track_level() {
        let mut c = Character::new("ghost", 0);
        c.weapon_tier = 15;
        c.armor_tier = 15;
        c.dragon_attack_bonus = 9;
        // Level 1: 10 + round(0) on both sides, gear irrelevant.
        assert_eq!(c.dead_combatant().attack, 10);
        assert_eq!(c.dead_combatant().defense, 10);
        assert_eq!(c.max_soulpoints(), 55);
        // Level 4: 10 + round(4.5) = 15 (PHP half-away rounding).
        c.level = 4;
        assert_eq!(c.dead_combatant().attack, 15);
        assert_eq!(c.max_soulpoints(), 70);
    }

    #[test]
    fn soul_restoration_prices_by_depletion() {
        let mut c = Character::new("ghost", 0); // level 1, max soul 55
        c.soulpoints = 0;
        assert_eq!(c.soul_restore_cost(), 10); // fully drained: the cap
        c.soulpoints = 27; // missing 28: round(280/55) = 5
        assert_eq!(c.soul_restore_cost(), 5);
        c.favor = 4;
        assert_eq!(c.restore_soul(), None); // can't afford
        c.favor = 5;
        assert_eq!(c.restore_soul(), Some(5));
        assert_eq!(c.soulpoints, 55);
        assert_eq!(c.favor, 0);
        assert_eq!(c.restore_soul(), None); // already whole
    }

    #[test]
    fn paid_resurrection_is_a_docked_extra_day() {
        let mut c = Character::new("hero", 10);
        c.level = 3;
        c.favor = 120;
        c.die();
        c.soulpoints = 12;
        c.grave_fights = 2;
        // Alive or broke: no sale.
        let mut alive = Character::new("alive", 10);
        alive.favor = 500;
        assert!(!alive.resurrect(0));
        let mut broke = Character::new("broke", 10);
        broke.die();
        broke.favor = 99;
        assert!(!broke.resurrect(0));

        assert!(c.resurrect(0));
        assert!(c.alive);
        assert_eq!(c.favor, 20);
        // Turns for the rest of today: base 10 - 6 = 4 (plus any ff points).
        assert_eq!(c.turns, (TURNS_PER_DAY as i32 + RESURRECTION_TURNS) as u32);
        assert_eq!(c.hitpoints, c.max_hitpoints());
        // Soulpoints and grave fights are NOT refreshed by the paid path.
        assert_eq!(c.soulpoints, 12);
        assert_eq!(c.grave_fights, 2);
        // last_day untouched: the real next dawn still rolls a full day.
        assert_eq!(c.last_day, 10);
    }

    #[test]
    fn new_day_spirits_jitter_turns() {
        // A live player (no resurrection penalty): base 10 + spirits.
        let mut high = Character::new("high", 10);
        high.roll_new_day(11, 0, 2); // very high spirits
        assert_eq!(high.turns, 12);
        let mut low = Character::new("low", 10);
        low.roll_new_day(11, 0, -2); // very low spirits
        assert_eq!(low.turns, 8);
        // ff dragon points feed the daily pool.
        let mut invested = Character::new("ff", 10);
        invested.dragon_ff_bonus = 4;
        invested.roll_new_day(11, 0, 0);
        assert_eq!(invested.turns, 14);
    }

    #[test]
    fn bank_interest_is_gated_on_using_your_turns() {
        // Worked for it: 0 turns left at day's end → interest is paid.
        let mut worker = Character::new("worker", 10);
        worker.gold_in_bank = 1000;
        worker.turns = 0;
        worker.roll_new_day(11, 10, 0); // 10% rolled
        assert_eq!(worker.gold_in_bank, 1100);

        // Slacked off: left more than the threshold unused → no interest.
        let mut slacker = Character::new("slacker", 10);
        slacker.gold_in_bank = 1000;
        slacker.turns = FIGHTS_FOR_INTEREST + 1;
        slacker.roll_new_day(11, 10, 0);
        assert_eq!(slacker.gold_in_bank, 1000);

        // Over the ceiling → no interest no matter how hard you worked.
        let mut rich = Character::new("rich", 10);
        rich.gold_in_bank = MAX_GOLD_FOR_INTEREST;
        rich.turns = 0;
        rich.roll_new_day(11, 10, 0);
        assert_eq!(rich.gold_in_bank, MAX_GOLD_FOR_INTEREST);

        // Debt compounds even when turns went unused (no "work for it" gate
        // on negative balances).
        let mut debtor = Character::new("debtor", 10);
        debtor.gold_in_bank = -100;
        debtor.turns = FIGHTS_FOR_INTEREST + 5;
        debtor.roll_new_day(11, 10, 0);
        assert_eq!(debtor.gold_in_bank, -110);
    }

    #[test]
    fn borrowing_drives_the_balance_negative() {
        let mut c = Character::new("hero", 0);
        c.level = 5; // lending ceiling 5 * 20 = 100
        assert_eq!(c.max_borrow(), 100);
        assert_eq!(c.borrow_available(), 100);
        assert_eq!(c.borrow(60), 60);
        assert_eq!(c.gold_in_bank, -60);
        assert_eq!(c.gold, 50 + 60);
        // Only 40 left before the floor; requests clamp.
        assert_eq!(c.borrow_available(), 40);
        assert_eq!(c.borrow(500), 40);
        assert_eq!(c.gold_in_bank, -100);
        // A positive balance raises the headroom.
        c.gold_in_bank = 30;
        assert_eq!(c.borrow_available(), 130);
        // Plain withdrawals never dip below zero.
        c.withdraw(500);
        assert_eq!(c.gold_in_bank, 0);
        // Deposits pay debt down.
        c.gold_in_bank = -50;
        c.gold = 80;
        c.deposit(80);
        assert_eq!(c.gold_in_bank, 30);
    }

    #[test]
    fn partial_heals_price_and_heal_by_percent() {
        let mut c = Character::new("hero", 0);
        c.level = 5;
        c.hitpoints = c.max_hitpoints() - 20; // 20 missing
        // Full price: round(ln(5) * 30) = 48; 50% = round(48*0.5) = 24.
        assert_eq!(c.heal_cost(100), 48);
        assert_eq!(c.heal_cost(50), 24);
        assert_eq!(c.heal_cost(10), 5);
        c.gold = 24;
        // 50% heals round(20 * 0.5) = 10 HP.
        assert_eq!(c.buy_heal(50), Some(10));
        assert_eq!(c.hitpoints, c.max_hitpoints() - 10);
        assert_eq!(c.gold, 0);
        // Can't afford the rest.
        assert_eq!(c.buy_heal(100), None);
    }

    #[test]
    fn overheal_normalizes_free() {
        let mut c = Character::new("hero", 0);
        c.hitpoints = c.max_hitpoints() + 7;
        assert!(c.normalize_overheal());
        assert_eq!(c.hitpoints, c.max_hitpoints());
        assert!(!c.normalize_overheal());
    }

    #[test]
    fn dragon_kill_banks_a_point_and_resets_run() {
        let mut c = Character::new("hero", 0);
        c.level = 15;
        c.weapon_tier = 15;
        c.armor_tier = 12;
        c.experience = 99999;
        c.gold = 4000; // wiped by the reset, not retained
        c.specialty = Specialty::Mystical;
        c.specialty_skill = 12;
        c.slay_dragon(false);

        assert_eq!(c.dragon_kills, 1);
        // One chooseable dragon point banked; no boons auto-applied.
        assert_eq!(c.dragon_points_unspent, 1);
        assert_eq!(c.dragon_attack_bonus, 0);
        assert_eq!(c.dragon_defense_bonus, 0);
        assert_eq!(c.dragon_hp_bonus, 0);
        assert_eq!(c.charm, CHARM_PER_DRAGON_KILL);
        // Run reset.
        assert_eq!(c.level, 1);
        assert_eq!(c.weapon_tier, 0);
        assert_eq!(c.armor_tier, 0);
        assert_eq!(c.experience, 0);
        // Restart gold: 50 + 50*1 = 100 (on-hand gold not retained).
        assert_eq!(c.gold, 100);
        // First kill is below the gem threshold (kills-7).
        assert_eq!(c.gems, 0);
        // Specialty path kept, skill/uses restart.
        assert_eq!(c.specialty, Specialty::Mystical);
        assert_eq!(c.specialty_skill, 0);
        assert!(!c.seen_dragon);
    }

    #[test]
    fn dragon_kill_gold_caps_then_flawless_adds_on_top() {
        let mut c = Character::new("hero", 0);
        c.level = 15;
        c.dragon_kills = 9; // 10th kill after increment
        c.gold = 100;
        c.slay_dragon(true);
        assert_eq!(c.dragon_kills, 10);
        // 50 + 50*10 = 550, capped to 300, then +150 flawless = 450.
        assert_eq!(c.gold, DRAGON_RUN_GOLD_CAP + FLAWLESS_GOLD_BONUS);
        // Gems: max(0, 10-7) = 3, plus 1 flawless = 4.
        assert_eq!(c.gems, 4);
    }

    #[test]
    fn dragon_points_spend_into_permanent_boons() {
        let mut c = Character::new("hero", 0);
        c.dragon_points_unspent = 4;
        assert!(c.spend_dragon_point(DragonPointKind::Hp));
        assert_eq!(c.dragon_hp_bonus, HP_PER_DRAGON_POINT);
        assert_eq!(c.hitpoints, HP_PER_LEVEL + HP_PER_DRAGON_POINT);
        assert!(c.spend_dragon_point(DragonPointKind::Attack));
        assert!(c.spend_dragon_point(DragonPointKind::Defense));
        assert_eq!(c.attack(), 2);
        assert_eq!(c.defense(), 2);
        let before = c.turns;
        assert!(c.spend_dragon_point(DragonPointKind::ForestFights));
        assert_eq!(c.dragon_ff_bonus, 1);
        assert_eq!(c.turns, before + 1); // today's pool grows immediately
        // Pool exhausted.
        assert_eq!(c.dragon_points_unspent, 0);
        assert!(!c.spend_dragon_point(DragonPointKind::Attack));
    }

    #[test]
    fn forest_victory_pays_rolls_and_refunds_flawless_turns() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut c = Character::new("hero", 0);
        c.level = 3;
        let turns_before = c.turns;
        let foe = SlainFoe {
            level: 3,
            gold: 148,
            exp: 34,
        };
        let mut rng = StdRng::seed_from_u64(7);
        let v = c.forest_victory(&[foe], true, &mut rng);
        // Single foe at your level: no level-diff bonus, exp = the foe's exp.
        assert_eq!(v.exp, 34);
        // Gold: e_rand(0,148) then e_rand(roll, 2*roll) — bounded by 2x base.
        assert!(v.gold <= 296);
        // Flawless at-level fight refunds the turn.
        assert!(v.turn_refunded);
        assert_eq!(c.turns, turns_before + 1);
        assert_eq!(c.experience, 34);

        // Over-leveled flawless fights refund nothing.
        let mut over = Character::new("over", 0);
        over.level = 10;
        let weak = SlainFoe {
            level: 3,
            gold: 10,
            exp: 34,
        };
        let v = over.forest_victory(&[weak], true, &mut rng);
        assert!(!v.turn_refunded);
        // Level-diff penalty: bonus round(34*(1+.25*(3-10)) - 34) = -60 drives
        // the total negative, so the -exp+1 floor pays exactly 1 exp.
        assert_eq!(v.exp, 1);
    }

    #[test]
    fn forest_victory_multi_fight_bonuses() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut c = Character::new("hero", 0);
        c.level = 5;
        c.dragon_kills = 12;
        let foe = SlainFoe {
            level: 5,
            gold: 198,
            exp: 55,
        };
        let mut rng = StdRng::seed_from_u64(3);
        let v = c.forest_victory(&[foe, foe, foe], false, &mut rng);
        // Per-foe exp average is 55; the multi bonus adds
        // round(dragonkills*level / n) = round(60/3) = 20, scaled by
        // 1.05^2 → round(20 * 1.1025) = 22. Total 77.
        assert_eq!(v.exp, 77);
        assert!(!v.turn_refunded);
    }

    #[test]
    fn mushroom_save_clamps_victory_at_one_hp() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut c = Character::new("hero", 0);
        c.hitpoints = 0;
        let foe = SlainFoe {
            level: 1,
            gold: 0,
            exp: 0,
        };
        c.forest_victory(&[foe], false, &mut StdRng::seed_from_u64(1));
        assert_eq!(c.hitpoints, 1);
    }

    #[test]
    fn buff_foe_scales_with_investment() {
        use rand::{SeedableRng, rngs::StdRng};
        let base = data::creature_tier(5);
        // No investment: the stat pool is 0, only the exp flux moves.
        let fresh = Character::new("fresh", 0);
        let foe = fresh.buff_foe(base, &mut StdRng::seed_from_u64(2));
        assert_eq!(foe.attack, base.attack);
        assert_eq!(foe.defense, base.defense);
        assert_eq!(foe.hp, base.hp);
        let expflux = (base.exp as f64 / 10.0).round() as u32;
        assert!(foe.exp >= base.exp - expflux && foe.exp <= base.exp + expflux);

        // Invested: dk = round(20 * (0.25 + 0.05*100/100)) = 6 points spread
        // over attack/defense/+5hp, with gold/exp compensation.
        let mut vet = Character::new("vet", 0);
        vet.dragon_kills = 100;
        vet.dragon_attack_bonus = 8;
        vet.dragon_defense_bonus = 7;
        vet.dragon_hp_bonus = 25; // 5 points
        let foe = vet.buff_foe(base, &mut StdRng::seed_from_u64(2));
        let spent =
            (foe.attack - base.attack) + (foe.defense - base.defense) + (foe.hp - base.hp) / 5;
        assert_eq!(spent, 6);
        assert!(foe.gold >= base.gold);
    }

    #[test]
    fn dragon_scaling_tracks_investment() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut c = Character::new("hero", 0);
        c.level = 15;
        // No boons → no scaling, the dragon is exactly its base (deterministic).
        let base = c.scaled_dragon(&mut StdRng::seed_from_u64(1));
        assert_eq!(base, c.scaled_dragon(&mut StdRng::seed_from_u64(99)));

        // Invest +4 attack, +2 defense, +30 HP (=6 HP-points). investment = 12,
        // scaling points = round(12 * 0.75) = 9.
        c.dragon_attack_bonus = 4;
        c.dragon_defense_bonus = 2;
        c.dragon_hp_bonus = 30;
        assert_eq!(c.investment_points(), 12);
        let (a, d, h) = c.scaled_dragon(&mut StdRng::seed_from_u64(3));
        // The flux always spends exactly the 9 points (as +1 atk/def or +5 HP).
        let stat_points = (a - base.0) + (d - base.1) + (h - base.2) / 5;
        assert_eq!(stat_points, 9);
        assert!(a >= base.0 && d >= base.1 && h >= base.2);
    }

    #[test]
    fn race_stat_bonuses_scale_with_level() {
        // The elf/troll formula: 1 + floor(level/5) — +1 at 1..=4, +2 at
        // 5..=9, +3 at 10..=14, +4 at 15.
        let mut c = Character::new("weald", 0);
        c.race = Race::Wealdkin;
        assert_eq!(c.defense(), 1 + 1); // level 1 + armor 0 + bonus 1
        assert_eq!(c.attack(), 1); // no attack bonus for the Wealdkin
        c.level = 5;
        assert_eq!(c.defense(), 5 + 2);
        c.level = 15;
        assert_eq!(c.defense(), 15 + 4);

        let mut t = Character::new("crag", 0);
        t.race = Race::Cragborn;
        t.level = 10;
        assert_eq!(t.attack(), 10 + 3);
        assert_eq!(t.defense(), 10);

        // The dead fight on level alone: no race bonus beyond the grave.
        let dead = t.dead_combatant();
        t.race = Race::None;
        assert_eq!(t.dead_combatant().attack, dead.attack);
        assert_eq!(t.dead_combatant().defense, dead.defense);
    }

    #[test]
    fn plainsborn_gain_bonus_fights_each_day() {
        let mut c = Character::new("plains", 10);
        c.race = Race::Plainsborn;
        c.roll_new_day(11, 0, 0);
        assert_eq!(c.turns, TURNS_PER_DAY + PLAINSBORN_FOREST_BONUS);

        // The race's newday hook fires on the paid resurrection too:
        // 10 + 2 - 6 = 6 turns.
        c.die();
        c.favor = 100;
        assert!(c.resurrect(0));
        assert_eq!(
            c.turns,
            (TURNS_PER_DAY as i32 + PLAINSBORN_FOREST_BONUS as i32 + RESURRECTION_TURNS) as u32
        );
    }

    #[test]
    fn deepfolk_scale_gold_and_shrug_off_cave_ins() {
        assert_eq!(Race::Deepfolk.creature_gold(100), 120);
        assert_eq!(Race::Deepfolk.creature_gold(97), 116); // round(116.4)
        assert_eq!(Race::Plainsborn.creature_gold(100), 100);
        assert_eq!(Race::Deepfolk.mine_death_percent(), 5);
        assert_eq!(Race::Wealdkin.mine_death_percent(), 90);
    }

    #[test]
    fn forest_hunt_shifts_creature_level() {
        assert_eq!(ForestHunt::Slumming.creature_level(5), 4);
        assert_eq!(ForestHunt::Hunt.creature_level(5), 5);
        assert_eq!(ForestHunt::Thrillseeking.creature_level(5), 6);
        assert_eq!(ForestHunt::Slumming.creature_level(1), 1); // clamps
        assert_eq!(ForestHunt::Thrillseeking.creature_level(15), 16); // clamps
    }
}
