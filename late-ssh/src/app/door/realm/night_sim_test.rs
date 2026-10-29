//! Whole campaign seasons, played against the real night engine.
//!
//! `simulation_test.rs` plays the instant engine this way; this file plays
//! `Resolution::Orders`. Simple policies write their orders by day from the
//! odds `projection` shows them, everyone's orders resolve together at the
//! night, and after every night the things that must never be true are
//! checked, the odds of every fight among them, priced again on the board
//! the night opened on. Worlds open the way `open_world` opens them and
//! players join through the service's own pure step (`svc::admit_player`),
//! at the house's rules and pace.
//!
//! The quick cases run with the suite. The full scale (every roster, on
//! Earth and on a drawn world, `REALM_SIM_SEEDS` seeds each) is ignored and
//! run on purpose: it prints one table, writes a row per season to the CSV
//! at `REALM_SIM_OUT`, and fails on the acceptance bars in `RULES.md`
//! ("Orders"):
//!
//! ```text
//! REALM_SIM_SEEDS=20 REALM_SIM_OUT=/tmp/realm-sim.csv \
//!   make test-llm ARGS="-p late-ssh --lib -E 'test(realm_campaign_sim)' --run-ignored ignored-only --no-capture"
//! ```
//!
//! Every run is seeded, so a failure here is reproducible.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::num::NonZeroU8;
use std::sync::Arc;

use uuid::Uuid;

use super::*;
use crate::app::door::realm::map::{EARTH_MAP_ID, GENERATED_MAP_ID, map_handle};
use crate::app::door::realm::mapgen::GeneratedMapSpec;
use crate::app::door::realm::resolver::{STATE_VERSION, day_seed, dissolve_if_abandoned};
use crate::app::door::realm::rulesets::pace_by_id;
use crate::app::door::realm::svc::{HOUSE_PACE, admit_player, house_ruleset};

/// The muster's day. The first day played is the one after it, season day 1.
const START_DAY: i32 = 20_000;

/// A settler pays a second point once one no longer buys this.
const SETTLE_ODDS: f64 = 0.70;
/// A warlord adds points until a take reaches this, or the budget runs out.
const RAID_ODDS: f64 = 0.75;
/// A cautious player takes nothing under this.
const SAFE_ODDS: f64 = 0.80;
/// The wall a turtle or a frontier digs its border to.
const DIG_TO: u8 = 3;
/// What a frontier or a coalition puts behind one take.
const PRESS_POINTS: u8 = 5;

/// How a simulated player writes their orders. Every policy prices with
/// `projection`, the board's own odds, and keeps ordering until its budget
/// is spent or nothing is left that it would order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Policy {
    /// Free land, a march before a landing and the best odds first: one
    /// point while it buys `SETTLE_ODDS`, two after. Fights only once no
    /// free land is in reach.
    Settler,
    /// The best-odds rival land, points until `RAID_ODDS`; free land while
    /// no rival is in reach.
    Warlord,
    /// Only takes at `SAFE_ODDS` or better, whatever that costs.
    Cautious,
    /// Two days on, three off, settling when it plays. Exercises the bank,
    /// dormancy and walls crumbling.
    Absentee,
    /// Walls its whole border to `DIG_TO`, settles free land in reach, and
    /// never strikes a player. The strategy the rules mean to make a bad
    /// one: a wall is for the ground you mean to take from, not a way to
    /// sit a season out.
    Turtle,
    /// Walls the border that faces a rival to `DIG_TO`, then `PRESS_POINTS`
    /// per take on the weakest rival land across a land border; settles
    /// when there is none.
    Frontier,
    /// `PRESS_POINTS` per take on the share leader's weakest land across a
    /// land border, when it has some; plays frontier otherwise.
    Coalition,
}

const POLICIES: [Policy; 7] = [
    Policy::Settler,
    Policy::Warlord,
    Policy::Cautious,
    Policy::Absentee,
    Policy::Turtle,
    Policy::Frontier,
    Policy::Coalition,
];

impl Policy {
    fn name(self) -> &'static str {
        match self {
            Policy::Settler => "settler",
            Policy::Warlord => "warlord",
            Policy::Cautious => "cautious",
            Policy::Absentee => "absentee",
            Policy::Turtle => "turtle",
            Policy::Frontier => "frontier",
            Policy::Coalition => "coalition",
        }
    }

    /// Does this player write orders on `day`? `offset` staggers absentees
    /// so they are not all away together.
    fn plays_on(self, day: i32, offset: i32) -> bool {
        match self {
            Policy::Absentee => (day + offset).rem_euclid(5) < 2,
            Policy::Settler
            | Policy::Warlord
            | Policy::Cautious
            | Policy::Turtle
            | Policy::Frontier
            | Policy::Coalition => true,
        }
    }
}

/// Mixed policies drawn by seed: whole decks of every policy, each shuffled
/// on its own, so a roster of seven or more seats every one of them.
fn draw_policies(rng: &mut SplitMix64, count: usize) -> Vec<Policy> {
    let mut drawn = Vec::with_capacity(count);
    while drawn.len() < count {
        let mut deck = POLICIES;
        rng.shuffle(&mut deck);
        drawn.extend(deck);
    }
    drawn.truncate(count);
    drawn
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum World {
    Earth,
    Drawn,
}

impl World {
    fn name(self) -> &'static str {
        match self {
            World::Earth => "earth",
            World::Drawn => "drawn",
        }
    }

    /// The house's rules at the house's pace, on this world. A drawn world
    /// has the house's shape, drawn from `seed`.
    fn ruleset(self, seed: u64) -> RealmRulesetSnapshot {
        let mut ruleset = RealmRulesetSnapshot::at_pace(
            house_ruleset(),
            pace_by_id(HOUSE_PACE).expect("the house pace is on the roster"),
        );
        match self {
            World::Earth => ruleset.map_id = EARTH_MAP_ID.to_string(),
            World::Drawn => {
                ruleset.map_id = GENERATED_MAP_ID.to_string();
                ruleset.map_spec = Some(
                    GeneratedMapSpec {
                        seed,
                        ..GeneratedMapSpec::default()
                    }
                    .normalized(),
                );
            }
        }
        ruleset
    }
}

/// Who plays a season: how many at the start, and who arrives later, as
/// (season day, how many).
struct Roster {
    label: &'static str,
    starters: usize,
    arrivals: &'static [(i32, usize)],
}

const ROSTERS: [Roster; 6] = [
    Roster {
        label: "2p",
        starters: 2,
        arrivals: &[],
    },
    Roster {
        label: "5p",
        starters: 5,
        arrivals: &[],
    },
    Roster {
        label: "10p",
        starters: 10,
        arrivals: &[],
    },
    Roster {
        label: "20p",
        starters: 20,
        arrivals: &[],
    },
    Roster {
        label: "10p+5@5+5@10",
        starters: 10,
        arrivals: &[(5, 5), (10, 5)],
    },
    Roster {
        label: "10p+5@20",
        starters: 10,
        arrivals: &[(20, 5)],
    },
];

