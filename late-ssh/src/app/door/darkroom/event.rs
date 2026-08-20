/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The engine below
 * is transcribed from `script/events.js`; the scenes it runs live in
 * `scenes_village`, `scenes_encounters` and `scenes_setpieces`. See
 * LICENSING.md and NOTICE. */

//! The scene machine and the fight.
//!
//! Upstream hangs closures off every scene and button, and drives fights with
//! live `setInterval` handles. This port keeps the same shape with two
//! changes the rest of the module depends on:
//!
//! - **Closures become closed enums.** A scene's side effects are an
//!   [`Effect`] list and its conditions a [`Condition`] list, both matched
//!   exhaustively, so a new one cannot be added without every arm being made
//!   to care.
//! - **Timers become countdowns.** Cooldowns and the enemy's swing are
//!   seconds held in [`Fight`], stepped by [`Active::tick`] from the session's
//!   wall clock. Nothing here runs on the render loop.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rand::Rng;

use super::data::{Building, Perk, Resource};
use super::model::{Deck, Expedition, Game, PendingReward, Thieves, View};
use super::world_data::{self, Damage, Tile, Weapon, WeaponKind};

/// Minutes between event rolls, uniform over upstream's `_EVENT_TIME_RANGE`.
pub const EVENT_TIME_RANGE: (u32, u32) = (3, 6);

// ---------------------------------------------------------------------------
// The static shape of an event
// ---------------------------------------------------------------------------

/// One event: a title and a graph of scenes, entered at `start`.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    /// Table key, used to name a parked fight in the save.
    pub key: &'static str,
    pub title: &'static str,
    /// Every condition must hold for the event to be offered.
    pub available: &'static [Condition],
    pub scenes: &'static [Scene],
}

impl Event {
    pub fn scene(&self, key: &str) -> Option<&'static Scene> {
        self.scenes.iter().find(|scene| scene.key == key)
    }
}

/// One screen of an event.
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    pub key: &'static str,
    pub text: &'static [&'static str],
    /// The line printed to the log when the scene loads.
    pub notification: Option<&'static str>,
    /// Stores handed over on load, no questions asked.
    pub reward: &'static [(Resource, i64)],
    pub on_load: &'static [Effect],
    /// Loot offered by a story scene (a fight's loot lives on the fight).
    pub loot: &'static [Loot],
    pub combat: Option<Combat>,
    pub buttons: &'static [Button],
}

impl Scene {
    /// A scene with nothing in it, for building the rest by struct update.
    pub const EMPTY: Scene = Scene {
        key: "",
        text: &[],
        notification: None,
        reward: &[],
        on_load: &[],
        loot: &[],
        combat: None,
        buttons: &[],
    };
}

/// One thing the player can choose.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub text: &'static str,
    pub cost: &'static [Cost],
    /// Every condition must hold for the row to be pressable.
    pub available: &'static [Condition],
    pub reward: &'static [(Resource, i64)],
    pub notification: Option<&'static str>,
    pub effects: &'static [Effect],
    pub next: Next,
}

impl Button {
    pub const EMPTY: Button = Button {
        text: "",
        cost: &[],
        available: &[],
        reward: &[],
        notification: None,
        effects: &[],
        next: Next::End,
    };

    /// The plain "leave" row every dead end ends with.
    pub const fn leave(text: &'static str) -> Button {
        Button {
            text,
            ..Button::EMPTY
        }
    }
}

/// Where a button goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Next {
    /// Close the modal.
    End,
    /// Stay on this scene (upstream's buttons with no `nextScene`).
    Stay,
    Scene(&'static str),
    /// Upstream's `{0.3: 'a', 1: 'b'}` roll: the first threshold the roll
    /// falls under, thresholds ascending.
    Weighted(&'static [(f64, &'static str)]),
    /// Hand off to another event outright (a setpiece within a setpiece).
    Event(&'static str),
}

/// What a button costs. Out in the world this comes out of the pack; at home,
/// out of the store room (upstream `Events.getQuantity`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cost {
    Store(Resource, i64),
    Water(i64),
    Hp(i64),
}

/// A row of loot, rolled when the scene opens.
#[derive(Clone, Copy, Debug)]
pub struct Loot {
    pub item: Resource,
    pub chance: f64,
    pub min: i64,
    pub max: i64,
}

/// A condition on one of the fighters. Upstream keeps a single free-form
/// string per fighter (`fighter.data('status')`), so these are mutually
/// exclusive: setting one clears whatever was there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// The next hit taken is absorbed and healed back instead. Breaks on that
    /// hit rather than on a clock.
    Shield,
    /// Swings at [`world_data::ENRAGE_ATTACK_DELAY`] whatever its own delay is.
    Enraged,
    /// Takes no damage and throws none, banking everything aimed at it, then
    /// returns the whole pile in one swing.
    Meditation,
    /// Its hits bleed afterwards.
    Venomous,
    /// Hits for [`world_data::ENERGISE_MULTIPLIER`] times as much.
    Energised,
    /// The stim: attack cooldowns halved.
    Boost,
}

impl Status {
    /// How long it lasts on its own. The ones that return `None` last until
    /// they are spent (the shield) or until the fight ends.
    pub fn duration(self) -> Option<f64> {
        match self {
            Status::Enraged => Some(world_data::ENRAGE_DURATION),
            Status::Meditation => Some(world_data::MEDITATE_DURATION),
            Status::Boost => Some(world_data::BOOST_DURATION),
            Status::Shield | Status::Venomous | Status::Energised => None,
        }
    }

    /// What the fight panel calls it.
    pub fn label(self) -> &'static str {
        match self {
            Status::Shield => "shield",
            Status::Enraged => "enraged",
            Status::Meditation => "meditation",
            Status::Venomous => "venomous",
            Status::Energised => "energised",
            Status::Boost => "boost",
        }
    }
}

/// A status riding on a fighter, with the clock that takes it off again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affliction {
    pub status: Status,
    /// Seconds left, for the statuses that wear off on their own.
    pub remaining: Option<f64>,
}

impl Affliction {
    fn new(status: Status) -> Affliction {
        Affliction {
            status,
            remaining: status.duration(),
        }
    }
}

/// Something the enemy does to itself on a timer, over and over. Upstream's
/// `scene.specials`, one `setInterval` apiece.
#[derive(Clone, Copy, Debug)]
pub struct Special {
    /// Seconds between firings.
    pub delay: f64,
    pub action: SpecialAction,
}

/// What a special does. Closed, so a fight cannot invent a new trick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialAction {
    /// Put this status on the enemy.
    Take(Status),
    /// The immortal wanderer: one of shield, enraged or meditation at random,
    /// never the same one twice running.
    RotateCommand,
}

/// The three the command deck's rotation draws from, in upstream's order.
const COMMAND_ROTATION: [Status; 3] = [Status::Shield, Status::Enraged, Status::Meditation];

/// A fight. Every number here is upstream's, straight off the scene.
#[derive(Clone, Copy, Debug)]
pub struct Combat {
    pub enemy: &'static str,
    /// The single character that stands for the enemy on screen.
    pub chara: char,
    pub health: i64,
    pub damage: i64,
    pub hit: f64,
    /// Seconds between the enemy's swings.
    pub attack_delay: f64,
    pub ranged: bool,
    pub death_message: &'static str,
    pub loot: &'static [Loot],
    /// Where the leave button goes once the spoils are taken.
    pub next: Next,
    /// Tricks the enemy pulls on a timer (upstream `specials`).
    pub specials: &'static [Special],
    /// Statuses the enemy takes the first time its health crosses a threshold
    /// downwards (upstream `atHealth`).
    pub at_health: &'static [(i64, Status)],
    /// What the enemy does to the wanderer when it dies, if it goes off
    /// instead of falling over (upstream `explosion`).
    pub explosion: Option<i64>,
}

