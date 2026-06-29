//! Per-session Green Dragon state: the authoritative character (this is a
//! single-player game, so the session owns the truth), a small mode machine for
//! which screen is open, the active combat encounter, and a short message log.
//!
//! All game actions live here as methods that mutate the character and push log
//! lines; `input.rs` maps keys to these and `ui.rs` renders the getters. Every
//! mutating action persists the character through the service, fire-and-forget.

use std::collections::VecDeque;

use uuid::Uuid;

use super::combat::{Combatant, resolve_round};
use super::data;
use super::model::{Character, ForestHunt};
use super::svc::{CharacterLoad, GreenDragonService};

/// Which Green Dragon screen the session is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Still waiting for the character to load from the DB.
    Loading,
    /// The village square: the main menu of destinations.
    Village,
    /// The forest: choose a hunting intensity.
    Forest,
    /// An active fight (creature, master, or the dragon).
    Fight,
    /// King Arthur's Weapons.
    WeaponShop,
    /// Abdul's Armour.
    ArmorShop,
    /// The Healer's Hut.
    Healer,
    /// Ye Olde Bank.
    Bank,
    /// Bluspring's Warrior Training (the master fight gate).
    Training,
    /// The graveyard: shown while dead, until the next new day.
    Graveyard,
}

/// What kind of foe the current encounter is, deciding the victory handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoeKind {
    Creature,
    Master,
    Dragon,
}

/// A live combat encounter.
#[derive(Clone, Debug)]
pub struct Encounter {
    pub name: String,
    pub weapon: String,
    pub foe: Combatant,
    pub hp: u32,
    pub max_hp: u32,
    pub reward_gold: u32,
    pub reward_exp: u32,
    pub kind: FoeKind,
}

const LOG_CAP: usize = 7;

pub struct State {
    user_id: Uuid,
    svc: GreenDragonService,
    load_rx: tokio::sync::watch::Receiver<CharacterLoad>,
    character: Option<Character>,
    mode: Mode,
    cursor: usize,
    log: VecDeque<String>,
    encounter: Option<Encounter>,
}

impl State {
    /// Open a Green Dragon session for `user_id`, kicking off the character
    /// load. `name` is the player's display name, used only if they have no
    /// save yet.
    pub fn new(svc: GreenDragonService, user_id: Uuid, name: String) -> Self {
        let load_rx = svc.load_character(user_id, name);
        State {
            user_id,
            svc,
            load_rx,
            character: None,
            mode: Mode::Loading,
            cursor: 0,
            log: VecDeque::new(),
            encounter: None,
        }
    }

    /// Drain the initial character load. Called every app tick.
    pub fn tick(&mut self) {
        if self.character.is_some() {
            return;
        }
        // Clone the loaded character out and drop the watch borrow before
        // touching `self` again.
        let ready = match &*self.load_rx.borrow_and_update() {
            CharacterLoad::Ready(character) => Some((**character).clone()),
            CharacterLoad::Loading => None,
        };
        if let Some(character) = ready {
            self.mode = if character.alive {
                Mode::Village
            } else {
                Mode::Graveyard
            };
            self.push_log(format!(
                "Welcome to Degolburg, {}. The Green Dragon awaits the brave.",
                character.name
            ));
            self.character = Some(character);
            self.cursor = 0;
        }
    }

    // --- getters for the UI -------------------------------------------------

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn character(&self) -> Option<&Character> {
        self.character.as_ref()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn encounter(&self) -> Option<&Encounter> {
        self.encounter.as_ref()
    }

    pub fn log_lines(&self) -> impl Iterator<Item = &str> {
        self.log.iter().map(String::as_str)
    }

    /// The selectable rows for the current mode, as `(label, enabled)`.
    pub fn menu(&self) -> Vec<(String, bool)> {
        let Some(c) = self.character.as_ref() else {
            return Vec::new();
        };
        match self.mode {
            Mode::Village => village_menu(c),
            Mode::Forest => forest_menu(c),
            Mode::WeaponShop => shop_menu(c, true),
            Mode::ArmorShop => shop_menu(c, false),
            Mode::Healer => healer_menu(c),
            Mode::Bank => bank_menu(c),
            Mode::Training => training_menu(c),
            Mode::Fight => fight_menu(),
            Mode::Graveyard => vec![("Wait for a new day (leave)".into(), true)],
            Mode::Loading => Vec::new(),
        }
    }

    // --- cursor + selection -------------------------------------------------

    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.menu().len();
        if len == 0 {
            return;
        }
        let cur = self.cursor as i32;
        self.cursor = (cur + delta).rem_euclid(len as i32) as usize;
    }

