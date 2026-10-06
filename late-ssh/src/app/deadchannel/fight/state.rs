//! The fight's rules: a pure state machine over one runner row's worth of
//! sheet (GAME.md, "The stat block") and the fight on it. No I/O, no
//! clock reads; the day and the dice are handed in. The service locks the
//! row, builds a `Sheet`, applies one command, and stores the result; the
//! session keeps a mirror and never decides anything from it.
//!
//! The static at the end of Static Row is where the road starts: the
//! day's ten rations are ten steps down it (`road.rs`, one road a day,
//! the same for every runner), and a step is something you ask for, never
//! something that happens to you (the city is the wallet; nothing there
//! can be missed). Half the steps are fights; the rest are a rest or a
//! cache, taken with one key.
//!
//! A fight is a round of cards (`cards.rs`; GAME.md, "The round"): five
//! drawn, three energy, the glyph's next move shown before you play.
//! Strikes hit for what the weapon makes them, blocks hold for what the
//! armor makes them and stay up until a hit eats them, and a hit that
//! lands puts static in the deck for the rest of the day's road. No dice
//! but the shuffle: block or race is a thing you decide, not a roll.
//! `Auto` plays the obvious turn (`policy.rs`), so the round is one key
//! for the runner who wants it to be.
//!
//! The deck changes four times on the way up (`cards::DRAFTS`): at a
//! draft's level the runner is owed one of two cards, the same two for
//! everybody, and the road waits until one is taken (`Command::Draft`).
//! The picks are on the row (`Sheet::cards`); a mark and the ledge take
//! them back with the level.
//!
//! The top of the ladder is the Old Signal (GAME.md, "Marks: the reset"):
//! at level 15 with the exp to leave it, the next glyph's node meets it
//! instead of a glyph. Putting it down leaves a mark and resets the runner to
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
//! beat, so a step onto a glyph picks its quarry (`Pick`): the glyph of
//! your level, or the one a level down at half pay, the way back from a
//! fight you cannot win (LoGD's slumming, cut).
//!
//! The armorer's till is here too (GAME.md, "Gear: two slots, fifteen
//! tiers"): the same sheet, the same lock, one more command. Bits buy a
//! tier above the one you carry; the piece you hand back comes off the
//! price at `TRADE_IN_PERCENT`. The wall is open top to bottom: what keeps
//! the kit level with the runner is the price, never a gate
//! (`fight/BALANCE.md`). The catalog (names, the price ladder) is
//! the city's, in `city/data.rs`, because the wall is the city's.
//!
//! So are the lockers and the bits machine, the two money places: the
//! locker keeps bits from the street for a cut on the way in, the machine
//! lends against the level, adds its fee to the debt once, and takes its
//! share off every kill. And the ledge: a step off it is the runner
//! started over, the marks and the debt kept.
//!
//! And the crystal pass (GAME.md): a kill now and then leaves a crystal,
//! the one thing bits cannot buy; two nodes of every day's road are a
//! bright glyph, harder, double pay, and always carrying one; Dead Air pours a glass for a crystal that lasts until the roll;
//! the blade shop sells the next tier up for crystals alone.
//!
//! Every number a balance pass turns comes in through `data::Rules`:
//! `apply` plays the live `RULES`, `apply_under` any candidate set, which
//! is how the sim and the arena compare one with another.

use chrono::NaiveDate;
use late_core::models::deadchannel_runner::{DeadchannelRunner, SheetWrite};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::cards::{self, Board, Card, Draft, ENERGY, Piles};
use super::data::{
    self, BRIGHT_LINE, CACHE_LINES, CLEAR_LINE, CRYSTAL_LINE, DRAFT_LINE, DRINK_CRYSTALS,
    DRINK_SIGNAL_PER_LEVEL, DROP_LINES, FOES, FoeKind, FoeTier, GARNISH_PERCENT, HEARD_LINE,
    Intent, KILL_LINES, LOAN_FEE_PERCENT, LOAN_PER_LEVEL, LOCKER_FEE_PERCENT, MARK_BONUS_CAP,
    MAX_LEVEL, MEND_LINE, NEAR_MISS_SIGNAL, NOISE_CARDS, OLD_SIGNAL, RATIONS_PER_DAY, RULES,
    RUN_LINES, Rules, SIGNAL_PER_LEVEL, SLAIN_LINE, START_BITS, STATIC_LINE, STEPPED_DOWN_LINE,
    percent_up,
};
use super::policy;
use super::road::{self, Mark, Node, Road, RoadRun, Trace};
use crate::app::deadchannel::city::data::{ARMOR, COST_LADDER, WEAPONS};

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
    /// A bright glyph: its numbers above are already the lifted ones; this
    /// names it and is why it leaves a crystal.
    pub bright: bool,
    /// Turns the glyph has taken: where it is in its pattern.
    pub turn: u32,
    /// Energy left this turn.
    pub energy: u8,
    /// Block standing: it holds until a hit eats it.
    pub block: i32,
    /// A mute was played this turn: the glyph's move does nothing.
    pub muted: bool,
    pub piles: Piles,
}

/// What the cards and the glyph are worth in one fight, from the sheet
/// and the foe as they stand: the numbers printed on the hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Powers {
    pub strike: i32,
    pub surge: i32,
    pub block: i32,
    /// The glyph's hit before your block; a heavy is twice it.
    pub hit: i32,
    /// The drafted cards': a jab's hit, what a siphon mends, a bulwark's
    /// hold, a burn's energy before static feeds it, what a ground adds
    /// for each static card in the hand, and a sever's hit once the
    /// glyph is at half.
    pub jab: i32,
    pub mend: i32,
    pub bulwark: i32,
    pub burn: u8,
    pub ground: i32,
    pub sever: i32,
}

impl Fight {
    fn new<R: Rng>(
        quarry: Quarry,
        tier: FoeTier,
        deck: Vec<Card>,
        static_cards: usize,
        rng: &mut R,
    ) -> Self {
        Self {
            quarry,
            foe_signal: tier.signal,
            foe_max_signal: tier.signal,
            foe_attack: tier.attack,
            foe_defense: tier.defense,
            foe_bits: tier.bits,
            foe_exp: tier.exp,
            log: Vec::new(),
            bright: false,
            turn: 0,
            energy: ENERGY,
            block: 0,
            muted: false,
            piles: Piles::deal(deck, static_cards, rng),
        }
    }

    /// The table as a card played now would find it.
    pub fn board(&self) -> Board {
        Board {
            block: self.block,
            foe_signal: self.foe_signal,
            foe_max_signal: self.foe_max_signal,
            statics_in_hand: self.piles.statics_in_hand(),
        }
    }

    /// What the glyph means to do with the turn it is on.
    pub fn intent(&self) -> Intent {
        self.intent_in(0)
    }

