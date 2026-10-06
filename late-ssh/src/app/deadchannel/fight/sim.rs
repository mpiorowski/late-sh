//! A runner played by a rule, through the real machine (`state.rs`), so
//! the balance is a number the tests can read: how many days the climb
//! from a fresh row to the first mark takes, walked a road a day, and
//! where the runner stands at the end of every one of them. GAME.md sets
//! the target (two to three weeks) and `sim_test.rs` pins it; every change
//! to the curves, the glyph tiers, the cards, the drafts, the road, the
//! Old Signal, or the rules goes through that test. Pure: seeded dice, a
//! day counter instead of a clock, no I/O. Callable from a REPL for a look
//! at the whole ladder.
//!
//! A climb can keep a journal ([`Journal`], [`Event`]): everything the
//! runner did, in order, from the first dawn to the mark. Every shop
//! visit, every step, every card of every turn. The arena prints it as a
//! run to read (`make deadchannel-run`), which is how a rule that looks
//! right in a table gets checked against what a day of it is like.
//!
//! Also the odds of one fight (`odds`, `Threat`): the same machine played
//! to the end on the `Auto` key a few hundred times from the sheet as it
//! stands, so the threat word the road prints is the sim's number, not a
//! guess from the stat gap. It is the floor: a hand played by somebody
//! reading it beats the word.

use std::collections::HashMap;

use chrono::{Days, NaiveDate};
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::cards::{Card, DRAFTS};
use super::data::{DRINK_CRYSTALS, Intent, MAX_LEVEL, RULES, Rules};
use super::policy;
use super::road::{Node, Road};
use super::state::{Applied, Call, Command, Drink, MAX_TIER, Pick, Quarry, Sheet, Slot};

/// The card a simulated runner takes at each draft, in the order the
/// drafts come.
pub type Build = [Card; DRAFTS.len()];

/// Every build there is: one option of every draft, sixteen in all.
pub fn builds() -> Vec<Build> {
    let mut builds = vec![Vec::new()];
    for draft in &DRAFTS {
        builds = builds
            .into_iter()
            .flat_map(|picks: Vec<Card>| {
                draft.options.map(|option| {
                    let mut picks = picks.clone();
                    picks.push(option);
                    picks
                })
            })
            .collect();
    }
    builds
        .into_iter()
        .map(|picks| Build::try_from(picks).expect("a pick for every draft"))
        .collect()
}

/// The build the named players carry: the plainest one, a card that hits
/// at every draft that offers one. The arena's builds table reads every
/// other against it (`fight/BALANCE.md`).
pub const HOUSE_BUILD: Build = [Card::Jab, Card::Bulwark, Card::Ground, Card::Sever];

/// How the simulated runner plays. Every field is spelled out: a player
/// is a named rule, not a default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    /// Run from a glyph once the signal is under this fraction of the
    /// max. Never from the Old Signal: that fight is always to the end.
    pub run_under: f64,
    /// Patch, when the bits cover it, once the signal is under this
    /// fraction of the max. `1.0` patches any missing point.
    pub patch_under: f64,
    /// Step down to the glyph a level below whenever the glyph of your
    /// level reads this or worse at full signal; `None` never steps down.
    /// Never at the gate: the Old Signal is not a glyph to avoid.
    pub step_down_at: Option<Threat>,
    /// When the runner starts heeding the armorer and the road's warning.
    pub heeds: Heeds,
    /// Step in against the bright glyph, on a step one waits behind,
    /// whenever it reads this or better at full signal; `None` never
    /// does.
    pub takes_bright: Option<Threat>,
    /// The glass bought before a step whenever a crystal covers it and
    /// none is poured yet; `None` never drinks.
    pub drinks: Option<Drink>,
    /// When the runner buys the blade shop's piece.
    pub carts: Carts,
    /// What the runner holds back from the armorer.
    pub purse: Purse,
    /// Who plays the cards.
    pub hand: Hand,
    /// The card taken at each draft.
    pub build: Build,
    /// Takes the bits machine's loan whenever it owes nothing and the
    /// loan is what puts the next piece for the weaker slot in reach.
    pub borrows: bool,
}

/// How a simulated runner plays a turn of cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    /// The game's own `Auto` key, every turn (`policy::auto`): the runner
    /// who is here for the ritual.
    Auto,
    /// Reads the hand (`policy::sharp`): the runner who plays the cards.
    Sharp,
}

/// How a simulated runner splits its bits between the armorer and patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purse {
    /// Every bit goes on the best piece it covers; patch gets what is
    /// left.
    SpendAll,
    /// Keeps the price of one full patch at its level back from the
    /// armorer: gear never leaves it unable to heal.
    KeepAPatch,
}

/// When a simulated runner spends crystals at the blade shop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carts {
    Never,
    /// The weaker slot's next piece, whenever the crystals cover it.
    Always,
    /// Only once that next piece is this tier or better: the hoarder,
    /// who saves every crystal for the top of the wall.
    From(i32),
}