    /// Activate the highlighted row. Returns false only when the row is the
    /// "leave the game" sentinel handled by the caller.
    pub fn select(&mut self) -> Selection {
        let menu = self.menu();
        if self.cursor >= menu.len() {
            return Selection::Stay;
        }
        if !menu[self.cursor].1 {
            self.push_log("You can't do that yet.".into());
            return Selection::Stay;
        }
        match self.mode {
            Mode::Village => self.select_village(),
            Mode::Forest => self.select_forest(),
            Mode::WeaponShop => self.buy_gear(true),
            Mode::ArmorShop => self.buy_gear(false),
            Mode::Healer => self.select_healer(),
            Mode::Bank => self.select_bank(),
            Mode::Training => self.select_training(),
            Mode::Fight => self.select_fight(),
            Mode::Graveyard => Selection::Leave,
            Mode::Loading => Selection::Stay,
        }
    }

    /// Back out one level: leaf screens return to the village; the village
    /// leaves the game.
    pub fn back(&mut self) -> Selection {
        match self.mode {
            Mode::Village | Mode::Loading => Selection::Leave,
            Mode::Fight => {
                // Esc during a fight flees back to the village (the turn is
                // already spent).
                self.push_log("You flee back to the safety of the village.".into());
                self.encounter = None;
                self.goto(Mode::Village);
                Selection::Stay
            }
            _ => {
                self.goto(Mode::Village);
                Selection::Stay
            }
        }
    }

    fn goto(&mut self, mode: Mode) {
        self.mode = mode;
        self.cursor = 0;
    }

    // --- village ------------------------------------------------------------

    fn select_village(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        let rows = village_menu(c);
        match rows[self.cursor].0.as_str() {
            s if s.starts_with("The Forest") => self.goto(Mode::Forest),
            s if s.starts_with("Bluspring") => self.goto(Mode::Training),
            s if s.starts_with("Seek Out the Green Dragon") => self.start_dragon(),
            s if s.starts_with("King Arthur") => self.goto(Mode::WeaponShop),
            s if s.starts_with("Abdul") => self.goto(Mode::ArmorShop),
            s if s.starts_with("The Healer") => self.goto(Mode::Healer),
            s if s.starts_with("Ye Olde Bank") => self.goto(Mode::Bank),
            s if s.starts_with("Leave") => return Selection::Leave,
            _ => {}
        }
        Selection::Stay
    }

    // --- forest -------------------------------------------------------------

    fn select_forest(&mut self) -> Selection {
        let hunt = match self.cursor {
            0 => ForestHunt::Slumming,
            1 => ForestHunt::Hunt,
            2 => ForestHunt::Thrillseeking,
            _ => return Selection::Stay,
        };
        self.start_forest_fight(hunt);
        Selection::Stay
    }

    fn start_forest_fight(&mut self, hunt: ForestHunt) {
        let c = self.character.as_mut().unwrap();
        if c.turns == 0 {
            self.push_log("You are too tired to fight. Come back tomorrow.".into());
            return;
        }
        c.turns -= 1;
        let level = hunt.creature_level(c.level);
        let tier = data::creature_tier(level);
        let names = data::CREATURE_NAMES[(level.clamp(1, 16) - 1) as usize];
        let pick = rng_index(names.len());
        let (name, weapon) = names[pick];
        self.encounter = Some(Encounter {
            name: name.to_string(),
            weapon: weapon.to_string(),
            foe: Combatant {
                attack: tier.attack,
                defense: tier.defense,
            },
            hp: tier.hp,
            max_hp: tier.hp,
            reward_gold: tier.gold,
            reward_exp: tier.exp,
            kind: FoeKind::Creature,
        });
        self.push_log(format!("You encounter {name} wielding {weapon}!"));
        self.goto(Mode::Fight);
    }