/// Five players, two of them pure turtles.
const TURTLES_LABEL: &str = "5p+2turtles";

/// How a season ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Won { kind: WinKind, policy: Policy },
    Dissolved,
}

impl Finish {
    fn name(self) -> &'static str {
        match self {
            Finish::Won {
                kind: WinKind::Dominated,
                ..
            } => "dominated",
            Finish::Won {
                kind: WinKind::SeasonOver,
                ..
            } => "clock",
            Finish::Won {
                kind: WinKind::LastStanding,
                ..
            } => "last_standing",
            Finish::Dissolved => "dissolved",
        }
    }

    fn winner_policy(self) -> Option<Policy> {
        match self {
            Finish::Won { policy, .. } => Some(policy),
            Finish::Dissolved => None,
        }
    }
}

/// Somebody due after the start.
struct Arrival {
    /// The season day they came on.
    day: i32,
    /// `None` when the doors were shut or the roster full.
    user_id: Option<Uuid>,
    /// What they held on the night their shield ran out; `None` when the
    /// season ended first, or they never got in.
    held_at_shield_end: Option<u16>,
}

/// Which holdings count as border when digging in.
#[derive(Clone, Copy)]
enum Border {
    /// Any land neighbour that is not ours, free or held.
    Whole,
    /// Only a land neighbour a rival holds.
    FacingRivals,
}

/// Whose land a pressed take may go for.
#[derive(Clone, Copy)]
enum Against {
    AnyRival,
    Leader(Uuid),
    /// The player at the domination bar: by land or by sea, nothing is
    /// held back.
    Dominator(Uuid),
}

/// What one season did: the row the table and the CSV are made of.
struct Season {
    label: &'static str,
    world: World,
    seed: u64,
    /// The season day whose night, or whose rollover, ended it.
    days: i32,
    finish: Finish,
    /// The first season day the eventual winner held the biggest share.
    winner_first_led: Option<i32>,
    /// Whether the share leader after day 5's night went on to win; `None`
    /// without a winner or a fifth day.
    day5_leader_won: Option<bool>,
    nights: u32,
    /// Territories that changed hands over the season.
    taken: u32,
    /// Takes the field ordered against a player at the bar.
    alarm: u32,
    bounces: u32,
    fizzles: u32,
    /// The biggest share of the world anyone alive held at the end: how
    /// close the season came to a domination.
    top_share: f64,
    /// The biggest share after each night, by season day: how a lead grows.
    top_by_day: Vec<(i32, f64)>,
    /// Wall levels ordered over the season, by the winner and by the average
    /// of everyone else.
    winner_walls: Option<u32>,
    loser_walls: Option<f64>,
    /// Territories taken from players, summed by policy, with how many
    /// players played it: (lost, players).
    lost_by_policy: BTreeMap<Policy, (u32, u32)>,
    /// The share of the world held at the end, summed by policy, with how
    /// many players played it: which way of playing pays.
    share_by_policy: BTreeMap<Policy, (f64, u32)>,
    /// Land nobody held after day 10's night.
    free_day10: Option<usize>,
    /// The share of the roster still alive at the end.
    alive_share: f64,
    /// What the absentees held at the end, on average.
    absentee_held: Option<f64>,
    /// The absentees who started, and how many were alive after day 7: a
    /// full cycle of two days on and three off, and a weekend more.
    absentees_day7: Option<(u32, u32)>,
    arrivals: Vec<Arrival>,
    /// Orders the engine refused, by reason. A policy only orders what the
    /// board allows, so anything here is a policy or an engine bug.
    rejects: BTreeMap<&'static str, u32>,
    /// Every night's log and the final board, hashed: the same seed must
    /// give the same digest.
    digest: String,
}

impl Season {
    /// One line for the test log.
    fn summary(&self) -> String {
        format!(
            "{} on {} (seed {}): {} by {} on day {} · winner led from day {} · {} bounces and {} \
             fizzles a night · {} free after day 10 · {:.0}% alive · orders refused {:?}",
            self.label,
            self.world.name(),
            self.seed,
            self.finish.name(),
            self.finish
                .winner_policy()
                .map(Policy::name)
                .unwrap_or("nobody"),
            self.days,
            show(self.winner_first_led),
            per(self.bounces, self.nights),
            per(self.fizzles, self.nights),
            show(self.free_day10),
            self.alive_share * 100.0,
            self.rejects,
        )
    }

    /// Territories lost per player of `policy`, when anyone played it.
    fn lost_per(&self, policy: Policy) -> Option<f64> {
        let (lost, players) = self.lost_by_policy.get(&policy).copied()?;
        (players > 0).then(|| f64::from(lost) / f64::from(players))
    }
}

struct Sim {
    label: &'static str,
    world: World,
    seed: u64,
    game_id: Uuid,
    state: RealmGameState,
    map: Arc<WorldMap>,
    /// Everyone admitted, in roster order, with how they play.
    policies: Vec<(Uuid, Policy)>,
    /// How many of `policies` started the season.
    starters: usize,
    /// Who is still to come: (season day, policy).
    pending: Vec<(i32, Policy)>,
    arrivals: Vec<Arrival>,
    /// The open day.
    day: i32,
    nights: u32,
    taken: u32,
    /// Takes ordered against a dominator by the field.
    alarm: u32,
    bounces: u32,
    fizzles: u32,
    walls: BTreeMap<Uuid, u32>,
    lost: BTreeMap<Uuid, u32>,
    /// The share leader after each night, by season day.
    leaders: Vec<(i32, Option<Uuid>)>,
    top_by_day: Vec<(i32, f64)>,
    free_day10: Option<usize>,
    absentees_day7: Option<(u32, u32)>,
    rejects: BTreeMap<&'static str, u32>,
    digest: blake3::Hasher,
}

