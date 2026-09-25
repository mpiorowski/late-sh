//! Full games, played out against the real engine.
//!
//! The rules live in pure functions, so a whole realm can be simulated in a
//! test: give each player a simple policy, run the days, and watch what comes
//! out. This is the only way to see the shapes that no unit test covers —
//! whether games actually end, whether one early leader runs away with it,
//! whether a quiet week is survivable — and to catch the outcomes that should
//! never happen at all (land owned by the dead, budgets overspent, a winner
//! who never played).
//!
//! Every run is seeded, so a failure here is reproducible.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::*;
use crate::app::lobby::realm::map::{TerritoryId, WorldMap, map_by_id};
use crate::app::lobby::realm::rulesets::{PACES, RealmRulesetSnapshot, STANDARD};
use crate::app::lobby::realm::svc::{paying_players, payout_plan};

const START_DAY: i32 = 20_000;
/// Long enough for a realm to resolve, short enough that the suite stays
/// quick. A game still running at this point is reported, not failed —
/// see `outcome`.
const MAX_DAYS: i32 = 400;

/// How a simulated player picks their moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    /// Takes free land while any is in reach, fights only when it must.
    Settler,
    /// Goes for enemy land as soon as the rules allow.
    Warlord,
    /// Plays every day, but only ever takes the safest thing available.
    Cautious,
    /// Plays in bursts: a few days on, a few days off. Exercises banking,
    /// dormancy and decay.
    Absentee,
    /// Digs in on everything it holds before it takes anything new. The
    /// strategy the rules are meant to make a bad one.
    Turtle,
}

impl Policy {
    /// Does this player act on `day`?
    fn plays_on(self, day: i32, offset: i32) -> bool {
        match self {
            Policy::Absentee => (day + offset).rem_euclid(5) < 2,
            _ => true,
        }
    }
}

struct Sim {
    state: RealmGameState,
    map: std::sync::Arc<WorldMap>,
    policies: Vec<(Uuid, Policy)>,
    rng: SplitMix64,
    day: i32,
    /// Every action attempted, for the invariant checks.
    actions_taken: u32,
    rejects: BTreeMap<&'static str, u32>,
    /// Timeline: how the game actually unfolded, for the summary line.
    first_elimination: Option<i32>,
    map_full_day: Option<i32>,
    /// The most players ever properly in the war, and the daily budget that
    /// came with it.
    max_involved: u8,
    tier_at_peak: u8,
}

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn new_state(
    players: &[(Uuid, Policy)],
    map: &WorldMap,
    rng: &mut SplitMix64,
    pace_id: &str,
) -> RealmGameState {
    let mut state = RealmGameState {
        version: STATE_VERSION,
        revision: 1,
        ruleset: RealmRulesetSnapshot::at_pace(
            &STANDARD,
            crate::app::lobby::realm::rulesets::pace_by_id(pace_id).expect("pace"),
        ),
        players: players
            .iter()
            .enumerate()
            .map(|(i, (id, _))| RealmPlayer {
                user_id: *id,
                username: format!("p{i}"),
                status: RealmPlayerStatus::Alive,
                joined_day: START_DAY,
                exit_day: None,
                exit_territories: 0,
                actions_day: START_DAY,
                actions_used: 0,
                actions_allowance: 0,
                last_action_day: None,
                active_days: 0,
                color: None,
                joined_claimed: 0,
            })
            .collect(),
        ownership: BTreeMap::new(),
        forts: BTreeMap::new(),
        last_day: None,
        start_player_count: players.len() as u8,
        start_day: START_DAY,
    };
    // Spawn everyone on distinct random territories, the way `start_game`
    // and `join_game` do.
    let mut free: Vec<TerritoryId> = (0..map.territories.len() as TerritoryId).collect();
    for (id, _) in players {
        let idx = rng.next_index(free.len());
        let territory = free.swap_remove(idx);
        state.ownership.insert(territory, *id);
    }
    state
}

impl Sim {
    fn new(map_id: &str, policies: Vec<Policy>, seed: u64) -> Self {
        Self::at_pace(map_id, policies, seed, "slow")
    }