/// When a simulated runner starts playing by its rule for gear and for
/// the step down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heeds {
    /// From the first step: the best tier the bits cover, per slot,
    /// before every fight, and the step down by the rule.
    Always,
    /// Not until the first dropped signal teaches it; from then on,
    /// always. The runner who walked past the armorer.
    AfterFirstDrop,
}

/// Plays it safe and reads the hand: runs early, patches every point.
/// The runner the two-week end of the target is set by.
pub const CAREFUL: Player = Player {
    run_under: 0.3,
    patch_under: 0.75,
    step_down_at: None,
    heeds: Heeds::Always,
    takes_bright: None,
    drinks: None,
    carts: Carts::Never,
    purse: Purse::KeepAPatch,
    hand: Hand::Sharp,
    build: HOUSE_BUILD,
    borrows: false,
};

/// The careful runner on the `Auto` key: every fight one button, the
/// armorer and patch still visited. The daily floor (GAME.md, "The
/// round"), and the runner the three-week end of the target is set by.
pub const AMBIENT: Player = Player {
    hand: Hand::Auto,
    ..CAREFUL
};

/// Pushes it: on the `Auto` key, runs late, patches only when half gone.
/// Drops a few times on the way up.
pub const RECKLESS: Player = Player {
    run_under: 0.1,
    patch_under: 0.35,
    purse: Purse::SpendAll,
    ..AMBIENT
};

/// Walks past the armorer and the road's warning until the street takes
/// everything, then plays carefully and steps down from a grim fight: the
/// runner who found the lock (bare hands at level 2, no bits, the hiss in
/// front of them).
pub const NEGLECTFUL: Player = Player {
    step_down_at: Some(Threat::Grim),
    heeds: Heeds::AfterFirstDrop,
    ..AMBIENT
};

/// The careful runner who also plays the crystal pass: routes to the
/// bright glyph when it reads even or better, buys the blade shop's
/// pieces, and drinks static on ice with what is left. What the crystals
/// are worth, measured against [`CAREFUL`].
pub const KEEN: Player = Player {
    takes_bright: Some(Threat::Even),
    drinks: Some(Drink::StaticOnIce),
    carts: Carts::Always,
    ..CAREFUL
};

/// The keen runner who never drinks and never buys a cheap blade: every
/// crystal waits for the top third of the wall, where three of them are
/// worth the most bits. What hoarding is worth, measured against
/// [`KEEN`].
pub const HOARDER: Player = Player {
    drinks: None,
    carts: Carts::From(11),
    ..KEEN
};

/// The ambient runner who takes the machine's loan whenever it puts a
/// piece in reach: what the loan is worth, and what its garnish costs.
pub const BORROWER: Player = Player {
    borrows: true,
    ..AMBIENT
};

/// The players the balance is read from, by name: the report and the
/// sweep print every one.
pub const PLAYERS: [(&str, Player); 7] = [
    ("careful", CAREFUL),
    ("ambient", AMBIENT),
    ("reckless", RECKLESS),
    ("neglectful", NEGLECTFUL),
    ("keen", KEEN),
    ("hoarder", HOARDER),
    ("borrower", BORROWER),
];

/// One climb from a fresh row, day 1 to the first mark or to `max_days`.
/// Days count from 1; `None` is "not within `max_days`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Climb {
    /// The day each level was first reached, indexed by level; entries 0
    /// and 1 are unused.
    pub level_on: [Option<u32>; MAX_LEVEL as usize + 1],
    /// The day the Old Signal first heard the runner (the gate open).
    pub heard_on: Option<u32>,
    /// The day the Old Signal was put down.
    pub marked_on: Option<u32>,
    /// Dropped signals along the way.
    pub deaths: u32,
    /// The day of the first dropped signal.
    pub first_drop_on: Option<u32>,
    /// The first day after the first drop that a level was gained: how
    /// long the drop kept the runner stuck.
    pub recovered_on: Option<u32>,
    /// Steps into the static that met the Old Signal.
    pub boss_tries: u32,
    /// The weapon and armor tiers carried on leaving each level (at the
    /// kill that gained the next one, or at the mark for the last),
    /// indexed by level like `level_on`: the kit the level was fought in.
    pub kit_on: [Option<(i32, i32)>; MAX_LEVEL as usize + 1],
    /// Where the bits and the crystals went.
    pub ledger: Ledger,
    /// Every day played, the first day first: the climb as a curve.
    pub days: Vec<Day>,
    /// Turns of cards played, and fights they were played in.
    pub turns: u32,
    pub fights: u32,
}

