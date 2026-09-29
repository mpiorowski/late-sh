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
//! The top of the ladder is the Old Signal (GAME.md, "Marks: the reset"):
//! at level 15 with the exp to leave it, the next step in meets it instead
//! of a glyph. Putting it down leaves a mark and resets the runner to
//! level 1, bare hands, and starting bits; the peak level (the tailor's
//! gate), the look, and the badges stay. Each mark adds a capped point of
//! attack and defense and scales every exp threshold.
//!
//! Levels climb on exp here, in the fight, for now. GAME.md gives that job
//! to the operators (beaten once per level); until they exist, the row
//! would sit at level 1 against level-1 flickers forever, so the threshold
//! itself levels you and the wire says so. The operators replace this.
//!
//! Levelling on exp can carry a runner into a glyph their gear cannot
//! beat, so every step in picks its quarry (`Pick`): the glyph of your
//! level, or the one a level down at half pay, the way back from a fight
//! you cannot win (LoGD's slumming, cut).
//!
//! The armorer's till is here too (GAME.md, "Gear: two slots, fifteen
//! tiers"): the same sheet, the same lock, one more command. Bits buy a
//! tier above the one you carry; the piece you hand back comes off the
//! price at `TRADE_IN_PERCENT`. The catalog (names, the price ladder) is
//! the city's, in `city/data.rs`, because the wall is the city's.
//!
//! So are the lockers and the bits machine, the two money places: the
//! locker keeps bits from the street for a cut on the way in, the machine
//! lends against the level and takes its interest at the day roll and its
//! share off every kill. And the ledge: a step off it is the runner
//! started over, the marks and the debt kept.

use chrono::NaiveDate;
use late_core::models::deadchannel_runner::{DeadchannelRunner, SheetWrite};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::data::{
    self, DEBT_INTEREST_PERCENT, DROP_LINES, EXP_KEEP_ON_DEATH, FOES, FoeKind, FoeTier,
    GARNISH_PERCENT, HEARD_LINE, KILL_LINES, LOAN_PER_LEVEL, LOCKER_FEE_PERCENT, MARK_BONUS_CAP,
    MAX_LEVEL, NEAR_MISS_SIGNAL, OLD_SIGNAL, OLD_SIGNAL_TIER, RATIONS_PER_DAY, RUN_FAILED_LINES,
    RUN_LINES, RUN_ODDS, RUN_ODDS_OUT_OF, SIGNAL_PER_LEVEL, SLAIN_LINE, START_BITS,
    STEPPED_DOWN_LINE, TRADE_IN_PERCENT, percent_up,
};
use crate::app::deadchannel::city::data::{ARMOR, COST_LADDER, WEAPONS};
use crate::app::door::greendragon::combat::{Combatant, resolve_extra_foe_strike, resolve_round};

/// The top of the armorer's wall.
pub const MAX_TIER: i32 = COST_LADDER.len() as i32;

/// Lines of the exchange the row remembers, so a fight found waiting after
/// a dropped session shows how it got there.
const LOG_KEEP: usize = 6;

/// What is in front of you: a glyph of the table, or the Old Signal.
/// Stored as `{"glyph": 3}` or `"old_signal"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quarry {
    /// Index into `data::FOES`.
    Glyph(usize),
    OldSignal,
}

/// The fight in progress, as stored on the row (`deadchannel_runners.fight`).
/// The foe's numbers ride along so a retuned table never changes a live
/// foe under somebody.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fight {
    pub quarry: Quarry,
    pub foe_signal: i32,
    pub foe_max_signal: i32,
    pub foe_attack: u32,
    pub foe_defense: u32,
    pub foe_bits: i64,
    pub foe_exp: i64,
    pub log: Vec<String>,
}

impl Fight {
    fn new(quarry: Quarry, tier: FoeTier) -> Self {
        Self {
            quarry,
            foe_signal: tier.signal,
            foe_max_signal: tier.signal,
            foe_attack: tier.attack,
            foe_defense: tier.defense,
            foe_bits: tier.bits,
            foe_exp: tier.exp,
            log: Vec::new(),
        }
    }