    /// A game at a given tempo. `slow` is the rules as written, which is what
    /// the other playouts use so their numbers stay comparable.
    fn at_pace(map_id: &str, policies: Vec<Policy>, seed: u64, pace_id: &str) -> Self {
        let map = map_by_id(map_id).expect("map");
        let mut rng = SplitMix64::new(seed);
        let players: Vec<(Uuid, Policy)> = policies
            .into_iter()
            .enumerate()
            .map(|(i, policy)| (uid(i as u128 + 1), policy))
            .collect();
        let state = new_state(&players, &map, &mut rng, pace_id);
        Self {
            state,
            map,
            policies: players,
            rng,
            day: START_DAY,
            actions_taken: 0,
            rejects: BTreeMap::new(),
            first_elimination: None,
            map_full_day: None,
            max_involved: 0,
            tier_at_peak: 0,
        }
    }

    /// A turtle spends on walls first: the deepest hole it can still dig.
    fn wall_to_raise(&self, actor: Uuid) -> Option<RealmAction> {
        let state = &self.state;
        if !state.option(crate::app::lobby::realm::rulesets::OPTION_FORTIFY) {
            return None;
        }
        state
            .holdings(actor)
            .into_iter()
            .find(|t| state.fort_level(*t) < state.ruleset.fort_max_level)
            .map(|target| RealmAction::Fortify { target })
    }