impl Sim {
    /// A season of `roster` on `world`, its policies drawn by `seed`.
    fn of_roster(roster: &Roster, world: World, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed ^ 0x5EA5_0002);
        let starters = draw_policies(&mut rng, roster.starters);
        let mut arrivals = Vec::new();
        for (day, count) in roster.arrivals {
            for policy in draw_policies(&mut rng, *count) {
                arrivals.push((*day, policy));
            }
        }
        Self::new(roster.label, world, seed, &starters, &arrivals)
    }

    /// Five players, two of them pure turtles, the other three drawn from
    /// everything else.
    fn turtles(world: World, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed ^ 0x7047_71E5);
        let mut starters = vec![Policy::Turtle, Policy::Turtle];
        starters.extend(
            draw_policies(&mut rng, 2 * POLICIES.len())
                .into_iter()
                .filter(|policy| *policy != Policy::Turtle)
                .take(3),
        );
        Self::new(TURTLES_LABEL, world, seed, &starters, &[])
    }

    /// A season with its policies spelled out: who starts, and who arrives
    /// on which season day.
    fn new(
        label: &'static str,
        world: World,
        seed: u64,
        starters: &[Policy],
        arrivals: &[(i32, Policy)],
    ) -> Self {
        let ruleset = world.ruleset(seed);
        let map = map_handle(&ruleset.map_id, ruleset.map_spec.as_ref()).expect("the world builds");
        // An empty world on its muster day, the way `open_world` opens one.
        let mut state = RealmGameState {
            version: STATE_VERSION,
            revision: 1,
            ruleset,
            players: Vec::new(),
            ownership: BTreeMap::new(),
            forts: BTreeMap::new(),
            last_day: None,
            start_player_count: 0,
            start_day: START_DAY,
            news: None,
            last_night: None,
        };
        state.roll_day(START_DAY, day_seed(Uuid::nil(), START_DAY));
        let mut sim = Self {
            label,
            world,
            seed,
            game_id: Uuid::from_u128(0x5EA5_0000_0000_0000_0000_0000_0000_0000 | u128::from(seed)),
            state,
            map,
            policies: Vec::new(),
            starters: 0,
            pending: arrivals.to_vec(),
            arrivals: Vec::new(),
            day: START_DAY,
            nights: 0,
            taken: 0,
            alarm: 0,
            bounces: 0,
            fizzles: 0,
            walls: BTreeMap::new(),
            lost: BTreeMap::new(),
            leaders: Vec::new(),
            top_by_day: Vec::new(),
            free_day10: None,
            absentees_day7: None,
            rejects: BTreeMap::new(),
            digest: blake3::Hasher::new(),
        };
        // The starters come through the gates during the muster, so their
        // clocks start on the first day played.
        for policy in starters {
            sim.admit(*policy, START_DAY + 1)
                .expect("an empty world lets its starters in");
        }
        sim.starters = sim.policies.len();
        sim
    }

    /// A join through the service's own pure step, refused the way
    /// `join_game` refuses: a full roster, or the doors shut.
    fn admit(&mut self, policy: Policy, joined_day: i32) -> Option<Uuid> {
        let full = self.state.players.len() >= usize::from(self.state.ruleset.max_players);
        if full || !self.state.joinable(&self.map) {
            return None;
        }
        let slot = self.policies.len();
        let user_id = Uuid::from_u128(slot as u128 + 1);
        let spawned = admit_player(
            &mut self.state,
            &self.map,
            self.game_id,
            user_id,
            &format!("p{slot}"),
            joined_day,
            slot as u8,
        );
        assert!(spawned, "a join the doors let in found no free land");
        self.state.revision += 1;
        self.policies.push((user_id, policy));
        Some(user_id)
    }

    /// Play the season out: each day the arrivals come, orders are written
    /// and the night resolves them, until a night (or an abandoned rollover)
    /// ends it. The clock's last night always does.
    fn run(mut self) -> Season {
        let last = season_last_day(&self.state).expect("the campaign runs on a clock");
        loop {
            self.arrive();
            self.write_orders();
            self.check_day();
            let opening = self.state.clone();
            let report = resolve_night(&mut self.state, &self.map);
            self.check_night(&opening, &report);
            self.measure_night(&report);
            let today = self.day - START_DAY;
            match report.end {
                ResolveEnd::Ongoing => {}
                ResolveEnd::Won { winner, kind } => {
                    return self.finish(today, Some((winner, kind)));
                }
                ResolveEnd::Dissolved => return self.finish(today, None),
            }
            assert!(
                self.day < last,
                "the clock's last night left the season running"
            );
            // The rollover, as `close_night` runs it after a night that did
            // not end the game.
            self.day += 1;
            self.state
                .roll_day(self.day, day_seed(self.game_id, self.day));
            if dissolve_if_abandoned(&mut self.state, self.day).is_some() {
                let dissolved = self.day - START_DAY;
                return self.finish(dissolved, None);
            }
        }
    }

    /// Today's arrivals come through the gates, before anybody orders.
    fn arrive(&mut self) {
        let today = self.day - START_DAY;
        let due: Vec<Policy> = self
            .pending
            .iter()
            .filter(|(day, _)| *day == today)
            .map(|(_, policy)| *policy)
            .collect();
        for policy in due {
            let user_id = self.admit(policy, self.day);
            self.arrivals.push(Arrival {
                day: today,
                user_id,
                held_at_shield_end: None,
            });
        }
    }

    /// Everyone who plays today writes their orders, in roster order. The
    /// order of writing does not matter: a player's board counts only their
    /// own orders, so nobody's choices see anybody else's.
    fn write_orders(&mut self) {
        // The muster: the service refuses every order until the gates open.
        if self.day == START_DAY {
            return;
        }
        let leader = self.share_leader();
        for (slot, (actor, policy)) in self.policies.clone().into_iter().enumerate() {
            if !self.state.is_alive(actor) || !policy.plays_on(self.day, slot as i32) {
                continue;
            }
            // Ownership and phases hold still all day, so what may be taken
            // is worked out once.
            let takeable: Vec<TerritoryId> = self
                .map
                .territories
                .iter()
                .map(|t| t.id)
                .filter(|t| check_take(&self.state, &self.map, actor, *t, self.day).is_ok())
                .collect();
            let dominator = self.dominator();
            while let Some(order) = self.next_order(actor, policy, &takeable, leader) {
                if let (Order::Take { target, .. }, Some(dominator)) = (order, dominator) {
                    if self.state.ownership.get(&target) == Some(&dominator) {
                        self.alarm += 1;
                    }
                }
                match place_order(&mut self.state, &self.map, actor, order, self.day) {
                    Ok(_) => match order {
                        Order::Fortify { levels, .. } => {
                            *self.walls.entry(actor).or_default() += u32::from(levels.get());
                        }
                        Order::Take { .. } => {}
                    },
                    Err(rejected) => {
                        *self.rejects.entry(reject_name(rejected)).or_default() += 1;
                        break;
                    }
                }
            }
        }
    }

    /// What `policy` orders next, given what this player has ordered
    /// already: `None` once it is out of points or of anything it would
    /// order.
    fn next_order(
        &self,
        actor: Uuid,
        policy: Policy,
        takeable: &[TerritoryId],
        leader: Option<Uuid>,
    ) -> Option<Order> {
        if self.state.points_left(actor, self.day) == 0 {
            return None;
        }
        let open = self.open_targets(actor, takeable);
        // The alarm: a player at the bar is everybody's problem, and the
        // board says so (`dom 1/3` in the standings). Whoever can reach them
        // across a land border presses them first, the three nights being
        // the window the rules give the field. Only a pure turtle sits it
        // out.
        if let Some(dominator) = self.dominator() {
            if dominator != actor && policy != Policy::Turtle {
                if let Some(order) = self.press(actor, &open, Against::Dominator(dominator)) {
                    return Some(order);
                }
            }
        }
        match policy {
            Policy::Settler | Policy::Absentee => self
                .settle(actor, &open)
                .or_else(|| self.raid(actor, &open)),
            Policy::Warlord => self
                .raid(actor, &open)
                .or_else(|| self.settle(actor, &open)),
            Policy::Cautious => self.safest(actor, &open),
            Policy::Turtle => self
                .dig_in(actor, Border::Whole)
                .or_else(|| self.settle(actor, &open)),
            Policy::Frontier => self.frontier(actor, &open),
            Policy::Coalition => match leader {
                Some(leader) if leader != actor => self
                    .press(actor, &open, Against::Leader(leader))
                    .or_else(|| self.frontier(actor, &open)),
                Some(_) | None => self.frontier(actor, &open),
            },
        }
    }

    /// Walls on the border facing rivals, then a pressed take, then free
    /// land.
    fn frontier(&self, actor: Uuid, open: &[(TerritoryId, Projection)]) -> Option<Order> {
        self.dig_in(actor, Border::FacingRivals)
            .or_else(|| self.press(actor, open, Against::AnyRival))
            .or_else(|| self.settle(actor, open))
    }

    /// Today's takeable targets this player has not ordered on yet, priced
    /// at one point on their own board.
    fn open_targets(
        &self,
        actor: Uuid,
        takeable: &[TerritoryId],
    ) -> Vec<(TerritoryId, Projection)> {
        let ordered = self.ordered(actor);
        takeable
            .iter()
            .filter(|target| !ordered.contains(target))
            .filter_map(|target| self.price(actor, *target, 1).map(|p| (*target, p)))
            .collect()
    }

    fn ordered(&self, actor: Uuid) -> BTreeSet<TerritoryId> {
        self.state
            .player(actor)
            .expect("a seated player")
            .orders
            .iter()
            .map(Order::target)
            .collect()
    }

    fn price(&self, actor: Uuid, target: TerritoryId, points: u8) -> Option<Projection> {
        projection(&self.state, &self.map, actor, target, points, self.day)
    }

    /// Free land, a march before a landing and the best odds first: one
    /// point while it buys `SETTLE_ODDS`, two after.
    fn settle(&self, actor: Uuid, open: &[(TerritoryId, Projection)]) -> Option<Order> {
        let (target, priced) = open
            .iter()
            .filter(|(_, p)| p.kind == TakeKind::Claim)
            .min_by(|(_, a), (_, b)| {
                a.landing
                    .cmp(&b.landing)
                    .then(b.probability.total_cmp(&a.probability))
            })?;
        let points = match priced.probability >= SETTLE_ODDS {
            true => 1,
            false => 2,
        };
        Some(self.take(actor, *target, points))
    }

    /// The best-odds rival land, with points until it reaches `RAID_ODDS`
    /// or the budget runs out.
    fn raid(&self, actor: Uuid, open: &[(TerritoryId, Projection)]) -> Option<Order> {
        let (target, _) = open
            .iter()
            .filter(|(_, p)| p.kind == TakeKind::Attack)
            .max_by(|(_, a), (_, b)| a.probability.total_cmp(&b.probability))?;
        let points = self.points_for(actor, *target, RAID_ODDS);
        Some(self.take(actor, *target, points))
    }

    /// The cheapest take at `SAFE_ODDS` or better, when one is affordable.
    fn safest(&self, actor: Uuid, open: &[(TerritoryId, Projection)]) -> Option<Order> {
        let left = self.state.points_left(actor, self.day);
        let safe = |target: TerritoryId, points: u8| {
            self.price(actor, target, points)
                .is_some_and(|p| p.probability >= SAFE_ODDS)
        };
        let (points, target) = open
            .iter()
            .filter(|(target, _)| safe(*target, left))
            .filter_map(|(target, _)| {
                (1..=left)
                    .find(|points| safe(*target, *points))
                    .map(|points| (points, *target))
            })
            .min()?;
        Some(self.take(actor, target, points))
    }

    /// Walls to `DIG_TO` on the first border holding not dug in yet nor
    /// ordered on today.
    fn dig_in(&self, actor: Uuid, border: Border) -> Option<Order> {
        if !self.state.option(OPTION_FORTIFY) {
            return None;
        }
        let to = DIG_TO.min(self.state.ruleset.fort_max_level);
        let ordered = self.ordered(actor);
        let target = self
            .state
            .holdings(actor)
            .into_iter()
            .filter(|t| !ordered.contains(t))
            .filter(|t| self.state.fort_level(*t) < to)
            .find(|t| self.on_border(actor, *t, border))?;
        let levels =
            (to - self.state.fort_level(target)).min(self.state.points_left(actor, self.day));
        Some(Order::Fortify {
            target,
            levels: NonZeroU8::new(levels).expect("a point is left and a level is missing"),
        })
    }

    /// Does this holding face land that is not its holder's?
    fn on_border(&self, actor: Uuid, holding: TerritoryId, border: Border) -> bool {
        self.map
            .territory(holding)
            .expect("a holding is on the map")
            .land_links
            .iter()
            .any(
                |(neighbour, _)| match (self.state.ownership.get(neighbour), border) {
                    (Some(holder), Border::Whole | Border::FacingRivals) => *holder != actor,
                    (None, Border::Whole) => true,
                    (None, Border::FacingRivals) => false,
                },
            )
    }

    /// `PRESS_POINTS` on the weakest rival land across a land border: the
    /// lowest defence, the leader's only when it is a coalition's.
    fn press(
        &self,
        actor: Uuid,
        open: &[(TerritoryId, Projection)],
        against: Against,
    ) -> Option<Order> {
        let (target, _) = open
            .iter()
            .filter(|(_, p)| p.kind == TakeKind::Attack)
            .filter(|(_, p)| match against {
                Against::AnyRival | Against::Leader(_) => !p.landing,
                Against::Dominator(_) => true,
            })
            .filter(|(target, _)| match against {
                Against::AnyRival => true,
                Against::Leader(leader) | Against::Dominator(leader) => {
                    self.state.ownership.get(target) == Some(&leader)
                }
            })
            .min_by(|(_, a), (_, b)| a.defence.total_cmp(&b.defence))?;
        Some(self.take(actor, *target, PRESS_POINTS))
    }

    /// The fewest points that reach `wanted`, or everything left when none
    /// do.
    fn points_for(&self, actor: Uuid, target: TerritoryId, wanted: f64) -> u8 {
        let left = self.state.points_left(actor, self.day);
        (1..=left)
            .find(|points| {
                self.price(actor, target, *points)
                    .is_some_and(|p| p.probability >= wanted)
            })
            .unwrap_or(left)
    }

    /// A take of `points`, or of what is left when that is less.
    fn take(&self, actor: Uuid, target: TerritoryId, points: u8) -> Order {
        let points = points.min(self.state.points_left(actor, self.day));
        Order::Take {
            target,
            points: NonZeroU8::new(points).expect("a take is only ordered with a point left"),
        }
    }

    /// Whoever stood at the domination bar last night, as the standings
    /// show it.
    fn dominator(&self) -> Option<Uuid> {
        self.state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive && p.dominating_nights > 0)
            .map(|p| p.user_id)
            .next()
    }

    /// The alive player with the biggest share of the world.
    fn share_leader(&self) -> Option<Uuid> {
        self.state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .map(|p| (p.user_id, share(&self.state, &self.map, p.user_id)))
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(user_id, _)| user_id)
    }

    /// Before the night: no budget overspent, the points spent are the
    /// orders standing, and nothing only a night may do has happened.
    fn check_day(&self) {
        let state = &self.state;
        let day = self.day;
        for player in state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
        {
            let standing = state.standing(player.user_id, day);
            assert!(
                standing.used <= standing.allowance,
                "day {day}: {} ordered {} of {}",
                player.username,
                standing.used,
                standing.allowance
            );
            // The base, the bank's cap in bases, and what the land pays.
            let ceiling = standing
                .base
                .saturating_mul(state.ruleset.bank_cap_days.saturating_add(1))
                .saturating_add(standing.land);
            assert!(
                standing.allowance <= ceiling,
                "day {day}: {} has an allowance of {} over a ceiling of {ceiling}",
                player.username,
                standing.allowance
            );
            let ordered: u16 = player.orders.iter().map(|o| u16::from(o.cost())).sum();
            assert_eq!(
                u16::from(standing.used),
                ordered,
                "day {day}: {} spent other points than their orders carry",
                player.username
            );
        }
        let open = state
            .last_day
            .as_ref()
            .expect("an orders game has an open day");
        for entry in &open.entries {
            assert!(
                !matches!(
                    entry,
                    LogEntry::Fought { .. } | LogEntry::Eliminated { .. } | LogEntry::Won { .. }
                ),
                "day {day}: {entry:?} was logged before the night"
            );
        }
    }

    /// After the night: the dead hold nothing, every order is drained, walls
    /// stay inside their bounds, a run of dominating nights moves by one or
    /// starts over, and every fight is what the rules say it is.
    fn check_night(&self, opening: &RealmGameState, report: &NightReport) {
        let state = &self.state;
        let day = self.day;
        for (territory, owner) in &state.ownership {
            assert!(
                self.map.territory(*territory).is_some(),
                "day {day}: ownership of a territory off the map"
            );
            let player = state.player(*owner).expect("an owner is a player");
            assert_eq!(
                player.status,
                RealmPlayerStatus::Alive,
                "day {day}: {} holds land while {:?}",
                player.username,
                player.status
            );
        }
        for player in &state.players {
            assert!(
                player.orders.is_empty(),
                "day {day}: the night left {}'s orders standing",
                player.username
            );
            if player.status != RealmPlayerStatus::Alive {
                assert!(
                    player.exit_day.is_some(),
                    "day {day}: {} is {:?} with no exit day",
                    player.username,
                    player.status
                );
            }
            if player.status == RealmPlayerStatus::Alive {
                let before = opening
                    .player(player.user_id)
                    .map(|p| p.dominating_nights)
                    .unwrap_or(0);
                assert!(
                    player.dominating_nights == before.saturating_add(1)
                        || player.dominating_nights == 0,
                    "day {day}: {}'s dominating nights went from {before} to {}",
                    player.username,
                    player.dominating_nights
                );
            }
        }
        let max = state.ruleset.fort_max_level;
        for (territory, level) in &state.forts {
            assert!(
                (1..=max).contains(level),
                "day {day}: walls of {level} on {territory}, outside 1 to {max}"
            );
        }
        self.check_fights(opening, report);
    }

    /// Every fight priced again on the board the night opened on, and
    /// landed the way its rolls say.
    fn check_fights(&self, opening: &RealmGameState, report: &NightReport) {
        let state = &self.state;
        let rs = &state.ruleset;
        let day = self.day;
        let night = state
            .last_night
            .as_ref()
            .expect("the night is kept on the row");
        let engaged: Vec<TerritoryId> = night.entries.iter().filter_map(fought_target).collect();
        let fizzled = night
            .entries
            .iter()
            .filter(|e| matches!(e, LogEntry::Fizzled { .. }))
            .count() as u32;
        assert_eq!(
            report.taken + report.held + report.bounced,
            engaged.len() as u32,
            "day {day}: the report counts other fights than the log holds"
        );
        assert_eq!(
            report.fizzled, fizzled,
            "day {day}: the report counts other fizzles than the log holds"
        );
        let board = night_board(opening, &engaged);
        for entry in &night.entries {
            let LogEntry::Fought {
                target,
                holder,
                taken_by,
                defence,
                takers,
                bounced,
            } = entry
            else {
                continue;
            };
            for roll in takers {
                assert!(
                    (rs.prob_floor..=rs.prob_ceil).contains(&roll.probability),
                    "day {day}: odds of {} on {target} outside the clamps",
                    roll.probability
                );
                let priced = projection(&board, &self.map, roll.user_id, *target, roll.points, day)
                    .expect("a fought take is priced");
                assert!(
                    (priced.probability - roll.probability).abs() < 1e-12
                        && (priced.force - roll.force).abs() < 1e-9
                        && (priced.defence - defence).abs() < 1e-9
                        && priced.from == roll.from,
                    "day {day}: the night rolled {:.6} (force {:.4} against {:.4}) on {target}, \
                     the board prices {:.6} (force {:.4} against {:.4})",
                    roll.probability,
                    roll.force,
                    defence,
                    priced.probability,
                    priced.force,
                    priced.defence
                );
            }
            let best = takers.iter().filter(|r| r.success).map(|r| r.force).fold(
                None,
                |top: Option<f64>, force| match top {
                    Some(top) if top >= force => Some(top),
                    _ => Some(force),
                },
            );
            let strongest = takers
                .iter()
                .filter(|r| r.success && Some(r.force) == best)
                .count();
            let now = state.ownership.get(target).copied();
            match (taken_by, bounced) {
                (Some(taker), false) => {
                    assert!(
                        strongest == 1
                            && takers
                                .iter()
                                .any(|r| r.user_id == *taker && r.success && Some(r.force) == best),
                        "day {day}: {target} went to a take that was not the one strongest success"
                    );
                    assert_eq!(
                        now,
                        Some(*taker),
                        "day {day}: {target} was taken but did not move"
                    );
                }
                (None, true) => {
                    assert!(
                        strongest >= 2,
                        "day {day}: {target} bounced without a tie at the top"
                    );
                    assert_eq!(now, *holder, "day {day}: a bounce on {target} moved it");
                }
                (None, false) => {
                    assert!(
                        best.is_none(),
                        "day {day}: {target} held through a successful take"
                    );
                    assert_eq!(now, *holder, "day {day}: {target} held but moved");
                }
                (Some(_), true) => panic!("day {day}: {target} was both taken and bounced"),
            }
        }
    }

    /// What tonight did, for the table.
    fn measure_night(&mut self, report: &NightReport) {
        let today = self.day - START_DAY;
        self.nights += 1;
        self.taken += report.taken;
        self.bounces += report.bounced;
        self.fizzles += report.fizzled;
        let night = self
            .state
            .last_night
            .as_ref()
            .expect("the night is kept on the row");
        for entry in &night.entries {
            if let LogEntry::Fought {
                holder: Some(holder),
                taken_by: Some(_),
                ..
            } = entry
            {
                *self.lost.entry(*holder).or_default() += 1;
            }
        }
        self.digest.update(
            serde_json::to_string(night)
                .expect("a night serializes")
                .as_bytes(),
        );
        let leader = self.share_leader();
        self.leaders.push((today, leader));
        let top = leader
            .map(|user_id| share(&self.state, &self.map, user_id))
            .unwrap_or(0.0);
        self.top_by_day.push((today, top));
        if today == 10 {
            self.free_day10 = Some(self.map.territories.len() - self.state.ownership.len());
        }
        if today == 7 {
            // The absentees who were there for the opening: one who sits
            // out the first three days holds a spawn and nothing else, and
            // no rule is meant to save that.
            let absentees: Vec<Uuid> = self.policies[..self.starters]
                .iter()
                .enumerate()
                .filter(|(slot, (_, policy))| {
                    *policy == Policy::Absentee && policy.plays_on(START_DAY + 1, *slot as i32)
                })
                .map(|(_, (user_id, _))| *user_id)
                .collect();
            if !absentees.is_empty() {
                let alive = absentees
                    .iter()
                    .filter(|u| self.state.is_alive(**u))
                    .count();
                self.absentees_day7 = Some((absentees.len() as u32, alive as u32));
            }
        }
        // A shield runs out tonight when tomorrow is the first day off it.
        for arrival in self.arrivals.iter_mut() {
            let Some(user_id) = arrival.user_id else {
                continue;
            };
            let shielded = |day: i32| matches!(self.state.phase(user_id, day), PlayerPhase::New);
            if arrival.held_at_shield_end.is_none() && shielded(self.day) && !shielded(self.day + 1)
            {
                arrival.held_at_shield_end = Some(self.state.territory_count(user_id));
            }
        }
    }

    /// The season's row. `winner` is `None` for a dissolve.
    fn finish(mut self, days: i32, winner: Option<(Uuid, WinKind)>) -> Season {
        self.digest.update(
            serde_json::to_string(&self.state)
                .expect("the board serializes")
                .as_bytes(),
        );
        let policy_of = |user_id: Uuid| {
            self.policies
                .iter()
                .find(|(u, _)| *u == user_id)
                .map(|(_, policy)| *policy)
                .expect("the winner was seated")
        };
        let finish = match winner {
            Some((user_id, kind)) => Finish::Won {
                kind,
                policy: policy_of(user_id),
            },
            None => Finish::Dissolved,
        };
        let winner = winner.map(|(user_id, _)| user_id);
        let walls = |user_id: Uuid| self.walls.get(&user_id).copied().unwrap_or(0);
        let mut lost_by_policy: BTreeMap<Policy, (u32, u32)> = BTreeMap::new();
        let mut share_by_policy: BTreeMap<Policy, (f64, u32)> = BTreeMap::new();
        for (user_id, policy) in &self.policies {
            let (lost, players) = lost_by_policy.entry(*policy).or_default();
            *lost += self.lost.get(user_id).copied().unwrap_or(0);
            *players += 1;
            let (held, players) = share_by_policy.entry(*policy).or_default();
            *held += share(&self.state, &self.map, *user_id);
            *players += 1;
        }
        let alive = self
            .state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .count();
        let top_share = self
            .state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .map(|p| share(&self.state, &self.map, p.user_id))
            .fold(0.0, f64::max);
        Season {
            label: self.label,
            world: self.world,
            seed: self.seed,
            days,
            finish,
            winner_first_led: winner.and_then(|w| {
                self.leaders
                    .iter()
                    .find(|(_, leader)| *leader == Some(w))
                    .map(|(day, _)| *day)
            }),
            day5_leader_won: winner.and_then(|w| {
                self.leaders
                    .iter()
                    .find(|(day, _)| *day == 5)
                    .map(|(_, leader)| *leader == Some(w))
            }),
            nights: self.nights,
            taken: self.taken,
            alarm: self.alarm,
            bounces: self.bounces,
            fizzles: self.fizzles,
            top_share,
            top_by_day: self.top_by_day,
            winner_walls: winner.map(walls),
            loser_walls: mean(
                self.policies
                    .iter()
                    .filter(|(user_id, _)| Some(*user_id) != winner)
                    .map(|(user_id, _)| f64::from(walls(*user_id))),
            ),
            lost_by_policy,
            share_by_policy,
            free_day10: self.free_day10,
            alive_share: alive as f64 / self.policies.len() as f64,
            absentee_held: mean(
                self.policies
                    .iter()
                    .filter(|(_, policy)| *policy == Policy::Absentee)
                    .map(|(user_id, _)| f64::from(self.state.territory_count(*user_id))),
            ),
            absentees_day7: self.absentees_day7,
            arrivals: self.arrivals,
            rejects: self.rejects,
            digest: self.digest.finalize().to_hex().to_string(),
        }
    }
}