    // --- training (master fight) -------------------------------------------

    fn select_training(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        if !c.can_challenge_master() {
            self.push_log("Your master shakes their head. Gain more experience first.".into());
            return Selection::Stay;
        }
        let Some((master, foe, hp)) = c.current_master() else {
            return Selection::Stay;
        };
        self.encounter = Some(Encounter {
            name: master.name.to_string(),
            weapon: master.weapon.to_string(),
            foe,
            hp,
            max_hp: hp,
            reward_gold: 0,
            reward_exp: 0,
            kind: FoeKind::Master,
        });
        self.push_log(format!("{} steps forward to test you!", master.name));
        self.goto(Mode::Fight);
        Selection::Stay
    }

    // --- dragon -------------------------------------------------------------

    fn start_dragon(&mut self) {
        let c = self.character.as_mut().unwrap();
        if !c.can_seek_dragon() {
            self.push_log("You are not ready to face the Green Dragon.".into());
            return;
        }
        c.seen_dragon = true;
        self.encounter = Some(Encounter {
            name: "The Green Dragon".to_string(),
            weapon: "Fearsome Claws and Flame".to_string(),
            foe: Combatant {
                attack: data::DRAGON_ATTACK,
                defense: data::DRAGON_DEFENSE,
            },
            hp: data::DRAGON_HP,
            max_hp: data::DRAGON_HP,
            reward_gold: 0,
            reward_exp: 0,
            kind: FoeKind::Dragon,
        });
        self.push_log("You step into the dragon's lair. The air turns to fire.".into());
        self.goto(Mode::Fight);
    }

    // --- fight resolution ---------------------------------------------------

    fn fight_menu_action(&self) -> usize {
        self.cursor
    }

    fn select_fight(&mut self) -> Selection {
        match self.fight_menu_action() {
            0 => {
                self.attack_round();
                Selection::Stay
            }
            1 => self.back(), // Flee
            _ => Selection::Stay,
        }
    }

    fn attack_round(&mut self) {
        let Some(mut enc) = self.encounter.take() else {
            return;
        };
        let player = self.character.as_ref().unwrap().combatant();
        let mut rng = rand::thread_rng();
        let outcome = resolve_round(&mut rng, player, enc.foe);

        if outcome.player_crit {
            self.push_log("A critical strike! You triple your power!".into());
        }
        enc.hp = enc.hp.saturating_sub(outcome.damage_to_enemy);
        self.push_log(format!(
            "You hit {} for {} ({} HP left).",
            enc.name, outcome.damage_to_enemy, enc.hp
        ));

        if enc.hp == 0 {
            self.victory(&enc);
            return;
        }

        // Foe strikes back.
        let c = self.character.as_mut().unwrap();
        c.hitpoints = c.hitpoints.saturating_sub(outcome.damage_to_player);
        let hp = c.hitpoints;
        self.push_log(format!(
            "{} hits you for {} ({} HP left).",
            enc.name, outcome.damage_to_player, hp
        ));

        if hp == 0 {
            self.defeat(&enc);
            return;
        }
        self.encounter = Some(enc);
        self.save();
    }

    fn victory(&mut self, enc: &Encounter) {
        match enc.kind {
            FoeKind::Creature => {
                let c = self.character.as_mut().unwrap();
                c.grant_rewards(enc.reward_gold, enc.reward_exp);
                self.push_log(format!(
                    "You slay {}! +{} gold, +{} experience.",
                    enc.name, enc.reward_gold, enc.reward_exp
                ));
                self.encounter = None;
                // Stay in the forest to fight again if turns remain.
                self.goto(Mode::Forest);
            }
            FoeKind::Master => {
                let c = self.character.as_mut().unwrap();
                c.advance_level();
                let lvl = c.level;
                self.push_log(format!(
                    "You defeat {}! You advance to level {} and are fully healed.",
                    enc.name, lvl
                ));
                self.encounter = None;
                self.goto(Mode::Village);
            }
            FoeKind::Dragon => {
                self.character.as_mut().unwrap().slay_dragon();
                let kills = self.character.as_ref().unwrap().dragon_kills;
                self.push_log(format!(
                    "THE GREEN DRAGON IS SLAIN! Dragon kill #{kills}. You begin a new, stronger journey."
                ));
                self.encounter = None;
                self.goto(Mode::Village);
            }
        }
        self.save();
    }