impl Combat {
    /// A fight with no tricks: nothing on a timer, no threshold trigger, no
    /// death blast. Every fight outside the ravaged battleship is one of
    /// these, so they are built by struct update from here.
    pub const PLAIN: Combat = Combat {
        enemy: "",
        chara: ' ',
        health: 0,
        damage: 0,
        hit: 0.0,
        attack_delay: 0.0,
        ranged: false,
        death_message: "",
        loot: &[],
        next: Next::End,
        specials: &[],
        at_health: &[],
        explosion: None,
    };
}

/// A condition on an event or a button, all of them matched exhaustively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Condition {
    InRoom,
    InOutside,
    /// Either village panel: upstream's `Room || Outside` check.
    InVillage,
    /// Holding at least one.
    HasStore(Resource),
    StoreAtLeast(Resource, i64),
    StoreBelow(Resource, i64),
    HutsAtLeast(u32),
    HutsBelow(u32),
    PopulationOver(u32),
    PopulationBelow(u32),
    /// The wasteland exists, which is what wakes the scout and the master.
    WorldUnlocked,
    CityCleared,
    ThievesActive,
    LacksPerk(Perk),
    /// There is still map left to uncover.
    MapNotFull,
    HasTrap,
    // ---- the wasteland ----
    DistanceAtMost(i32),
    DistanceOver(i32),
    TerrainIs(Tile),
    /// This deck of the battleship has not been picked clean yet, which is
    /// what keeps its elevator button on the antechamber's wall.
    DeckPending(Deck),
    /// All three decks report clear, which unlocks the command deck.
    DecksClear,
}

/// How many villagers a disaster takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kill {
    /// `floor(random * span) + low`, upstream's shape.
    Range(u32, u32),
    /// `floor(random * floor(pop / 2)) + 1`.
    HalfPopulation,
}

/// Everything a scene or button can do beyond moving stores. One arm per
/// upstream closure; there is deliberately no general-purpose escape hatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// The store room raid: a tenth of the wood becomes a fifth as much of
    /// something else.
    TradeWoodFor(Resource),
    BuildHut,
    WreckTraps,
    DestroyHuts(u32),
    KillVillagers(Kill),
    GrantPerk(Perk),
    /// The thief is dealt with: the skim stops, and either the missing stores
    /// come back or a lesson does.
    SettleThieves {
        pay_back: bool,
        perk: Option<Perk>,
    },
    /// The Mysterious Wanderer's return: rolled now, paid later on wall clock.
    ScheduleReward {
        chance: f64,
        resource: Resource,
        amount: i64,
        delay_secs: u32,
        message: &'static str,
    },
    /// The scout's map: light up a patch of the world at random.
    UncoverMap,
    // ---- the wasteland ----
    /// Turn this square into an outpost and run a road home.
    ClearDungeon,
    /// Run a road home without changing the square (the mines, the ship).
    DrawRoad,
    /// Top the waterskin up from a well.
    RefillWater,
    /// This setpiece is played out; the square stops offering it.
    MarkVisited,
    /// A mine, handed to the village on a safe return.
    GrantBuilding(Building),
    /// The crashed starship: the ship tab opens on a safe return.
    FoundShip,
    /// Drink an outpost dry.
    UseOutpost,
    /// The city falls, and the soldiers come looking.
    ClearCity,
    // ---- the ravaged battleship ----
    /// The entrance turret is down and the strange device is taken: every
    /// later visit opens on the antechamber, and the fabricator comes home on
    /// a safe return.
    EnterBattleship,
    /// One of the three decks is picked clean; its elevator stops offering.
    ClearDeck(Deck),
    /// The battleship's regenerative machines: back to full health.
    HealFull,
}

// ---------------------------------------------------------------------------
// The running event
// ---------------------------------------------------------------------------

/// What everything in here reads and writes: the save, the trip if there is
/// one, the panel the player is on, and the wall clock.
pub struct Ctx<'a> {
    pub game: &'a mut Game,
    pub trip: Option<&'a mut Expedition>,
    pub view: View,
    pub now: DateTime<Utc>,
}

impl Ctx<'_> {
    /// The read-only view of the same thing, for rendering and for the checks
    /// that run before anything is spent.
    pub fn look(&self) -> Look<'_> {
        Look {
            game: self.game,
            trip: self.trip.as_deref(),
            view: self.view,
        }
    }

    pub fn quantity(&self, item: Resource) -> i64 {
        self.look().quantity(item)
    }

    fn add(&mut self, item: Resource, amount: i64) {
        match &mut self.trip {
            Some(trip) => trip.add(item, amount),
            None => self.game.add_store(item, amount),
        }
    }
}

/// Everything the modal needs to *read*. Kept apart from [`Ctx`] so the
/// renderer never has to hold a mutable borrow (or clone the save) just to
/// list the rows.
#[derive(Clone, Copy)]
pub struct Look<'a> {
    pub game: &'a Game,
    pub trip: Option<&'a Expedition>,
    pub view: View,
}

impl Look<'_> {
    /// How much of something is to hand. Out in the world that means the
    /// pack; at home it means the store room.
    pub fn quantity(&self, item: Resource) -> i64 {
        match self.trip {
            Some(trip) => trip.carrying(item),
            None => self.game.store(item),
        }
    }

    /// Free space in the pack. At home there is no pack, so nothing binds.
    pub fn free_space(&self) -> f64 {
        match self.trip {
            Some(trip) => self.game.capacity() - trip.load(),
            None => f64::MAX,
        }
    }
}

/// A live fight.
#[derive(Clone, Debug)]
pub struct Fight {
    /// The stat line this fight is being run against. Held here so the fight
    /// can answer for itself what its tricks and thresholds are.
    pub combat: Combat,
    pub enemy_hp: i64,
    pub enemy_max: i64,
    /// Seconds until the enemy swings again.
    pub enemy_timer: f64,
    /// Seconds left of a bolas tangle.
    pub stun: f64,
    pub weapon_cooldown: BTreeMap<Weapon, f64>,
    pub eat_cooldown: f64,
    pub meds_cooldown: f64,
    pub hypo_cooldown: f64,
    pub stim_cooldown: f64,
    pub shield_cooldown: f64,
    /// What the enemy is currently under, and what the wanderer is.
    pub enemy_status: Option<Affliction>,
    pub player_status: Option<Affliction>,
    /// Seconds until each of the scene's specials fires again, one per entry
    /// in `Combat::specials`.
    pub special_timers: Vec<f64>,
    /// The last status the command-deck rotation picked, so it never picks the
    /// same one twice running (upstream `Events._lastSpecial`).
    pub last_special: Option<Status>,
    /// Damage a meditating enemy has banked and will throw back.
    pub banked: i64,
    /// A venom bleed on the wanderer: damage a tick, and the seconds until the
    /// next one. Upstream's `_dotTimer` runs until the fight ends, so this
    /// does too.
    pub bleed: Option<(i64, f64)>,
    /// Health thresholds this fight's `at_health` triggers have already fired
    /// on, so each fires once.
    pub triggered: Vec<i64>,
    /// The last thing that happened, shown beside the fighters.
    pub last_hit: Option<String>,
}