/// One day of a climb: where the runner stood when it ended, and what
/// the day held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Day {
    /// At dusk. The level on the day of the mark is the top one: the
    /// reset is the next climb's.
    pub level: i32,
    pub signal: i32,
    pub weapon_tier: i32,
    pub armor_tier: i32,
    /// Bits on hand.
    pub bits: i64,
    pub crystals: i32,
    /// Cards drafted so far.
    pub drafted: usize,
    /// Glyphs put down, and how many of them were bright.
    pub kills: u32,
    pub bright_kills: u32,
    pub runs: u32,
    /// The signal dropped: the day ended there.
    pub dropped: bool,
    /// Bits the day paid, and where they went.
    pub earned: i64,
    pub gear: i64,
    pub patched: i64,
    /// Steps of the road taken.
    pub steps: u32,
}

/// One climb's money, summed: what the glyphs paid and where it went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Bits the glyphs and the caches paid, before the machine's garnish.
    pub earned: i64,
    /// The caches' share of that.
    pub cached: i64,
    /// Bits paid at the armorer, net of trade-ins.
    pub gear: i64,
    /// Bits paid at patch.
    pub patched: i64,
    /// Bits the street took with a dropped signal.
    pub dropped: i64,
    /// Bits the machine lent, and what its fees and nothing else cost.
    pub borrowed: i64,
    pub loan_fees: i64,
    pub crystals_found: i32,
    pub crystals_spent: i32,
    /// Steps in against a bright glyph, and how many put it down.
    pub bright_tries: u32,
    pub bright_kills: u32,
}

/// The sheet at one moment of a climb, for the journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub level: i32,
    pub exp: i64,
    pub signal: i32,
    pub max_signal: i32,
    pub weapon_tier: i32,
    pub armor_tier: i32,
    pub bits: i64,
    pub debt: i64,
    pub crystals: i32,
    pub static_cards: u8,
    pub deck: Vec<Card>,
}

impl Snapshot {
    fn of(sheet: &Sheet) -> Self {
        Self {
            level: sheet.level,
            exp: sheet.exp,
            signal: sheet.signal,
            max_signal: sheet.max_signal(),
            weapon_tier: sheet.weapon_tier,
            armor_tier: sheet.armor_tier,
            bits: sheet.bits,
            debt: sheet.debt,
            crystals: sheet.crystals,
            static_cards: sheet.road.static_cards,
            deck: sheet.deck(),
        }
    }
}

/// One thing a climbing runner did, in the order it happened: what a
/// journal is made of. A closed list, so a new thing the sim does cannot
/// be left out of the run the arena prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A day begins: the roll has refilled the signal and the rations.
    Dawn { day: u32, sheet: Snapshot },
    Drafted { card: Card, over: Card },
    Patched { restored: i32, paid: i64 },
    Borrowed { amount: i64, fee: i64 },
    Outfitted { slot: Slot, tier: i32, paid: i64 },
    Carted { slot: Slot, tier: i32, crystals: i32 },
    Drank { drink: Drink },
    /// A step down the road, onto `node`.
    Step { step: i32, lane: u8, node: Node },
    Mended { restored: i32, signal: i32 },
    Cleared { cards: usize },
    Cached { bits: i64 },
    /// A fight begins.
    Met {
        foe: String,
        pick: Pick,
        boss: bool,
        foe_signal: i32,
        /// Its plain hit; a heavy is twice it.
        hit: i32,
        signal: i32,
        static_cards: u8,
    },
    /// One turn: what the glyph showed, the hand dealt, the cards played
    /// in order, and where both stood once it had moved.
    Turn {
        intent: Intent,
        hand: Vec<Card>,
        played: Vec<Card>,
        /// After the cards, before the glyph moved.
        foe_signal: i32,
        block: i32,
        /// The runner's, after the glyph moved.
        signal: i32,
    },
    Ran { signal: i32 },
    Won {
        turns: u32,
        signal: i32,
        bits: i64,
        exp: i64,
        crystals: i32,
        leveled: Option<i32>,
        static_cards: u8,
    },
    Fell { turns: u32, bits_lost: i64 },
    /// The Old Signal put down.
    Slain { turns: u32 },
    /// The day ends.
    Dusk { day: u32, sheet: Snapshot, today: Day },
}

/// Where a climb writes what it did. `Off` for the batches the bands are
/// read from, which only want the `Climb`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Journal {
    Off,
    On(Vec<Event>),
}

impl Journal {
    /// `event` onto the journal, built only when one is being kept.
    fn note(&mut self, event: impl FnOnce() -> Event) {
        match self {
            Journal::Off => {}
            Journal::On(events) => events.push(event()),
        }
    }
}

/// The bench a batch of climbs is played at: the rules, and what each
/// pick reads at full signal under them, by level, gear, deck, marks, and
/// glass, remembered. The step-down and bright rules ask before every
/// fight, the answer only moves when one of those does, and it is the same
/// answer on every seed (the odds run on fixed dice), so one bench serves
/// a whole batch. One bench per set of rules: the memo is only true under
/// its own.
pub struct Bench {
    pub rules: Rules,
    threats: HashMap<ThreatKey, Threat>,
}