fn fought_target(entry: &LogEntry) -> Option<TerritoryId> {
    let LogEntry::Fought { target, .. } = entry else {
        return None;
    };
    Some(*target)
}

/// The board the night priced its fights on, rebuilt from the opening board
/// by hand: tonight's walls up, and every target a take survived to set as
/// everyone's own orders, so `projection` engages exactly what the night
/// engaged.
fn night_board(opening: &RealmGameState, engaged: &[TerritoryId]) -> RealmGameState {
    let mut board = opening.clone();
    for player in opening
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
    {
        for order in &player.orders {
            match *order {
                Order::Fortify { target, levels } => {
                    board
                        .forts
                        .insert(target, opening.fort_level(target) + levels.get());
                }
                Order::Take { .. } => {}
            }
        }
    }
    let takes: Vec<Order> = engaged
        .iter()
        .map(|target| Order::Take {
            target: *target,
            points: NonZeroU8::MIN,
        })
        .collect();
    for player in board.players.iter_mut() {
        player.orders = takes.clone();
    }
    board
}

fn reject_name(rejected: OrderRejected) -> &'static str {
    match rejected {
        OrderRejected::NotPlaying => "not_playing",
        OrderRejected::UnknownTerritory => "unknown",
        OrderRejected::AlreadyYours => "already_yours",
        OrderRejected::NotYours => "not_yours",
        OrderRejected::NoFortifying => "no_fortifying",
        OrderRejected::FullyFortified { .. } => "fully_fortified",
        OrderRejected::NoFront => "no_front",
        OrderRejected::NotEnoughPoints { .. } => "no_points",
        OrderRejected::AttacksLocked(_) => "attacks_locked",
        OrderRejected::LandingsLocked(_) => "landings_locked",
        OrderRejected::TargetIsNew(_) => "target_new",
        OrderRejected::TargetIsRamping(_) => "target_ramping",
        OrderRejected::NoSuchOrder => "no_such_order",
    }
}