    pub fn foe(&self) -> &'static FoeKind {
        match self.quarry {
            Quarry::Glyph(kind) => &FOES[kind],
            Quarry::OldSignal => &OLD_SIGNAL,
        }
    }

    /// The glyph's level (one kind per level); `None` for the Old Signal,
    /// which has none.
    pub fn foe_level(&self) -> Option<i32> {
        match self.quarry {
            Quarry::Glyph(kind) => Some(kind as i32 + 1),
            Quarry::OldSignal => None,
        }
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
    /// Glyphs put down, ever. Exact where exp is not: a death keeps most
    /// of the exp, so exp alone cannot say "never won".
    pub kills: i32,
    /// Today's tally, zeroed by the day roll: the wire's last-ration line.
    pub kills_today: i32,
    pub runs_today: i32,
    /// The highest level ever reached; a mark resets `level`, never this.
    /// The tailor's rack is cut to it.
    pub peak_level: i32,
    /// Old Signal kills, ever.
    pub marks: i32,
    /// The mark whose chips the house still owes: set by the kill, cleared
    /// by the service once the grant answers (migration 210). `None` when
    /// nothing is owed.
    pub unpaid_mark: Option<i32>,
    /// Bits in the locker: a drop never reaches them.
    pub stash: i64,
    /// Bits owed to the machine.
    pub debt: i64,
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

/// Which glyph a step in goes looking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// The glyph of your level, or at the top with the exp to leave it,
    /// the Old Signal.
    Fair,
    /// The glyph a level down, at [`data::LOWER_PAY_PERCENT`] of its pay.
    Lower,
}

/// What a session asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Step into the static: spend a ration, meet the `pick`. With a fight
    /// already waiting on the row, this resumes it, whatever the pick, and
    /// spends nothing.
    Start {
        pick: Pick,
    },
    Attack,
    Run,
    /// Buy `tier` (1 to `MAX_TIER`) for `slot` at the armorer, handing
    /// back what the slot holds.
    Outfit {
        slot: Slot,
        tier: i32,
    },
    /// Buy the signal back to full at patch, for `Sheet::patch_price`.
    Patch,
    /// Everything on hand into the locker, less its cut.
    Deposit,
    /// Everything in the locker back on hand.
    Withdraw,
    /// The bits machine's loan: up to the level's cap.
    Borrow,
    /// As much of the debt as the bits on hand cover.
    Repay,
    /// Off the ledge: the runner starts over.
    Reset,
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
    Short {
        by: i64,
    },
    /// Patch with the signal already full.
    NothingToPatch,
    /// Patch, the locker, the machine, or the ledge with a glyph waiting
    /// on the row: the fight is the fight.
    FightWaiting,
    /// A step down at level 1: nothing is below the flicker.
    NoLowerGlyph,
    /// A deposit or a repayment with no bits on hand.
    NothingOnHand,
    /// A withdrawal from an empty locker.
    LockerEmpty,
    /// A loan with the debt already at the level's cap.
    LoanCapped,
    /// A repayment with nothing owed.
    NoDebt,
}

/// How one command settled. `Won`, `Lost`, and `Escaped` clear the fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    Refused(Refusal),
    Started {
        pick: Pick,
    },
    /// A fight was already waiting on the row; nothing was spent.
    Resumed,
    /// One exchange, both still standing.
    Round,
    Won {
        /// The glyph's name, for the wire: the fight leaves the row on a
        /// win, so the answer carries it.
        foe: &'static str,
        /// The glyph's whole pay; `garnished` of it went to the debt.
        bits: i64,
        garnished: i64,
        exp: i64,
        /// The level reached, if the exp crossed a threshold.
        leveled: Option<i32>,
    },
    /// The Old Signal put down: the runner is level 1 again with `marks`.
    Slain {
        marks: i32,
    },
    Lost {
        bits_lost: i64,
    },
    Escaped,
    /// A piece bought at the armorer for `paid` bits net of the trade-in.
    Outfitted {
        slot: Slot,
        tier: i32,
        paid: i64,
    },
    /// The signal bought back to full at patch: `restored` points for
    /// `paid` bits.
    Patched {
        restored: i32,
        paid: i64,
    },
    /// `stored` bits into the locker; `fee` more was its cut.
    Deposited {
        stored: i64,
        fee: i64,
    },
    Withdrew {
        amount: i64,
    },
    Borrowed {
        amount: i64,
    },
    Repaid {
        amount: i64,
    },
    /// Off the ledge: level 1, bare hands, nothing on hand or in the
    /// locker. The marks, the peak, the kills, and the debt stay.
    Reset,
}