/// Everything a pick's threat reads off the sheet, so the memo is only
/// reused when none of it moved.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ThreatKey {
    pick: Pick,
    level: i32,
    weapon_tier: i32,
    armor_tier: i32,
    marks: i32,
    drink: Option<Drink>,
    cards: Vec<Card>,
}

impl Bench {
    /// The live game.
    pub fn live() -> Self {
        Self::under(RULES)
    }

    /// A candidate set of rules.
    pub fn under(rules: Rules) -> Self {
        Self {
            rules,
            threats: HashMap::new(),
        }
    }

    fn threat(&mut self, pick: Pick, sheet: &Sheet) -> Threat {
        let key = ThreatKey {
            pick,
            level: sheet.level,
            weapon_tier: sheet.weapon_tier,
            armor_tier: sheet.armor_tier,
            marks: sheet.marks,
            drink: sheet.drink,
            cards: sheet.cards.clone(),
        };
        let rules = self.rules;
        *self
            .threats
            .entry(key)
            .or_insert_with(|| fresh_threat(&rules, sheet, pick))
    }
}

/// A climb in progress: the runner, the dice, and what is being kept of
/// it.
struct Run<'a> {
    player: Player,
    rules: Rules,
    sheet: Sheet,
    rng: StdRng,
    report: Climb,
    journal: &'a mut Journal,
    day: u32,
    today: Day,
}

/// Play `player` from a fresh row with `seed` for at most `max_days`,
/// writing what it does to `journal`. Each day: settle the roll, then
/// walk the day's road until the rations are gone or the signal is down.
/// Before every step the runner shops ([`Run::shop`]: the draft owed,
/// patch, the loan, the armorer, the blade shop, the bar, each by its
/// rule), then picks the lane ([`route`]) and what to do there, and fights
/// what answers by the rule.
pub fn climb(
    player: Player,
    seed: u64,
    max_days: u32,
    bench: &mut Bench,
    journal: &mut Journal,
) -> Climb {
    let mut report = Climb {
        level_on: [None; MAX_LEVEL as usize + 1],
        heard_on: None,
        marked_on: None,
        deaths: 0,
        first_drop_on: None,
        recovered_on: None,
        boss_tries: 0,
        kit_on: [None; MAX_LEVEL as usize + 1],
        ledger: Ledger::default(),
        days: Vec::new(),
        turns: 0,
        fights: 0,
    };
    report.level_on[1] = Some(1);
    let mut run = Run {
        player,
        rules: bench.rules,
        sheet: Sheet::fresh(Uuid::nil(), date(1)),
        rng: StdRng::seed_from_u64(seed),
        report,
        journal,
        day: 0,
        today: Day::default(),
    };
    for day in 1..=max_days {
        run.dawn(day);
        let road = run.sheet.todays_road();
        while run.sheet.shut().is_none() {
            run.shop();
            if run.step(&road, bench) {
                run.dusk();
                return run.report;
            }
        }
        run.dusk();
    }
    run.report
}