    /// The best thing this player could do right now, by the odds the engine
    /// would actually roll. Returns None when nothing is legal.
    fn choose(&self, actor: Uuid, policy: Policy) -> Option<RealmAction> {
        if policy == Policy::Turtle
            && let Some(wall) = self.wall_to_raise(actor)
        {
            return Some(wall);
        }
        let state = &self.state;
        let index = ReachIndex::build(state, &self.map, actor);
        let mut best: Option<(f64, RealmAction)> = None;
        for territory in &self.map.territories {
            let owner = state.ownership.get(&territory.id).copied();
            if owner == Some(actor) {
                continue;
            }
            let reach = index.reach(&self.map, &state.ruleset, territory.id);
            // Nobody sensibly reaches across the world; keep the search to
            // what a player would actually consider.
            if reach.cost > 4.0 {
                continue;
            }
            if let Some(defender) = owner
                && state
                    .attack_allowed(actor, defender, reach, self.day)
                    .is_err()
            {
                continue;
            }
            let Some(odds) =
                projected_probability(state, &self.map, actor, territory.id, reach, self.day)
            else {
                continue;
            };
            // Policies differ in what they are willing to pay for.
            let score = match (policy, owner.is_some()) {
                (Policy::Settler, true) => odds * 0.4,
                (Policy::Warlord, true) => odds * 1.6,
                (Policy::Cautious, true) => odds * 0.2,
                (Policy::Turtle, true) => odds * 0.2,
                _ => odds,
            };
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                let action = match owner {
                    Some(_) => RealmAction::Attack {
                        target: territory.id,
                    },
                    None => RealmAction::Claim {
                        target: territory.id,
                    },
                };
                best = Some((score, action));
            }
        }
        best.map(|(_, action)| action)
    }

    /// One realm day for everyone, in a rotation that changes daily so no
    /// player permanently acts first.
    fn play_day(&mut self) {
        self.state.roll_day(self.day, day_seed(uid(99), self.day));
        let involved = self.state.involved_count(self.day);
        if involved > self.max_involved {
            self.max_involved = involved;
            self.tier_at_peak = self.state.daily_base(self.day);
        }
        let mut order: Vec<usize> = (0..self.policies.len()).collect();
        self.rng.shuffle(&mut order);

        // Interleave actions so the race for the same territory is realistic
        // rather than one player emptying their budget first.
        let mut budgets: Vec<u8> = order
            .iter()
            .map(|i| {
                let (id, policy) = self.policies[*i];
                if !self.state.is_alive(id) || !policy.plays_on(self.day, *i as i32) {
                    return 0;
                }
                self.state.standing(id, self.day).left()
            })
            .collect();

        while budgets.iter().any(|b| *b > 0) {
            for (slot, &player_index) in order.iter().enumerate() {
                if budgets[slot] == 0 {
                    continue;
                }
                budgets[slot] -= 1;
                let (id, policy) = self.policies[player_index];
                if !self.state.is_alive(id) {
                    budgets[slot] = 0;
                    continue;
                }
                let Some(action) = self.choose(id, policy) else {
                    budgets[slot] = 0;
                    continue;
                };
                let seed = self.rng.next_u64();
                match apply_action(&mut self.state, &self.map, id, action, self.day, seed) {
                    Ok(outcome) => {
                        self.actions_taken += 1;
                        if !matches!(outcome.end, ResolveEnd::Ongoing) {
                            return;
                        }
                    }
                    Err(rejected) => {
                        // A rejection the policy could not foresee is worth
                        // counting: too many means the AI (or the rules) are
                        // fighting themselves.
                        *self.rejects.entry(reject_name(rejected)).or_default() += 1;
                        budgets[slot] = 0;
                    }
                }
            }
        }
    }

    /// Play until somebody wins or `MAX_DAYS` passes.
    fn run(&mut self) -> Outcome {
        loop {
            self.play_day();
            self.check_invariants();
            let (_, end) = sweep_idle(&mut self.state, &self.map, self.day);
            self.check_invariants();
            if self.first_elimination.is_none()
                && self
                    .state
                    .players
                    .iter()
                    .any(|p| p.status == RealmPlayerStatus::Eliminated)
            {
                self.first_elimination = Some(self.day - START_DAY);
            }
            if self.map_full_day.is_none()
                && self.state.ownership.len() == self.map.territories.len()
            {
                self.map_full_day = Some(self.day - START_DAY);
            }
            match end {
                ResolveEnd::Won(winner) => {
                    return Outcome {
                        days: self.day - START_DAY,
                        winner: Some(winner),
                        dissolved: false,
                        state: self.state.clone(),
                        actions: self.actions_taken,
                        first_elimination: self.first_elimination,
                        map_full_day: self.map_full_day,
                        max_involved: self.max_involved,
                        tier_at_peak: self.tier_at_peak,
                        territories: self.map.territories.len(),
                    };
                }
                ResolveEnd::Dissolved => {
                    return Outcome {
                        days: self.day - START_DAY,
                        winner: None,
                        dissolved: true,
                        state: self.state.clone(),
                        actions: self.actions_taken,
                        first_elimination: self.first_elimination,
                        map_full_day: self.map_full_day,
                        max_involved: self.max_involved,
                        tier_at_peak: self.tier_at_peak,
                        territories: self.map.territories.len(),
                    };
                }
                ResolveEnd::Ongoing => {}
            }
            self.day += 1;
            if self.day - START_DAY > MAX_DAYS {
                return Outcome {
                    days: self.day - START_DAY,
                    winner: None,
                    dissolved: false,
                    state: self.state.clone(),
                    actions: self.actions_taken,
                    first_elimination: self.first_elimination,
                    map_full_day: self.map_full_day,
                    max_involved: self.max_involved,
                    tier_at_peak: self.tier_at_peak,
                    territories: self.map.territories.len(),
                };
            }
        }
    }

    /// The things that must never be true, checked after every day. These are
    /// the "crazy outcomes" this file exists to catch.
    fn check_invariants(&self) {
        let state = &self.state;
        let day = self.day;

        // Nobody holds land they should not, and the dead hold nothing.
        for (territory, owner) in &state.ownership {
            assert!(
                self.map.territory(*territory).is_some(),
                "day {day}: ownership of a territory off the map"
            );
            let player = state.player(*owner).expect("owner is a player");
            assert_eq!(
                player.status,
                RealmPlayerStatus::Alive,
                "day {day}: {} holds land while {:?}",
                player.username,
                player.status
            );
        }
        assert!(
            state.ownership.len() <= self.map.territories.len(),
            "day {day}: more owned territories than exist"
        );

        for player in &state.players {
            let standing = state.standing(player.user_id, day);
            // A budget is never overspent, and never exceeds base + the cap.
            assert!(
                standing.used <= standing.allowance,
                "day {day}: {} spent {} of {}",
                player.username,
                standing.used,
                standing.allowance
            );
            let ceiling = standing
                .base
                .saturating_mul(state.ruleset.bank_cap_days.saturating_add(1));
            assert!(
                standing.allowance <= ceiling,
                "day {day}: {} has an allowance of {} over a ceiling of {ceiling}",
                player.username,
                standing.allowance
            );
            // Decay stays inside its floor.
            let decay = state.decay_multiplier(player.user_id, day);
            assert!(
                decay >= state.ruleset.decay_floor && decay <= 1.0,
                "day {day}: {} decayed to {decay}",
                player.username
            );
            // You cannot have played more days than have passed.
            assert!(
                i32::from(player.active_days) <= day - START_DAY + 1,
                "day {day}: {} claims {} active days",
                player.username,
                player.active_days
            );
            // An eliminated player has an exit stamped and no land.
            if player.status != RealmPlayerStatus::Alive {
                assert!(
                    player.exit_day.is_some(),
                    "day {day}: {} is {:?} with no exit day",
                    player.username,
                    player.status
                );
                assert_eq!(
                    state.territory_count(player.user_id),
                    0,
                    "day {day}: {} is out but still holds land",
                    player.username
                );
            }
        }

        // Odds stay inside the ruleset's own bounds, wherever you look. This
        // is the expensive check (a BFS per player), so it runs on the days
        // where the rules are actually changing — the opening, when phases
        // expire — and periodically after that.
        let elapsed = day - START_DAY;
        if elapsed > i32::from(state.ruleset.distant_attack_grace_days) + 1 && elapsed % 5 != 0 {
            return;
        }
        let rs = &state.ruleset;
        for player in state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
        {
            let index = ReachIndex::build(state, &self.map, player.user_id);
            for territory in self.map.territories.iter().take(30) {
                let reach = index.reach(&self.map, &state.ruleset, territory.id);
                if let Some(p) = projected_probability(
                    state,
                    &self.map,
                    player.user_id,
                    territory.id,
                    reach,
                    day,
                ) {
                    assert!(
                        p >= rs.far_prob_floor && p <= rs.prob_ceil,
                        "day {day}: odds {p} outside [{}, {}]",
                        rs.far_prob_floor,
                        rs.prob_ceil
                    );
                }
            }
        }
    }
}