/// One command's result: what settled, and the lines to show for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub applied: Applied,
    pub lines: Vec<String>,
}

/// What the wire hears about one command (GAME.md, "The wire sees the
/// news, never the play-by-play"): every result worth a story, and
/// nothing else. A kill, a round, a run, a purchase are the runner's
/// business. The service formats these; the sheet decides them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum News {
    /// The signal dropped; the street took `bits_lost`.
    Dropped { bits_lost: i64 },
    /// A level gained: the line the face rides.
    Leveled { level: i32 },
    /// The Old Signal put down, the runner reset: the other line the face
    /// rides.
    Slain { marks: i32 },
    /// A step off the ledge: the runner started over by choice.
    SteppedOff,
    /// The runner's first glyph, ever.
    FirstBlood { foe: &'static str },
    /// A win with `signal` at or under [`NEAR_MISS_SIGNAL`].
    NearMiss { foe: &'static str, signal: i32 },
    /// The last ration of the day is spent and the fight it bought is
    /// over: the day's card.
    LastRation {
        kills: i32,
        runs: i32,
        signal: i32,
        max_signal: i32,
    },
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
            kills: 0,
            kills_today: 0,
            runs_today: 0,
            peak_level: 1,
            marks: 0,
            unpaid_mark: None,
            stash: 0,
            debt: 0,
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
                match fight.quarry {
                    Quarry::Glyph(kind) if kind >= FOES.len() => {
                        return Err(SheetError::Fight(format!("unknown glyph kind {kind}")));
                    }
                    Quarry::Glyph(_) | Quarry::OldSignal => Some(fight),
                }
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
            kills: row.kills,
            kills_today: row.kills_today,
            runs_today: row.runs_today,
            peak_level: row.peak_level,
            marks: row.marks,
            unpaid_mark: row.unpaid_mark,
            stash: row.stash,
            debt: row.debt,
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
            kills: self.kills,
            kills_today: self.kills_today,
            runs_today: self.runs_today,
            peak_level: self.peak_level,
            marks: self.marks,
            unpaid_mark: self.unpaid_mark,
            stash: self.stash,
            debt: self.debt,
        }
    }

    pub fn max_signal(&self) -> i32 {
        self.level * SIGNAL_PER_LEVEL
    }

    pub fn attack(&self) -> u32 {
        (self.level + self.weapon_tier + self.mark_bonus()) as u32
    }

    pub fn defense(&self) -> u32 {
        (self.level + self.armor_tier + self.mark_bonus()) as u32
    }

    /// What the marks add to attack and defense: one each, capped.
    pub fn mark_bonus(&self) -> i32 {
        self.marks.min(MARK_BONUS_CAP)
    }

    /// At the top with the exp to leave it: the next step in meets the Old
    /// Signal.
    pub fn signal_hears(&self) -> bool {
        self.level == MAX_LEVEL && self.exp >= data::exp_to_seek(self.marks)
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

    /// What patch charges to bring the signal back to full: a bit a
    /// point, times the level, so the price climbs with the glyphs that
    /// did the damage. Zero with nothing missing; the panel shows it,
    /// `Patch` charges it.
    pub fn patch_price(&self) -> i64 {
        i64::from(self.max_signal() - self.signal) * i64::from(self.level)
    }

    /// The lazy day roll (GAME.md, "Three bars, one clock"): the first
    /// touch after midnight UTC refills signal and rations together, and
    /// nothing refills in between. A fight left hanging overnight is
    /// dropped with the day; the ration it cost is refilled with the rest.
    /// The machine's interest lands here too, once per roll, not per
    /// calendar day: a runner who stays away owes what they owed.
    /// Returns whether anything changed.
    pub fn settle(&mut self, today: NaiveDate) -> bool {
        if self.day >= today {
            return false;
        }
        self.day = today;
        self.signal = self.max_signal();
        self.rations_left = RATIONS_PER_DAY;
        self.debt += percent_up(self.debt, DEBT_INTEREST_PERCENT);
        self.fight = None;
        self.kills_today = 0;
        self.runs_today = 0;
        true
    }

    /// The news in `applied`, in the order the wire prints it. At most one
    /// story line per win (a level, else first blood, else a near miss:
    /// the biggest one), then the day's card if this was the last ration's
    /// fight and it ended standing. A dropped signal is its own line and
    /// ends the day by itself.
    pub fn news(&self, applied: &Applied) -> Vec<News> {
        let mut news = Vec::new();
        match applied {
            Applied::Lost { bits_lost } => {
                news.push(News::Dropped {
                    bits_lost: *bits_lost,
                });
                return news;
            }
            Applied::Slain { marks } => news.push(News::Slain { marks: *marks }),
            Applied::Won { foe, leveled, .. } => {
                if let Some(level) = leveled {
                    news.push(News::Leveled { level: *level });
                } else if self.kills == 1 {
                    news.push(News::FirstBlood { foe });
                } else if self.signal <= NEAR_MISS_SIGNAL {
                    news.push(News::NearMiss {
                        foe,
                        signal: self.signal,
                    });
                }
            }
            Applied::Escaped => {}
            Applied::Reset => {
                news.push(News::SteppedOff);
                return news;
            }
            Applied::Refused(_)
            | Applied::Started { .. }
            | Applied::Resumed
            | Applied::Round
            | Applied::Outfitted { .. }
            | Applied::Patched { .. }
            | Applied::Deposited { .. }
            | Applied::Withdrew { .. }
            | Applied::Borrowed { .. }
            | Applied::Repaid { .. } => return news,
        }
        if self.rations_left == 0 {
            news.push(News::LastRation {
                kills: self.kills_today,
                runs: self.runs_today,
                signal: self.signal,
                max_signal: self.max_signal(),
            });
        }
        news
    }

    pub fn apply<R: Rng>(&mut self, command: Command, rng: &mut R) -> Outcome {
        match command {
            Command::Start { pick } => self.start(pick),
            Command::Attack => self.attack_round(rng),
            Command::Run => self.run(rng),
            Command::Outfit { slot, tier } => self.outfit(slot, tier),
            Command::Patch => self.patch(),
            Command::Deposit => self.deposit(),
            Command::Withdraw => self.withdraw(),
            Command::Borrow => self.borrow(),
            Command::Repay => self.repay(),
            Command::Reset => self.reset(),
        }
    }

    /// What the bits machine will have lent at most: the level's cap.
    pub fn loan_cap(&self) -> i64 {
        i64::from(self.level) * LOAN_PER_LEVEL
    }

    /// What a loan would hand over now: the cap less the debt, nothing at
    /// or past the cap (the interest can carry the debt past it).
    pub fn loan_room(&self) -> i64 {
        (self.loan_cap() - self.debt).max(0)
    }

    /// The locker's cut of a deposit of everything on hand.
    pub fn deposit_fee(&self) -> i64 {
        percent_up(self.bits, LOCKER_FEE_PERCENT)
    }

    /// Everything on hand into the locker, less the cut. Not with a glyph
    /// waiting: the bits you carry into a fight are the bits you risk.
    fn deposit(&mut self) -> Outcome {
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. the locker can wait.");
        }
        if self.bits <= 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::NothingOnHand),
                lines: vec!["you have nothing on you to lock up.".to_string()],
            };
        }
        let fee = self.deposit_fee();
        let stored = self.bits - fee;
        self.stash += stored;
        self.bits = 0;
        Outcome {
            applied: Applied::Deposited { stored, fee },
            lines: vec![format!(
                "the locker takes {stored} bits and keeps {fee} for the trouble."
            )],
        }
    }

    /// Everything in the locker back on hand. Free, and not with a glyph
    /// waiting.
    fn withdraw(&mut self) -> Outcome {
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. the locker can wait.");
        }
        if self.stash <= 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::LockerEmpty),
                lines: vec!["your locker is empty.".to_string()],
            };
        }
        let amount = self.stash;
        self.bits += amount;
        self.stash = 0;
        Outcome {
            applied: Applied::Withdrew { amount },
            lines: vec![format!("{amount} bits out of the locker and on you.")],
        }
    }

    /// The machine lends up to the level's cap, all the room at once.
    fn borrow(&mut self) -> Outcome {
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. the machine can wait.");
        }
        let amount = self.loan_room();
        if amount == 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::LoanCapped),
                lines: vec![format!(
                    "the machine flashes your debt, {} bits, and pays nothing.",
                    self.debt
                )],
            };
        }
        self.bits += amount;
        self.debt += amount;
        Outcome {
            applied: Applied::Borrowed { amount },
            lines: vec![format!(
                "the bits machine pays out for once. {amount} bits, and it will remember."
            )],
        }
    }

    /// As much of the debt as the bits on hand cover.
    fn repay(&mut self) -> Outcome {
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. the machine can wait.");
        }
        if self.debt <= 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::NoDebt),
                lines: vec!["you owe the machine nothing.".to_string()],
            };
        }
        if self.bits <= 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::NothingOnHand),
                lines: vec!["you have nothing on you to feed it.".to_string()],
            };
        }
        let amount = self.bits.min(self.debt);
        self.bits -= amount;
        self.debt -= amount;
        let line = match self.debt {
            0 => format!("{amount} bits into the machine. you owe it nothing."),
            left => format!("{amount} bits into the machine. {left} still owed."),
        };
        Outcome {
            applied: Applied::Repaid { amount },
            lines: vec![line],
        }
    }

    /// Off the ledge: level 1, exp 0, bare hands, no bits on hand or in
    /// the locker, a level-1 signal. What stays is what was earned or owed:
    /// the marks and their title, the peak (the tailor's rack), the kills,
    /// the look, today's rations, and the debt. Not with the signal down
    /// (a reset is not a way back on the wire before the roll) and not
    /// with a glyph waiting.
    fn reset(&mut self) -> Outcome {
        if self.is_down() {
            return Outcome {
                applied: Applied::Refused(Refusal::SignalDown),
                lines: vec![
                    "your signal is down. you cannot even find the edge until tomorrow."
                        .to_string(),
                ],
            };
        }
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. finish it first.");
        }
        self.level = 1;
        self.exp = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.bits = 0;
        self.stash = 0;
        self.signal = self.max_signal();
        let mut lines = vec![
            "you step off the ledge. the fall is longer than the city.".to_string(),
            "you wake at the top of Static Row. level 1, bare hands, empty pockets.".to_string(),
        ];
        if self.debt > 0 {
            lines.push(format!(
                "the bits machine still knows your name. {} bits owed.",
                self.debt
            ));
        }
        Outcome {
            applied: Applied::Reset,
            lines,
        }
    }

    /// Patch. A signal that dropped stays down until the roll (the day
    /// is the day), a fight waiting on the row is finished first, a
    /// runner spent for the day is sold nothing (no fight can spend the
    /// signal before the roll refills it for free), and a full signal
    /// buys nothing; otherwise the whole gap, paid in full.
    fn patch(&mut self) -> Outcome {
        if self.is_down() {
            return Outcome {
                applied: Applied::Refused(Refusal::SignalDown),
                lines: vec![
                    "your signal is down. nothing here brings it back before the roll.".to_string(),
                ],
            };
        }
        if self.fight.is_some() {
            return Outcome {
                applied: Applied::Refused(Refusal::FightWaiting),
                lines: vec!["not with a glyph waiting on you. finish it first.".to_string()],
            };
        }
        if self.rations_left <= 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::NoRations),
                lines: vec![
                    "you are spent for today. the roll brings the signal back for nothing."
                        .to_string(),
                ],
            };
        }
        let restored = self.max_signal() - self.signal;
        if restored == 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::NothingToPatch),
                lines: vec!["nothing on you needs patching.".to_string()],
            };
        }
        let paid = self.patch_price();
        if paid > self.bits {
            let by = paid - self.bits;
            return Outcome {
                applied: Applied::Refused(Refusal::Short { by }),
                lines: vec![format!("you are {by} bits short of a patch.")],
            };
        }
        self.bits -= paid;
        self.signal = self.max_signal();
        Outcome {
            applied: Applied::Patched { restored, paid },
            lines: vec![format!(
                "patch works fast. +{restored} signal, back to full. {paid} bits."
            )],
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

    fn start(&mut self, pick: Pick) -> Outcome {
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
        let mut fight = match (pick, self.signal_hears()) {
            (Pick::Fair, true) => Fight::new(Quarry::OldSignal, OLD_SIGNAL_TIER),
            (Pick::Fair, false) => {
                let (kind, _, tier) = data::foe_for_level(self.level);
                Fight::new(Quarry::Glyph(kind), tier)
            }
            (Pick::Lower, _) => match data::lower_foe_for_level(self.level) {
                Some((kind, _, tier)) => Fight::new(Quarry::Glyph(kind), tier),
                None => return refused(Refusal::NoLowerGlyph),
            },
        };
        self.rations_left -= 1;
        fight.push(fight.foe().arrives.to_string());
        if pick == Pick::Lower {
            fight.push(STEPPED_DOWN_LINE.to_string());
        }
        let lines = fight.log.clone();
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Started { pick },
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
            self.runs_today += 1;
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

    fn win<R: Rng>(&mut self, fight: Fight, rng: &mut R, lines: Vec<String>) -> Outcome {
        match fight.quarry {
            Quarry::OldSignal => self.slay(lines),
            Quarry::Glyph(_) => self.put_down(fight, rng, lines),
        }
    }

    /// The Old Signal is down: a mark, and the climb starts over. Level,
    /// exp, gear, and bits go back to a fresh runner's and the locker is
    /// emptied; the peak, the kills, today's rations, the debt, and
    /// everything off the sheet (the look, the badges) stay. The mark's
    /// chips are owed from this moment (`unpaid_mark`), until the service
    /// settles them.
    fn slay(&mut self, mut lines: Vec<String>) -> Outcome {
        self.marks += 1;
        self.unpaid_mark = Some(self.marks);
        self.kills += 1;
        self.kills_today += 1;
        self.level = 1;
        self.exp = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.bits = START_BITS;
        self.signal = self.max_signal();
        let emptied = self.stash;
        self.stash = 0;
        lines.push(SLAIN_LINE.to_string());
        lines.push(format!(
            "you wake at the top of Static Row. level 1, bare hands, {START_BITS} bits, and mark {} that does not come off.",
            self.marks
        ));
        if emptied > 0 {
            lines.push(format!(
                "your locker stands open. the {emptied} bits in it went with the broadcast."
            ));
        }
        Outcome {
            applied: Applied::Slain { marks: self.marks },
            lines,
        }
    }

    fn put_down<R: Rng>(&mut self, fight: Fight, rng: &mut R, mut lines: Vec<String>) -> Outcome {
        let heard_before = self.signal_hears();
        let garnished = (fight.foe_bits * GARNISH_PERCENT / 100).min(self.debt);
        self.debt -= garnished;
        self.bits += fight.foe_bits - garnished;
        self.exp += fight.foe_exp;
        self.kills += 1;
        self.kills_today += 1;
        lines.push(format!(
            "{} +{} bits, +{} exp.",
            pick(rng, &KILL_LINES),
            fight.foe_bits,
            fight.foe_exp
        ));
        if garnished > 0 {
            let owed = match self.debt {
                0 => "you owe it nothing.".to_string(),
                left => format!("{left} still owed."),
            };
            lines.push(format!("the bits machine takes {garnished} of it. {owed}"));
        }
        let mut leveled = None;
        while let Some(need) = data::exp_to_advance(self.level, self.marks) {
            if self.exp < need {
                break;
            }
            self.level += 1;
            self.signal += SIGNAL_PER_LEVEL;
            leveled = Some(self.level);
        }
        self.peak_level = self.peak_level.max(self.level);
        if let Some(level) = leveled {
            lines.push(format!("you are level {level}. the street will hear."));
        }
        if !heard_before && self.signal_hears() {
            lines.push(HEARD_LINE.to_string());
        }
        Outcome {
            applied: Applied::Won {
                foe: fight.foe().name,
                bits: fight.foe_bits,
                garnished,
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
        Refusal::NoLowerGlyph => "there is nothing smaller than a flicker in there.",
        Refusal::NotAnUpgrade
        | Refusal::Short { .. }
        | Refusal::NothingToPatch
        | Refusal::FightWaiting
        | Refusal::NothingOnHand
        | Refusal::LockerEmpty
        | Refusal::LoanCapped
        | Refusal::NoDebt => {
            unreachable!("till refusals are lined where they are refused")
        }
    };
    Outcome {
        applied: Applied::Refused(refusal),
        lines: vec![line.to_string()],
    }
}

/// A till command with a glyph waiting on the row, in the till's words.
fn fight_waiting(line: &str) -> Outcome {
    Outcome {
        applied: Applied::Refused(Refusal::FightWaiting),
        lines: vec![line.to_string()],
    }
}

/// The wall: the name of `tier` in `slot`, `None` at tier 0.
pub fn gear_name(slot: Slot, tier: i32) -> Option<&'static str> {
    if !(1..=MAX_TIER).contains(&tier) {
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
