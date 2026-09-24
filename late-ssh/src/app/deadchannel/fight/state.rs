//! The fight's rules: a pure state machine over one runner row's worth of
//! sheet (GAME.md, "The stat block") and the fight on it. No I/O, no
//! clock reads; the day and the dice are handed in. The service locks the
//! row, builds a `Sheet`, applies one command, and stores the result; the
//! session keeps a mirror and never decides anything from it.
//!
//! The forest is the static at the end of Static Row: a fight is a scene
//! you ask for by stepping into the screen, never something that happens
//! to you (the city is the wallet; nothing there can be missed). The
//! exchange is LoGD's one attack loop over the door's pure resolver
//! (`door/greendragon/combat.rs`, imported as the parts bin GAME.md
//! promised; the door itself is untouched) with `run` as the one live
//! decision, because that is what makes a forest a forest.
//!
//! Levels climb on exp here, in the fight, for now. GAME.md gives that job
//! to the operators (beaten once per level); until they exist, the row
//! would sit at level 1 against level-1 flickers forever, so the threshold
//! itself levels you and the wire says so. The operators replace this.
//!
//! The armorer's till is here too (GAME.md, "Gear: two slots, fifteen
//! tiers"): the same sheet, the same lock, one more command. Bits buy a
//! tier above the one you carry; the piece you hand back comes off the
//! price at `TRADE_IN_PERCENT`. The catalog (names, the price ladder) is
//! the city's, in `city/data.rs`, because the wall is the city's.

use chrono::NaiveDate;
use late_core::models::deadchannel_runner::{DeadchannelRunner, SheetWrite};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::data::{
    self, DROP_LINES, EXP_KEEP_ON_DEATH, FOES, FoeKind, KILL_LINES, RATIONS_PER_DAY,
    RUN_FAILED_LINES, RUN_LINES, RUN_ODDS, RUN_ODDS_OUT_OF, SIGNAL_PER_LEVEL, START_BITS,
    TRADE_IN_PERCENT,
};
use crate::app::deadchannel::city::data::{ARMOR, COST_LADDER, WEAPONS};
use crate::app::door::greendragon::combat::{Combatant, resolve_extra_foe_strike, resolve_round};

/// The top of the armorer's wall.
pub const MAX_TIER: i32 = COST_LADDER.len() as i32;

/// Lines of the exchange the row remembers, so a fight found waiting after
/// a dropped session shows how it got there.
const LOG_KEEP: usize = 6;

/// The fight in progress, as stored on the row (`deadchannel_runners.fight`).
/// The foe's numbers ride along so a retuned table never changes a live
/// foe under somebody.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fight {
    /// Index into `data::FOES`.
    pub kind: usize,
    pub foe_signal: i32,
    pub foe_max_signal: i32,
    pub foe_attack: u32,
    pub foe_defense: u32,
    pub foe_bits: i64,
    pub foe_exp: i64,
    pub log: Vec<String>,
}

impl Fight {
    pub fn foe(&self) -> &'static FoeKind {
        &FOES[self.kind]
    }

    fn push(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > LOG_KEEP {
            let drop = self.log.len() - LOG_KEEP;
            self.log.drain(..drop);
        }
    }
}

/// The row's sheet, typed. Everything the fight loop and the day roll
/// read and write; the look and the leave stamp are not here.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub user_id: Uuid,
    pub level: i32,
    pub exp: i64,
    pub signal: i32,
    pub weapon_tier: i32,
    pub armor_tier: i32,
    pub bits: i64,
    pub rations_left: i32,
    pub day: NaiveDate,
    pub fight: Option<Fight>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SheetError {
    /// The stored fight is not a `Fight`, or names a glyph the table does
    /// not have. A bug, never a blank: the row is the truth and it is wrong.
    Fight(String),
}

impl std::fmt::Display for SheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fight(detail) => write!(f, "stored fight is unreadable: {detail}"),
        }
    }
}

impl std::error::Error for SheetError {}

/// The two gear slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Weapon,
    Armor,
}

/// What a session asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Step into the static: spend a ration, meet a glyph. With a fight
    /// already waiting on the row, this resumes it and spends nothing.
    Start,
    Attack,
    Run,
    /// Buy `tier` (1 to `MAX_TIER`) for `slot` at the armorer, handing
    /// back what the slot holds.
    Outfit { slot: Slot, tier: i32 },
}

