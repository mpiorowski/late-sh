//! The persistent Legend of the Green Dragon character and the pure rules that
//! act on it: stat derivation, leveling, shop pricing, healing, banking, and
//! the win/lose outcomes. All authentic LoGD numbers (see [`super::data`]).
//!
//! This module is pure and serde-able: no DB, no RNG except where a fight is
//! resolved through [`super::combat`]. Tests assert the transcribed formulas.

use serde::{Deserialize, Serialize};

use super::combat::Combatant;
use super::data;

/// Starting on-hand gold for a fresh character (`newplayerstartgold`).
pub const START_GOLD: u64 = 50;
/// Forest fights granted per day (`turns`).
pub const TURNS_PER_DAY: u32 = 10;
/// Hitpoints per level — max HP is a flat `HP_PER_LEVEL * level`.
pub const HP_PER_LEVEL: u32 = 10;
/// Fraction of experience kept after a forest death (`1 - forestexploss`).
pub const EXP_KEEP_ON_DEATH: f64 = 0.90;
/// Gold reward ceiling carried into a fresh run after a dragon kill.
pub const DRAGON_RUN_GOLD_CAP: u64 = 300;

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
    /// The creature level this hunt produces for a given player level.
    pub fn creature_level(self, player_level: u8) -> u8 {
        let delta: i16 = match self {
            ForestHunt::Slumming => -2,
            ForestHunt::Hunt => 0,
            ForestHunt::Thrillseeking => 2,
        };
        (player_level as i16 + delta).clamp(1, 16) as u8
    }
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
    pub gold_in_bank: u64,
    /// Forest fights remaining today.
    pub turns: u32,
    /// False after a forest death; revived on the next new day.
    pub alive: bool,
    /// Whether the player has sought the dragon this run (resets per run).
    pub seen_dragon: bool,
    /// Lifetime dragon kills.
    pub dragon_kills: u32,
    /// Permanent max-HP bonus retained across runs (dragon-kill reward).
    pub dragon_hp_bonus: u32,
    /// UTC day-number of the last new-day reset, for turn/heal regeneration.
    pub last_day: i64,
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

    /// Attack stat fed to the combat roll: `level + weapon_tier`.
    pub fn attack(&self) -> u32 {
        self.level as u32 + self.weapon_tier as u32
    }

    /// Defense stat fed to the combat roll: `level + armor_tier`.
    pub fn defense(&self) -> u32 {
        self.level as u32 + self.armor_tier as u32
    }

    /// The player as a [`Combatant`] for [`super::combat::resolve_round`].
    pub fn combatant(&self) -> Combatant {
        Combatant {
            attack: self.attack(),
            defense: self.defense(),
        }
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

    /// Apply a forest/master victory's gold and experience rewards.
    pub fn grant_rewards(&mut self, gold: u32, exp: u32) {
        self.gold = self.gold.saturating_add(gold as u64);
        self.experience = self.experience.saturating_add(exp as u64);
    }

    /// Advance one level after beating the master: +1 level (so +10 max HP, +1
    /// attack, +1 defense via derivation), then heal to full.
    pub fn advance_level(&mut self) {
        if self.level < data::MAX_LEVEL {
            self.level += 1;
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
        Some((master, Combatant { attack: atk, defense: def }, hp))
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
        ((self.level as f64).ln() * (missing as f64 + 10.0)).round().max(0.0) as u64
    }

    /// Pay to fully heal if affordable. Returns true on success (including the
    /// free level-1 case).
    pub fn buy_full_heal(&mut self) -> bool {
        let cost = self.full_heal_cost();
        if self.gold >= cost {
            self.gold -= cost;
            self.hitpoints = self.max_hitpoints();
            true
        } else {
            false
        }
    }

    /// Deposit on-hand gold into the bank (clamped to what's on hand).
    pub fn deposit(&mut self, amount: u64) {
        let amount = amount.min(self.gold);
        self.gold -= amount;
        self.gold_in_bank = self.gold_in_bank.saturating_add(amount);
    }

    /// Withdraw banked gold to hand (clamped to what's banked).
    pub fn withdraw(&mut self, amount: u64) {
        let amount = amount.min(self.gold_in_bank);
        self.gold_in_bank -= amount;
        self.gold = self.gold.saturating_add(amount);
    }

    /// Resolve a forest/PvE death: all on-hand gold lost, 10% experience lost,
    /// sent to the graveyard (revived on the next new day).
    pub fn die(&mut self) {
        self.gold = 0;
        self.experience = (self.experience as f64 * EXP_KEEP_ON_DEATH).round() as u64;
        self.alive = false;
        self.hitpoints = 0;
    }

    /// Reward a Green Dragon kill: bank the lifetime kill, retain a permanent
    /// max-HP bonus, then reset to a fresh, fully-healed run.
    pub fn slay_dragon(&mut self) {
        self.dragon_kills = self.dragon_kills.saturating_add(1);
        // Retain a permanent slice of max HP across runs (LoGD keeps DK buffs).
        self.dragon_hp_bonus = self.dragon_hp_bonus.saturating_add(HP_PER_LEVEL);
        self.level = 1;
        self.experience = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.gold = (START_GOLD + self.dragon_kills as u64 * 100).min(DRAGON_RUN_GOLD_CAP);
        self.seen_dragon = false;
        self.alive = true;
        self.hitpoints = self.max_hitpoints();
    }

    /// Run the daily reset if `today` is past the stored day: refill forest
    /// turns, fully heal, and revive. Returns true if a reset happened.
    pub fn roll_new_day(&mut self, today: i64, dk_forest_bonus: u32) -> bool {
        if today <= self.last_day {
            return false;
        }
        self.last_day = today;
        self.turns = TURNS_PER_DAY + dk_forest_bonus;
        self.alive = true;
        self.hitpoints = self.max_hitpoints();
        true
    }

    /// Apply a daily bank interest multiplier (percent, e.g. 7 for 7%).
    pub fn apply_bank_interest(&mut self, percent: u32) {
        let factor = 1.0 + percent as f64 / 100.0;
        self.gold_in_bank = (self.gold_in_bank as f64 * factor).round() as u64;
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
        c.die();
        assert!(c.roll_new_day(11, 0));
        assert_eq!(c.turns, 10);
        assert!(c.alive);
        assert_eq!(c.hitpoints, c.max_hitpoints());
        // Same day again: no reset.
        c.turns = 3;
        assert!(!c.roll_new_day(11, 0));
        assert_eq!(c.turns, 3);
    }

    #[test]
    fn dragon_kill_resets_run_but_keeps_progress() {
        let mut c = Character::new("hero", 0);
        c.level = 15;
        c.weapon_tier = 15;
        c.experience = 99999;
        c.slay_dragon();
        assert_eq!(c.dragon_kills, 1);
        assert_eq!(c.level, 1);
        assert_eq!(c.weapon_tier, 0);
        assert_eq!(c.dragon_hp_bonus, 10);
        assert_eq!(c.max_hitpoints(), 20); // 10*1 + 10 bonus
        assert_eq!(c.hitpoints, 20);
        assert!(!c.seen_dragon);
    }

    #[test]
    fn forest_hunt_shifts_creature_level() {
        assert_eq!(ForestHunt::Slumming.creature_level(5), 3);
        assert_eq!(ForestHunt::Hunt.creature_level(5), 5);
        assert_eq!(ForestHunt::Thrillseeking.creature_level(5), 7);
        assert_eq!(ForestHunt::Slumming.creature_level(1), 1); // clamps
        assert_eq!(ForestHunt::Thrillseeking.creature_level(15), 16); // clamps
    }
}