impl Run<'_> {
    fn heeding(&self) -> bool {
        match self.player.heeds {
            Heeds::Always => true,
            Heeds::AfterFirstDrop => self.report.first_drop_on.is_some(),
        }
    }

    fn act(&mut self, command: Command) -> Applied {
        self.sheet
            .apply_under(&self.rules, command, &mut self.rng)
            .applied
    }

    fn dawn(&mut self, day: u32) {
        self.day = day;
        self.today = Day::default();
        self.sheet.settle(date(day));
        let sheet = &self.sheet;
        self.journal.note(|| Event::Dawn {
            day,
            sheet: Snapshot::of(sheet),
        });
    }

    fn dusk(&mut self) {
        let marked = self.report.marked_on == Some(self.day);
        self.today = Day {
            level: match marked {
                true => MAX_LEVEL,
                false => self.sheet.level,
            },
            signal: self.sheet.signal,
            weapon_tier: self.sheet.weapon_tier,
            armor_tier: self.sheet.armor_tier,
            bits: self.sheet.bits,
            crystals: self.sheet.crystals,
            drafted: self.sheet.cards.len(),
            ..self.today
        };
        self.report.days.push(self.today);
        let (day, today, sheet) = (self.day, self.today, &self.sheet);
        self.journal.note(|| Event::Dusk {
            day,
            sheet: Snapshot::of(sheet),
            today,
        });
    }

    /// Everything a runner does between two steps, in the order a person
    /// would: the card owed, then patch (the heal is what the next fight
    /// is fought on, and the armorer only sees what the purse rule
    /// leaves), the loan, the armorer, the blade shop, the bar.
    fn shop(&mut self) {
        while let Some(draft) = self.sheet.draft() {
            let card = self.player.build[self.sheet.cards.len()];
            match self.act(Command::Draft { card }) {
                Applied::Drafted { card, replaces } => {
                    debug_assert_eq!(replaces, draft.replaces);
                    self.journal.note(|| Event::Drafted {
                        card,
                        over: replaces,
                    });
                }
                other => panic!("a draft of {card:?} answered {other:?}"),
            }
        }
        let patch_line =
            (f64::from(self.sheet.max_signal()) * self.player.patch_under).ceil() as i32;
        if self.sheet.signal < patch_line
            && let Applied::Patched { restored, paid } = self.act(Command::Patch)
        {
            self.report.ledger.patched += paid;
            self.today.patched += paid;
            self.journal.note(|| Event::Patched { restored, paid });
        }
        if self.heeding() {
            if self.player.borrows {
                self.borrow();
            }
            self.outfit();
        }
        let slot = match self.sheet.weapon_tier <= self.sheet.armor_tier {
            true => Slot::Weapon,
            false => Slot::Armor,
        };
        let carts = match (self.player.carts, self.sheet.cart_tier(slot)) {
            (Carts::Never, _) | (_, None) => false,
            (Carts::Always, Some(_)) => true,
            (Carts::From(from), Some(tier)) => tier >= from,
        };
        if carts
            && let Applied::Carted {
                slot,
                tier,
                crystals,
            } = self.act(Command::Cart { slot })
        {
            self.report.ledger.crystals_spent += crystals;
            self.journal.note(|| Event::Carted {
                slot,
                tier,
                crystals,
            });
        }
        if let Some(drink) = self.player.drinks
            && let Applied::Drank { drink } = self.act(Command::Drink { drink })
        {
            self.report.ledger.crystals_spent += DRINK_CRYSTALS;
            self.journal.note(|| Event::Drank { drink });
        }
    }

    /// The loan, when the runner owes nothing and the bits on hand do not
    /// reach the weaker slot's next piece but the loan would carry them
    /// there.
    fn borrow(&mut self) {
        if self.sheet.debt > 0 {
            return;
        }
        let slot = match self.sheet.weapon_tier <= self.sheet.armor_tier {
            true => Slot::Weapon,
            false => Slot::Armor,
        };
        let next = self.sheet.tier_of(slot) + 1;
        if next > MAX_TIER {
            return;
        }
        let price = self.sheet.outfit_price_under(&self.rules, slot, next);
        let reach = self.sheet.bits + self.sheet.loan_room();
        if self.sheet.bits < price
            && reach >= price
            && let Applied::Borrowed { amount, fee } = self.act(Command::Borrow)
        {
            self.report.ledger.borrowed += amount;
            self.report.ledger.loan_fees += fee;
            self.journal.note(|| Event::Borrowed { amount, fee });
        }
    }

    /// The best tier the purse covers, for each slot, the weaker slot
    /// first, bought: the wall walked from the top down to the first
    /// piece whose price leaves the purse rule's reserve.
    fn outfit(&mut self) {
        let reserve = match self.player.purse {
            Purse::SpendAll => 0,
            Purse::KeepAPatch => {
                i64::from(self.sheet.max_signal())
                    * i64::from(self.sheet.level)
                    * self.rules.patch_percent
                    / 100
            }
        };
        let slots = match self.sheet.weapon_tier <= self.sheet.armor_tier {
            true => [Slot::Weapon, Slot::Armor],
            false => [Slot::Armor, Slot::Weapon],
        };
        for slot in slots {
            let affordable = (self.sheet.tier_of(slot) + 1..=MAX_TIER).rev().find(|tier| {
                self.sheet.outfit_price_under(&self.rules, slot, *tier) + reserve
                    <= self.sheet.bits
            });
            if let Some(tier) = affordable
                && let Applied::Outfitted { slot, tier, paid } =
                    self.act(Command::Outfit { slot, tier })
            {
                self.report.ledger.gear += paid;
                self.today.gear += paid;
                self.journal.note(|| Event::Outfitted { slot, tier, paid });
            }
        }
    }

    /// One step down the road, and the fight it meets played out. True
    /// when that fight was the Old Signal's and it is down: the climb is
    /// over.
    fn step(&mut self, road: &Road, bench: &mut Bench) -> bool {
        let heeding = self.heeding();
        let gate = self.sheet.signal_hears();
        if gate && self.report.heard_on.is_none() {
            self.report.heard_on = Some(self.day);
        }
        let (lane, node) = route(&self.player, &self.sheet, road, bench);
        let call = match node {
            Node::Glyph => {
                let lower = match (heeding, gate, self.player.step_down_at) {
                    (true, false, Some(line)) => {
                        self.sheet.level > 1 && bench.threat(Pick::Fair, &self.sheet) >= line
                    }
                    (true, true, _) | (true, false, None) | (false, _, _) => false,
                };
                Call::Fight(match lower {
                    true => Pick::Lower,
                    false => Pick::Fair,
                })
            }
            Node::Bright => Call::Fight(Pick::Bright),
            Node::Rest => {
                let missing = self.sheet.max_signal() - self.sheet.signal;
                // The deck when the signal is nearly whole and there is
                // static to shake out; the signal otherwise.
                match self.sheet.road.static_cards > 0 && missing * 6 < self.sheet.max_signal() {
                    true => Call::Clear,
                    false => Call::Mend,
                }
            }
            Node::Cache => Call::Take,
        };
        let step = self.sheet.step();
        self.journal.note(|| Event::Step { step, lane, node });
        self.today.steps += 1;
        let kit = (self.sheet.weapon_tier, self.sheet.armor_tier);
        let pick = match self.act(Command::Step { lane, call }) {
            Applied::Started { pick } => pick,
            Applied::Mended { restored } => {
                let signal = self.sheet.signal;
                self.journal.note(|| Event::Mended { restored, signal });
                return false;
            }
            Applied::Cleared { cards } => {
                self.journal.note(|| Event::Cleared { cards });
                return false;
            }
            Applied::Cached { bits, .. } => {
                self.report.ledger.earned += bits;
                self.report.ledger.cached += bits;
                self.today.earned += bits;
                self.journal.note(|| Event::Cached { bits });
                return false;
            }
            other => panic!("a step answered {other:?}"),
        };
        let fight = self.sheet.fight.as_ref().expect("a fight just started");
        let boss = fight.quarry == Quarry::OldSignal;
        if boss {
            self.report.boss_tries += 1;
        }
        if pick == Pick::Bright {
            self.report.ledger.bright_tries += 1;
        }
        self.report.fights += 1;
        {
            let sheet = &self.sheet;
            let rules = &self.rules;
            self.journal.note(|| Event::Met {
                foe: fight.name(),
                pick,
                boss,
                foe_signal: fight.foe_signal,
                hit: sheet.powers_under(rules, fight).hit,
                signal: sheet.signal,
                static_cards: sheet.road.static_cards,
            });
        }
        let run_line = (f64::from(self.sheet.max_signal()) * self.player.run_under).ceil() as i32;
        let mut turns = 0;
        loop {
            // Running takes what the glyph meant to do this turn: a
            // runner under its line still stays and plays the turn when
            // the way out would be the end of the day.
            let fight = self.sheet.fight.as_ref().expect("a fight in progress");
            let table = policy::Table::read(&self.sheet, &self.rules, fight);
            let way_out = match fight.intent() {
                Intent::Hit => table.powers.hit,
                Intent::Heavy => table.powers.hit * 2,
                Intent::Charge | Intent::Noise => 0,
            } - fight.block;
            let runs = !boss && self.sheet.signal < run_line && way_out < self.sheet.signal;
            let applied = match runs {
                true => self.act(Command::Run),
                false => {
                    self.report.turns += 1;
                    turns += 1;
                    let intent = fight.intent();
                    let hand: Vec<Card> = fight.piles.hand.iter().flatten().copied().collect();
                    let turn = play_turn(
                        &mut self.sheet,
                        &self.rules,
                        self.player.hand,
                        &mut self.rng,
                    );
                    let signal = self.sheet.signal;
                    self.journal.note(|| Event::Turn {
                        intent,
                        hand,
                        played: turn.played,
                        foe_signal: turn.foe_signal,
                        block: turn.block,
                        signal,
                    });
                    turn.applied
                }
            };
            match applied {
                Applied::Round => {}
                Applied::Escaped => {
                    self.today.runs += 1;
                    let signal = self.sheet.signal;
                    self.journal.note(|| Event::Ran { signal });
                    return false;
                }
                Applied::Won {
                    leveled,
                    bits,
                    exp,
                    bright,
                    crystals,
                    ..
                } => {
                    self.report.ledger.earned += bits;
                    self.report.ledger.crystals_found += crystals;
                    self.today.earned += bits;
                    self.today.kills += 1;
                    if bright {
                        self.report.ledger.bright_kills += 1;
                        self.today.bright_kills += 1;
                    }
                    if let Some(level) = leveled {
                        self.report.level_on[level as usize].get_or_insert(self.day);
                        self.report.kit_on[level as usize - 1].get_or_insert(kit);
                        if self.report.first_drop_on.is_some() {
                            self.report.recovered_on.get_or_insert(self.day);
                        }
                    }
                    let (signal, static_cards) = (self.sheet.signal, self.sheet.road.static_cards);
                    self.journal.note(|| Event::Won {
                        turns,
                        signal,
                        bits,
                        exp,
                        crystals,
                        leveled,
                        static_cards,
                    });
                    return false;
                }
                Applied::Lost { bits_lost } => {
                    self.report.ledger.dropped += bits_lost;
                    self.report.deaths += 1;
                    self.report.first_drop_on.get_or_insert(self.day);
                    self.today.dropped = true;
                    self.journal.note(|| Event::Fell { turns, bits_lost });
                    return false;
                }
                Applied::Slain { .. } => {
                    self.report.kit_on[MAX_LEVEL as usize] = Some(kit);
                    self.report.marked_on = Some(self.day);
                    self.today.kills += 1;
                    self.journal.note(|| Event::Slain { turns });
                    return true;
                }
                other => panic!("a fight command answered {other:?}"),
            }
        }
    }
}