fn reject_name(rejected: ActionRejected) -> &'static str {
    match rejected {
        ActionRejected::NotPlaying => "not_playing",
        ActionRejected::NoPointsLeft => "no_points",
        ActionRejected::UnknownTerritory => "unknown",
        ActionRejected::TargetOwned => "target_owned",
        ActionRejected::TargetFree => "target_free",
        ActionRejected::AlreadyYours => "already_yours",
        ActionRejected::OutOfRange => "out_of_range",
        ActionRejected::NoRoute => "no_route",
        ActionRejected::NoFortifying => "no_fortifying",
        ActionRejected::NotYours => "not_yours",
        ActionRejected::FullyFortified(_) => "fully_fortified",
        ActionRejected::AttacksLocked(_) => "attacks_locked",
        ActionRejected::DistantAttacksLocked(_) => "distant_locked",
        ActionRejected::TargetIsNew(_) => "target_new",
        ActionRejected::TargetIsRamping(_) => "target_ramping",
    }
}

struct Outcome {
    days: i32,
    winner: Option<Uuid>,
    dissolved: bool,
    state: RealmGameState,
    actions: u32,
    first_elimination: Option<i32>,
    map_full_day: Option<i32>,
    max_involved: u8,
    tier_at_peak: u8,
    territories: usize,
}

impl Outcome {
    fn alive(&self) -> usize {
        self.state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .count()
    }

