//! The road: the day's ten rations as ten steps down three lanes (GAME.md,
//! "The road: ten rations, ten steps, one road for everyone"). The road is
//! a pure function of the UTC date, so it is the same road for every
//! runner that day, needs no table, and cannot be rerolled: "the bright
//! one is in the top lane at six" is a thing the wire can say.
//!
//! Every road has the same number of fights on every path through it
//! ([`FIGHT_STEPS`] steps are fights in all three lanes), so the pace of
//! the climb belongs to the rations and never to the route. What a route
//! chooses is which fights are the bright ones, and what the steps between
//! them give: a rest or a cache.
//!
//! And the run on it: where one runner stands on today's road and how
//! each step went (`RoadRun`, the `road` column of the runner row), wiped
//! by the day roll.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::data::RATIONS_PER_DAY;

/// Lanes of the road, top to bottom.
pub const LANES: usize = 3;
/// Steps of the road: one per ration.
pub const STEPS: usize = RATIONS_PER_DAY as usize;
/// Steps of every road that are a fight in every lane.
pub const FIGHT_STEPS: usize = 5;
/// Bright glyphs on every road, each on a fight step of its own, never
/// the first: the day opens on a plain glyph.
pub const BRIGHT_NODES: usize = 2;

/// What waits on one lane of one step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    /// The glyph of your level (or the one below, at the runner's call);
    /// at the gate, the Old Signal.
    Glyph,
    /// The bright glyph of your level: harder, double bits, a crystal.
    Bright,
    /// A doorway out of the rain: mend the signal, or clear the deck.
    Rest,
    /// Bits somebody left.
    Cache,
}

impl Node {
    pub fn is_fight(self) -> bool {
        match self {
            Node::Glyph | Node::Bright => true,
            Node::Rest | Node::Cache => false,
        }
    }
}

/// One day's road: `steps[step][lane]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Road {
    pub steps: [[Node; LANES]; STEPS],
}

impl Road {
    /// The node on `lane` of `step` (1 is the first ration's).
    pub fn node(&self, step: i32, lane: u8) -> Option<Node> {
        let step = usize::try_from(step - 1).ok()?;
        self.steps.get(step)?.get(usize::from(lane)).copied()
    }
}

/// SplitMix64, as a stream: the road's dice, seeded by the date alone.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The longest run of `true` (or of `false`) in `fights`.
fn longest_run(fights: &[bool; STEPS], of: bool) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for fight in fights {
        match *fight == of {
            true => {
                run += 1;
                longest = longest.max(run);
            }
            false => run = 0,
        }
    }
    longest
}

/// Which steps are fights: the first and the last always (the day opens
/// on a glyph and ends on one), the rest drawn until no three fights and
/// no three quiet steps stand in a row, so the road breathes.
fn fight_steps(dice: &mut Dice) -> [bool; STEPS] {
    for _ in 0..64 {
        let mut fights = [false; STEPS];
        fights[0] = true;
        fights[STEPS - 1] = true;
        let mut placed = 2;
        while placed < FIGHT_STEPS {
            let at = 1 + dice.below(STEPS - 2);
            if !fights[at] {
                fights[at] = true;
                placed += 1;
            }
        }
        if longest_run(&fights, true) <= 2 && longest_run(&fights, false) <= 2 {
            return fights;
        }
    }
    // Sixty-four draws that all clump is not a thing the dice do; a road
    // is still owed if they ever did.
    [
        true, false, true, false, false, true, false, true, false, true,
    ]
}

/// The road of `day`.
pub fn road_for(day: NaiveDate) -> Road {
    let mut dice = Dice(day.num_days_from_ce() as u64);
    let fights = fight_steps(&mut dice);
    let mut steps = [[Node::Glyph; LANES]; STEPS];
    for (step, fight) in fights.iter().enumerate() {
        if *fight {
            continue;
        }
        let mut lanes = [Node::Cache; LANES];
        for lane in &mut lanes {
            if dice.below(100) < 45 {
                *lane = Node::Rest;
            }
        }
        // Three of a kind is no choice: the middle lane turns over.
        if lanes[0] == lanes[1] && lanes[1] == lanes[2] {
            lanes[1] = match lanes[1] {
                Node::Rest => Node::Cache,
                Node::Cache | Node::Glyph | Node::Bright => Node::Rest,
            };
        }
        steps[step] = lanes;
    }
    // The bright ones: each on a fight step of its own, after the first.
    let later: Vec<usize> = (1..STEPS).filter(|step| fights[*step]).collect();
    let first = dice.below(later.len());
    let second = (first + 1 + dice.below(later.len() - 1)) % later.len();
    for at in [first, second].into_iter().take(BRIGHT_NODES) {
        steps[later[at]][dice.below(LANES)] = Node::Bright;
    }
    Road { steps }
}

/// How one step of the run went: what the share card and the map light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// A fight on the row, not over.
    Fighting,
    /// A glyph put down.
    Won,
    /// A bright glyph put down.
    BrightWon,
    /// Ran from it.
    Ran,
    /// The signal dropped here.
    Fell,
    Mended,
    Cleared,
    Cached,
}

/// One step taken: the lane, and how it went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub lane: u8,
    pub mark: Mark,
}

/// One runner's day on the road, as stored on the row
/// (`deadchannel_runners.road`). The day roll wipes it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoadRun {
    /// The steps taken today, in order.
    pub path: Vec<Trace>,
    /// Static riding the deck between fights.
    pub static_cards: u8,
}

impl RoadRun {
    /// The lane the runner stands in; `None` before the first step, when
    /// every lane is open.
    pub fn lane(&self) -> Option<u8> {
        self.path.last().map(|trace| trace.lane)
    }

    /// Whether `lane` can be stepped to from where the runner stands: on
    /// the road, and the same lane or the one beside it.
    pub fn reaches(&self, lane: u8) -> bool {
        usize::from(lane) < LANES && self.lane().is_none_or(|from| from.abs_diff(lane) <= 1)
    }

    /// The lanes open to the next step, top to bottom.
    pub fn open_lanes(&self) -> Vec<u8> {
        (0..LANES as u8)
            .filter(|lane| self.reaches(*lane))
            .collect()
    }

    /// How the fight the runner is standing in ended: the last step's
    /// mark, when it is still a fight. A fight started off the road (the
    /// sim's odds, the arena) has no step to mark.
    pub fn settle_fight(&mut self, mark: Mark) {
        if let Some(trace) = self.path.last_mut()
            && trace.mark == Mark::Fighting
        {
            trace.mark = mark;
        }
    }
}

#[cfg(test)]
#[path = "road_test.rs"]
mod road_test;