impl Fight {
    /// A fight at its first frame.
    fn start(combat: Combat, enemy_hp: i64) -> Fight {
        Fight {
            combat,
            enemy_hp,
            enemy_max: combat.health,
            enemy_timer: combat.attack_delay,
            stun: 0.0,
            weapon_cooldown: BTreeMap::new(),
            eat_cooldown: 0.0,
            meds_cooldown: 0.0,
            hypo_cooldown: 0.0,
            stim_cooldown: 0.0,
            shield_cooldown: 0.0,
            enemy_status: None,
            player_status: None,
            special_timers: combat.specials.iter().map(|s| s.delay).collect(),
            last_special: None,
            banked: 0,
            bleed: None,
            triggered: Vec::new(),
            last_hit: None,
        }
    }

    fn has_enemy_status(&self, status: Status) -> bool {
        self.enemy_status.is_some_and(|held| held.status == status)
    }

    fn has_player_status(&self, status: Status) -> bool {
        self.player_status.is_some_and(|held| held.status == status)
    }

    /// Fire any `at_health` trigger this hit crossed downwards. Upstream
    /// checks `hp <= threshold && hp + dmg > threshold`, so each fires once;
    /// `triggered` is what keeps that true when a shielded heal walks the
    /// enemy back up over the line.
    fn check_thresholds(&mut self, before: i64) {
        for (threshold, status) in self.pending_thresholds(before) {
            self.triggered.push(threshold);
            self.take_status(status);
        }
    }

    /// Put a status on the enemy, with the two side effects upstream's
    /// `setStatus` carries: going enraged restarts its swing clock on the
    /// short delay rather than waiting out the current one, and starting to
    /// meditate empties whatever it was already holding.
    fn take_status(&mut self, status: Status) {
        self.enemy_status = Some(Affliction::new(status));
        match status {
            Status::Enraged => self.enemy_timer = world_data::ENRAGE_ATTACK_DELAY,
            Status::Meditation => self.banked = 0,
            Status::Shield | Status::Venomous | Status::Energised | Status::Boost => {}
        }
        self.last_hit = Some(status.label().to_string());
    }

    fn pending_thresholds(&self, before: i64) -> Vec<(i64, Status)> {
        self.combat
            .at_health
            .iter()
            .filter(|(threshold, _)| {
                self.enemy_hp <= *threshold
                    && before > *threshold
                    && !self.triggered.contains(threshold)
            })
            .copied()
            .collect()
    }
}

/// Which part of a scene is on screen.
#[derive(Clone, Debug)]
pub enum Phase {
    Story,
    /// Boxed because a live fight carries far more than any other phase (a
    /// stat line, three timer collections and two statuses), and an unboxed
    /// variant would make every `Phase` that size.
    Fighting(Box<Fight>),
    /// The enemy is dead but has not finished dying: an unstable automaton
    /// sits there for a few seconds and then goes off in the wanderer's face.
    Exploding {
        timer: f64,
        damage: i64,
    },
    /// The enemy is down: the loot rows and a way out.
    Spoils {
        leave_cooldown: f64,
    },
    /// Selecting an item in the pack to drop in order to fit a target loot item.
    DropFor {
        loot_index: usize,
    },
}

/// One row of the modal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    Button(usize),
    Attack(Weapon),
    Eat,
    Meds,
    /// A hypo out of the fabricator: the same shape as medicine, more of it.
    Hypo,
    /// A stim: halves attack cooldowns for a few seconds, at a cost in blood.
    Stim,
    /// The kinetic armour's shield: absorb and heal the next hit taken.
    Shield,
    /// Take one of the loot row at this index.
    Take(usize),
    /// Take everything that fits.
    TakeAll,
    /// Drop `count` of `item` from pack to fit target loot.
    Drop {
        item: Resource,
        count: i64,
    },
    /// Cancel drop selection and return to spoils/story.
    DropCancel,
    Leave,
}

/// A rolled loot row waiting to be picked up.
#[derive(Clone, Copy, Debug)]
pub struct LootRow {
    pub item: Resource,
    pub left: i64,
}

/// What acting on the modal did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The modal stays up.
    Continue,
    /// The modal is finished.
    Done,
    /// The wanderer died out there.
    Died,
}

/// The event currently on screen.
pub struct Active {
    pub event: &'static Event,
    pub scene: &'static Scene,
    pub phase: Phase,
    pub loot: Vec<LootRow>,
}

impl Active {
    /// Open an event at its `start` scene.
    pub fn start(
        event: &'static Event,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) -> Active {
        let scene = event.scene("start").unwrap_or(&Scene::EMPTY);
        let mut active = Active {
            event,
            scene,
            phase: Phase::Story,
            loot: Vec::new(),
        };
        active.load_scene(scene, ctx, rng, out);
        active
    }

    /// Re-open a parked fight: the same scene, with the enemy where it was.
    pub fn resume(event: &'static Event, scene: &'static Scene, enemy_hp: i64) -> Active {
        let phase = match scene.combat {
            Some(combat) => Phase::Fighting(Box::new(Fight::start(combat, enemy_hp))),
            None => Phase::Story,
        };
        Active {
            event,
            scene,
            phase,
            loot: Vec::new(),
        }
    }

    /// The fight in progress, for the save.
    pub fn fight(&self) -> Option<&Fight> {
        match &self.phase {
            Phase::Fighting(fight) => Some(fight.as_ref()),
            // A blast already on its way is not worth parking: the enemy is
            // dead, and resuming would mean either dodging damage that was
            // earned or taking it twice.
            Phase::Exploding { .. }
            | Phase::Story
            | Phase::Spoils { .. }
            | Phase::DropFor { .. } => None,
        }
    }

    fn load_scene(
        &mut self,
        scene: &'static Scene,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) {
        self.scene = scene;
        self.loot = Vec::new();
        for effect in scene.on_load {
            apply(*effect, ctx, rng, out);
        }
        if let Some(message) = scene.notification {
            out.push(message.to_string());
        }
        for (resource, amount) in scene.reward {
            ctx.add(*resource, *amount);
        }
        match scene.combat {
            Some(combat) => {
                self.phase = Phase::Fighting(Box::new(Fight::start(combat, combat.health)));
            }
            None => {
                self.phase = Phase::Story;
                self.loot = roll_loot(scene.loot, rng);
            }
        }
    }