/// Why nothing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NoRations,
    SignalDown,
    /// Attack or run with no fight on the row.
    NoFight,
    /// The armorer does not sell down: the tier is not above the one you
    /// carry (or is not on the wall at all).
    NotAnUpgrade,
    /// Bits short of the net price, by this much.
    Short { by: i64 },
}

/// How one command settled. `Won`, `Lost`, and `Escaped` clear the fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Refused(Refusal),
    Started,
    /// A fight was already waiting on the row; nothing was spent.
    Resumed,
    /// One exchange, both still standing.
    Round,
    Won {
        bits: i64,
        exp: i64,
        /// The level reached, if the exp crossed a threshold.
        leveled: Option<i32>,
    },
    Lost {
        bits_lost: i64,
    },
    Escaped,
    /// A piece bought at the armorer for `paid` bits net of the trade-in.
    Outfitted { slot: Slot, tier: i32, paid: i64 },
}

/// One command's result: what settled, and the lines to show for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub applied: Applied,
    pub lines: Vec<String>,
}

impl Sheet {
    /// A fresh runner's sheet: the column defaults of migration 199.
    pub fn fresh(user_id: Uuid, today: NaiveDate) -> Self {
        Self {
            user_id,
            level: 1,
            exp: 0,
            signal: SIGNAL_PER_LEVEL,
            weapon_tier: 0,
            armor_tier: 0,
            bits: START_BITS,
            rations_left: RATIONS_PER_DAY,
            day: today,
            fight: None,
        }
    }

    /// The row, typed. The fight JSON is the one field that can be wrong.
    pub fn from_row(row: &DeadchannelRunner) -> Result<Self, SheetError> {
        let fight = match &row.fight {
            None => None,
            Some(value) => {
                let fight: Fight = match serde_json::from_value(value.clone()) {
                    Ok(fight) => fight,
                    Err(error) => return Err(SheetError::Fight(error.to_string())),
                };
                if fight.kind >= FOES.len() {
                    return Err(SheetError::Fight(format!(
                        "unknown glyph kind {}",
                        fight.kind
                    )));
                }
                Some(fight)
            }
        };
        Ok(Self {
            user_id: row.user_id,
            level: row.level,
            exp: row.exp,
            signal: row.signal,
            weapon_tier: row.weapon_tier,
            armor_tier: row.armor_tier,
            bits: row.bits,
            rations_left: row.rations_left,
            day: row.day,
            fight,
        })
    }

    pub fn to_write(&self) -> SheetWrite {
        SheetWrite {
            user_id: self.user_id,
            level: self.level,
            exp: self.exp,
            signal: self.signal,
            weapon_tier: self.weapon_tier,
            armor_tier: self.armor_tier,
            bits: self.bits,
            rations_left: self.rations_left,
            day: self.day,
            fight: self
                .fight
                .as_ref()
                .map(|fight| serde_json::to_value(fight).expect("a fight serializes")),
        }
    }

    pub fn max_signal(&self) -> i32 {
        self.level * SIGNAL_PER_LEVEL
    }

    pub fn attack(&self) -> u32 {
        (self.level + self.weapon_tier) as u32
    }

    pub fn defense(&self) -> u32 {
        (self.level + self.armor_tier) as u32
    }

    /// Off the wire: the signal dropped and the day has not rolled.
    pub fn is_down(&self) -> bool {
        self.signal <= 0
    }

    pub fn tier_of(&self, slot: Slot) -> i32 {
        match slot {
            Slot::Weapon => self.weapon_tier,
            Slot::Armor => self.armor_tier,
        }
    }