/// One turn of cards, as it was played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnPlayed {
    /// How the turn settled: `Round` with both standing, or the fight's
    /// end.
    pub applied: Applied,
    /// The cards played, in order.
    pub played: Vec<Card>,
    /// The glyph's signal and the block standing after the cards, before
    /// the glyph moved.
    pub foe_signal: i32,
    pub block: i32,
}

/// One turn of the fight on `sheet`, played by `hand` a card at a time
/// and ended. The `Auto` hand is the `Auto` key's turn exactly
/// (`Sheet::auto` plays `policy::auto` the same way; `sim_test.rs` holds
/// the two together), spelled out here so the journal can name the cards.
pub fn play_turn(sheet: &mut Sheet, rules: &Rules, hand: Hand, rng: &mut StdRng) -> TurnPlayed {
    let fight = sheet.fight.as_ref().expect("a fight to play a turn of");
    let table = policy::Table::read(sheet, rules, fight);
    let plays = match hand {
        Hand::Auto => policy::auto(&table),
        Hand::Sharp => policy::sharp(&table),
    };
    let mut played = Vec::with_capacity(plays.len());
    for slot in plays {
        let slot = slot as u8;
        match sheet
            .apply_under(rules, Command::Play { slot }, rng)
            .applied
        {
            Applied::Played { card } => played.push(card),
            ended @ (Applied::Won { .. } | Applied::Slain { .. }) => {
                return TurnPlayed {
                    applied: ended,
                    played,
                    foe_signal: 0,
                    block: 0,
                };
            }
            other => panic!("a played card answered {other:?}"),
        }
    }
    let fight = sheet.fight.as_ref().expect("the fight is still on");
    let (foe_signal, block) = (fight.foe_signal, fight.block);
    TurnPlayed {
        applied: sheet.apply_under(rules, Command::EndTurn, rng).applied,
        played,
        foe_signal,
        block,
    }
}