    /// One line for the test log, so a change in the feel of the game is
    /// visible when these run.
    fn summary(&self, label: &str) -> String {
        let leader = self
            .state
            .players
            .iter()
            .max_by_key(|p| self.state.territory_count(p.user_id))
            .map(|p| {
                format!(
                    "{} with {}",
                    p.username,
                    self.state.territory_count(p.user_id)
                )
            })
            .unwrap_or_default();
        format!(
            "{label}: won on day {} by {} · {} actions · map full day {} · first elimination day {} \
             · peak {} involved at {} points/day · final claim {}/{} · leader {leader}",
            self.days,
            match self.winner {
                Some(w) => self
                    .state
                    .player(w)
                    .map(|p| p.username.clone())
                    .unwrap_or_default(),
                None if self.dissolved => "dissolved".into(),
                None => "nobody yet".into(),
            },
            self.actions,
            self.map_full_day
                .map(|d| d.to_string())
                .unwrap_or_else(|| "never".into()),
            self.first_elimination
                .map(|d| d.to_string())
                .unwrap_or_else(|| "never".into()),
            self.max_involved,
            self.tier_at_peak,
            self.state.ownership.len(),
            self.territories,
        )
    }
}

/// A finished game must be finished for a reason the rules allow.
fn assert_sane_finish(outcome: &Outcome) {
    let state = &outcome.state;
    if let Some(winner) = outcome.winner {
        assert!(
            state.can_win(winner),
            "a winner who had not played the week: {:?}",
            state.player(winner).map(|p| p.active_days)
        );
        let holds_all = state.territory_count(winner) as usize == outcome.territories;
        assert!(
            outcome.alive() == 1 || holds_all,
            "a winner with rivals still standing and less than the whole map"
        );
        // The prize is sane and scales with who actually played.
        let pot: i64 = payout_plan(paying_players(state), state.ruleset.pot_scale)
            .iter()
            .map(|(_, chips)| chips)
            .sum();
        assert!(
            (1_000..=crate::app::lobby::realm::svc::REALM_POT_MAX).contains(&pot),
            "pot of {pot} is outside anything reasonable"
        );
    }
}

#[test]
fn a_two_player_realm_plays_out() {
    let mut sim = Sim::new("earth", vec![Policy::Settler, Policy::Warlord], 0xD1CE_0002);
    let outcome = sim.run();
    println!("{}", outcome.summary("2p settler vs warlord"));
    assert_sane_finish(&outcome);
    assert!(
        outcome.winner.is_some(),
        "a duel should resolve inside {MAX_DAYS} days, ran {} — {}",
        outcome.days,
        outcome.summary("2p")
    );
    // Winning is a week of play at minimum, by construction.
    assert!(outcome.days >= i32::from(STANDARD.min_active_days_to_win));
}

#[test]
fn a_five_player_realm_plays_out() {
    let mut sim = Sim::new(
        "earth",
        vec![
            Policy::Settler,
            Policy::Warlord,
            Policy::Cautious,
            Policy::Absentee,
            Policy::Warlord,
        ],
        0xD1CE_0005,
    );
    let outcome = sim.run();
    println!("{}", outcome.summary("5p mixed"));
    assert_sane_finish(&outcome);
    // Five players on a whole world take longer; what matters is that the
    // map fills and the field thins rather than stalling untouched.
    assert!(
        outcome.state.ownership.len() > 200,
        "the world should be carved up by now: {}",
        outcome.summary("5p")
    );
}

#[test]
fn an_eight_player_realm_plays_out() {
    let mut sim = Sim::new(
        "earth",
        vec![
            Policy::Settler,
            Policy::Warlord,
            Policy::Cautious,
            Policy::Absentee,
            Policy::Warlord,
            Policy::Settler,
            Policy::Cautious,
            Policy::Absentee,
        ],
        0xD1CE_0008,
    );
    let outcome = sim.run();
    println!("{}", outcome.summary("8p mixed"));
    assert_sane_finish(&outcome);
    assert!(
        outcome.state.ownership.len() > 200,
        "the world should be carved up by now: {}",
        outcome.summary("8p")
    );
    // With eight in the war, the daily budget sits on the tightest tier.
    assert_eq!(outcome.max_involved, 8);
    assert_eq!(outcome.tier_at_peak, 4);
}