    /// What it means to do `turns` turns from now.
    pub fn intent_in(&self, turns: u32) -> Intent {
        let pattern = self.foe().pattern;
        pattern[(self.turn + turns) as usize % pattern.len()]
    }

    /// The foe as the lines call it: its kind's name, `bright` ahead of
    /// it when it is.
    pub fn name(&self) -> String {
        match self.bright {
            true => format!("bright {}", self.foe().name),
            false => self.foe().name.to_string(),
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
    /// Crystals held: a drop never takes them, a mark and the ledge do.
    pub crystals: i32,
    /// Today's glass at Dead Air, gone at the roll.
    pub drink: Option<Drink>,
    /// Today's run on the road: the steps taken and the static in the
    /// deck. Wiped by the roll.
    pub road: RoadRun,
    /// The cards drafted on the way up, one per draft passed, in order:
    /// `cards[i]` is an option of `cards::DRAFTS[i]`. The deck is built
    /// from them ([`Sheet::deck`]); a mark and the ledge take them back.
    pub cards: Vec<Card>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SheetError {
    /// The stored fight is not a `Fight`, or names a glyph the table does
    /// not have. A bug, never a blank: the row is the truth and it is wrong.
    Fight(String),
    /// The stored drink is not on the bar's menu.
    Drink(String),
    /// The stored road is not a `RoadRun`.
    Road(String),
    /// The stored cards are not a list of cards, or hold one its draft
    /// never offered.
    Cards(String),
}

/// What Dead Air pours, a crystal a glass, one a day, gone at the roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Drink {
    /// Attack, by [`data::drink_edge`].
    StaticOnIce,
    /// Defense, by [`data::drink_edge`].
    DeadAirNeat,
    /// The signal's max, by [`DRINK_SIGNAL_PER_LEVEL`], and filled.
    TestPattern,
}

impl Drink {
    /// The menu, in the order the bar lists it.
    pub const MENU: [Drink; 3] = [Drink::StaticOnIce, Drink::DeadAirNeat, Drink::TestPattern];

    /// The code on the row (`deadchannel_runners.drink`).
    pub fn code(self) -> &'static str {
        match self {
            Drink::StaticOnIce => "static_on_ice",
            Drink::DeadAirNeat => "dead_air_neat",
            Drink::TestPattern => "test_pattern",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::MENU.into_iter().find(|drink| drink.code() == code)
    }

    pub fn name(self) -> &'static str {
        match self {
            Drink::StaticOnIce => "static on ice",
            Drink::DeadAirNeat => "dead air, neat",
            Drink::TestPattern => "test pattern",
        }
    }
}

impl std::fmt::Display for SheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fight(detail) => write!(f, "stored fight is unreadable: {detail}"),
            Self::Drink(code) => write!(f, "stored drink is not on the menu: {code}"),
            Self::Road(detail) => write!(f, "stored road is unreadable: {detail}"),
            Self::Cards(detail) => write!(f, "stored cards are unreadable: {detail}"),
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

/// Why a step down the road would start nothing ([`Sheet::shut`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shut {
    /// The signal is down until the roll.
    SignalDown,
    /// The day's rations are spent.
    NoRations,
}

/// Which glyph a step onto a glyph's node goes looking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pick {
    /// The glyph of your level, or at the top with the exp to leave it,
    /// the Old Signal.
    Fair,
    /// The glyph a level down, at [`data::LOWER_PAY_PERCENT`] of its pay.
    Lower,
    /// The bright glyph of your level: the pick of a [`Node::Bright`].
    Bright,
}

/// What a runner does with the node they step onto. The node decides
/// which calls it answers (`Sheet::step_to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Call {
    /// A glyph's node (`Fair` or `Lower`) or a bright one (`Bright`).
    Fight(Pick),
    /// A rest: mend the signal.
    Mend,
    /// A rest: shake the static out of the deck.
    Clear,
    /// A cache: take it.
    Take,
}

/// What a session asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// A step down the road: spend a ration, move to `lane` of the next
    /// step, and do `call` with what waits there. With a fight already
    /// waiting on the row, this resumes it, whatever the lane and the
    /// call, and spends nothing.
    Step { lane: u8, call: Call },
    /// Back into the fight waiting on the row, and nothing else: with no
    /// fight there it is refused. What a session sends when its mirror
    /// shows a fight, so a stale mirror never turns into a step.
    Resume,
    /// Play the card in hand slot `slot` (0 to `cards::HAND - 1`).
    Play { slot: u8 },
    /// The glyph takes its turn; a fresh hand and full energy after.
    EndTurn,
    /// The obvious turn, played and ended (`policy::auto`).
    Auto,
    /// Out of the fight, through whatever the glyph meant to do this turn.
    Run,
    /// Take `card` from the draft owed ([`Sheet::draft`]): it goes into
    /// the deck over one of the cards the draft replaces.
    Draft { card: Card },
    /// Buy `tier` (1 to `MAX_TIER`) for `slot` at the armorer, handing
    /// back what the slot holds.
    Outfit { slot: Slot, tier: i32 },
    /// Buy the signal back to full at patch, for `Sheet::patch_price`.
    Patch,
    /// Everything on hand into the locker, less its cut.
    Deposit,
    /// Everything in the locker back on hand.
    Withdraw,
    /// The bits machine's loan: up to the level's cap, its fee on top.
    Borrow,
    /// As much of the debt as the bits on hand cover.
    Repay,
    /// Off the ledge: the runner starts over.
    Reset,
    /// A glass at Dead Air, for [`DRINK_CRYSTALS`].
    Drink { drink: Drink },
    /// The blade shop's piece for `slot`: the next tier up from the one
    /// carried, for `data::CART_CRYSTALS` and no bits.
    Cart { slot: Slot },
}