    /// The name of what the slot holds; `None` at tier 0 (bare hands,
    /// street clothes).
    pub fn gear_name(&self, slot: Slot) -> Option<&'static str> {
        gear_name(slot, self.tier_of(slot))
    }

    /// What the armorer would charge for `tier` in `slot`, net of the
    /// trade-in on what the slot holds. Meaningful only above the carried
    /// tier; the panel shows it, `Outfit` charges it.
    pub fn outfit_price(&self, slot: Slot, tier: i32) -> i64 {
        price(tier) - trade_in(self.tier_of(slot))
    }

    /// The lazy day roll (GAME.md, "Three bars, one clock"): the first
    /// touch after midnight UTC refills signal and rations together, and
    /// nothing refills in between. A fight left hanging overnight is
    /// dropped with the day; the ration it cost is refilled with the rest.
    /// Returns whether anything changed.
    pub fn settle(&mut self, today: NaiveDate) -> bool {
        if self.day >= today {
            return false;
        }
        self.day = today;
        self.signal = self.max_signal();
        self.rations_left = RATIONS_PER_DAY;
        self.fight = None;
        true
    }

    pub fn apply<R: Rng>(&mut self, command: Command, rng: &mut R) -> Outcome {
        match command {
            Command::Start => self.start(),
            Command::Attack => self.attack_round(rng),
            Command::Run => self.run(rng),
            Command::Outfit { slot, tier } => self.outfit(slot, tier),
        }
    }

    /// The till. Above what you carry, on the wall, and paid for in full
    /// after the trade-in, or nothing moves.
    fn outfit(&mut self, slot: Slot, tier: i32) -> Outcome {
        let carried = self.tier_of(slot);
        if tier <= carried || tier > MAX_TIER {
            let line = match self.gear_name(slot) {
                Some(name) => format!("the armorer does not sell down. you carry the {name}."),
                None => "the armorer has nothing like that on the wall.".to_string(),
            };
            return Outcome {
                applied: Applied::Refused(Refusal::NotAnUpgrade),
                lines: vec![line],
            };
        }
        let name = gear_name(slot, tier).expect("a tier on the wall");
        let paid = self.outfit_price(slot, tier);
        if paid > self.bits {
            let by = paid - self.bits;
            return Outcome {
                applied: Applied::Refused(Refusal::Short { by }),
                lines: vec![format!("you are {by} bits short of the {name}.")],
            };
        }
        let handed_back = self.gear_name(slot);
        self.bits -= paid;
        match slot {
            Slot::Weapon => self.weapon_tier = tier,
            Slot::Armor => self.armor_tier = tier,
        }
        let line = match handed_back {
            Some(old) => format!(
                "the armorer hands over the {name}. the {old} goes back on the wall. {paid} bits."
            ),
            None => format!("the armorer hands over the {name}. {paid} bits."),
        };
        Outcome {
            applied: Applied::Outfitted { slot, tier, paid },
            lines: vec![line],
        }
    }

    fn start(&mut self) -> Outcome {
        if self.fight.is_some() {
            return Outcome {
                applied: Applied::Resumed,
                lines: Vec::new(),
            };
        }
        if self.is_down() {
            return refused(Refusal::SignalDown);
        }
        if self.rations_left <= 0 {
            return refused(Refusal::NoRations);
        }
        self.rations_left -= 1;
        let (kind, foe, tier) = data::foe_for_level(self.level);
        let mut fight = Fight {
            kind,
            foe_signal: tier.signal,
            foe_max_signal: tier.signal,
            foe_attack: tier.attack,
            foe_defense: tier.defense,
            foe_bits: tier.bits,
            foe_exp: tier.exp,
            log: Vec::new(),
        };
        fight.push(foe.arrives.to_string());
        let lines = fight.log.clone();
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Started,
            lines,
        }
    }

    fn attack_round<R: Rng>(&mut self, rng: &mut R) -> Outcome {
        let Some(mut fight) = self.fight.take() else {
            return refused(Refusal::NoFight);
        };
        let player = self.combatant();
        let foe = Combatant {
            attack: fight.foe_attack,
            defense: fight.foe_defense,
        };
        let round = resolve_round(rng, player, foe);
        let name = fight.foe().name;
        let mut lines = Vec::new();

        let mut hit = match round.damage_to_enemy {
            n if n > 0 => match self.gear_name(Slot::Weapon) {
                Some(weapon) => format!("your {weapon} hits the {name} for {n}."),
                None => format!("you hit the {name} for {n}."),
            },
            0 => format!("you swing through the {name}."),
            n => format!("your hit glances. the {name} feeds on it, +{}.", -n),
        };
        if round.player_crit {
            hit = format!("a clean hit. {hit}");
        }
        if round.power_move.is_some() {
            hit.push_str(" the street hears it.");
        }
        lines.push(hit);
        fight.foe_signal =
            (fight.foe_signal - round.damage_to_enemy).clamp(0, fight.foe_max_signal);
        if fight.foe_signal == 0 {
            return self.win(fight, rng, lines);
        }

        lines.push(match round.damage_to_player {
            n if n > 0 => format!("it hits you for {n}."),
            0 => "it misses.".to_string(),
            n => format!("it glances off you. +{} signal.", -n),
        });
        self.take(round.damage_to_player);
        if self.is_down() {
            return self.lose(fight, rng, lines);
        }
        for line in &lines {
            fight.push(line.clone());
        }
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Round,
            lines,
        }
    }

    fn run<R: Rng>(&mut self, rng: &mut R) -> Outcome {
        let Some(mut fight) = self.fight.take() else {
            return refused(Refusal::NoFight);
        };
        if rng.gen_range(0..RUN_ODDS_OUT_OF) < RUN_ODDS {
            return Outcome {
                applied: Applied::Escaped,
                lines: vec![pick(rng, &RUN_LINES).to_string()],
            };
        }
        let mut lines = vec![pick(rng, &RUN_FAILED_LINES).to_string()];
        let foe = Combatant {
            attack: fight.foe_attack,
            defense: fight.foe_defense,
        };
        let damage = resolve_extra_foe_strike(rng, self.combatant(), foe, &[]);
        lines.push(match damage {
            n if n > 0 => format!("it hits you for {n}."),
            0 => "it misses.".to_string(),
            n => format!("it glances off you. +{} signal.", -n),
        });
        self.take(damage);
        if self.is_down() {
            return self.lose(fight, rng, lines);
        }
        for line in &lines {
            fight.push(line.clone());
        }
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Round,
            lines,
        }
    }

    fn combatant(&self) -> Combatant {
        Combatant {
            attack: self.attack(),
            defense: self.defense(),
        }
    }

    /// Signed damage to the signal: negative heals, never past the max.
    fn take(&mut self, damage: i32) {
        self.signal = (self.signal - damage).clamp(0, self.max_signal());
    }

    fn win<R: Rng>(&mut self, fight: Fight, rng: &mut R, mut lines: Vec<String>) -> Outcome {
        self.bits += fight.foe_bits;
        self.exp += fight.foe_exp;
        lines.push(format!(
            "{} +{} bits, +{} exp.",
            pick(rng, &KILL_LINES),
            fight.foe_bits,
            fight.foe_exp
        ));
        let mut leveled = None;
        while let Some(need) = data::exp_to_advance(self.level) {
            if self.exp < need {
                break;
            }
            self.level += 1;
            self.signal += SIGNAL_PER_LEVEL;
            leveled = Some(self.level);
        }
        if let Some(level) = leveled {
            lines.push(format!("you are level {level}. the street will hear."));
        }
        Outcome {
            applied: Applied::Won {
                bits: fight.foe_bits,
                exp: fight.foe_exp,
                leveled,
            },
            lines,
        }
    }

    fn lose<R: Rng>(&mut self, _fight: Fight, rng: &mut R, mut lines: Vec<String>) -> Outcome {
        let bits_lost = self.bits;
        self.bits = 0;
        self.exp = (self.exp as f64 * EXP_KEEP_ON_DEATH).round() as i64;
        lines.push(pick(rng, &DROP_LINES).to_string());
        lines.push(match bits_lost {
            0 => "the street finds nothing on you.".to_string(),
            n => format!("the street takes {n} bits off you. back tomorrow."),
        });
        Outcome {
            applied: Applied::Lost { bits_lost },
            lines,
        }
    }
}