#[test]
fn the_absentee_survives_a_quiet_spell_but_pays_for_it() {
    // One player plays every day, one plays two days in five. The point is
    // not who wins — it is that being away is survivable and visibly costly.
    let mut sim = Sim::new(
        "earth",
        vec![Policy::Settler, Policy::Absentee],
        0xD1CE_A0B0,
    );
    // A fortnight is enough for the pattern to show.
    for _ in 0..14 {
        sim.play_day();
        sim.check_invariants();
        sim.day += 1;
    }
    let state = &sim.state;
    let regular = state.territory_count(uid(1));
    let absentee = state.territory_count(uid(2));
    println!("regular held {regular}, absentee held {absentee} after a fortnight");
    assert!(absentee > 0, "an absentee should not simply evaporate");
    assert!(
        regular > absentee,
        "playing daily should beat playing two days in five"
    );
    // And the banking ladder did something: the absentee's allowance on a
    // return day is above the plain base.
    let standing = state.standing(uid(2), sim.day);
    assert!(
        standing.allowance >= standing.base,
        "a returning player should never have less than a day's points"
    );
}

#[test]
fn the_join_window_stays_open_for_at_least_a_week() {
    // The promise is that people do not have to start together — turning up
    // within a few days of each other is enough. So the doors have to stay
    // open longer than that, measured against players actually carving the
    // map up as fast as they can.
    for (label, policies) in [
        (
            "3p",
            vec![Policy::Settler, Policy::Warlord, Policy::Cautious],
        ),
        (
            "5p",
            vec![
                Policy::Settler,
                Policy::Settler,
                Policy::Warlord,
                Policy::Cautious,
                Policy::Settler,
            ],
        ),
    ] {
        let mut sim = Sim::new("earth", policies, 0xD1CE_0D00);
        let mut closed_on = None;
        for _ in 0..40 {
            sim.play_day();
            sim.check_invariants();
            sim.day += 1;
            if !sim.state.joinable(&sim.map) {
                closed_on = Some(sim.day - START_DAY);
                break;
            }
        }
        let closed_on = closed_on.expect("the doors close eventually");
        println!(
            "{label}: doors closed on day {closed_on} at {:.0}% claimed",
            sim.state.claimed_fraction(&sim.map) * 100.0
        );
        // The promise is "within a few days of each other", not a fixed
        // number — but it has to stay comfortably wider than that, and this
        // is where a rules change that quietly narrows it gets caught.
        assert!(
            closed_on >= 5,
            "{label}: only {closed_on} days to arrive — the point is that nobody has to sync"
        );
    }
}

/// Drop a player into a running game at `seed`'s world and play a week.
/// Returns what they held afterwards, and whether they survived at all.
/// `(held when the shield lifted, held a week later, alive, claimed at arrival)`
fn latecomer_week(seed: u64) -> (u16, u16, bool, f64) {
    let mut sim = Sim::new(
        "earth",
        vec![Policy::Settler, Policy::Warlord, Policy::Cautious],
        seed,
    );
    // Founders carve at the world until the doors are about to shut — the
    // worst moment to arrive, which is the one worth measuring.
    while sim.state.joinable(&sim.map) && sim.day - START_DAY < 40 {
        sim.play_day();
        sim.day += 1;
    }
    let claimed = sim.state.claimed_fraction(&sim.map);

    let latecomer = uid(99);
    sim.state.players.push(RealmPlayer {
        user_id: latecomer,
        username: "late".into(),
        status: RealmPlayerStatus::Alive,
        joined_day: sim.day,
        exit_day: None,
        exit_territories: 0,
        actions_day: sim.day,
        actions_used: 0,
        actions_allowance: 0,
        last_action_day: None,
        active_days: 0,
        color: None,
        // The world they are walking into, the way `join_game` records it —
        // this is what buys them their longer shield.
        joined_claimed: (claimed * 100.0).round().clamp(0.0, 100.0) as u8,
    });
    // The same rule the service spawns by: a land border, and free ground
    // next to it — so the latecomer is tested on arriving late rather than on
    // a placement nobody would have chosen for them.
    let has_room = |t: &crate::app::lobby::realm::map::Territory| {
        !t.land_links.is_empty()
            && t.land_links
                .iter()
                .any(|(n, _)| !sim.state.ownership.contains_key(n))
    };
    let free_ids: Vec<TerritoryId> = (0..sim.map.territories.len() as TerritoryId)
        .filter(|t| !sim.state.ownership.contains_key(t))
        .collect();
    let spawn = free_ids
        .iter()
        .copied()
        .find(|t| sim.map.territory(*t).is_some_and(has_room))
        .or_else(|| free_ids.first().copied())
        .expect("free land to spawn on");
    sim.state.ownership.insert(spawn, latecomer);
    sim.policies.push((latecomer, Policy::Settler));
    assert_eq!(sim.state.phase(latecomer, sim.day), PlayerPhase::New);

    // Their protection has to actually protect: nothing may touch them
    // while they are new.
    // The whole shield, not just the base grace: a late arrival's protection
    // is longer, and the point of this loop is that all of it holds.
    let protected_until = sim.day + sim.state.shield_days(latecomer);
    while sim.day < protected_until {
        sim.play_day();
        sim.check_invariants();
        sim.day += 1;
        assert!(
            sim.state.territory_count(latecomer) > 0,
            "a new arrival cannot be touched"
        );
    }
    let held_at_lift = sim.state.territory_count(latecomer);
    for _ in 0..7 {
        sim.play_day();
        sim.check_invariants();
        sim.day += 1;
    }
    (
        held_at_lift,
        sim.state.territory_count(latecomer),
        sim.state.is_alive(latecomer),
        claimed,
    )
}