    /// The rows on screen, in display order.
    pub fn rows(&self, look: &Look<'_>) -> Vec<Row> {
        let mut rows = Vec::new();
        match &self.phase {
            Phase::Fighting(_) => {
                for weapon in available_weapons(look) {
                    rows.push(Row::Attack(weapon));
                }
                if look.quantity(Resource::CuredMeat) > 0 {
                    rows.push(Row::Eat);
                }
                if look.quantity(Resource::Medicine) > 0 {
                    rows.push(Row::Meds);
                }
                if look.quantity(Resource::Hypo) > 0 {
                    rows.push(Row::Hypo);
                }
                if look.quantity(Resource::Stim) > 0 {
                    rows.push(Row::Stim);
                }
                // The shield is the armour's, not the pack's: upstream reads
                // the store room, so it works whether or not it was packed.
                if look.game.store(Resource::KineticArmour) > 0 {
                    rows.push(Row::Shield);
                }
            }
            // The blast is on its way and nothing can be done about it.
            Phase::Exploding { .. } => {}
            Phase::DropFor { loot_index } => {
                if let Some(target) = self.loot.get(*loot_index) {
                    let needed = world_data::weight(target.item) - look.free_space();
                    if let Some(trip) = look.trip {
                        for item in world_data::CARRYABLE {
                            let count = trip.carrying(item);
                            if count > 0 && item != target.item {
                                let item_weight = world_data::weight(item);
                                if item_weight > 0.0 {
                                    let num_to_drop = (needed / item_weight).ceil() as i64;
                                    if num_to_drop <= count {
                                        rows.push(Row::Drop {
                                            item,
                                            count: num_to_drop.max(1),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                rows.push(Row::DropCancel);
            }
            Phase::Story | Phase::Spoils { .. } => {
                for (index, _) in self.loot.iter().enumerate() {
                    rows.push(Row::Take(index));
                }
                if !self.loot.is_empty() {
                    rows.push(Row::TakeAll);
                }
                if matches!(self.phase, Phase::Spoils { .. }) && self.scene.buttons.is_empty() {
                    rows.push(Row::Leave);
                } else {
                    for (index, _) in self.scene.buttons.iter().enumerate() {
                        rows.push(Row::Button(index));
                    }
                }
                if !rows.contains(&Row::Leave) && !rows.iter().any(|row| self.row_ready(*row, look))
                {
                    // A dead-end scene, or one whose every row is cost-gated
                    // out of reach (the burning junction on an empty
                    // canteen): always leavable. A browser player could
                    // refresh the page out of that corner; over SSH this row
                    // is the only door.
                    rows.push(Row::Leave);
                }
            }
        }
        rows
    }

    /// Whether a row can be pressed right now.
    pub fn row_ready(&self, row: Row, look: &Look<'_>) -> bool {
        match row {
            Row::Button(index) => match self.scene.buttons.get(index) {
                Some(button) => {
                    button.available.iter().all(|c| holds(*c, look))
                        && button.cost.iter().all(|cost| affordable(*cost, look))
                }
                None => false,
            },
            Row::Attack(weapon) => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.weapon_cooldown.get(&weapon).copied().unwrap_or(0.0) <= 0.0
                        && weapon_loaded(weapon, look)
                }
                Phase::Story
                | Phase::Exploding { .. }
                | Phase::Spoils { .. }
                | Phase::DropFor { .. } => false,
            },
            Row::Eat => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.eat_cooldown <= 0.0 && look.quantity(Resource::CuredMeat) > 0
                }
                Phase::Exploding { .. } => false,
                Phase::Story | Phase::Spoils { .. } | Phase::DropFor { .. } => {
                    look.quantity(Resource::CuredMeat) > 0
                }
            },
            Row::Meds => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.meds_cooldown <= 0.0 && look.quantity(Resource::Medicine) > 0
                }
                Phase::Exploding { .. } => false,
                Phase::Story | Phase::Spoils { .. } | Phase::DropFor { .. } => {
                    look.quantity(Resource::Medicine) > 0
                }
            },
            Row::Hypo => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.hypo_cooldown <= 0.0 && look.quantity(Resource::Hypo) > 0
                }
                Phase::Exploding { .. } => false,
                Phase::Story | Phase::Spoils { .. } | Phase::DropFor { .. } => {
                    look.quantity(Resource::Hypo) > 0
                }
            },
            // The stim costs blood, so it refuses at the point where taking
            // it would be what killed the wanderer.
            Row::Stim => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.stim_cooldown <= 0.0
                        && look.quantity(Resource::Stim) > 0
                        && look
                            .trip
                            .is_some_and(|trip| trip.hp > world_data::BOOST_DAMAGE)
                }
                Phase::Story
                | Phase::Exploding { .. }
                | Phase::Spoils { .. }
                | Phase::DropFor { .. } => false,
            },
            Row::Shield => match &self.phase {
                Phase::Fighting(fight) => {
                    fight.shield_cooldown <= 0.0 && look.game.store(Resource::KineticArmour) > 0
                }
                Phase::Story
                | Phase::Exploding { .. }
                | Phase::Spoils { .. }
                | Phase::DropFor { .. } => false,
            },
            Row::Take(index) => match self.loot.get(index) {
                Some(row) => {
                    if row.left <= 0 {
                        false
                    } else if world_data::weight(row.item) <= look.free_space() {
                        true
                    } else if let Some(trip) = look.trip {
                        let target_weight = world_data::weight(row.item);
                        world_data::CARRYABLE.iter().any(|item| {
                            let count = trip.carrying(*item);
                            if count > 0 && *item != row.item {
                                let total_item_weight = count as f64 * world_data::weight(*item);
                                total_item_weight + look.free_space() >= target_weight
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    }
                }
                None => false,
            },
            Row::TakeAll => self
                .loot
                .iter()
                .any(|row| row.left > 0 && world_data::weight(row.item) <= look.free_space()),
            Row::Drop { item, count } => look.quantity(item) >= count,
            Row::DropCancel => true,
            Row::Leave => match &self.phase {
                Phase::Spoils { leave_cooldown } => *leave_cooldown <= 0.0,
                // Walking out mid-blast would be a free escape from damage
                // that has already been earned.
                Phase::Exploding { .. } => false,
                Phase::Story | Phase::Fighting(_) | Phase::DropFor { .. } => true,
            },
        }
    }

    /// Act on a row.
    pub fn press(
        &mut self,
        row: Row,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) -> Outcome {
        if !self.row_ready(row, &ctx.look()) {
            return Outcome::Continue;
        }
        match row {
            Row::Button(index) => {
                let Some(button) = self.scene.buttons.get(index) else {
                    return Outcome::Done;
                };
                let button = *button;
                for cost in button.cost {
                    spend(*cost, ctx);
                }
                for effect in button.effects {
                    apply(*effect, ctx, rng, out);
                }
                for (resource, amount) in button.reward {
                    ctx.add(*resource, *amount);
                }
                if let Some(message) = button.notification {
                    out.push(message.to_string());
                }
                self.follow(button.next, ctx, rng, out)
            }
            Row::Attack(weapon) => self.attack(weapon, ctx, rng, out),
            Row::Eat => {
                let healed = ctx.game.meat_heal();
                self.heal(Resource::CuredMeat, healed, ctx);
                Outcome::Continue
            }
            Row::Meds => {
                self.heal(Resource::Medicine, world_data::MEDS_HEAL, ctx);
                Outcome::Continue
            }
            Row::Hypo => {
                self.heal(Resource::Hypo, world_data::HYPO_HEAL, ctx);
                Outcome::Continue
            }
            // Upstream's stim: the boost goes on and the wanderer pays for it
            // in blood on the spot. `row_ready` refuses when that blood is all
            // there is, so this can never be the thing that kills them.
            Row::Stim => {
                let Phase::Fighting(fight) = &mut self.phase else {
                    return Outcome::Continue;
                };
                fight.stim_cooldown = world_data::STIM_COOLDOWN;
                fight.player_status = Some(Affliction::new(Status::Boost));
                fight.last_hit = Some(format!("-{}", world_data::BOOST_DAMAGE));
                if let Some(trip) = &mut ctx.trip {
                    trip.add(Resource::Stim, -1);
                    trip.hp = (trip.hp - world_data::BOOST_DAMAGE).max(0);
                    if trip.hp == 0 {
                        return Outcome::Died;
                    }
                }
                Outcome::Continue
            }
            // The kinetic armour's shield. It costs nothing but the cooldown:
            // the armour is worn, not spent.
            Row::Shield => {
                if let Phase::Fighting(fight) = &mut self.phase {
                    fight.shield_cooldown = world_data::SHIELD_COOLDOWN;
                    fight.player_status = Some(Affliction::new(Status::Shield));
                    fight.last_hit = Some(Status::Shield.label().to_string());
                }
                Outcome::Continue
            }
            Row::Take(index) => {
                let Some(target) = self.loot.get(index) else {
                    return Outcome::Continue;
                };
                if world_data::weight(target.item) <= ctx.look().free_space() {
                    self.take_one(index, ctx);
                } else {
                    self.phase = Phase::DropFor { loot_index: index };
                }
                Outcome::Continue
            }
            Row::TakeAll => {
                for index in 0..self.loot.len() {
                    while let Some(target) = self.loot.get(index) {
                        if target.left > 0
                            && world_data::weight(target.item) <= ctx.look().free_space()
                        {
                            self.take_one(index, ctx);
                        } else {
                            break;
                        }
                    }
                }
                Outcome::Continue
            }
            Row::Drop { item, count } => {
                if let Phase::DropFor { loot_index } = self.phase {
                    if let Some(trip) = &mut ctx.trip {
                        trip.add(item, -count);
                    }
                    if let Some(existing) = self.loot.iter_mut().find(|r| r.item == item) {
                        existing.left += count;
                    } else {
                        self.loot.push(LootRow { item, left: count });
                    }
                    self.take_one(loot_index, ctx);
                    self.phase = match self.scene.combat {
                        Some(_) => Phase::Spoils {
                            leave_cooldown: 0.0,
                        },
                        None => Phase::Story,
                    };
                }
                Outcome::Continue
            }
            Row::DropCancel => {
                if matches!(self.phase, Phase::DropFor { .. }) {
                    self.phase = match self.scene.combat {
                        Some(_) => Phase::Spoils {
                            leave_cooldown: 0.0,
                        },
                        None => Phase::Story,
                    };
                }
                Outcome::Continue
            }
            Row::Leave => match &self.phase {
                Phase::Spoils { .. } => {
                    let next = self
                        .scene
                        .combat
                        .map(|combat| combat.next)
                        .unwrap_or(Next::End);
                    self.follow(next, ctx, rng, out)
                }
                Phase::Story
                | Phase::Fighting(_)
                | Phase::Exploding { .. }
                | Phase::DropFor { .. } => Outcome::Done,
            },
        }
    }

    fn follow(
        &mut self,
        next: Next,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) -> Outcome {
        match next {
            Next::End => Outcome::Done,
            Next::Stay => Outcome::Continue,
            Next::Scene(key) => match self.event.scene(key) {
                Some(scene) => {
                    self.load_scene(scene, ctx, rng, out);
                    Outcome::Continue
                }
                None => Outcome::Done,
            },
            Next::Weighted(table) => {
                let roll: f64 = rng.r#gen();
                let key = table
                    .iter()
                    .find(|(threshold, _)| roll < *threshold)
                    .map(|(_, key)| *key);
                match key.and_then(|key| self.event.scene(key)) {
                    Some(scene) => {
                        self.load_scene(scene, ctx, rng, out);
                        Outcome::Continue
                    }
                    None => Outcome::Done,
                }
            }
            Next::Event(key) => match super::scenes_setpieces::by_key(key)
                .or_else(|| super::scenes_executioner::by_key(key))
            {
                Some(event) => {
                    self.event = event;
                    let scene = event.scene("start").unwrap_or(&Scene::EMPTY);
                    self.load_scene(scene, ctx, rng, out);
                    Outcome::Continue
                }
                None => Outcome::Done,
            },
        }
    }

    // ---- the fight ----

    fn attack(
        &mut self,
        weapon: Weapon,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) -> Outcome {
        let Phase::Fighting(fight) = &mut self.phase else {
            return Outcome::Continue;
        };
        if let Some((ammo, amount)) = weapon.cost() {
            match &mut ctx.trip {
                Some(trip) => trip.add(ammo, -amount),
                None => ctx.game.add_store(ammo, -amount),
            }
        }
        // The unarmed master punches twice as fast (upstream halves the
        // button's cooldown in `createAttackButton`), and a stim halves it
        // again for as long as it lasts.
        let mut cooldown = weapon.cooldown();
        if weapon.kind() == WeaponKind::Unarmed && ctx.game.has_perk(Perk::UnarmedMaster) {
            cooldown /= 2.0;
        }
        if fight.has_player_status(Status::Boost) {
            cooldown /= 2.0;
        }
        fight.weapon_cooldown.insert(weapon, cooldown);

        let landed = rng.r#gen::<f64>() <= ctx.game.hit_chance();
        match (landed, weapon.damage()) {
            (false, _) => fight.last_hit = Some("miss".to_string()),
            (true, Damage::Stun) => {
                fight.stun = world_data::STUN_DURATION;
                fight.last_hit = Some("stunned".to_string());
            }
            (true, Damage::Hits(base)) => {
                let damage = perk_damage(base, weapon.kind(), ctx.game);
                strike_enemy(fight, damage);
            }
        }
        // Punching enough things teaches you to punch.
        if weapon.kind() == WeaponKind::Unarmed {
            train_fists(ctx.game, out);
        }
        if fight.enemy_hp <= 0 {
            self.kill(ctx, rng, out);
        }
        Outcome::Continue
    }

    /// The enemy is out of health. Most of them fall over; an unstable
    /// automaton sits there for a moment and then goes off.
    fn kill(&mut self, ctx: &mut Ctx<'_>, rng: &mut impl Rng, out: &mut Vec<String>) {
        // Whatever happens next, the fight itself is over: a parked snapshot
        // would put the enemy back on its feet.
        if let Some(trip) = &mut ctx.trip {
            trip.combat = None;
        }
        match self.scene.combat.and_then(|combat| combat.explosion) {
            Some(damage) => {
                self.phase = Phase::Exploding {
                    timer: world_data::EXPLOSION_DELAY,
                    damage,
                };
            }
            None => self.win(ctx, rng, out),
        }
    }

    fn win(&mut self, ctx: &mut Ctx<'_>, rng: &mut impl Rng, out: &mut Vec<String>) {
        let Some(combat) = self.scene.combat else {
            return;
        };
        // Setpiece fights carry no death line of their own; only the wandering
        // encounters announce the kill.
        if !combat.death_message.is_empty() {
            out.push(combat.death_message.to_string());
        }
        self.loot = roll_loot(combat.loot, rng);
        self.phase = Phase::Spoils {
            leave_cooldown: world_data::LEAVE_COOLDOWN,
        };
        // A won fight is not worth resuming; drop the parked snapshot.
        if let Some(trip) = &mut ctx.trip {
            trip.combat = None;
        }
    }

    /// One step of the wall clock: enemy swings, cooldowns run down.
    pub fn tick(
        &mut self,
        seconds: f64,
        ctx: &mut Ctx<'_>,
        rng: &mut impl Rng,
        out: &mut Vec<String>,
    ) -> Outcome {
        match &mut self.phase {
            Phase::Story | Phase::DropFor { .. } => Outcome::Continue,
            Phase::Spoils { leave_cooldown } => {
                *leave_cooldown = (*leave_cooldown - seconds).max(0.0);
                Outcome::Continue
            }
            // The enemy is dead and about to take the wanderer with it. The
            // blast lands whether or not the session is still watching, which
            // is why it is a phase and not a timeout.
            Phase::Exploding { timer, damage } => {
                *timer -= seconds;
                if *timer > 0.0 {
                    return Outcome::Continue;
                }
                let damage = *damage;
                let mut died = false;
                if let Some(trip) = &mut ctx.trip {
                    trip.hp = (trip.hp - damage).max(0);
                    died = trip.hp == 0;
                }
                if died {
                    return Outcome::Died;
                }
                self.win(ctx, rng, out);
                Outcome::Continue
            }
            Phase::Fighting(fight) => {
                for cooldown in fight.weapon_cooldown.values_mut() {
                    *cooldown = (*cooldown - seconds).max(0.0);
                }
                fight.eat_cooldown = (fight.eat_cooldown - seconds).max(0.0);
                fight.meds_cooldown = (fight.meds_cooldown - seconds).max(0.0);
                fight.hypo_cooldown = (fight.hypo_cooldown - seconds).max(0.0);
                fight.stim_cooldown = (fight.stim_cooldown - seconds).max(0.0);
                fight.shield_cooldown = (fight.shield_cooldown - seconds).max(0.0);
                fight.stun = (fight.stun - seconds).max(0.0);
                expire(&mut fight.enemy_status, seconds);
                expire(&mut fight.player_status, seconds);

                let Some(combat) = self.scene.combat else {
                    return Outcome::Continue;
                };

                // The enemy's tricks, each on its own clock.
                for index in 0..fight.special_timers.len() {
                    let Some(special) = combat.specials.get(index) else {
                        continue;
                    };
                    fight.special_timers[index] -= seconds;
                    if fight.special_timers[index] > 0.0 {
                        continue;
                    }
                    fight.special_timers[index] += special.delay;
                    let status = match special.action {
                        SpecialAction::Take(status) => status,
                        SpecialAction::RotateCommand => rotate_command(fight.last_special, rng),
                    };
                    fight.last_special = Some(status);
                    fight.take_status(status);
                }

                // A venom bite keeps bleeding for the rest of the fight.
                if let Some((damage, timer)) = &mut fight.bleed {
                    *timer -= seconds;
                    if *timer <= 0.0 {
                        *timer += world_data::DOT_TICK;
                        let damage = *damage;
                        fight.last_hit = Some(format!("-{damage}"));
                        let mut died = false;
                        if let Some(trip) = &mut ctx.trip {
                            trip.hp = (trip.hp - damage).max(0);
                            died = trip.hp == 0;
                        }
                        if died {
                            return Outcome::Died;
                        }
                    }
                }

                let Phase::Fighting(fight) = &mut self.phase else {
                    return Outcome::Continue;
                };
                fight.enemy_timer -= seconds;
                if fight.enemy_timer > 0.0 {
                    return Outcome::Continue;
                }
                // An enraged enemy swings on its own short clock rather than
                // on the scene's.
                fight.enemy_timer = match fight.has_enemy_status(Status::Enraged) {
                    true => world_data::ENRAGE_ATTACK_DELAY,
                    false => combat.attack_delay,
                };
                // A tangled enemy does not swing, and neither does a
                // meditating one: it is busy holding the damage it is owed.
                if fight.stun > 0.0 || fight.has_enemy_status(Status::Meditation) {
                    return Outcome::Continue;
                }

                // Everything banked while meditating comes back in one swing,
                // and it never misses (upstream skips the to-hit roll for it).
                let mut damage = if fight.banked > 0 {
                    std::mem::take(&mut fight.banked)
                } else {
                    // The evasive perk makes them miss more often.
                    let to_hit = match ctx.game.has_perk(Perk::Evasive) {
                        true => combat.hit * 0.8,
                        false => combat.hit,
                    };
                    if rng.r#gen::<f64>() > to_hit {
                        fight.last_hit = Some("miss".to_string());
                        return Outcome::Continue;
                    }
                    combat.damage
                };
                if fight.has_enemy_status(Status::Energised) {
                    damage *= world_data::ENERGISE_MULTIPLIER;
                }

                // A shield takes the hit and heals it back instead, then
                // breaks. Upstream shields break in one hit, whatever it was.
                if fight.has_player_status(Status::Shield) {
                    fight.player_status = None;
                    fight.last_hit = Some(format!("+{damage}"));
                    let max = ctx.game.max_health();
                    if let Some(trip) = &mut ctx.trip {
                        trip.hp = (trip.hp + damage).min(max);
                    }
                    return Outcome::Continue;
                }

                // A venomous enemy's hits keep bleeding afterwards.
                if fight.has_enemy_status(Status::Venomous) {
                    fight.bleed = Some((damage / 2, world_data::DOT_TICK));
                }

                fight.last_hit = Some(format!("-{damage}"));
                // Fights only ever happen out in the world; there is nothing
                // at home for an enemy to hurt.
                if let Some(trip) = &mut ctx.trip {
                    trip.hp = (trip.hp - damage).max(0);
                    if trip.hp == 0 {
                        return Outcome::Died;
                    }
                }
                let _ = out;
                Outcome::Continue
            }
        }
    }