fn mean(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values
        .into_iter()
        .fold((0.0, 0u32), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / f64::from(count))
}

fn per(count: u32, nights: u32) -> String {
    match nights {
        0 => "-".into(),
        nights => format!("{:.2}", f64::from(count) / f64::from(nights)),
    }
}

fn show<T: std::fmt::Display>(value: Option<T>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".into(),
    }
}

fn show_f(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.2}"),
        None => "-".into(),
    }
}

fn required_env(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) => value,
        Err(e) => panic!("{name} must be set for the full-scale campaign simulation: {e}"),
    }
}

/// One line per roster and world, everything averaged over its seeds.
fn table(seasons: &[Season]) -> String {
    let mut batches: Vec<(&'static str, World)> = Vec::new();
    for season in seasons {
        if !batches.contains(&(season.label, season.world)) {
            batches.push((season.label, season.world));
        }
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<13} {:<5} {:>5} {:>11} {:>14} {:>14} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>11} {:>11} {:>6} {:>5} {:>6} {:>4} {:>4}",
        "roster",
        "world",
        "seeds",
        "days",
        "dom/clk/ls/x",
        "top7/14/21",
        "top%",
        "led",
        "d5",
        "take",
        "alarm",
        "bnc",
        "fiz",
        "walls w/l",
        "lost f/w",
        "free10",
        "alive",
        "absent",
        "shut",
        "rej"
    );
    for (label, world) in batches {
        let batch: Vec<&Season> = seasons
            .iter()
            .filter(|s| s.label == label && s.world == world)
            .collect();
        let days: Vec<i32> = batch.iter().map(|s| s.days).collect();
        let finished = |name: &str| batch.iter().filter(|s| s.finish.name() == name).count();
        let d5: Vec<bool> = batch.iter().filter_map(|s| s.day5_leader_won).collect();
        let nights: u32 = batch.iter().map(|s| s.nights).sum();
        let lost = |policy: Policy| {
            let (lost, players) = batch
                .iter()
                .filter_map(|s| s.lost_by_policy.get(&policy))
                .fold((0u32, 0u32), |(l, p), (lost, players)| {
                    (l + lost, p + players)
                });
            (players > 0).then(|| f64::from(lost) / f64::from(players))
        };
        let _ = writeln!(
            out,
            "{:<13} {:<5} {:>5} {:>11} {:>14} {:>14} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>11} {:>11} {:>6} {:>5} {:>6} {:>4} {:>4}",
            label,
            world.name(),
            batch.len(),
            format!(
                "{}/{}/{}",
                show(days.iter().min()),
                show_f(mean(days.iter().map(|d| f64::from(*d)))),
                show(days.iter().max())
            ),
            format!(
                "{}/{}/{}/{}",
                finished("dominated"),
                finished("clock"),
                finished("last_standing"),
                finished("dissolved")
            ),
            {
                let at = |day: i32| {
                    show_f(mean(batch.iter().filter_map(|s| {
                        s.top_by_day
                            .iter()
                            .find(|(d, _)| *d == day)
                            .map(|(_, top)| top * 100.0)
                    })))
                };
                format!("{}/{}/{}", at(7), at(14), at(21))
            },
            show_f(mean(batch.iter().map(|s| s.top_share * 100.0))),
            show_f(mean(
                batch
                    .iter()
                    .filter_map(|s| s.winner_first_led.map(f64::from))
            )),
            show_f(mean(d5.iter().map(|won| f64::from(u8::from(*won))))),
            per(batch.iter().map(|s| s.taken).sum(), nights),
            batch.iter().map(|s| s.alarm).sum::<u32>(),
            per(batch.iter().map(|s| s.bounces).sum(), nights),
            per(batch.iter().map(|s| s.fizzles).sum(), nights),
            format!(
                "{}/{}",
                show_f(mean(
                    batch.iter().filter_map(|s| s.winner_walls.map(f64::from))
                )),
                show_f(mean(batch.iter().filter_map(|s| s.loser_walls)))
            ),
            format!(
                "{}/{}",
                show_f(lost(Policy::Frontier)),
                show_f(lost(Policy::Warlord))
            ),
            show_f(mean(
                batch.iter().filter_map(|s| s.free_day10.map(|f| f as f64))
            )),
            show_f(mean(batch.iter().map(|s| s.alive_share))),
            show_f(mean(batch.iter().filter_map(|s| s.absentee_held))),
            batch
                .iter()
                .flat_map(|s| &s.arrivals)
                .filter(|a| a.user_id.is_none())
                .count(),
            batch.iter().flat_map(|s| s.rejects.values()).sum::<u32>()
        );
    }
    // Which way of playing pays, over every season: the share held at the
    // end per player of each policy, and the seasons it won.
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{:<10} {:>7} {:>5} {:>7}",
        "policy", "share%", "won", "players"
    );
    for policy in POLICIES {
        let (held, players) = seasons
            .iter()
            .filter_map(|s| s.share_by_policy.get(&policy))
            .fold((0.0, 0u32), |(h, p), (held, players)| {
                (h + held, p + players)
            });
        let won = seasons
            .iter()
            .filter(|s| s.finish.winner_policy() == Some(policy))
            .count();
        let _ = writeln!(
            out,
            "{:<10} {:>7} {:>5} {:>7}",
            policy.name(),
            show_f((players > 0).then(|| held / f64::from(players) * 100.0)),
            won,
            players
        );
    }
    out
}