    fn defeat(&mut self, enc: &Encounter) {
        let c = self.character.as_mut().unwrap();
        match enc.kind {
            FoeKind::Master => {
                // A training loss isn't lethal in LoGD; you just get sent home.
                c.hitpoints = 1;
                self.push_log(format!(
                    "{} defeats you soundly. You limp home to train harder.",
                    enc.name
                ));
                self.encounter = None;
                self.goto(Mode::Village);
            }
            _ => {
                c.die();
                self.push_log(format!(
                    "{} has slain you! Your gold is lost and you are dragged to the graveyard.",
                    enc.name
                ));
                self.encounter = None;
                self.goto(Mode::Graveyard);
            }
        }
        self.save();
    }

    // --- shops --------------------------------------------------------------

    fn buy_gear(&mut self, weapon: bool) -> Selection {
        let c = self.character.as_ref().unwrap();
        let tiers = available_tiers(c, weapon);
        if self.cursor >= tiers.len() {
            return Selection::Stay;
        }
        let (tier, _cost) = tiers[self.cursor];
        let c = self.character.as_mut().unwrap();
        let ok = if weapon {
            c.buy_weapon(tier)
        } else {
            c.buy_armor(tier)
        };
        if ok {
            self.push_log(format!(
                "You purchase {} tier {}.",
                if weapon { "weapon" } else { "armor" },
                tier
            ));
            self.save();
        } else {
            self.push_log("You can't afford that.".into());
        }
        Selection::Stay
    }

    // --- healer -------------------------------------------------------------

    fn select_healer(&mut self) -> Selection {
        let c = self.character.as_mut().unwrap();
        if c.hitpoints >= c.max_hitpoints() {
            self.push_log("You are already at full health.".into());
            return Selection::Stay;
        }
        let cost = c.full_heal_cost();
        if c.buy_full_heal() {
            self.push_log(format!("The healer restores you to full health for {cost} gold."));
            self.save();
        } else {
            self.push_log("You can't afford a full healing.".into());
        }
        Selection::Stay
    }

    // --- bank ---------------------------------------------------------------

    fn select_bank(&mut self) -> Selection {
        let c = self.character.as_mut().unwrap();
        match self.cursor {
            0 => {
                let amount = c.gold;
                c.deposit(amount);
                self.push_log(format!("You deposit {amount} gold."));
            }
            1 => {
                let amount = c.gold_in_bank;
                c.withdraw(amount);
                self.push_log(format!("You withdraw {amount} gold."));
            }
            _ => return Selection::Stay,
        }
        self.save();
        Selection::Stay
    }

    // --- helpers ------------------------------------------------------------

    fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > LOG_CAP {
            self.log.pop_front();
        }
    }

    /// Persist the current character, fire-and-forget.
    fn save(&mut self) {
        if let Some(c) = self.character.as_ref() {
            self.svc.save_character(self.user_id, c);
        }
    }

    /// Persist on the way out of the game (called from `leave`).
    pub fn save_on_leave(&self) {
        if let Some(c) = self.character.as_ref() {
            self.svc.save_character(self.user_id, c);
        }
    }
}

/// The result of activating a menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Stay in the game; the UI updates.
    Stay,
    /// Leave the door, returning to the Games hub.
    Leave,
}

/// Pick a random index in `0..len`, or 0 for a singleton/empty list. Uses a
/// fresh thread RNG so `State` holds no `!Send` RNG handle.
fn rng_index(len: usize) -> usize {
    use rand::Rng;
    if len <= 1 {
        0
    } else {
        rand::thread_rng().gen_range(0..len)
    }
}

// --- menu builders (pure, so they can be unit-tested) -----------------------

fn village_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows = vec![
        (format!("The Forest ({} turns left)", c.turns), c.turns > 0),
        (
            "Bluspring's Warrior Training".into(),
            c.can_challenge_master(),
        ),
    ];
    if c.can_seek_dragon() {
        rows.push(("Seek Out the Green Dragon".into(), true));
    }
    rows.push(("King Arthur's Weapons".into(), true));
    rows.push(("Abdul's Armour".into(), true));
    rows.push((
        "The Healer's Hut".into(),
        c.hitpoints < c.max_hitpoints(),
    ));
    rows.push(("Ye Olde Bank".into(), true));
    rows.push(("Leave the realm".into(), true));
    rows
}

