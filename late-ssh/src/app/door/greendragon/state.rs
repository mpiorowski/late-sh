//! Per-session Green Dragon state: the authoritative character (this is a
//! single-player game, so the session owns the truth), a small mode machine for
//! which screen is open, the active combat encounter, and a short message log.
//!
//! All game actions live here as methods that mutate the character and push log
//! lines; `input.rs` maps keys to these and `ui.rs` renders the getters. Every
//! mutating action persists the character through the service, fire-and-forget.

use std::collections::VecDeque;

use rand::Rng;
use uuid::Uuid;

use super::combat::{Buff, Combatant, resolve_round_buffed};
use super::data;
use super::events::{self, ForestEvent};
use super::model::{Character, ForestHunt, Specialty};
use super::specialty::{self, SkillEffect};
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
    /// Ironroost Weapons.
    WeaponShop,
    /// Duskmail Armoury.
    ArmorShop,
    /// The Mendery (healer).
    Healer,
    /// The Coinvault (bank).
    Bank,
    /// The Mercenary Camp: hire and mend companions.
    MercenaryCamp,
    /// The Proving Yard (the master fight gate).
    Training,
    /// A forest special event awaiting the player's accept/decline choice.
    Event,
    /// The one-time specialty chooser (Mystical / Dark Arts / Thief).
    ChooseSpecialty,
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
    /// Active specialty buffs, ticked each round by [`resolve_round_buffed`].
    pub buffs: Vec<Buff>,
    /// Whether the player has taken any damage this fight (drives the dragon's
    /// flawless-kill bonus).
    pub took_damage: bool,
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
    /// The forest event awaiting an accept/decline choice, while in [`Mode::Event`].
    pending_event: Option<ForestEvent>,
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
            pending_event: None,
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
                "Welcome to Duskmere, {}. The Green Dragon awaits the brave.",
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

    /// The forest event currently awaiting a choice, if any (for rendering its
    /// framing text in [`Mode::Event`]).
    pub fn pending_event(&self) -> Option<ForestEvent> {
        self.pending_event
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
            Mode::MercenaryCamp => mercenary_menu(c),
            Mode::Training => training_menu(c),
            Mode::Fight => fight_menu(c),
            Mode::Event => event_menu(c, self.pending_event),
            Mode::ChooseSpecialty => specialty_menu(),
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
            Mode::MercenaryCamp => self.select_mercenary(),
            Mode::Training => self.select_training(),
            Mode::Fight => self.select_fight(),
            Mode::Event => self.select_event(),
            Mode::ChooseSpecialty => self.select_specialty(),
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
                // already spent). Persist so the fled fight stays fled.
                self.push_log("You flee back to the safety of the village.".into());
                self.encounter = None;
                self.goto(Mode::Village);
                self.save();
                Selection::Stay
            }
            Mode::Event => {
                // Esc on an event declines it (the no-choice branch), then
                // returns to the forest.
                self.cursor = 1;
                self.select_event()
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
            s if s.starts_with("Choose a Specialty") => self.goto(Mode::ChooseSpecialty),
            s if s.starts_with("The Proving Yard") => self.goto(Mode::Training),
            s if s.starts_with("Seek Out the Green Dragon") => self.start_dragon(),
            s if s.starts_with("Ironroost") => self.goto(Mode::WeaponShop),
            s if s.starts_with("Duskmail") => self.goto(Mode::ArmorShop),
            s if s.starts_with("The Mendery") => self.goto(Mode::Healer),
            s if s.starts_with("The Coinvault") => self.goto(Mode::Bank),
            s if s.starts_with("The Mercenary Camp") => self.goto(Mode::MercenaryCamp),
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
        // A fraction of searches turn up a special event instead of a fight. The
        // event itself doesn't spend the forest turn (some, like the mine, spend
        // it as their own effect), so roll before decrementing.
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < events::FOREST_EVENT_PERCENT {
            let event = events::roll(&mut rng);
            self.start_event(event);
            return;
        }
        c.turns -= 1;
        let player_level = c.level;
        // The hunt sets a ±1 base shift; LoGD then layers a small random jitter:
        // roughly a third of searches nudge the level up (1/5) and/or down (1/3).
        let mut level = hunt.creature_level(player_level) as i16;
        if rng.gen_range(0..3) == 0 {
            if rng.gen_range(0..5) == 0 {
                level += 1;
            }
            if rng.gen_range(0..3) == 0 {
                level -= 1;
            }
        }
        let level = level.clamp(1, 16) as u8;
        let tier = data::creature_tier(level);
        let names = data::CREATURE_NAMES[(level - 1) as usize];
        let (name, weapon) = names[rng.gen_range(0..names.len())];
        // Thrillseeking pays 10% more gold and experience for the added risk.
        let (reward_gold, reward_exp) = if matches!(hunt, ForestHunt::Thrillseeking) {
            (
                (tier.gold as f64 * 1.10).round() as u32,
                (tier.exp as f64 * 1.10).round() as u32,
            )
        } else {
            (tier.gold, tier.exp)
        };
        self.encounter = Some(Encounter {
            name: name.to_string(),
            weapon: weapon.to_string(),
            foe: Combatant {
                attack: tier.attack,
                defense: tier.defense,
            },
            hp: tier.hp,
            max_hp: tier.hp,
            reward_gold,
            reward_exp,
            kind: FoeKind::Creature,
            buffs: Vec::new(),
            took_damage: false,
        });
        self.push_log(format!("You encounter {name} wielding {weapon}!"));
        self.goto(Mode::Fight);
        // Persist the spent forest turn now, so a disconnect mid-fight can't
        // refund it on reconnect.
        self.save();
    }

    // --- forest special events ----------------------------------------------

    /// Begin a forest event: log its framing, then either resolve it instantly
    /// (no choice) or open [`Mode::Event`] to await the player's decision.
    fn start_event(&mut self, event: ForestEvent) {
        let c = self.character.as_ref().unwrap();
        let pres = event.present(c);
        if pres.choice.is_none() {
            // Instant event: narration and outcome both go to the log, then we
            // drop straight back to the forest.
            for line in &pres.intro {
                self.push_log((*line).to_string());
            }
            let mut rng = rand::thread_rng();
            let lines = event.resolve(true, self.character.as_mut().unwrap(), &mut rng);
            for line in lines {
                self.push_log(line);
            }
            self.after_event();
        } else {
            // Choice event: the framing is shown in the panel (Mode::Event), so
            // it isn't logged until the outcome lands.
            self.pending_event = Some(event);
            self.goto(Mode::Event);
        }
    }

    /// Resolve the pending event with the player's choice (cursor 0 = accept).
    fn select_event(&mut self) -> Selection {
        let Some(event) = self.pending_event.take() else {
            self.goto(Mode::Forest);
            return Selection::Stay;
        };
        let accepted = self.cursor == 0;
        let mut rng = rand::thread_rng();
        let lines = event.resolve(accepted, self.character.as_mut().unwrap(), &mut rng);
        for line in lines {
            self.push_log(line);
        }
        self.after_event();
        Selection::Stay
    }

    /// Land somewhere sensible after an event: the graveyard if it killed you
    /// (the mine cave-in, the stream), otherwise back to the forest to hunt on.
    fn after_event(&mut self) {
        self.pending_event = None;
        let alive = self.character.as_ref().unwrap().alive;
        self.goto(if alive { Mode::Forest } else { Mode::Graveyard });
        self.save();
    }

    // --- specialty chooser --------------------------------------------------

    /// Apply the one-time specialty choice and return to the village.
    fn select_specialty(&mut self) -> Selection {
        let choice = match self.cursor {
            0 => Specialty::Mystical,
            1 => Specialty::DarkArts,
            2 => Specialty::Thief,
            _ => return Selection::Stay,
        };
        let c = self.character.as_mut().unwrap();
        c.choose_specialty(choice);
        self.push_log(format!("You devote yourself to the {}.", choice.name()));
        self.save();
        self.goto(Mode::Village);
        Selection::Stay
    }

    // --- training (master fight) -------------------------------------------

    fn select_training(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        if !c.can_challenge_master() {
            self.push_log("Your master shakes their head. Gain more experience first.".into());
            return Selection::Stay;
        }
        let Some((master, foe, hp)) = c.scaled_master(&mut rand::thread_rng()) else {
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
            buffs: Vec::new(),
            took_damage: false,
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
        let (attack, defense, hp) = c.scaled_dragon(&mut rand::thread_rng());
        self.encounter = Some(Encounter {
            name: "The Green Dragon".to_string(),
            weapon: "Fearsome Claws and Flame".to_string(),
            foe: Combatant { attack, defense },
            hp,
            max_hp: hp,
            reward_gold: 0,
            reward_exp: 0,
            kind: FoeKind::Dragon,
            buffs: Vec::new(),
            took_damage: false,
        });
        self.push_log("You step into the dragon's lair. The air turns to fire.".into());
        self.goto(Mode::Fight);
        // Persist `seen_dragon` now so the once-per-run dragon seek can't be
        // retried by disconnecting before the fight resolves.
        self.save();
    }

    // --- fight resolution ---------------------------------------------------

    fn fight_menu_action(&self) -> usize {
        self.cursor
    }

    fn select_fight(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        let skill_count = specialty::skills(c.specialty).len();
        let cursor = self.fight_menu_action();
        // Layout: [0] Attack, [1..=skill_count] skills, [last] Flee.
        if cursor == 0 {
            self.attack_round();
            Selection::Stay
        } else if cursor <= skill_count {
            self.cast_specialty_skill(cursor - 1)
        } else {
            self.back() // Flee
        }
    }

    fn attack_round(&mut self) {
        let Some(mut enc) = self.encounter.take() else {
            return;
        };
        let mut rng = rand::thread_rng();
        let player = self.character.as_ref().unwrap().combatant();
        let player_max = self.character.as_ref().unwrap().max_hitpoints();
        // Companions live on the character and fight each round; the resolver
        // mutates their HP and removes any that fall.
        let outcome = {
            let c = self.character.as_mut().unwrap();
            resolve_round_buffed(&mut rng, player, enc.foe, &mut enc.buffs, &mut c.companions)
        };

        if outcome.player_crit {
            self.push_log("A critical strike! You triple your power!".into());
        }
        if let Some(pm) = outcome.power_move {
            self.push_log(pm.label().into());
        }
        // Buff/companion flavor for this round.
        for msg in &outcome.messages {
            self.push_log(msg.clone());
        }

        // Damage is signed: a glancing blow (negative) heals the target.
        enc.hp = apply_signed(enc.hp, outcome.damage_to_enemy, enc.max_hp);
        if outcome.damage_to_enemy >= 0 {
            self.push_log(format!(
                "You hit {} for {} ({} HP left).",
                enc.name, outcome.damage_to_enemy, enc.hp
            ));
        } else {
            self.push_log(format!(
                "Your blow glances off {}; it recovers {} HP ({} left).",
                enc.name, -outcome.damage_to_enemy, enc.hp
            ));
        }

        if enc.hp == 0 {
            self.victory(&enc);
            return;
        }

        // Foe strikes back. A hit that lands marks the fight as no longer
        // flawless (the dragon's flawless bonus rides on this).
        if outcome.damage_to_player > 0 {
            enc.took_damage = true;
        }
        let c = self.character.as_mut().unwrap();
        c.hitpoints = apply_signed(c.hitpoints, outcome.damage_to_player, player_max);
        if outcome.player_heal > 0 {
            c.hitpoints = (c.hitpoints + outcome.player_heal).min(c.max_hitpoints());
        }
        let hp = c.hitpoints;
        if outcome.damage_to_player >= 0 {
            self.push_log(format!(
                "{} hits you for {} ({} HP left).",
                enc.name, outcome.damage_to_player, hp
            ));
        } else {
            self.push_log(format!("{} fumbles its strike ({} HP left).", enc.name, hp));
        }
        if outcome.player_heal > 0 {
            self.push_log(format!(
                "You knit {} HP back together.",
                outcome.player_heal
            ));
        }

        if hp == 0 {
            self.defeat(&enc);
            return;
        }
        self.encounter = Some(enc);
        self.save();
    }

    /// Cast the specialty skill at `skill_index` (rows after Attack/Flee in the
    /// fight menu): spend its uses, apply its buff to the encounter, then resolve
    /// a round with it active. Mirrors LoGD, where invoking a skill *is* the
    /// round's action.
    fn cast_specialty_skill(&mut self, skill_index: usize) -> Selection {
        let c = self.character.as_ref().unwrap();
        let skills = specialty::skills(c.specialty);
        let Some(skill) = skills.get(skill_index) else {
            return Selection::Stay;
        };
        let (level, attack) = (c.level as u32, c.attack());
        let (name, cost) = (skill.name, skill.cost);
        let effect = skill.effect(level, attack);
        if !self.character.as_mut().unwrap().spend_specialty_uses(cost) {
            self.push_log("You haven't the focus left for that skill.".into());
            return Selection::Stay;
        }
        match effect {
            SkillEffect::Buff(buff) => {
                if let Some(enc) = self.encounter.as_mut() {
                    enc.buffs.push(buff);
                }
            }
            SkillEffect::Summon(companion) => {
                let name = companion.name.clone();
                if self.character.as_mut().unwrap().add_companion(companion) {
                    self.push_log(format!("{name} claws up from the earth to fight at your side."));
                } else {
                    self.push_log(
                        "The earth stirs, but you already lead too many to command another.".into(),
                    );
                }
            }
        }
        self.push_log(format!("You invoke {name}!"));
        self.attack_round();
        Selection::Stay
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
                let flawless = !enc.took_damage;
                self.character.as_mut().unwrap().slay_dragon(flawless);
                let kills = self.character.as_ref().unwrap().dragon_kills;
                let mut msg = format!(
                    "THE GREEN DRAGON IS SLAIN! Dragon kill #{kills}. Your strength and guard harden for the run ahead."
                );
                if flawless {
                    msg.push_str(" Flawless - not a scratch on you! Bonus gold and a gem.");
                }
                self.push_log(msg);
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
                // A training loss isn't lethal in LoGD: the master halts before
                // the final blow and mends your wounds (heal to full), sending
                // you off to train harder. No death, no penalty.
                c.hitpoints = c.max_hitpoints();
                self.push_log(format!(
                    "{} bests you, then stays the final blow and heals your wounds. Train harder.",
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
            let name = if weapon {
                data::weapon_name(tier)
            } else {
                data::armor_name(tier)
            };
            self.push_log(format!("You equip the {name}."));
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
            self.push_log(format!(
                "The healer restores you to full health for {cost} gold."
            ));
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

    // --- mercenary camp -----------------------------------------------------

    fn select_mercenary(&mut self) -> Selection {
        let c = self.character.as_ref().unwrap();
        let actions = mercenary_actions(c);
        let Some(action) = actions.get(self.cursor).copied() else {
            return Selection::Stay;
        };
        match action {
            MercAction::Hire(i) => {
                let merc = &data::MERCENARIES[i];
                if self.character.as_mut().unwrap().hire_mercenary(merc) {
                    self.push_log(format!("{} joins your band. {}", merc.name, merc.blurb));
                    self.save();
                } else {
                    self.push_log("They look you over, spit, and turn away.".into());
                }
            }
            MercAction::Heal(i) => {
                let name = c.companions[i].name.clone();
                let cost = c.companion_heal_cost(i).unwrap_or(0);
                if self.character.as_mut().unwrap().heal_companion(i) {
                    self.push_log(format!(
                        "The camp surgeon patches {name} back to full for {cost} gold."
                    ));
                    self.save();
                } else {
                    self.push_log("You haven't the gold to mend them.".into());
                }
            }
        }
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

/// Apply signed combat damage to an HP pool, clamping into `0..=max`. Positive
/// damage subtracts; negative damage (a glancing blow) heals the target.
fn apply_signed(hp: u32, dmg: i32, max: u32) -> u32 {
    (hp as i64 - dmg as i64).clamp(0, max as i64) as u32
}

/// The result of activating a menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Stay in the game; the UI updates.
    Stay,
    /// Leave the door, returning to the Games hub.
    Leave,
}

// --- menu builders (pure, so they can be unit-tested) -----------------------

fn village_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows = vec![
        (format!("The Forest ({} turns left)", c.turns), c.turns > 0),
        (
            "The Proving Yard (warrior training)".into(),
            c.can_challenge_master(),
        ),
    ];
    if c.specialty == Specialty::None {
        rows.push(("Choose a Specialty".into(), true));
    }
    if c.can_seek_dragon() {
        rows.push(("Seek Out the Green Dragon".into(), true));
    }
    rows.push(("Ironroost Weapons".into(), true));
    rows.push(("Duskmail Armoury".into(), true));
    rows.push((
        "The Mendery (healer)".into(),
        c.hitpoints < c.max_hitpoints(),
    ));
    rows.push(("The Coinvault (bank)".into(), true));
    rows.push(("The Mercenary Camp (companions)".into(), true));
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

/// The fight menu: Attack, then any unlocked specialty skills (shown with their
/// use-cost and disabled when the pool can't pay), then Flee. The skill rows sit
/// between Attack and Flee so those two keep stable positions.
fn fight_menu(c: &Character) -> Vec<(String, bool)> {
    let mut rows = vec![("Attack".into(), true)];
    for skill in specialty::skills(c.specialty) {
        rows.push((
            format!(
                "{} ({} use{})",
                skill.name,
                skill.cost,
                if skill.cost == 1 { "" } else { "s" }
            ),
            c.specialty_uses >= skill.cost,
        ));
    }
    rows.push(("Flee".into(), true));
    rows
}

/// The three specialty choices for the one-time chooser.
fn specialty_menu() -> Vec<(String, bool)> {
    vec![
        ("Mystical Powers (regeneration, life-siphon)".into(), true),
        ("Dark Arts (minions, curses)".into(), true),
        ("Thief Skills (poison, backstab)".into(), true),
    ]
}

/// The pending forest event's two choices, or empty if none is staged.
fn event_menu(c: &Character, event: Option<ForestEvent>) -> Vec<(String, bool)> {
    match event.and_then(|e| e.present(c).choice) {
        Some((accept, decline)) => vec![(accept.into(), true), (decline.into(), true)],
        None => Vec::new(),
    }
}

fn healer_menu(c: &Character) -> Vec<(String, bool)> {
    let needs = c.hitpoints < c.max_hitpoints();
    vec![(format!("Heal fully ({} gold)", c.full_heal_cost()), needs)]
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

/// One selectable action in the Mercenary Camp: hire the roster entry at this
/// index, or mend the owned companion at this index. Built identically by the
/// menu renderer and the selection handler so cursor rows line up (mirroring the
/// shop's `available_tiers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MercAction {
    Hire(usize),
    Heal(usize),
}

/// The camp's live action list: hires first (roster entries whose dragon-kill
/// gate you've cleared and don't already lead), then heals (owned, damaged,
/// mendable companions). Band-full and can't-afford states stay *listed* here
/// but render disabled, so the player can still see what's on offer.
fn mercenary_actions(c: &Character) -> Vec<MercAction> {
    let mut actions = Vec::new();
    for (i, m) in data::MERCENARIES.iter().enumerate() {
        if c.dragon_kills >= m.cost_dks && !c.owns_companion(m.name) {
            actions.push(MercAction::Hire(i));
        }
    }
    for i in 0..c.companions.len() {
        if c.companion_heal_cost(i).is_some() {
            actions.push(MercAction::Heal(i));
        }
    }
    actions
}

fn merc_price_label(m: &data::Mercenary) -> String {
    let gems = |n: u64| format!("{n} gem{}", if n == 1 { "" } else { "s" });
    match (m.cost_gold, m.cost_gems) {
        (0, g) => gems(g),
        (gold, 0) => format!("{gold} gold"),
        (gold, g) => format!("{gold} gold + {}", gems(g)),
    }
}

fn mercenary_menu(c: &Character) -> Vec<(String, bool)> {
    let actions = mercenary_actions(c);
    if actions.is_empty() {
        return vec![(
            "No one here will follow you today. (nothing to do)".into(),
            false,
        )];
    }
    actions
        .into_iter()
        .map(|action| match action {
            MercAction::Hire(i) => {
                let m = &data::MERCENARIES[i];
                (
                    format!("Hire {} - {}", m.name, merc_price_label(m)),
                    c.can_hire(m),
                )
            }
            MercAction::Heal(i) => {
                let cost = c.companion_heal_cost(i).unwrap_or(0);
                let comp = &c.companions[i];
                (
                    format!(
                        "Mend {} - {cost} gold ({}/{} HP)",
                        comp.name, comp.hitpoints, comp.max_hitpoints
                    ),
                    c.gold >= cost,
                )
            }
        })
        .collect()
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
///
/// Level-gated, mirroring LoGD: a shop only stocks gear up to the character's
/// own level, so you can't grind gold to out-gear your rank and trivialize the
/// master fights. The cost ladder still gates affordability on top of this.
fn available_tiers(c: &Character, weapon: bool) -> Vec<(u8, u64)> {
    let current = if weapon { c.weapon_tier } else { c.armor_tier };
    let ceiling = c.level.min(data::COST_LADDER.len() as u8);
    (current + 1..=ceiling)
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
        let current = if weapon { c.weapon_tier } else { c.armor_tier };
        let msg = if current >= data::MAX_LEVEL {
            "You already wield the finest in the land. (nothing to buy)"
        } else {
            "Nothing here befits your rank yet. Advance a level for finer gear. (nothing to buy)"
        };
        return vec![(msg.into(), false)];
    }
    let name = if weapon {
        data::weapon_name
    } else {
        data::armor_name
    };
    tiers
        .into_iter()
        .map(|(tier, cost)| {
            (
                format!("{} (power {tier}) - {cost} gold", name(tier)),
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
        let healer = rows
            .iter()
            .find(|(l, _)| l.starts_with("The Mendery"))
            .unwrap();
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
        let mut c = lvl(2); // level 2 stocks tiers 1 and 2
        c.gold = 100; // affords tier 1 (48) but not tier 2 (189 after trade-in)
        let tiers = available_tiers(&c, true);
        assert_eq!(tiers[0], (1, 48));
        let menu = shop_menu(&c, true);
        assert!(menu[0].1); // tier 1 affordable
        assert!(!menu[1].1); // tier 2 not
    }

    #[test]
    fn shop_is_level_gated() {
        // Even with limitless gold, a shop only stocks gear up to your level.
        let mut c = lvl(3);
        c.gold = 1_000_000;
        let tiers = available_tiers(&c, true);
        assert!(tiers.iter().all(|(t, _)| *t <= 3));
        assert_eq!(tiers.last().unwrap().0, 3);
        // Out of upgrades for your rank shows the level-gated nudge, not "finest".
        c.weapon_tier = 3;
        let menu = shop_menu(&c, true);
        assert!(menu[0].0.contains("Advance a level"));
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

    #[test]
    fn village_menu_offers_mercenary_camp() {
        let rows = village_menu(&lvl(1));
        assert!(rows.iter().any(|(l, en)| l.starts_with("The Mercenary Camp") && *en));
    }

    #[test]
    fn mercenary_menu_lists_hires_and_gates_on_cost_and_dks() {
        let mut c = lvl(5);
        c.gold = 300; // affords only the cheapest (Stray Cur, 250)
        let menu = mercenary_menu(&c);
        // Cheapest hire is enabled.
        assert!(menu.iter().any(|(l, en)| l.starts_with("Hire Stray Cur") && *en));
        // A pricier one is still listed, but disabled.
        assert!(menu
            .iter()
            .any(|(l, en)| l.contains("Copper Sellsword") && !*en));
        // Dragon-kill-gated mercs don't appear at all with 0 kills.
        assert!(!menu.iter().any(|(l, _)| l.contains("Hedge Warden")));
    }

    #[test]
    fn mercenary_menu_shows_mend_row_for_wounded_companion() {
        let mut c = lvl(5);
        c.gold = 100_000;
        assert!(c.hire_mercenary(&data::MERCENARIES[1])); // Copper Sellsword
        c.companions[0].hitpoints = 1;
        let menu = mercenary_menu(&c);
        assert!(menu
            .iter()
            .any(|(l, en)| l.starts_with("Mend Copper Sellsword") && *en));
    }
}