/// One row per season.
fn csv(seasons: &[Season]) -> String {
    let mut out = String::from(
        "roster,world,seed,days,finish,winner_policy,top_share,winner_first_led,day5_leader_won,\
         nights,taken,bounces,fizzles,winner_walls,loser_walls,frontier_lost,warlord_lost,\
         free_day10,alive_share,absentee_held,joins_refused,orders_refused\n",
    );
    for s in seasons {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{:.3},{},{},{},{},{},{},{},{},{},{},{},{:.3},{},{},{}",
            s.label,
            s.world.name(),
            s.seed,
            s.days,
            s.finish.name(),
            s.finish.winner_policy().map(Policy::name).unwrap_or(""),
            s.top_share,
            show(s.winner_first_led),
            show(s.day5_leader_won),
            s.nights,
            s.taken,
            s.bounces,
            s.fizzles,
            show(s.winner_walls),
            show_f(s.loser_walls),
            show_f(s.lost_per(Policy::Frontier)),
            show_f(s.lost_per(Policy::Warlord)),
            show(s.free_day10),
            s.alive_share,
            show_f(s.absentee_held),
            s.arrivals.iter().filter(|a| a.user_id.is_none()).count(),
            s.rejects.values().sum::<u32>()
        );
    }
    out
}

/// The bars the full-scale run fails on (`RULES.md`, "Orders"). Everything
/// else in the table is a number to read.
fn accept(seasons: &[Season]) {
    // Every bar is checked and every miss reported together, so one run
    // says everything that is wrong with the numbers.
    let mut failures: Vec<String> = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            failures.push(what);
        }
    };
    let roster = |label: &'static str| seasons.iter().filter(move |s| s.label == label);

    // Ten players and twenty, at the house pace: a winner between days 14
    // and 28, and the clock deciding at most half of them.
    for label in ["10p", "20p"] {
        let batch: Vec<&Season> = roster(label).collect();
        for s in &batch {
            check(
                matches!(s.finish, Finish::Won { .. }) && (14..=28).contains(&s.days),
                format!(
                    "{label} should leave a winner between days 14 and 28: {}",
                    s.summary()
                ),
            );
        }
        let by_clock = batch
            .iter()
            .filter(|s| {
                matches!(
                    s.finish,
                    Finish::Won {
                        kind: WinKind::SeasonOver,
                        ..
                    }
                )
            })
            .count();
        check(
            by_clock * 2 <= batch.len(),
            format!(
                "the clock decided {by_clock} of {} {label} seasons",
                batch.len()
            ),
        );
    }

    // A duel ends inside the clock: with a winner, or dissolved when
    // neither player kept playing it.
    for s in roster("2p") {
        check(
            s.days < 28 || matches!(s.finish, Finish::Won { .. }),
            format!("the duel should end: {}", s.summary()),
        );
    }

    // An early lead is not the result.
    let d5: Vec<bool> = seasons.iter().filter_map(|s| s.day5_leader_won).collect();
    let d5_rate =
        mean(d5.iter().map(|won| f64::from(u8::from(*won)))).expect("seasons with a day 5");
    check(
        d5_rate < 0.60,
        format!(
            "the day-5 leader won {:.0}% of {} seasons",
            d5_rate * 100.0,
            d5.len()
        ),
    );

    // Digging in is not a way to win: a pure turtle never takes the world,
    // and is handed it by the clock in fewer than one stalemate in five.
    // In a season where nobody fights much the one who lost nothing can
    // be ahead at the bell; the rules' job is that walls never conquer.
    let turtles: Vec<&Season> = roster(TURTLES_LABEL).collect();
    for s in &turtles {
        check(
            !matches!(
                s.finish,
                Finish::Won {
                    kind: WinKind::Dominated | WinKind::LastStanding,
                    policy: Policy::Turtle
                }
            ),
            format!("a pure turtle took the world: {}", s.summary()),
        );
    }
    let turtle_clocks = turtles
        .iter()
        .filter(|s| s.finish.winner_policy() == Some(Policy::Turtle))
        .count();
    check(
        turtle_clocks * 5 < turtles.len().max(1),
        format!(
            "a pure turtle was handed {turtle_clocks} of {} turtle seasons at the bell",
            turtles.len()
        ),
    );

    // Walls on the front cost a frontier less ground than a warlord loses.
    let lost_per = |policy: Policy| {
        let (lost, players) = seasons
            .iter()
            .filter_map(|s| s.lost_by_policy.get(&policy))
            .fold((0u32, 0u32), |(l, p), (lost, players)| {
                (l + lost, p + players)
            });
        assert!(players > 0, "no season seated a {}", policy.name());
        f64::from(lost) / f64::from(players)
    };
    let (frontier, warlord) = (lost_per(Policy::Frontier), lost_per(Policy::Warlord));
    check(
        frontier < warlord,
        format!("a frontier lost {frontier:.2} territories a season, a warlord {warlord:.2}"),
    );

    // A quiet spell is survivable: two days on and three off, twice over,
    // and three in four are still on the board. Not all: twenty players
    // on one Earth carve up a border nobody is holding.
    let (absentees, alive) = seasons
        .iter()
        .filter_map(|s| s.absentees_day7)
        .fold((0u32, 0u32), |(a, l), (absentees, alive)| {
            (a + absentees, l + alive)
        });
    check(
        alive * 4 >= absentees * 3,
        format!("only {alive} of {absentees} absentees were alive on day 7"),
    );

    // A day-10 arrival comes out of the shield with more than the spawn,
    // more often than not: a world nine tenths claimed can put a spawn on
    // a speck with nothing free in reach, and an arrival that sits its
    // first days out holds what it was given.
    let late: Vec<&Arrival> = seasons
        .iter()
        .flat_map(|s| &s.arrivals)
        .filter(|a| a.day == 10)
        .collect();
    check(
        late.iter().any(|a| a.user_id.is_some()),
        "every day-10 arrival found the doors shut".to_string(),
    );
    let shielded: Vec<u16> = late.iter().filter_map(|a| a.held_at_shield_end).collect();
    let grew = shielded.iter().filter(|held| **held > 1).count();
    check(
        grew * 2 >= shielded.len(),
        format!(
            "only {grew} of {} day-10 arrivals held more than their spawn when their shield ended",
            shielded.len()
        ),
    );

    assert!(
        failures.is_empty(),
        "{} bars missed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_campaign_season_plays_out_and_keeps_its_invariants() {
    let season = Sim::new(
        "5p",
        World::Earth,
        7,
        &[
            Policy::Settler,
            Policy::Warlord,
            Policy::Cautious,
            Policy::Frontier,
            Policy::Coalition,
        ],
        &[],
    )
    .run();
    println!("{}", season.summary());
    assert!(
        matches!(season.finish, Finish::Won { .. }),
        "five players who all play leave a winner: {}",
        season.summary()
    );
    assert!(
        season.rejects.is_empty(),
        "every order a policy wrote was one the board allowed: {}",
        season.summary()
    );
}