/// The fight's refusals, with their one line each. The till's carry their
/// own lines, since they name the piece.
fn refused(refusal: Refusal) -> Outcome {
    let line = match refusal {
        Refusal::NoRations => "you are spent for today. the static will keep.",
        Refusal::SignalDown => "your signal is down. nothing in there can see you until tomorrow.",
        Refusal::NoFight => "there is nothing in front of you.",
        Refusal::NotAnUpgrade | Refusal::Short { .. } => {
            unreachable!("till refusals are lined in outfit")
        }
    };
    Outcome {
        applied: Applied::Refused(refusal),
        lines: vec![line.to_string()],
    }
}

/// The wall: the name of `tier` in `slot`, `None` at tier 0.
pub fn gear_name(slot: Slot, tier: i32) -> Option<&'static str> {
    if tier < 1 || tier > MAX_TIER {
        return None;
    }
    let names = match slot {
        Slot::Weapon => &WEAPONS,
        Slot::Armor => &ARMOR,
    };
    Some(names[(tier - 1) as usize])
}

/// The price on the wall for `tier` (1 to `MAX_TIER`).
fn price(tier: i32) -> i64 {
    i64::from(COST_LADDER[(tier - 1) as usize])
}

/// What the armorer pays for a carried `tier`; nothing for bare hands.
fn trade_in(tier: i32) -> i64 {
    match tier {
        0 => 0,
        tier => price(tier) * TRADE_IN_PERCENT / 100,
    }
}

fn pick<'a, R: Rng>(rng: &mut R, pool: &[&'a str]) -> &'a str {
    pool[rng.gen_range(0..pool.len())]
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