/// Why nothing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NoRations,
    SignalDown,
    /// A card, an end of turn, or a run with no fight on the row.
    NoFight,
    /// A step to a lane the road does not reach from where you stand.
    NotThatWay,
    /// A call the node stepped onto does not answer.
    WrongCall,
    /// A card that costs more energy than the turn has left.
    NoEnergy,
    /// A hand slot with no card in it.
    NoCard,
    /// A rest spent on clearing a deck with no static in it.
    NoStatic,
    /// A step down the road with a draft owed: the card is picked first.
    CardWaiting,
    /// A draft with none owed.
    NoDraft,
    /// A draft of a card the one owed does not offer.
    NotOffered,
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
    /// A deposit the locker's cut would take whole.
    DepositAllCut,
    /// A withdrawal from an empty locker.
    LockerEmpty,
    /// A loan with the debt already at the level's cap.
    LoanCapped,
    /// A repayment with nothing owed.
    NoDebt,
    /// A step off the ledge by a runner the fall would take nothing from.
    NothingToLose,
    /// A second glass in one day.
    GlassPoured,
    /// Crystals short of the price, by this many.
    ShortCrystals {
        by: i32,
    },
    /// The blade shop for a slot already at the wall's last tier:
    /// nothing is made past it.
    PastTheWall,
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
    /// A card played, the fight still on.
    Played {
        card: Card,
    },
    /// A turn ended, both still standing: the glyph moved, a fresh hand.
    Round,
    /// A card drafted into the deck over one `replaces`.
    Drafted {
        card: Card,
        replaces: Card,
    },
    /// A rest spent on the signal: `restored` points.
    Mended {
        restored: i32,
    },
    /// A rest spent on the deck: `cards` of static out of it.
    Cleared {
        cards: usize,
    },
    /// A cache taken: `bits`, of which `garnished` went to the debt.
    Cached {
        bits: i64,
        garnished: i64,
    },
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
        /// It was a bright one.
        bright: bool,
        /// Crystals it left: one from a bright glyph, one now and then
        /// from the glyph of your level.
        crystals: i32,
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
    /// `amount` bits lent; the debt grew by that and the machine's `fee`.
    Borrowed {
        amount: i64,
        fee: i64,
    },
    Repaid {
        amount: i64,
    },
    /// Off the ledge: level 1, bare hands, nothing on hand or in the
    /// locker. The marks, the peak, the kills, and the debt stay.
    Reset,
    /// A glass poured at Dead Air.
    Drank {
        drink: Drink,
    },
    /// A piece off the blade shop, for `crystals`.
    Carted {
        slot: Slot,
        tier: i32,
        crystals: i32,
    },
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
    /// The signal dropped at `step` of the road; the street took
    /// `bits_lost`.
    Dropped { bits_lost: i64, step: i32 },
    /// A level gained: the line the face rides.
    Leveled { level: i32 },
    /// The Old Signal put down, the runner reset: the other line the face
    /// rides.
    Slain { marks: i32 },
    /// A step off the ledge: the runner started over by choice.
    SteppedOff,
    /// The runner's first glyph, ever.
    FirstBlood { foe: &'static str },
    /// A bright glyph put down.
    BrightDown { foe: &'static str },
    /// A win with `signal` at or under [`NEAR_MISS_SIGNAL`].
    NearMiss { foe: &'static str, signal: i32 },
    /// The last step of the road is taken and what it met is over: the
    /// day's card.
    RoadWalked {
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
            crystals: 0,
            drink: None,
            road: RoadRun::default(),
            cards: Vec::new(),
        }
    }

    /// The row, typed. The fight and the road are JSON and can be wrong.
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
        let drink = match &row.drink {
            None => None,
            Some(code) => match Drink::parse(code) {
                Some(drink) => Some(drink),
                None => return Err(SheetError::Drink(code.clone())),
            },
        };
        let road = match &row.road {
            None => RoadRun::default(),
            Some(value) => match serde_json::from_value(value.clone()) {
                Ok(road) => road,
                Err(error) => return Err(SheetError::Road(error.to_string())),
            },
        };
        let cards: Vec<Card> = match &row.cards {
            None => Vec::new(),
            Some(value) => match serde_json::from_value(value.clone()) {
                Ok(cards) => cards,
                Err(error) => return Err(SheetError::Cards(error.to_string())),
            },
        };
        if cards.len() > cards::DRAFTS.len() {
            return Err(SheetError::Cards(format!(
                "{} cards for {} drafts",
                cards.len(),
                cards::DRAFTS.len()
            )));
        }
        for (draft, card) in cards::DRAFTS.iter().zip(&cards) {
            if !draft.options.contains(card) {
                return Err(SheetError::Cards(format!(
                    "{} was never offered at level {}",
                    card.name(),
                    draft.level
                )));
            }
        }
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
            crystals: row.crystals,
            drink,
            road,
            cards,
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
            crystals: self.crystals,
            drink: self.drink.map(|drink| drink.code().to_string()),
            road: match self.road == RoadRun::default() {
                true => None,
                false => Some(serde_json::to_value(&self.road).expect("a road serializes")),
            },
            cards: match self.cards.is_empty() {
                true => None,
                false => Some(serde_json::to_value(&self.cards).expect("cards serialize")),
            },
        }
    }

    /// The deck this runner carries into a fight, before the day's static.
    pub fn deck(&self) -> Vec<Card> {
        cards::deck(&self.cards)
    }

    /// The draft this runner is owed, if any: the road waits on it.
    pub fn draft(&self) -> Option<&'static Draft> {
        cards::draft_owed(self.level, self.cards.len())
    }

    pub fn max_signal(&self) -> i32 {
        let glass = match self.drink {
            Some(Drink::TestPattern) => self.level * DRINK_SIGNAL_PER_LEVEL,
            Some(Drink::StaticOnIce) | Some(Drink::DeadAirNeat) | None => 0,
        };
        self.level * SIGNAL_PER_LEVEL + glass
    }

    pub fn attack(&self) -> u32 {
        let glass = match self.drink {
            Some(Drink::StaticOnIce) => data::drink_edge(self.level),
            Some(Drink::DeadAirNeat) | Some(Drink::TestPattern) | None => 0,
        };
        (self.level + self.weapon_tier + self.mark_bonus() + glass) as u32
    }

    pub fn defense(&self) -> u32 {
        let glass = match self.drink {
            Some(Drink::DeadAirNeat) => data::drink_edge(self.level),
            Some(Drink::StaticOnIce) | Some(Drink::TestPattern) | None => 0,
        };
        (self.level + self.armor_tier + self.mark_bonus() + glass) as u32
    }

    /// The step the next ration buys: 1 for the day's first, up to
    /// [`RATIONS_PER_DAY`].
    pub fn step(&self) -> i32 {
        RATIONS_PER_DAY - self.rations_left + 1
    }

    /// Today's road ([`road::road_for`], the same for every runner).
    pub fn todays_road(&self) -> Road {
        road::road_for(self.day)
    }

    /// What waits on `lane` of the next step; `None` off the road, or
    /// with the day's steps all taken.
    pub fn node_ahead(&self, lane: u8) -> Option<Node> {
        match self.rations_left > 0 {
            true => self.todays_road().node(self.step(), lane),
            false => None,
        }
    }

    /// The day's road is over for this runner: every step taken and
    /// nothing waiting, or the signal down.
    pub fn road_over(&self) -> bool {
        self.fight.is_none() && (self.is_down() || self.rations_left <= 0)
    }

    /// The numbers on the cards against `fight`'s glyph.
    pub fn powers(&self, fight: &Fight) -> Powers {
        self.powers_under(&RULES, fight)
    }

    pub fn powers_under(&self, rules: &Rules, fight: &Fight) -> Powers {
        let strike = rules.strike(self.attack(), fight.foe_defense);
        let block = rules.block(self.defense());
        Powers {
            strike,
            surge: rules.surge(strike),
            block,
            hit: rules.hit(fight.foe_attack, self.defense()),
            jab: rules.share(strike, rules.jab_percent),
            mend: rules.share(strike, rules.siphon_mend_percent),
            bulwark: rules.share(block, rules.bulwark_percent),
            burn: rules.burn_energy,
            ground: rules.share(strike, rules.ground_percent),
            sever: rules.share(strike, rules.sever_percent),
        }
    }

    /// Why a step down the road would start nothing, read off the sheet
    /// as it stands: the signal down, or the rations spent. `None` with a
    /// fight waiting (a step resumes it) and while there is road to walk.
    pub fn shut(&self) -> Option<Shut> {
        match (self.fight.is_some(), self.is_down(), self.rations_left <= 0) {
            (true, _, _) => None,
            (false, true, _) => Some(Shut::SignalDown),
            (false, false, true) => Some(Shut::NoRations),
            (false, false, false) => None,
        }
    }

    /// Why the bartender would not pour, and the line he says it with:
    /// the one order and the one wording, for `Command::Drink` and for the
    /// panel that spells the refusal out ahead of the keys. `None` when a
    /// glass would be poured.
    pub fn glass_refused(&self) -> Option<(Refusal, String)> {
        if self.is_down() {
            return Some((
                Refusal::SignalDown,
                "your signal is down. the bartender does not pour for static.".to_string(),
            ));
        }
        if self.fight.is_some() {
            return Some((
                Refusal::FightWaiting,
                "not with a glyph waiting on you. the glass can wait.".to_string(),
            ));
        }
        if self.rations_left <= 0 {
            return Some((
                Refusal::NoRations,
                "you are spent for today. it would wear off before you used it.".to_string(),
            ));
        }
        if let Some(had) = self.drink {
            return Some((
                Refusal::GlassPoured,
                format!("one glass a day. you still have the {} in you.", had.name()),
            ));
        }
        if DRINK_CRYSTALS > self.crystals {
            let by = DRINK_CRYSTALS - self.crystals;
            return Some((
                Refusal::ShortCrystals { by },
                "a glass is a crystal, and you have none.".to_string(),
            ));
        }
        None
    }

    /// The tier the blade shop holds for `slot`: the next one up from
    /// what it carries, `None` at the top, where nothing is made past the
    /// wall.
    pub fn cart_tier(&self, slot: Slot) -> Option<i32> {
        match self.tier_of(slot) + 1 {
            tier if tier > MAX_TIER => None,
            tier => Some(tier),
        }
    }

    /// What the marks add to attack and defense: one each, capped.
    pub fn mark_bonus(&self) -> i32 {
        self.marks.min(MARK_BONUS_CAP)
    }

    /// At the top with the exp to leave it: the next glyph's node meets
    /// the Old Signal.
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
        self.outfit_price_under(&RULES, slot, tier)
    }

    pub fn outfit_price_under(&self, rules: &Rules, slot: Slot, tier: i32) -> i64 {
        rules.price(tier) - rules.trade_in(self.tier_of(slot))
    }

    /// What patch charges to bring the signal back to full: a bit a
    /// point, times the level, so the price climbs with the glyphs that
    /// did the damage. Zero with nothing missing; the panel shows it,
    /// `Patch` charges it.
    pub fn patch_price(&self) -> i64 {
        self.patch_price_under(&RULES)
    }

    pub fn patch_price_under(&self, rules: &Rules) -> i64 {
        let whole = i64::from(self.max_signal() - self.signal) * i64::from(self.level);
        // Rounded up: a scratch is never patched for nothing.
        (whole * rules.patch_percent + 99) / 100
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
        self.drink = None;
        self.signal = self.max_signal();
        self.rations_left = RATIONS_PER_DAY;
        self.fight = None;
        self.kills_today = 0;
        self.runs_today = 0;
        self.road = RoadRun::default();
        true
    }

    /// The news in `applied`, in the order the wire prints it. At most one
    /// story line per win (a level, else first blood, else a near miss:
    /// the biggest one), then the day's card if this was the road's last
    /// step and it ended standing. A dropped signal is its own line and
    /// ends the day by itself.
    pub fn news(&self, applied: &Applied) -> Vec<News> {
        let mut news = Vec::new();
        match applied {
            Applied::Lost { bits_lost } => {
                news.push(News::Dropped {
                    bits_lost: *bits_lost,
                    step: self.step() - 1,
                });
                return news;
            }
            Applied::Slain { marks } => news.push(News::Slain { marks: *marks }),
            Applied::Won {
                foe,
                leveled,
                bright,
                ..
            } => {
                if let Some(level) = leveled {
                    news.push(News::Leveled { level: *level });
                } else if self.kills == 1 {
                    news.push(News::FirstBlood { foe });
                } else if *bright {
                    news.push(News::BrightDown { foe });
                } else if self.signal <= NEAR_MISS_SIGNAL {
                    news.push(News::NearMiss {
                        foe,
                        signal: self.signal,
                    });
                }
            }
            Applied::Escaped
            | Applied::Mended { .. }
            | Applied::Cleared { .. }
            | Applied::Cached { .. } => {}
            Applied::Reset => {
                news.push(News::SteppedOff);
                return news;
            }
            Applied::Refused(_)
            | Applied::Started { .. }
            | Applied::Resumed
            | Applied::Played { .. }
            | Applied::Round
            | Applied::Drafted { .. }
            | Applied::Outfitted { .. }
            | Applied::Patched { .. }
            | Applied::Deposited { .. }
            | Applied::Withdrew { .. }
            | Applied::Borrowed { .. }
            | Applied::Repaid { .. }
            | Applied::Drank { .. }
            | Applied::Carted { .. } => return news,
        }
        if self.rations_left == 0 {
            news.push(News::RoadWalked {
                kills: self.kills_today,
                runs: self.runs_today,
                signal: self.signal,
                max_signal: self.max_signal(),
            });
        }
        news
    }

    /// One command under the live rules.
    pub fn apply<R: Rng>(&mut self, command: Command, rng: &mut R) -> Outcome {
        self.apply_under(&RULES, command, rng)
    }

    /// One command under `rules`: the same machine with a candidate set of
    /// numbers, for the sim and the arena.
    pub fn apply_under<R: Rng>(&mut self, rules: &Rules, command: Command, rng: &mut R) -> Outcome {
        match command {
            Command::Step { lane, call } => self.step_to(rules, lane, call, rng),
            Command::Resume => match self.fight {
                Some(_) => Outcome {
                    applied: Applied::Resumed,
                    lines: Vec::new(),
                },
                None => refused(Refusal::NoFight),
            },
            Command::Play { slot } => self.play(rules, usize::from(slot), rng),
            Command::EndTurn => self.end_turn(rules, rng),
            Command::Auto => self.auto(rules, rng),
            Command::Run => self.run(rules, rng),
            Command::Draft { card } => self.draft_card(card),
            Command::Outfit { slot, tier } => self.outfit(rules, slot, tier),
            Command::Patch => self.patch(rules),
            Command::Deposit => self.deposit(),
            Command::Withdraw => self.withdraw(),
            Command::Borrow => self.borrow(),
            Command::Repay => self.repay(),
            Command::Reset => self.reset(),
            Command::Drink { drink } => self.drink(drink),
            Command::Cart { slot } => self.cart(rules, slot),
        }
    }

    /// What the bits machine will have lent at most: the level's cap.
    pub fn loan_cap(&self) -> i64 {
        i64::from(self.level) * LOAN_PER_LEVEL
    }

    /// What a loan would hand over now: the cap less the debt, nothing at
    /// or past the cap (the fee carries the debt past it).
    pub fn loan_room(&self) -> i64 {
        (self.loan_cap() - self.debt).max(0)
    }

    /// The machine's fee on that loan, added to the debt with it.
    pub fn loan_fee(&self) -> i64 {
        percent_up(self.loan_room(), LOAN_FEE_PERCENT)
    }

    /// Whether a step off the ledge would take anything: a level, exp, a
    /// piece of gear, or a bit on hand or in the locker.
    pub fn has_something_to_lose(&self) -> bool {
        self.level > 1
            || self.exp > 0
            || self.weapon_tier > 0
            || self.armor_tier > 0
            || self.bits > 0
            || self.stash > 0
            || self.crystals > 0
    }

    /// The locker's cut of a deposit of everything on hand.
    pub fn deposit_fee(&self) -> i64 {
        percent_up(self.bits, LOCKER_FEE_PERCENT)
    }

    /// Everything on hand into the locker, less the cut. Not with a glyph
    /// waiting: the bits you carry into a fight are the bits you risk. Not
    /// when the cut would be all of it: the locker keeps nothing for nothing.
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
        if stored == 0 {
            return Outcome {
                applied: Applied::Refused(Refusal::DepositAllCut),
                lines: vec!["the locker's cut would take all of it. bring more.".to_string()],
            };
        }
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

    /// The machine lends up to the level's cap, all the room at once, and
    /// adds its fee to the debt then and never again.
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
        let fee = self.loan_fee();
        self.bits += amount;
        self.debt += amount + fee;
        Outcome {
            applied: Applied::Borrowed { amount, fee },
            lines: vec![format!(
                "the bits machine pays out for once. {amount} bits, and {fee} more on the debt. you owe it {}.",
                self.debt
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
    /// the locker, no crystals, the starting deck, a level-1 signal. What stays is what was earned or owed:
    /// the marks and their title, the peak (the tailor's rack), the kills,
    /// the look, today's rations, and the debt. Not with the signal down
    /// (a reset is not a way back on the wire before the roll), not with a
    /// glyph waiting, and not for a runner the fall would take nothing
    /// from: a step off is news, and the wire is not a key to hold down.
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
        if !self.has_something_to_lose() {
            return Outcome {
                applied: Applied::Refused(Refusal::NothingToLose),
                lines: vec![
                    "you have nothing the fall could take. the ledge is only a view.".to_string(),
                ],
            };
        }
        self.level = 1;
        self.exp = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.bits = 0;
        self.stash = 0;
        self.crystals = 0;
        self.cards.clear();
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
    fn patch(&mut self, rules: &Rules) -> Outcome {
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
        let paid = self.patch_price_under(rules);
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
    fn outfit(&mut self, rules: &Rules, slot: Slot, tier: i32) -> Outcome {
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
        let paid = self.outfit_price_under(rules, slot, tier);
        if paid > self.bits {
            let by = paid - self.bits;
            return Outcome {
                applied: Applied::Refused(Refusal::Short { by }),
                lines: vec![format!("you are {by} bits short of the {name}.")],
            };
        }
        let handed_back = self.gear_name(slot);
        self.bits -= paid;
        self.carry(slot, tier);
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

    fn carry(&mut self, slot: Slot, tier: i32) {
        match slot {
            Slot::Weapon => self.weapon_tier = tier,
            Slot::Armor => self.armor_tier = tier,
        }
    }

    /// The blade shop: what the armorer won't sell for bits. The next
    /// tier up from the one the slot carries, for crystals alone; the
    /// piece handed back buys nothing.
    fn cart(&mut self, rules: &Rules, slot: Slot) -> Outcome {
        let Some(tier) = self.cart_tier(slot) else {
            return Outcome {
                applied: Applied::Refused(Refusal::PastTheWall),
                lines: vec!["nothing is made past the top of the wall. not even here.".to_string()],
            };
        };
        let name = gear_name(slot, tier).expect("a tier on the wall");
        let crystals = rules.cart_crystals;
        if crystals > self.crystals {
            let by = crystals - self.crystals;
            return Outcome {
                applied: Applied::Refused(Refusal::ShortCrystals { by }),
                lines: vec![format!(
                    "the vendor wants {crystals} crystals for the {name}. you are {by} short."
                )],
            };
        }
        self.crystals -= crystals;
        self.carry(slot, tier);
        Outcome {
            applied: Applied::Carted {
                slot,
                tier,
                crystals,
            },
            lines: vec![format!(
                "the {name} comes off the rack. no receipt, no bits. {crystals} crystals."
            )],
        }
    }

    /// Dead Air. One glass a day for a crystal, and it is gone at the
    /// roll. Not with the signal down, not with a glyph waiting, and not
    /// for a runner spent for the day: no fight is left to use it in.
    fn drink(&mut self, drink: Drink) -> Outcome {
        if let Some((refusal, line)) = self.glass_refused() {
            return Outcome {
                applied: Applied::Refused(refusal),
                lines: vec![line],
            };
        }
        self.crystals -= DRINK_CRYSTALS;
        self.drink = Some(drink);
        let line = match drink {
            Drink::StaticOnIce => format!(
                "static on ice. it goes down like a short circuit. +{} attack until the roll.",
                data::drink_edge(self.level)
            ),
            Drink::DeadAirNeat => format!(
                "dead air, neat. everything gets quieter and further away. +{} defense until the roll.",
                data::drink_edge(self.level)
            ),
            Drink::TestPattern => {
                self.signal = self.max_signal();
                format!(
                    "test pattern. the bars come up behind your eyes. signal {} until the roll, and full.",
                    self.max_signal()
                )
            }
        };
        Outcome {
            applied: Applied::Drank { drink },
            lines: vec![line],
        }
    }

    /// A step down the road: the ration, the lane, and what the node
    /// there answers to. A fight already on the row resumes instead,
    /// whatever was asked, and nothing is spent. A refusal spends nothing.
    /// A draft owed is taken first: the road waits on the card.
    fn step_to<R: Rng>(&mut self, rules: &Rules, lane: u8, call: Call, rng: &mut R) -> Outcome {
        if self.fight.is_some() {
            return Outcome {
                applied: Applied::Resumed,
                lines: Vec::new(),
            };
        }
        match self.shut() {
            Some(Shut::SignalDown) => return refused(Refusal::SignalDown),
            Some(Shut::NoRations) => return refused(Refusal::NoRations),
            None => {}
        }
        if self.draft().is_some() {
            return refused(Refusal::CardWaiting);
        }
        if !self.road.reaches(lane) {
            return refused(Refusal::NotThatWay);
        }
        let node = self
            .node_ahead(lane)
            .expect("a lane on the road with a ration left");
        match node {
            Node::Glyph => match call {
                Call::Fight(pick @ (Pick::Fair | Pick::Lower)) => {
                    if pick == Pick::Lower && rules.lower_foe(self.level).is_none() {
                        return refused(Refusal::NoLowerGlyph);
                    }
                    self.take_step(lane, Mark::Fighting);
                    self.engage(rules, pick, rng)
                }
                Call::Fight(Pick::Bright) | Call::Mend | Call::Clear | Call::Take => {
                    refused(Refusal::WrongCall)
                }
            },
            Node::Bright => match call {
                Call::Fight(Pick::Bright) => {
                    self.take_step(lane, Mark::Fighting);
                    self.engage(rules, Pick::Bright, rng)
                }
                Call::Fight(Pick::Fair | Pick::Lower) | Call::Mend | Call::Clear | Call::Take => {
                    refused(Refusal::WrongCall)
                }
            },
            Node::Rest => match call {
                Call::Mend => self.mend(rules, lane),
                Call::Clear => self.clear(lane),
                Call::Fight(_) | Call::Take => refused(Refusal::WrongCall),
            },
            Node::Cache => match call {
                Call::Take => self.cache(rules, lane, rng),
                Call::Fight(_) | Call::Mend | Call::Clear => refused(Refusal::WrongCall),
            },
        }
    }

    /// One step taken: the ration spent, the lane and its mark on the run.
    fn take_step(&mut self, lane: u8, mark: Mark) {
        self.rations_left -= 1;
        self.road.path.push(Trace { lane, mark });
    }

    /// Meet `pick`: the glyph on the row, the deck dealt with the day's
    /// static in it, the first hand drawn. No ration and no step: the
    /// road's step spends those, and the sim and the arena read a fight's
    /// odds from here without walking a road to it. The caller has checked
    /// that the row holds no fight and the signal is up.
    pub(crate) fn engage<R: Rng>(&mut self, rules: &Rules, pick: Pick, rng: &mut R) -> Outcome {
        let static_cards = usize::from(self.road.static_cards);
        let deck = self.deck();
        let mut fight = match (pick, self.signal_hears()) {
            (Pick::Fair, true) => {
                Fight::new(Quarry::OldSignal, rules.old_signal, deck, static_cards, rng)
            }
            (Pick::Fair, false) => {
                let (kind, _, tier) = rules.foe(self.level);
                Fight::new(Quarry::Glyph(kind), tier, deck, static_cards, rng)
            }
            (Pick::Lower, _) => match rules.lower_foe(self.level) {
                Some((kind, _, tier)) => {
                    Fight::new(Quarry::Glyph(kind), tier, deck, static_cards, rng)
                }
                None => return refused(Refusal::NoLowerGlyph),
            },
            (Pick::Bright, _) => {
                let (kind, _, tier) = rules.bright_foe(self.level);
                let mut fight = Fight::new(Quarry::Glyph(kind), tier, deck, static_cards, rng);
                fight.bright = true;
                fight
            }
        };
        fight.push(fight.foe().arrives.to_string());
        match pick {
            Pick::Fair => {}
            Pick::Lower => fight.push(STEPPED_DOWN_LINE.to_string()),
            Pick::Bright => fight.push(BRIGHT_LINE.to_string()),
        }
        let lines = fight.log.clone();
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Started { pick },
            lines,
        }
    }

    /// A rest spent on the signal. Allowed with nothing to mend: a rest is
    /// also just a step, and somebody whole still has to get past it.
    fn mend(&mut self, rules: &Rules, lane: u8) -> Outcome {
        self.take_step(lane, Mark::Mended);
        let restored = rules
            .mend(self.max_signal())
            .min(self.max_signal() - self.signal);
        self.signal += restored;
        let line = match restored {
            0 => "your signal was already whole.".to_string(),
            n => format!("+{n} signal. {}/{}.", self.signal, self.max_signal()),
        };
        Outcome {
            applied: Applied::Mended { restored },
            lines: vec![MEND_LINE.to_string(), line],
        }
    }

    /// A rest spent on the deck: every static card out of it. Refused,
    /// and the ration kept, with no static to clear.
    fn clear(&mut self, lane: u8) -> Outcome {
        let cards = usize::from(self.road.static_cards);
        if cards == 0 {
            return refused(Refusal::NoStatic);
        }
        self.take_step(lane, Mark::Cleared);
        self.road.static_cards = 0;
        let line = match cards {
            1 => "1 static card gone. the deck is clean.".to_string(),
            n => format!("{n} static cards gone. the deck is clean."),
        };
        Outcome {
            applied: Applied::Cleared { cards },
            lines: vec![CLEAR_LINE.to_string(), line],
        }
    }

    /// A cache: bits for the taking, the machine's share first, like any
    /// other thing the road pays.
    fn cache<R: Rng>(&mut self, rules: &Rules, lane: u8, rng: &mut R) -> Outcome {
        self.take_step(lane, Mark::Cached);
        let bits = rules.cache(self.level);
        let garnished = (bits * GARNISH_PERCENT / 100).min(self.debt);
        self.debt -= garnished;
        self.bits += bits - garnished;
        let mut lines = vec![format!("{} +{bits} bits.", pick(rng, &CACHE_LINES))];
        if garnished > 0 {
            lines.push(self.garnish_line(garnished));
        }
        Outcome {
            applied: Applied::Cached { bits, garnished },
            lines,
        }
    }

    fn garnish_line(&self, garnished: i64) -> String {
        let owed = match self.debt {
            0 => "you owe it nothing.".to_string(),
            left => format!("{left} still owed."),
        };
        format!("the bits machine takes {garnished} of it. {owed}")
    }

    /// The draft owed, answered: `card` into the deck over one of the
    /// cards the draft replaces, until a mark or the ledge. Not with a
    /// glyph waiting: the deck on the table is the deck that was dealt.
    fn draft_card(&mut self, card: Card) -> Outcome {
        if self.fight.is_some() {
            return fight_waiting("not with a glyph waiting on you. finish it first.");
        }
        let Some(draft) = self.draft() else {
            return refused(Refusal::NoDraft);
        };
        if !draft.options.contains(&card) {
            return refused(Refusal::NotOffered);
        }
        self.cards.push(card);
        let mut lines = vec![format!(
            "{} is in your deck now, in place of a {}.",
            card.name(),
            draft.replaces.name()
        )];
        if self.draft().is_some() {
            lines.push(DRAFT_LINE.to_string());
        }
        Outcome {
            applied: Applied::Drafted {
                card,
                replaces: draft.replaces,
            },
            lines,
        }
    }

    /// One card out of the hand. A hit that empties the glyph's signal
    /// ends the fight there, before it ever moves.
    fn play<R: Rng>(&mut self, rules: &Rules, slot: usize, rng: &mut R) -> Outcome {
        let Some(mut fight) = self.fight.take() else {
            return refused(Refusal::NoFight);
        };
        let Some(card) = fight.piles.card(slot) else {
            self.fight = Some(fight);
            return refused(Refusal::NoCard);
        };
        if card.cost() > fight.energy {
            self.fight = Some(fight);
            return refused(Refusal::NoEnergy);
        }
        let powers = self.powers_under(rules, &fight);
        let effect = card.effect(&powers, &fight.board());
        fight.piles.play(slot);
        fight.energy = fight.energy - card.cost() + effect.energy;
        let wiped = match effect.clears_hand {
            true => fight.piles.wipe_hand(),
            false => 0,
        };
        fight.foe_signal = (fight.foe_signal - effect.damage).max(0);
        fight.block += effect.block;
        let mended = effect.mend.min(self.max_signal() - self.signal).max(0);
        self.signal += mended;
        fight.muted = fight.muted || effect.mutes;
        let name = fight.name();
        let damage = effect.damage;
        let line = match card {
            Card::Strike => match self.gear_name(Slot::Weapon) {
                Some(weapon) => format!("your {weapon} hits the {name} for {damage}."),
                None => format!("you hit the {name} for {damage}."),
            },
            Card::Surge => match self.gear_name(Slot::Weapon) {
                Some(weapon) => {
                    format!("you surge. your {weapon} tears {damage} out of the {name}.")
                }
                None => format!("you surge. you tear {damage} out of the {name}."),
            },
            Card::Block => format!("you set yourself. block {}.", fight.block),
            Card::Wipe => match wiped {
                0 => format!("you wipe your hand clean. block {}.", fight.block),
                n => format!(
                    "you wipe the static off your hand. {n} gone. block {}.",
                    fight.block
                ),
            },
            Card::Jab => format!("a quick jab. {damage} into the {name}."),
            Card::Siphon => match mended {
                0 => format!("you siphon {damage} out of the {name}. your signal is whole."),
                n => format!("you siphon {damage} out of the {name}. +{n} signal."),
            },
            Card::Riposte => format!("you hit back with your guard. {damage} into the {name}."),
            Card::Bulwark => format!("you dig in. block {}.", fight.block),
            Card::Burn => match wiped {
                0 => format!("you burn hot. +{} energy.", effect.energy),
                n => format!(
                    "you burn {n} static off your hand. +{} energy.",
                    effect.energy
                ),
            },
            Card::Ground => match wiped {
                0 => format!("you ground yourself on the {name}. {damage}."),
                n => format!("you ground {n} static into the {name}. {damage}."),
            },
            Card::Sever => match damage > powers.strike {
                true => format!("you sever the {name}'s feed. {damage}."),
                false => format!("you cut at the {name}'s feed for {damage}."),
            },
            Card::Mute => format!("you mute the {name}. its move this turn does nothing."),
            Card::Static => "you shake a static card loose. it is gone.".to_string(),
        };
        let lines = vec![line];
        if fight.foe_signal == 0 {
            return self.win(rules, fight, rng, lines);
        }
        for line in &lines {
            fight.push(line.clone());
        }
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Played { card },
            lines,
        }
    }

    /// The glyph takes the turn it showed; then a fresh hand and full
    /// energy, and its next move is on show.
    fn end_turn<R: Rng>(&mut self, rules: &Rules, rng: &mut R) -> Outcome {
        let Some(mut fight) = self.fight.take() else {
            return refused(Refusal::NoFight);
        };
        let mut lines = Vec::new();
        let powers = self.powers_under(rules, &fight);
        match (fight.muted, fight.intent()) {
            // A charge is no move to mute: the heavy still comes.
            (_, Intent::Charge) => lines.push(format!(
                "the {} gathers itself. the next one lands for {}.",
                fight.name(),
                powers.hit * 2
            )),
            (true, Intent::Hit | Intent::Heavy | Intent::Noise) => {
                lines.push(format!(
                    "the {} moves, and nothing comes out.",
                    fight.name()
                ));
            }
            (false, Intent::Hit) => self.struck(&mut fight, powers.hit, false, &mut lines),
            (false, Intent::Heavy) => self.struck(&mut fight, powers.hit * 2, true, &mut lines),
            (false, Intent::Noise) => lines.push(noise(&mut fight)),
        }
        if self.is_down() {
            return self.lose(rules, fight, rng, lines);
        }
        fight.turn += 1;
        fight.muted = false;
        fight.energy = ENERGY;
        fight.piles.draw_hand(rng);
        for line in &lines {
            fight.push(line.clone());
        }
        self.fight = Some(fight);
        Outcome {
            applied: Applied::Round,
            lines,
        }
    }

    /// A hit of `incoming` against the block standing: the block eats
    /// what it can, the rest lands, and a hit that lands at all leaves
    /// static in the deck.
    fn struck(&mut self, fight: &mut Fight, incoming: i32, heavy: bool, lines: &mut Vec<String>) {
        let held = fight.block.min(incoming);
        fight.block -= held;
        let landed = incoming - held;
        let verb = match heavy {
            true => "comes down on you",
            false => "hits you",
        };
        lines.push(match (held, landed) {
            (0, n) => format!("it {verb} for {n}."),
            (_, 0) => format!("it {verb} for {incoming}. your block holds."),
            (held, n) => {
                format!("it {verb} for {incoming}. your block takes {held}, you take {n}.")
            }
        });
        if landed > 0 {
            self.take(landed);
            if fight.piles.add_static(1) > 0 {
                lines.push(STATIC_LINE.to_string());
            }
        }
    }

    /// The obvious turn (`policy::auto`), played card by card and ended:
    /// one key for the runner who is here for the ritual.
    fn auto<R: Rng>(&mut self, rules: &Rules, rng: &mut R) -> Outcome {
        let Some(fight) = &self.fight else {
            return refused(Refusal::NoFight);
        };
        let plays = policy::auto(&policy::Table::read(self, rules, fight));
        let mut lines = Vec::new();
        for slot in plays {
            let outcome = self.play(rules, slot, rng);
            lines.extend(outcome.lines);
            match outcome.applied {
                Applied::Played { .. } => {}
                Applied::Won { .. } | Applied::Slain { .. } | Applied::Refused(_) => {
                    return Outcome {
                        applied: outcome.applied,
                        lines,
                    };
                }
                other => unreachable!("a played card answered {other:?}"),
            }
        }
        let outcome = self.end_turn(rules, rng);
        lines.extend(outcome.lines);
        Outcome {
            applied: outcome.applied,
            lines,
        }
    }

    /// Out of the fight. No dice: what the glyph meant to do this turn
    /// lands on the way out, against whatever block is standing, so
    /// running from a glyph that is gathering itself (or muted) is free
    /// and running from the heavy is not. The step is spent and pays nothing.
    fn run<R: Rng>(&mut self, rules: &Rules, rng: &mut R) -> Outcome {
        let Some(mut fight) = self.fight.take() else {
            return refused(Refusal::NoFight);
        };
        let mut lines = Vec::new();
        let powers = self.powers_under(rules, &fight);
        match (fight.muted, fight.intent()) {
            (true, _) | (false, Intent::Charge) => {}
            (false, Intent::Hit) => self.struck(&mut fight, powers.hit, false, &mut lines),
            (false, Intent::Heavy) => self.struck(&mut fight, powers.hit * 2, true, &mut lines),
            (false, Intent::Noise) => lines.push(noise(&mut fight)),
        }
        if self.is_down() {
            return self.lose(rules, fight, rng, lines);
        }
        self.runs_today += 1;
        self.leave_fight(&fight, Mark::Ran);
        lines.push(pick(rng, &RUN_LINES).to_string());
        Outcome {
            applied: Applied::Escaped,
            lines,
        }
    }

    /// The fight is over, whichever way: the static it left rides the
    /// deck down the rest of the road, and the step is marked.
    fn leave_fight(&mut self, fight: &Fight, mark: Mark) {
        self.road.static_cards = fight.piles.static_cards() as u8;
        self.road.settle_fight(mark);
    }

    /// Damage to the signal, never under zero.
    fn take(&mut self, damage: i32) {
        self.signal = (self.signal - damage).max(0);
    }

    fn win<R: Rng>(
        &mut self,
        rules: &Rules,
        fight: Fight,
        rng: &mut R,
        lines: Vec<String>,
    ) -> Outcome {
        match fight.quarry {
            Quarry::OldSignal => self.slay(lines),
            Quarry::Glyph(_) => self.put_down(rules, fight, rng, lines),
        }
    }

    /// The Old Signal is down: a mark, and the climb starts over. Level,
    /// exp, gear, the deck, and bits go back to a fresh runner's, the
    /// locker is emptied, and the crystals go with it; the peak, the kills, today's rations, the debt, and
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
        self.crystals = 0;
        self.cards.clear();
        self.signal = self.max_signal();
        self.road.static_cards = 0;
        self.road.settle_fight(Mark::Won);
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

    fn put_down<R: Rng>(
        &mut self,
        rules: &Rules,
        fight: Fight,
        rng: &mut R,
        mut lines: Vec<String>,
    ) -> Outcome {
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
            lines.push(self.garnish_line(garnished));
        }
        self.leave_fight(
            &fight,
            match fight.bright {
                true => Mark::BrightWon,
                false => Mark::Won,
            },
        );
        // A bright one always carries a crystal; the glyph of your level
        // leaves one now and then; a step down never does.
        let stepped_down = fight.foe_level() < Some(self.level);
        let crystals = match (fight.bright, stepped_down) {
            (true, _) => 1,
            (false, true) => 0,
            (false, false) => match rng.gen_range(0..rules.crystal_drop_one_in) {
                0 => 1,
                _ => 0,
            },
        };
        if crystals > 0 {
            self.crystals += crystals;
            lines.push(CRYSTAL_LINE.to_string());
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
        if leveled.is_some() && self.draft().is_some() {
            lines.push(DRAFT_LINE.to_string());
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
                bright: fight.bright,
                crystals,
            },
            lines,
        }
    }

    fn lose<R: Rng>(
        &mut self,
        rules: &Rules,
        fight: Fight,
        rng: &mut R,
        mut lines: Vec<String>,
    ) -> Outcome {
        self.leave_fight(&fight, Mark::Fell);
        let bits_lost = self.bits;
        self.bits = 0;
        self.exp = (self.exp as f64 * rules.exp_keep_on_death).round() as i64;
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

#[cfg(test)]
impl Sheet {
    /// For a runner a test sets down at a level: the first option of
    /// every draft that level has reached, taken, as the road would have
    /// had it.
    pub(crate) fn draft_up(&mut self) {
        while let Some(draft) = self.draft() {
            self.cards.push(draft.options[0]);
        }
    }
}

/// A noise turn: static into the deck, as much as it still holds.
fn noise(fight: &mut Fight) -> String {
    match fight.piles.add_static(NOISE_CARDS) {
        0 => "it floods you with noise. your deck cannot hold any more.".to_string(),
        1 => "it floods you with noise. 1 static card into your deck.".to_string(),
        n => format!("it floods you with noise. {n} static cards into your deck."),
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
        Refusal::NotThatWay => "the road does not run that way from where you stand.",
        Refusal::WrongCall => "that is not what waits there.",
        Refusal::NoEnergy => "not enough energy left this turn.",
        Refusal::NoCard => "there is no card there.",
        Refusal::NoStatic => "there is no static in your deck to shake out.",
        Refusal::CardWaiting => "a new card is waiting for you. pick it before the next step.",
        Refusal::NoDraft => "no new card is waiting for you.",
        Refusal::NotOffered => "that card is not on offer.",
        Refusal::NotAnUpgrade
        | Refusal::Short { .. }
        | Refusal::NothingToPatch
        | Refusal::FightWaiting
        | Refusal::NothingOnHand
        | Refusal::DepositAllCut
        | Refusal::LockerEmpty
        | Refusal::LoanCapped
        | Refusal::NoDebt
        | Refusal::NothingToLose
        | Refusal::GlassPoured
        | Refusal::ShortCrystals { .. }
        | Refusal::PastTheWall => {
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

/// The live price on the wall for `tier` (1 to `MAX_TIER`).
pub fn wall_price(tier: i32) -> i64 {
    RULES.price(tier)
}

fn pick<'a, R: Rng>(rng: &mut R, pool: &[&'a str]) -> &'a str {
    pool[rng.gen_range(0..pool.len())]
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