    fn heal(&mut self, item: Resource, amount: i64, ctx: &mut Ctx<'_>) {
        if ctx.quantity(item) <= 0 {
            return;
        }
        let max = ctx.game.max_health();
        let Some(trip) = &mut ctx.trip else {
            return;
        };
        if trip.hp >= max {
            return;
        }
        trip.add(item, -1);
        trip.hp = (trip.hp + amount).min(max);
        if let Phase::Fighting(fight) = &mut self.phase {
            match item {
                Resource::CuredMeat => fight.eat_cooldown = world_data::EAT_COOLDOWN,
                _ => fight.meds_cooldown = world_data::MEDS_COOLDOWN,
            }
            fight.last_hit = Some(format!("+{amount}"));
        }
    }

    fn take_one(&mut self, index: usize, ctx: &mut Ctx<'_>) {
        let Some(row) = self.loot.get_mut(index) else {
            return;
        };
        if row.left <= 0 {
            return;
        }
        row.left -= 1;
        let item = row.item;
        ctx.add(item, 1);
    }
}

// ---------------------------------------------------------------------------
// Choosing what happens
// ---------------------------------------------------------------------------

/// Seconds until the next event roll.
pub fn next_event_delay(rng: &mut impl Rng) -> f64 {
    let (low, high) = EVENT_TIME_RANGE;
    let minutes = (rng.r#gen::<f64>() * f64::from(high - low)).floor() + f64::from(low);
    minutes * 60.0
}

/// Pick an event whose conditions hold, or nothing if none do.
pub fn pick(pool: &'static [Event], look: &Look<'_>, rng: &mut impl Rng) -> Option<&'static Event> {
    let possible: Vec<&'static Event> = pool
        .iter()
        .filter(|event| event.available.iter().all(|c| holds(*c, look)))
        .collect();
    if possible.is_empty() {
        return None;
    }
    let index = (rng.r#gen::<f64>() * possible.len() as f64).floor() as usize;
    possible.get(index.min(possible.len() - 1)).copied()
}

/// Whether a condition holds right now.
pub fn holds(condition: Condition, look: &Look<'_>) -> bool {
    let game = look.game;
    match condition {
        Condition::InRoom => look.view == View::Room,
        Condition::InOutside => look.view == View::Outside,
        Condition::InVillage => matches!(look.view, View::Room | View::Outside),
        Condition::HasStore(item) => game.store(item) > 0,
        Condition::StoreAtLeast(item, n) => game.store(item) >= n,
        Condition::StoreBelow(item, n) => game.store(item) < n,
        Condition::HutsAtLeast(n) => game.building_count(Building::Hut) >= n,
        Condition::HutsBelow(n) => game.building_count(Building::Hut) < n,
        Condition::PopulationOver(n) => game.population > n,
        Condition::PopulationBelow(n) => game.population < n,
        Condition::WorldUnlocked => game.path_unlocked,
        Condition::CityCleared => game.city_cleared,
        Condition::ThievesActive => game.thieves == Thieves::Active,
        Condition::LacksPerk(perk) => !game.has_perk(perk),
        Condition::MapNotFull => !game.seen_all_map,
        Condition::HasTrap => game.building_count(Building::Trap) > 0,
        Condition::DistanceAtMost(n) => look.trip.is_some_and(|trip| trip.distance() <= n),
        Condition::DistanceOver(n) => look.trip.is_some_and(|trip| trip.distance() > n),
        Condition::TerrainIs(tile) => look
            .trip
            .is_some_and(|trip| trip.map.tile(trip.x, trip.y) == tile),
        Condition::DeckPending(deck) => look
            .trip
            .is_some_and(|trip| !trip.battleship.decks.contains(&deck)),
        Condition::DecksClear => look.trip.is_some_and(|trip| trip.battleship.decks_clear()),
    }
}

/// Whether a cost is waived outright. The glow stone's light never goes out,
/// so anything that would burn a torch is free while it is in the pack
/// (upstream deletes `cost.torch` wherever a button's cost is read).
fn waived(cost: Cost, look: &Look<'_>) -> bool {
    matches!(cost, Cost::Store(Resource::Torch, _)) && look.quantity(Resource::Glowstone) > 0
}

fn affordable(cost: Cost, look: &Look<'_>) -> bool {
    if waived(cost, look) {
        return true;
    }
    match cost {
        Cost::Store(item, amount) => look.quantity(item) >= amount,
        Cost::Water(amount) => look.trip.is_some_and(|trip| trip.water >= amount),
        // Strictly more than the cost: upstream lets the payment land the
        // wanderer on 0 hp, walking dead until the next hit. Same rule as the
        // stim: over SSH a button that silently ends the run reads as a bug,
        // so a cost may never be the killing blow.
        Cost::Hp(amount) => look.trip.is_some_and(|trip| trip.hp > amount),
    }
}

fn spend(cost: Cost, ctx: &mut Ctx<'_>) {
    if waived(cost, &ctx.look()) {
        return;
    }
    match cost {
        Cost::Store(item, amount) => ctx.add(item, -amount),
        Cost::Water(amount) => {
            if let Some(trip) = &mut ctx.trip {
                trip.water = (trip.water - amount).max(0);
            }
        }
        Cost::Hp(amount) => {
            if let Some(trip) = &mut ctx.trip {
                trip.hp = (trip.hp - amount).max(0);
            }
        }
    }
}

/// The wanderer lands a hit. Upstream's `Events.damage` in the direction that
/// actually happens in this port: the enemy is the one carrying the shield,
/// the meditation and the health thresholds.
fn strike_enemy(fight: &mut Fight, damage: i64) {
    // Meditation banks the damage instead of taking it; the enemy throws the
    // whole pile back on its next swing.
    if fight.has_enemy_status(Status::Meditation) {
        fight.banked += damage;
        fight.last_hit = Some(damage.to_string());
        return;
    }
    // A shield turns the hit into healing and then breaks.
    if fight.has_enemy_status(Status::Shield) {
        fight.enemy_status = None;
        fight.enemy_hp = (fight.enemy_hp + damage).min(fight.enemy_max);
        fight.last_hit = Some(format!("+{damage}"));
        return;
    }
    let before = fight.enemy_hp;
    fight.enemy_hp = (fight.enemy_hp - damage).max(0);
    fight.last_hit = Some(format!("-{damage}"));
    fight.check_thresholds(before);
}

/// Run a status's clock down, taking it off when it expires. The ones with no
/// clock (the shield, venom, an energised enemy) stay until they are used up
/// or the fight ends.
fn expire(held: &mut Option<Affliction>, seconds: f64) {
    let Some(affliction) = held else {
        return;
    };
    let Some(remaining) = &mut affliction.remaining else {
        return;
    };
    *remaining -= seconds;
    if *remaining <= 0.0 {
        *held = None;
    }
}

/// The command deck's rotation: one of shield, enraged or meditation, never
/// the one it picked last time.
fn rotate_command(last: Option<Status>, rng: &mut impl Rng) -> Status {
    let choices: Vec<Status> = COMMAND_ROTATION
        .into_iter()
        .filter(|status| Some(*status) != last)
        .collect();
    let index = (rng.r#gen::<f64>() * choices.len() as f64).floor() as usize;
    choices
        .get(index.min(choices.len() - 1))
        .copied()
        // `COMMAND_ROTATION` has three entries and the filter drops at most
        // one, so the list is never empty.
        .unwrap_or(Status::Shield)
}

/// Roll a loot table into rows.
fn roll_loot(table: &'static [Loot], rng: &mut impl Rng) -> Vec<LootRow> {
    let mut rows = Vec::new();
    for loot in table {
        if rng.r#gen::<f64>() >= loot.chance {
            continue;
        }
        let span = (loot.max - loot.min).max(0) as f64;
        let count = (rng.r#gen::<f64>() * span).floor() as i64 + loot.min;
        if count > 0 {
            rows.push(LootRow {
                item: loot.item,
                left: count,
            });
        }
    }
    rows
}

/// The weapons the pack can actually swing, fists last if there is nothing
/// else or if no damaging weapons have ammunition (upstream falls back to punching).
pub fn available_weapons(look: &Look<'_>) -> Vec<Weapon> {
    let mut weapons: Vec<Weapon> = Weapon::ALL
        .into_iter()
        .filter(|weapon| *weapon != Weapon::Fists)
        .filter(|weapon| match weapon.item() {
            Some(item) => look.quantity(item) > 0,
            None => false,
        })
        .collect();
    // Upstream adds fists if numWeapons (usable damaging weapons) is 0.
    let usable_damaging = weapons.iter().any(|&w| match w.damage() {
        Damage::Hits(h) if h > 0 => weapon_loaded(w, look),
        _ => false,
    });
    if !usable_damaging && !weapons.contains(&Weapon::Fists) {
        weapons.push(Weapon::Fists);
    }
    weapons
}

/// Whether a weapon has the ammunition to fire.
fn weapon_loaded(weapon: Weapon, look: &Look<'_>) -> bool {
    match weapon.cost() {
        Some((ammo, amount)) => look.quantity(ammo) >= amount,
        None => true,
    }
}

/// The perks that move a number on the way out.
fn perk_damage(base: i64, kind: WeaponKind, game: &Game) -> i64 {
    let mut damage = base;
    match kind {
        WeaponKind::Unarmed => {
            if game.has_perk(Perk::Boxer) {
                damage *= 2;
            }
            if game.has_perk(Perk::MartialArtist) {
                damage *= 3;
            }
            if game.has_perk(Perk::UnarmedMaster) {
                damage *= 2;
            }
        }
        WeaponKind::Melee => {
            if game.has_perk(Perk::Barbarian) {
                damage = (damage as f64 * 1.5).floor() as i64;
            }
        }
        WeaponKind::Ranged => {}
    }
    damage
}

/// Throwing enough punches teaches you how to throw them.
fn train_fists(game: &mut Game, out: &mut Vec<String>) {
    game.punches = game.punches.saturating_add(1);
    let learned = match game.punches {
        50 => Some(Perk::Boxer),
        150 => Some(Perk::MartialArtist),
        300 => Some(Perk::UnarmedMaster),
        _ => None,
    };
    if let Some(perk) = learned
        && game.add_perk(perk)
    {
        out.push(perk.notify().to_string());
    }
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

/// Apply one effect. Every arm is upstream's `onLoad`/`onChoose` body.
pub fn apply(effect: Effect, ctx: &mut Ctx<'_>, rng: &mut impl Rng, out: &mut Vec<String>) {
    match effect {
        Effect::TradeWoodFor(resource) => {
            let wood = ctx.game.store(Resource::Wood);
            let taken = ((wood as f64 * 0.1).floor() as i64).max(1);
            let given = (taken / 5).max(1);
            ctx.game.add_store(Resource::Wood, -taken);
            ctx.game.add_store(resource, given);
        }
        Effect::BuildHut => {
            let huts = ctx.game.building_count(Building::Hut);
            if huts < 20 {
                ctx.game.buildings.insert(Building::Hut, huts + 1);
            }
        }
        Effect::WreckTraps => {
            let traps = ctx.game.building_count(Building::Trap);
            if traps == 0 {
                return;
            }
            let wrecked = (rng.r#gen::<f64>() * f64::from(traps)).floor() as u32 + 1;
            ctx.game
                .buildings
                .insert(Building::Trap, traps.saturating_sub(wrecked));
        }
        Effect::DestroyHuts(count) => {
            destroy_huts(ctx.game, count, rng);
        }
        Effect::KillVillagers(kill) => {
            let num = match kill {
                Kill::Range(low, high) => {
                    (rng.r#gen::<f64>() * f64::from(high - low)).floor() as u32 + low
                }
                Kill::HalfPopulation => {
                    (rng.r#gen::<f64>() * f64::from(ctx.game.population / 2)).floor() as u32 + 1
                }
            };
            kill_villagers(ctx.game, num);
        }
        Effect::GrantPerk(perk) => {
            if ctx.game.add_perk(perk) {
                out.push(perk.notify().to_string());
            }
        }
        Effect::SettleThieves { pay_back, perk } => {
            ctx.game.thieves = Thieves::Dealt;
            if pay_back {
                let stolen: Vec<(Resource, i64)> =
                    ctx.game.stolen.iter().map(|(r, n)| (*r, *n)).collect();
                for (resource, amount) in stolen {
                    ctx.game.add_store(resource, amount);
                }
            }
            ctx.game.stolen.clear();
            if let Some(perk) = perk
                && ctx.game.add_perk(perk)
            {
                out.push(perk.notify().to_string());
            }
        }
        Effect::ScheduleReward {
            chance,
            resource,
            amount,
            delay_secs,
            message,
        } => {
            if rng.r#gen::<f64>() < chance {
                ctx.game.pending_rewards.push(PendingReward {
                    resource,
                    amount,
                    due: ctx.now.timestamp() + i64::from(delay_secs),
                    message: message.to_string(),
                });
            }
        }
        Effect::UncoverMap => {
            uncover_random(ctx.game, rng);
        }
        Effect::ClearDungeon => {
            if let Some(trip) = &mut ctx.trip {
                super::world::clear_dungeon(trip);
            }
        }
        Effect::DrawRoad => {
            if let Some(trip) = &mut ctx.trip {
                super::world::draw_road(trip);
            }
        }
        Effect::RefillWater => {
            let water = ctx.game.max_water();
            if let Some(trip) = &mut ctx.trip {
                trip.water = water;
                out.push(world_data::MSG_WATER_REPLENISHED.to_string());
            }
        }
        Effect::MarkVisited => {
            if let Some(trip) = &mut ctx.trip {
                trip.map.set_visited(trip.x, trip.y);
            }
        }
        Effect::GrantBuilding(building) => {
            if let Some(trip) = &mut ctx.trip {
                trip.cleared.insert(building);
            }
        }
        Effect::FoundShip => {
            if let Some(trip) = &mut ctx.trip {
                trip.found_ship = true;
            }
        }
        Effect::UseOutpost => {
            let water = ctx.game.max_water();
            if let Some(trip) = &mut ctx.trip {
                trip.water = water;
                trip.used_outposts.insert(format!("{},{}", trip.x, trip.y));
                out.push(world_data::MSG_WATER_REPLENISHED.to_string());
            }
        }
        Effect::ClearCity => {
            ctx.game.city_cleared = true;
        }
        Effect::EnterBattleship => {
            if let Some(trip) = &mut ctx.trip {
                trip.battleship.entered = true;
            }
        }
        Effect::ClearDeck(deck) => {
            if let Some(trip) = &mut ctx.trip {
                trip.battleship.decks.insert(deck);
            }
        }
        Effect::HealFull => {
            let max = ctx.game.max_health();
            if let Some(trip) = &mut ctx.trip {
                trip.hp = max;
            }
        }
    }
}

/// Villagers die, and the trades give up whoever is left over.
pub fn kill_villagers(game: &mut Game, num: u32) {
    game.population = game.population.saturating_sub(num);
    let assigned: u32 = game.workers.values().sum();
    let mut gap = assigned.saturating_sub(game.population);
    if gap == 0 {
        return;
    }
    let jobs: Vec<super::data::Job> = game.workers.keys().copied().collect();
    for job in jobs {
        let count = game.worker_count(job);
        if count <= gap {
            gap -= count;
            game.workers.insert(job, 0);
        } else {
            game.workers.insert(job, count - gap);
            break;
        }
    }
}

/// Fire takes huts, and whoever was asleep in them.
fn destroy_huts(game: &mut Game, count: u32, rng: &mut impl Rng) {
    for _ in 0..count {
        let population = game.population;
        let rate = f64::from(population) / f64::from(super::data::HUT_ROOM);
        let full = rate.floor() as u32;
        let huts = rate.ceil() as u32;
        if huts == 0 || game.building_count(Building::Hut) == 0 {
            break;
        }
        let target = (rng.r#gen::<f64>() * f64::from(huts)).floor() as u32 + 1;
        let inhabitants = if target <= full {
            super::data::HUT_ROOM
        } else if target == full + 1 {
            population % super::data::HUT_ROOM
        } else {
            0
        };
        let standing = game.building_count(Building::Hut);
        game.buildings
            .insert(Building::Hut, standing.saturating_sub(1));
        if inhabitants > 0 {
            kill_villagers(game, inhabitants);
        }
    }
}

/// The scout's map: light a five-square patch somewhere still dark.
fn uncover_random(game: &mut Game, rng: &mut impl Rng) {
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let grid = super::model::GRID;
    for _ in 0..1000 {
        let x = (rng.r#gen::<f64>() * f64::from(grid)).floor() as i32;
        let y = (rng.r#gen::<f64>() * f64::from(grid)).floor() as i32;
        if world.seen(x, y) {
            continue;
        }
        super::world::uncover(world, x, y, 5);
        break;
    }
    game.seen_all_map = world.all_seen();
}