const LATECOMER_SEEDS: [u64; 12] = [
    0xD1CE_1A7E,
    0xD1CE_1A7F,
    0xD1CE_1A80,
    0xD1CE_1A81,
    0xD1CE_1A82,
    0xD1CE_1A83,
    0xD1CE_1A84,
    0xD1CE_1A85,
    0xD1CE_1A86,
    0xD1CE_1A87,
    0xD1CE_1A88,
    0xD1CE_1A89,
];

/// The guarantee, as opposed to the balance: a new arrival cannot be touched
/// while they are new, and a week later at least one of them has taken a
/// second territory — so a late spawn is a position, not a wall.
///
/// This used to also assert that half of them survived the week. It stopped
/// being true when landmass-aware routing landed, and the reason is worth
/// writing down: under the old routing every player could "reach" anything
/// on the globe at the cost of a border move, so the bots spent their points
/// on phantom conquests continents away. With routes real, every point goes
/// into the neighbourhood a player actually stands in — including the
/// newcomer's. The survival rate is now measured rather than asserted (see
/// `measure_latecomer_survival`) because what it should be is a design
/// question about newcomer protection, not something to pin at whatever the
/// bug happened to produce.
#[test]
fn a_latecomer_is_protected_and_comes_out_with_a_position() {
    let mut at_lift = Vec::new();
    for seed in LATECOMER_SEEDS {
        let (held_at_lift, held, alive, claimed) = latecomer_week(seed);
        println!(
            "latecomer at {:.0}% claimed: held {held_at_lift} when the shield lifted, \
             {held} a week later, alive {alive}",
            claimed * 100.0
        );
        at_lift.push(held_at_lift);
    }
    // What the crowded-map shield is for: time to turn one square into
    // something worth defending. Arriving at the door and coming out of
    // protection still holding only the spawn would make the shield
    // decoration.
    let with_a_position = at_lift.iter().filter(|held| **held >= 5).count();
    assert!(
        with_a_position >= at_lift.len() * 3 / 4,
        "only {with_a_position} of {} latecomers built a position behind their shield: {at_lift:?}",
        at_lift.len()
    );
}

/// How often a player who joins at the last legal moment is still alive a
/// week later. Not an assertion — a measurement, for when newcomer
/// protection is next tuned.
#[test]
#[ignore]
fn measure_latecomer_survival() {
    let mut survivors = 0;
    for seed in LATECOMER_SEEDS {
        let (held_at_lift, held, alive, claimed) = latecomer_week(seed);
        survivors += usize::from(alive);
        println!(
            "at {:.0}% claimed: {held_at_lift} at the lift, {held} a week on, alive {alive}",
            claimed * 100.0
        );
    }
    println!(
        "MEASURE latecomers surviving their first week: {survivors}/{}",
        LATECOMER_SEEDS.len()
    );
}