#[test]
fn the_same_seed_plays_the_same_season() {
    let play = || {
        Sim::new(
            "3p+1",
            World::Earth,
            11,
            &[Policy::Settler, Policy::Warlord, Policy::Absentee],
            &[(3, Policy::Coalition)],
        )
        .run()
        .digest
    };
    assert_eq!(play(), play());
}

/// The full scale, run on purpose (see the module doc): every roster on both
/// worlds, `REALM_SIM_SEEDS` seeds each, the table printed, the rows written
/// to `REALM_SIM_OUT`, and the acceptance bars checked last so a failing bar
/// still leaves the numbers to read.
#[test]
#[ignore]
fn realm_campaign_sim() {
    let seeds: u64 = match required_env("REALM_SIM_SEEDS").parse() {
        Ok(seeds) => seeds,
        Err(e) => panic!("REALM_SIM_SEEDS must be a whole number of seeds: {e}"),
    };
    let out = required_env("REALM_SIM_OUT");
    let mut seasons = Vec::new();
    for world in [World::Earth, World::Drawn] {
        for roster in &ROSTERS {
            for seed in 0..seeds {
                seasons.push(Sim::of_roster(roster, world, seed).run());
            }
        }
        for seed in 0..seeds {
            seasons.push(Sim::turtles(world, seed).run());
        }
    }
    println!("{}", table(&seasons));
    if let Err(e) = std::fs::write(&out, csv(&seasons)) {
        panic!("writing the simulation rows to {out}: {e}");
    }
    accept(&seasons);
}