fn forest_menu(c: &Character) -> Vec<(String, bool)> {
    let has_turns = c.turns > 0;
    vec![
        ("Go Slumming (weaker prey)".into(), has_turns),
        ("Look for Something to Kill".into(), has_turns),
        ("Go Thrillseeking (deadlier prey)".into(), has_turns),
    ]
}

fn fight_menu() -> Vec<(String, bool)> {
    vec![("Attack".into(), true), ("Flee".into(), true)]
}

fn healer_menu(c: &Character) -> Vec<(String, bool)> {
    let needs = c.hitpoints < c.max_hitpoints();
    vec![(
        format!("Heal fully ({} gold)", c.full_heal_cost()),
        needs,
    )]
}

fn bank_menu(c: &Character) -> Vec<(String, bool)> {
    vec![
        (format!("Deposit all ({} gold)", c.gold), c.gold > 0),
        (
            format!("Withdraw all ({} gold)", c.gold_in_bank),
            c.gold_in_bank > 0,
        ),
    ]
}

fn training_menu(c: &Character) -> Vec<(String, bool)> {
    match c.current_master() {
        Some((master, _, _)) => vec![(
            format!("Challenge {}", master.name),
            c.can_challenge_master(),
        )],
        None => vec![("You have mastered all training.".into(), false)],
    }
}

/// Up to the next five gear upgrade tiers with their trade-in-adjusted cost.
fn available_tiers(c: &Character, weapon: bool) -> Vec<(u8, u64)> {
    let current = if weapon { c.weapon_tier } else { c.armor_tier };
    (current + 1..=data::COST_LADDER.len() as u8)
        .take(5)
        .filter_map(|tier| {
            let cost = if weapon {
                c.weapon_upgrade_cost(tier)
            } else {
                c.armor_upgrade_cost(tier)
            }?;
            Some((tier, cost))
        })
        .collect()
}

fn shop_menu(c: &Character, weapon: bool) -> Vec<(String, bool)> {
    let tiers = available_tiers(c, weapon);
    if tiers.is_empty() {
        return vec![("You wield the finest available. (nothing to buy)".into(), false)];
    }
    tiers
        .into_iter()
        .map(|(tier, cost)| {
            (
                format!("Tier {tier} (power {tier}) - {cost} gold"),
                c.gold >= cost,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lvl(level: u8) -> Character {
        let mut c = Character::new("t", 0);
        c.level = level;
        c.hitpoints = c.max_hitpoints();
        c
    }

    #[test]
    fn village_menu_gates_on_state() {
        let mut c = lvl(1);
        c.turns = 0;
        let rows = village_menu(&c);
        // Forest row disabled with no turns.
        assert!(!rows[0].1);
        // Healer disabled at full health.
        let healer = rows.iter().find(|(l, _)| l.starts_with("The Healer")).unwrap();
        assert!(!healer.1);
        // Dragon not offered below level 15.
        assert!(!rows.iter().any(|(l, _)| l.starts_with("Seek Out")));
    }

    #[test]
    fn dragon_offered_at_max_level() {
        let c = lvl(15);
        let rows = village_menu(&c);
        assert!(rows.iter().any(|(l, _)| l.starts_with("Seek Out")));
    }

    #[test]
    fn shop_lists_affordable_upgrades() {
        let mut c = lvl(1);
        c.gold = 100; // affords tier 1 (48) but not tier 2 (189 after trade-in)
        let tiers = available_tiers(&c, true);
        assert_eq!(tiers[0], (1, 48));
        let menu = shop_menu(&c, true);
        assert!(menu[0].1); // tier 1 affordable
        assert!(!menu[1].1); // tier 2 not
    }

    #[test]
    fn bank_menu_reflects_balances() {
        let mut c = lvl(3);
        c.gold = 200;
        c.gold_in_bank = 0;
        let rows = bank_menu(&c);
        assert!(rows[0].1); // can deposit
        assert!(!rows[1].1); // nothing to withdraw
    }
}