#[test]
fn digging_in_is_not_a_way_to_win() {
    // The whole balance of fortification: worth a point on the pass you mean
    // to keep, never worth a strategy. If a player who only ever digs can
    // win, or can stall a realm into never ending, the numbers are wrong.
    let mut turtle_wins = 0;
    let mut lengths = Vec::new();
    for seed in [0xD1CE_7017, 0xD1CE_7018, 0xD1CE_7019] {
        let mut sim = Sim::new(
            "earth",
            vec![
                Policy::Turtle,
                Policy::Warlord,
                Policy::Settler,
                Policy::Turtle,
                Policy::Cautious,
            ],
            seed,
        );
        let outcome = sim.run();
        println!("{}", outcome.summary("5p with two turtles"));
        assert_sane_finish(&outcome);
        assert!(
            outcome.winner.is_some(),
            "a realm with turtles in it still has to end: {}",
            outcome.summary("turtles")
        );
        lengths.push(outcome.days);
        // Players 0 and 3 are the turtles.
        if let Some(winner) = outcome.winner
            && (winner == uid(1) || winner == uid(4))
        {
            turtle_wins += 1;
        }
    }
    assert!(
        turtle_wins == 0,
        "a pure turtle won {turtle_wins} of 3 — walls are supposed to be the weaker way to \
         spend a day"
    );
    println!("turtle games ran {lengths:?} days");
}

#[test]
fn every_pace_resolves_and_the_order_holds() {
    // The tempo roster is a promise about how long a realm takes. This is
    // where that promise is measured rather than asserted from a tagline:
    // the same five players, the same seed, one game per pace.
    let mut lengths: Vec<(&str, i32, usize)> = Vec::new();
    for pace in PACES {
        let mut sim = Sim::at_pace(
            "earth",
            vec![
                Policy::Settler,
                Policy::Warlord,
                Policy::Cautious,
                Policy::Settler,
                Policy::Warlord,
            ],
            0xD1CE_FACE,
            pace.id,
        );
        let outcome = sim.run();
        println!(
            "{:<7} {}",
            pace.id,
            outcome.summary(&format!("5p {}", pace.id))
        );
        assert_sane_finish(&outcome);
        assert!(
            outcome.winner.is_some(),
            "{} should resolve inside {MAX_DAYS} days",
            pace.id
        );
        lengths.push((pace.id, outcome.days, outcome.actions as usize));
    }

    // Slowest first in the roster, so the games should get shorter down it.
    assert!(
        lengths.windows(2).all(|w| w[0].1 >= w[1].1),
        "a faster tempo produced a longer game: {lengths:?}"
    );
    let epic = lengths.first().expect("epic").1;
    let blitz = lengths.last().expect("blitz").1;
    assert!(
        epic >= blitz * 3,
        "the span from blitz to epic should be worth having: {blitz} to {epic} days"
    );
    // And the reference tempo is meaningfully quicker than the rules' own.
    let slow = lengths
        .iter()
        .find(|(id, _, _)| *id == "slow")
        .expect("slow")
        .1;
    let normal = lengths
        .iter()
        .find(|(id, _, _)| *id == "normal")
        .expect("normal")
        .1;
    assert!(
        (normal as f64) < slow as f64 * 0.75,
        "normal ({normal}d) should be well clear of slow ({slow}d)"
    );
}

#[test]
fn games_are_deterministic_end_to_end() {
    let run = || {
        let mut sim = Sim::new(
            "earth",
            vec![Policy::Settler, Policy::Warlord, Policy::Cautious],
            0xD1CE_1234,
        );
        let outcome = sim.run();
        (
            outcome.days,
            outcome.winner,
            serde_json::to_string(&outcome.state).expect("state"),
        )
    };
    let (days_a, winner_a, state_a) = run();
    let (days_b, winner_b, state_b) = run();
    assert_eq!(days_a, days_b);
    assert_eq!(winner_a, winner_b);
    assert_eq!(state_a, state_b, "the same seed must play the same game");
}