/// How far ahead a simulated runner reads the road when it picks a lane.
const ROUTE_DEPTH: i32 = 3;
/// What a step further off is worth against the one in front.
const ROUTE_FADE: f64 = 0.6;

/// The lane a simulated runner steps to, and what waits there: the one
/// whose next few steps are worth the most to the runner as it stands. A
/// bright glyph is worth a detour to the player who takes them and is
/// walked around by the one who does not; a rest is worth what the signal
/// and the deck are missing; a cache is a cache.
fn route(player: &Player, sheet: &Sheet, road: &Road, bench: &mut Bench) -> (u8, Node) {
    let wants_bright = match player.takes_bright {
        Some(line) => bench.threat(Pick::Bright, sheet) <= line,
        None => false,
    };
    let missing = 1.0 - f64::from(sheet.signal) / f64::from(sheet.max_signal().max(1));
    let worth = |node: Node| match node {
        Node::Glyph => 0.0,
        Node::Bright => match wants_bright {
            true => 3.0,
            false => -3.0,
        },
        Node::Rest => missing * 4.0 + f64::from(sheet.road.static_cards) * 0.6,
        Node::Cache => 1.0,
    };
    fn best(road: &Road, worth: &dyn Fn(Node) -> f64, step: i32, from: u8, depth: i32) -> f64 {
        let Some(node) = road.node(step, from) else {
            return 0.0;
        };
        let ahead = match depth > 1 {
            true => (0..super::road::LANES as u8)
                .filter(|lane| lane.abs_diff(from) <= 1)
                .map(|lane| best(road, worth, step + 1, lane, depth - 1))
                .fold(f64::MIN, f64::max),
            false => 0.0,
        };
        worth(node) + ROUTE_FADE * ahead
    }
    let step = sheet.step();
    let lane = sheet
        .road
        .open_lanes()
        .into_iter()
        .map(|lane| (best(road, &worth, step, lane, ROUTE_DEPTH), lane))
        // The first of equals: the top lane, so a tie reads the same on
        // every seed.
        .fold(None, |top: Option<(f64, u8)>, (score, lane)| match top {
            Some((best, _)) if best >= score => top,
            Some(_) | None => Some((score, lane)),
        })
        .map(|(_, lane)| lane)
        .expect("a road always has a lane open");
    (
        lane,
        road.node(step, lane).expect("an open lane is on the road"),
    )
}

/// `pick`'s threat for `sheet`'s runner at full signal with a clean deck,
/// a spent day's rations topped up so the estimate is about the fight,
/// not the day.
fn fresh_threat(rules: &Rules, sheet: &Sheet, pick: Pick) -> Threat {
    let mut rested = sheet.clone();
    rested.signal = rested.max_signal();
    rested.rations_left = rested.rations_left.max(1);
    rested.road.static_cards = 0;
    let odds = odds_under(rules, &rested, pick, ODDS_FIGHTS).expect("a rested runner can step in");
    Threat::of(odds)
}

/// How a fight reads before stepping in, worst last, from the chance of
/// winning it fought to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Threat {
    /// Nine in ten or better.
    Easy,
    /// Six in ten or better.
    Even,
    /// Three in ten or better.
    Risky,
    /// Worse than three in ten.
    Grim,
}

impl Threat {
    pub fn of(odds: f64) -> Self {
        match odds {
            o if o >= 0.9 => Threat::Easy,
            o if o >= 0.6 => Threat::Even,
            o if o >= 0.3 => Threat::Risky,
            _ => Threat::Grim,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Threat::Easy => "easy",
            Threat::Even => "even",
            Threat::Risky => "risky",
            Threat::Grim => "grim",
        }
    }
}

/// Fights the odds are read from: enough that the word holds still across
/// the threshold noise, few enough to read on a key press.
pub const ODDS_FIGHTS: u32 = 200;
/// Fixed dice for the odds: the same sheet always reads the same word.
const ODDS_SEED: u64 = 0x5eed_0dd5;
/// A fight that has not ended in this many turns counts as lost.
const ODDS_TURNS: u32 = 60;

/// The chance of winning `pick`, played to the end on the `Auto` key
/// without running, from `sheet` as it stands (its signal, its gear, the
/// static in its deck), over `fights` seeded fights through the real
/// machine. `None` when no fight would start: the signal is down, the
/// rations are spent, a fight is already waiting, or there is nothing
/// below the flicker.
pub fn odds(sheet: &Sheet, pick: Pick, fights: u32) -> Option<f64> {
    odds_under(&RULES, sheet, pick, fights)
}

/// [`odds`] under a candidate set of rules.
pub fn odds_under(rules: &Rules, sheet: &Sheet, pick: Pick, fights: u32) -> Option<f64> {
    odds_played(rules, sheet, pick, Hand::Auto, fights)
}

/// [`odds_under`] with the cards played by `hand`: the arena's measure of
/// what reading the hand is worth.
pub fn odds_played(
    rules: &Rules,
    sheet: &Sheet,
    pick: Pick,
    hand: Hand,
    fights: u32,
) -> Option<f64> {
    if sheet.fight.is_some() || sheet.shut().is_some() {
        return None;
    }
    let mut rng = StdRng::seed_from_u64(ODDS_SEED);
    let mut wins = 0u32;
    for _ in 0..fights {
        let mut trial = sheet.clone();
        match trial.engage(rules, pick, &mut rng).applied {
            Applied::Started { .. } => {}
            Applied::Refused(_) => return None,
            other => panic!("a fight started with {other:?}"),
        }
        for _ in 0..ODDS_TURNS {
            match play_turn(&mut trial, rules, hand, &mut rng).applied {
                Applied::Round => continue,
                Applied::Won { .. } | Applied::Slain { .. } => {
                    wins += 1;
                    break;
                }
                Applied::Lost { .. } => break,
                other => panic!("a turn answered {other:?}"),
            }
        }
    }
    Some(f64::from(wins) / f64::from(fights))
}

/// The median of `days`, a climb that never got there counting as one
/// day past `max_days` so it drags the median out instead of vanishing.
pub fn median(days: impl Iterator<Item = Option<u32>>, max_days: u32) -> u32 {
    let mut sorted: Vec<u32> = days.map(|day| day.unwrap_or(max_days + 1)).collect();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// The ladder at a glance: the median day of every level, the gate, and
/// the mark across `climbs`. For the failing test's message and the REPL.
pub fn summary(climbs: &[Climb], max_days: u32) -> String {
    let mut lines = Vec::new();
    for level in 2..=MAX_LEVEL as usize {
        let day = median(climbs.iter().map(|climb| climb.level_on[level]), max_days);
        lines.push(format!("level {level:>2} on day {day}"));
    }
    lines.push(format!(
        "heard on day {}",
        median(climbs.iter().map(|climb| climb.heard_on), max_days)
    ));
    lines.push(format!(
        "marked on day {}",
        median(climbs.iter().map(|climb| climb.marked_on), max_days)
    ));
    lines.push(format!(
        "deaths {} (median), boss tries {} (median)",
        median(climbs.iter().map(|climb| Some(climb.deaths)), max_days),
        median(climbs.iter().map(|climb| Some(climb.boss_tries)), max_days)
    ));
    lines.join("\n")
}

/// The calendar day a climb's `day` (from 1) is played on.
pub fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 1)
        .expect("a real date")
        .checked_add_days(Days::new(u64::from(day)))
        .expect("a day within the calendar")
}

#[cfg(test)]
#[path = "sim_test.rs"]
mod sim_test;
