use std::collections::BTreeMap;

use uuid::Uuid;

use super::*;
use crate::app::lobby::realm::map::map_by_id;
use crate::app::lobby::realm::rulesets::{RouteMode, STANDARD};

const DAY: i32 = 20_000;

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn base_state(players: &[(u128, &str)]) -> RealmGameState {
    RealmGameState {
        version: STATE_VERSION,
        revision: 1,
        ruleset: (&STANDARD).into(),
        players: players
            .iter()
            .map(|(n, name)| RealmPlayer {
                user_id: uid(*n),
                username: name.to_string(),
                status: RealmPlayerStatus::Alive,
                // Long-settled by default; phase tests move this back.
                joined_day: DAY - 30,
                exit_day: None,
                exit_territories: 0,
                actions_day: DAY,
                actions_used: 0,
                last_action_day: Some(DAY),
                actions_allowance: 0,
                // Played enough to win, except where a test says otherwise.
                active_days: 20,
                color: None,
                joined_claimed: 0,
            })
            .collect(),
        ownership: BTreeMap::new(),
        forts: BTreeMap::new(),
        last_day: None,
        start_player_count: players.len() as u8,
        // Old enough that the opening grace has expired, except where a test
        // moves it back on purpose.
        start_day: DAY - 10,
    }
}

/// Force the roll: a seed whose first `next_f64` lands under/over the odds.
fn seed_for(succeed: bool, probability: f64) -> u64 {
    for seed in 0..10_000u64 {
        let rolled = SplitMix64::new(seed).next_f64();
        if (rolled < probability) == succeed {
            return seed;
        }
    }
    panic!("no seed produced the wanted roll");
}

fn claim(target: u16) -> RealmAction {
    RealmAction::Claim { target }
}

fn attack(target: u16) -> RealmAction {
    RealmAction::Attack { target }
}

#[test]
fn an_action_resolves_on_the_spot_and_spends_one_point() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let cap = state.ruleset.actions_per_day(2);

    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        claim(1),
        DAY,
        seed_for(true, odds),
    )
    .expect("legal claim");

    assert!(out.success);
    assert_eq!(out.probability, odds, "the shown odds are the rolled odds");
    assert_eq!(state.ownership.get(&1), Some(&uid(1)));
    assert_eq!(out.points_left, cap - 1);
    assert_eq!(state.points_left(uid(1), DAY), cap - 1);
    // The result is in the log immediately, not at some midnight.
    let entries = &state.last_day.as_ref().expect("day log").entries;
    assert!(matches!(
        entries.last(),
        Some(LogEntry::Claimed { success: true, .. })
    ));
}

#[test]
fn a_failed_roll_still_costs_the_point() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let cap = state.ruleset.actions_per_day(2);

    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        claim(1),
        DAY,
        seed_for(false, odds),
    )
    .expect("legal claim");

    assert!(!out.success);
    assert_eq!(state.ownership.get(&1), None);
    assert_eq!(state.points_left(uid(1), DAY), cap - 1);
}

#[test]
fn points_run_out_and_refill_with_the_new_day() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let cap = state.ruleset.actions_per_day(2);

    for i in 0..cap {
        // A fresh free territory each time, so every attempt is legal and
        // actually spends its point.
        let target = (1..map.territories.len() as u16)
            .find(|t| !state.ownership.contains_key(t))
            .expect("testmap has free land");
        apply_action(
            &mut state,
            &map,
            uid(1),
            claim(target),
            DAY,
            u64::from(i) + 7,
        )
        .expect("legal claim");
    }
    assert_eq!(state.points_left(uid(1), DAY), 0);
    assert_eq!(
        apply_action(&mut state, &map, uid(1), claim(8), DAY, 3).unwrap_err(),
        ActionRejected::NoPointsLeft
    );
    // Next realm day: the budget is back without anything having to run.
    assert_eq!(state.points_left(uid(1), DAY + 1), cap);
}

#[test]
fn illegal_actions_are_refused_and_cost_nothing() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let cap = state.ruleset.actions_per_day(2);

    // Claiming an owned territory, attacking a free one, hitting your own
    // land, and an id off the map: all refused, none spends a point.
    assert_eq!(
        apply_action(&mut state, &map, uid(1), claim(11), DAY, 1).unwrap_err(),
        ActionRejected::TargetOwned
    );
    assert_eq!(
        apply_action(&mut state, &map, uid(1), attack(5), DAY, 1).unwrap_err(),
        ActionRejected::TargetFree
    );
    assert_eq!(
        apply_action(&mut state, &map, uid(1), attack(0), DAY, 1).unwrap_err(),
        ActionRejected::AlreadyYours
    );
    assert_eq!(
        apply_action(&mut state, &map, uid(1), claim(9_999), DAY, 1).unwrap_err(),
        ActionRejected::UnknownTerritory
    );
    assert_eq!(
        apply_action(&mut state, &map, uid(3), claim(1), DAY, 1).unwrap_err(),
        ActionRejected::NotPlaying
    );
    assert_eq!(state.points_left(uid(1), DAY), cap);
    assert!(state.last_day.is_none(), "refusals never touch the log");
}

#[test]
fn first_action_wins_the_race_and_the_second_is_refused() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(2, uid(2));

    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    apply_action(
        &mut state,
        &map,
        uid(1),
        claim(1),
        DAY,
        seed_for(true, odds),
    )
    .expect("first claim lands");
    // The second player went for the same free territory a moment later.
    assert_eq!(
        apply_action(&mut state, &map, uid(2), claim(1), DAY, 5).unwrap_err(),
        ActionRejected::TargetOwned
    );
}

#[test]
fn a_route_costs_what_it_crosses() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    // Testmap is a 4x3 grid of neighbours: 0 is a corner, 1 and 2 sit along
    // its row.
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));

    let index = ReachIndex::build(&state, &map, uid(1));
    let cost = |t: u16| index.reach(&map, &state.ruleset, t).cost;
    // A neighbour is the plain move, and costs the plain amount.
    assert!((cost(1) - 1.0).abs() < 1e-6);
    // One empty territory in between costs its transit.
    assert!((cost(2) - (1.0 + state.ruleset.free_transit_cost)).abs() < 1e-6);

    // Put a rival in the gap and the same target gets far dearer — going
    // *through* someone is the expensive way round.
    let mut blocked = base_state(&[(1, "a"), (2, "b")]);
    blocked.ownership.insert(0, uid(1));
    blocked.ownership.insert(1, uid(2));
    let blocked_index = ReachIndex::build(&blocked, &map, uid(1));
    let through_rival = blocked_index.reach(&map, &blocked.ruleset, 2).cost;
    assert!(
        through_rival > cost(2) + 1.0,
        "crossing a rival ({through_rival}) should cost far more than empty land ({})",
        cost(2)
    );
    assert!(matches!(
        blocked_index.reach(&map, &blocked.ruleset, 2).mode,
        RouteMode::Land { through_rivals: 1 }
    ));

    // Your own land is free to move through: holding the gap makes the far
    // side a border move again.
    let mut owned = base_state(&[(1, "a"), (2, "b")]);
    owned.ownership.insert(0, uid(1));
    owned.ownership.insert(1, uid(1));
    let owned_index = ReachIndex::build(&owned, &map, uid(1));
    assert!((owned_index.reach(&map, &owned.ruleset, 2).cost - 1.0).abs() < 1e-6);
}

#[test]
fn the_odds_follow_the_route_and_never_hit_zero() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let index = ReachIndex::build(&state, &map, uid(1));

    let odds = |t: u16| {
        projected_probability(
            &state,
            &map,
            uid(1),
            t,
            index.reach(&map, &state.ruleset, t),
            DAY,
        )
        .unwrap()
    };
    // Further is worse, all the way out.
    assert!(odds(1) > odds(2));
    assert!(odds(2) > odds(3));
    assert!(odds(3) >= state.ruleset.far_prob_floor);
    // And the border move pays no distance penalty at all.
    let border = index.reach(&map, &state.ruleset, 1);
    assert!(border.is_adjacent());
}

#[test]
fn the_sea_is_its_own_route_and_a_strait_is_nearly_free() {
    let map = map_by_id("earth").unwrap();
    // An island holder: no land route anywhere, so every move is a crossing.
    let island = map
        .territories
        .iter()
        .find(|t| t.neighbors.is_empty() && t.coastal)
        .expect("earth has islands");
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(island.id, uid(1));

    let index = ReachIndex::build(&state, &map, uid(1));
    // The nearest coast to them is a short crossing and stays cheap.
    let nearest = map
        .territories
        .iter()
        .filter(|t| t.id != island.id && t.coastal)
        .min_by(|a, b| {
            map.distance_km(island.id, a.id)
                .partial_cmp(&map.distance_km(island.id, b.id))
                .unwrap()
        })
        .expect("somewhere to sail to");
    let near = index.reach(&map, &state.ruleset, nearest.id);
    assert!(near.by_sea(), "with no land route, the sea is the route");
    assert!(
        near.cost < 2.0,
        "a short crossing should cost less than marching a territory: {}",
        near.cost
    );

    // The far side of the world is priced like the ocean it is.
    let far = map
        .territories
        .iter()
        .filter(|t| t.coastal)
        .max_by(|a, b| {
            map.distance_km(island.id, a.id)
                .partial_cmp(&map.distance_km(island.id, b.id))
                .unwrap()
        })
        .expect("somewhere far");
    assert!(index.reach(&map, &state.ruleset, far.id).cost > near.cost * 2.0);

    // An inland country cannot be taken off a boat.
    let landlocked = map.territories.iter().find(|t| !t.coastal);
    if let Some(inland) = landlocked {
        let reach = index.reach(&map, &state.ruleset, inland.id);
        assert!(!reach.by_sea(), "{} has no coast to land on", inland.name);
    }
}

#[test]
fn the_cheaper_of_land_and_sea_is_the_one_that_counts() {
    let map = map_by_id("earth").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    // Hold one coastal country, and wall its land route off with a rival.
    let home = map
        .territories
        .iter()
        .find(|t| t.coastal && t.neighbors.len() == 1)
        .or_else(|| {
            map.territories
                .iter()
                .find(|t| t.coastal && !t.neighbors.is_empty())
        })
        .expect("a coastal country with neighbours");
    state.ownership.insert(home.id, uid(1));
    for neighbour in &home.neighbors {
        state.ownership.insert(*neighbour, uid(2));
    }

    let index = ReachIndex::build(&state, &map, uid(1));
    // Somewhere across water that the land route would have to cross a
    // rival to reach: the sea should win.
    let sea_targets = map
        .territories
        .iter()
        .filter(|t| t.coastal && t.id != home.id)
        .filter(|t| !state.ownership.contains_key(&t.id))
        .filter(|t| map.distance_km(home.id, t.id) < 2_000.0)
        .count();
    assert!(sea_targets > 0, "there is somewhere to sail");
    let any_sea = map
        .territories
        .iter()
        .any(|t| index.reach(&map, &state.ruleset, t.id).by_sea());
    assert!(any_sea, "the sea route should win somewhere");
}

#[test]
fn reaching_a_far_target_is_legal_never_impossible() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));

    // The far corner is a long march and owned — legal to strike, just bad
    // odds. It must not be refused the way the old queue fizzled it.
    let reach = reach_for(&state, &map, uid(1), 11);
    let odds = projected_probability(&state, &map, uid(1), 11, reach, DAY).unwrap();
    assert!(odds >= state.ruleset.far_prob_floor && odds < 0.2);
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        attack(11),
        DAY,
        seed_for(true, odds),
    )
    .expect("a long strike is a legal action");
    assert!(out.success);
    assert!(
        out.reach.cost > 3.0,
        "and it was a long way: {}",
        out.reach.cost
    );
}

#[test]
fn an_island_start_sails_instead_of_being_stranded() {
    let map = map_by_id("earth").unwrap();
    let island = map
        .territories
        .iter()
        .find(|t| t.neighbors.is_empty() && t.coastal)
        .expect("earth has island countries");
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(island.id, uid(1));
    let mainland = map
        .territories
        .iter()
        .find(|t| !t.neighbors.is_empty() && t.coastal)
        .expect("earth has mainland countries");
    state.ownership.insert(mainland.id, uid(2));

    // No land route exists, so the route is the sea — no special case, no
    // bonus, just what the crossing costs.
    let reach = reach_for(&state, &map, uid(1), mainland.id);
    assert!(reach.by_sea(), "the only way off an island is the water");
    let odds = projected_probability(&state, &map, uid(1), mainland.id, reach, DAY)
        .expect("a price for the crossing");
    assert!(
        odds >= state.ruleset.far_prob_floor,
        "a crossing is never impossible"
    );
    // And an island player is never without a move.
    let index = ReachIndex::build(&state, &map, uid(1));
    assert!(
        map.territories
            .iter()
            .any(|t| index.reach(&map, &state.ruleset, t.id).cost < 3.0),
        "somewhere is within reach of a boat"
    );
}

#[test]
fn a_newcomer_can_only_take_free_land_and_cannot_be_touched() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    for p in &mut state.players {
        p.joined_day = DAY;
    }
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));

    assert_eq!(state.phase(uid(1), DAY), PlayerPhase::New);
    // A newcomer may not swing...
    assert!(matches!(
        apply_action(&mut state, &map, uid(1), attack(1), DAY, 1).unwrap_err(),
        ActionRejected::AttacksLocked(_)
    ));
    // ...but free land is always open to them. That is the whole point of
    // arriving: build something before anyone can come for it.
    apply_action(&mut state, &map, uid(1), claim(4), DAY, 1).expect("claims are always open");
}

#[test]
fn ramping_up_means_neighbours_only_in_both_directions() {
    let map = map_by_id("testmap").unwrap();
    let grace = STANDARD.attack_grace_days as i32;
    let involved = STANDARD.distant_attack_grace_days as i32;

    let mut state = base_state(&[(1, "a"), (2, "b")]);
    for p in &mut state.players {
        p.joined_day = DAY;
    }
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    state.ownership.insert(11, uid(2));

    // Ramping up: a neighbour is fair game, the far corner is not.
    let day = DAY + grace;
    assert_eq!(state.phase(uid(1), day), PlayerPhase::Rampup);
    assert!(matches!(
        apply_action(&mut state, &map, uid(1), attack(11), day, 1).unwrap_err(),
        ActionRejected::DistantAttacksLocked(_)
    ));
    apply_action(&mut state, &map, uid(1), attack(1), day, 1).expect("neighbours are fair game");

    // The protection runs the other way too: a settled player cannot reach
    // across the map for someone still finding their feet.
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.player_mut(uid(1)).unwrap().joined_day = DAY - 30;
    state.player_mut(uid(2)).unwrap().joined_day = DAY;
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    let day = DAY + grace;
    assert_eq!(state.phase(uid(1), day), PlayerPhase::Involved);
    assert_eq!(state.phase(uid(2), day), PlayerPhase::Rampup);
    assert!(matches!(
        apply_action(&mut state, &map, uid(1), attack(11), day, 1).unwrap_err(),
        ActionRejected::TargetIsRamping(_)
    ));

    // Once they are involved, everything is open.
    let day = DAY + involved;
    assert_eq!(state.phase(uid(2), day), PlayerPhase::Involved);
    apply_action(&mut state, &map, uid(1), attack(11), day, 1).expect("the map is open now");
}

#[test]
fn phases_count_from_each_players_own_arrival() {
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    // A founder, and someone who turns up a fortnight later.
    state.player_mut(uid(1)).unwrap().joined_day = DAY;
    state.player_mut(uid(2)).unwrap().joined_day = DAY + 14;
    let late_day = DAY + 14;

    assert_eq!(state.phase(uid(1), late_day), PlayerPhase::Involved);
    assert_eq!(
        state.phase(uid(2), late_day),
        PlayerPhase::New,
        "arriving late still buys you your own first days"
    );
    assert_eq!(
        state.days_until(uid(2), late_day, PlayerPhase::Involved),
        state.ruleset.distant_attack_grace_days
    );
    // And a newcomer does not yet count towards the budget's tier.
    assert_eq!(state.involved_count(late_day), 1);
    assert_eq!(
        state.involved_count(late_day + state.ruleset.distant_attack_grace_days as i32),
        2
    );
}

#[test]
fn the_doors_close_once_the_map_is_half_taken() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a")]);
    assert!(state.joinable(&map), "an empty world takes all comers");

    let limit = (map.territories.len() as f64 * state.ruleset.join_max_claimed).ceil() as u16;
    for t in 0..limit {
        state.ownership.insert(t, uid(1));
    }
    assert!(state.claimed_fraction(&map) >= state.ruleset.join_max_claimed);
    assert!(
        !state.joinable(&map),
        "past halfway a newcomer is only food"
    );
}

#[test]
fn the_daily_budget_follows_the_players_who_are_actually_in() {
    let mut state = base_state(&[(1, "a"), (2, "b"), (3, "c"), (4, "d"), (5, "e")]);
    for p in &mut state.players {
        p.joined_day = DAY;
    }
    // A realm in its first days pays the most generous tier — the founders
    // are alone on a whole world.
    assert_eq!(state.involved_count(DAY), 0);
    assert_eq!(state.daily_base(DAY), 7);

    // Once everyone is properly in, five players means the middle tier.
    let later = DAY + state.ruleset.distant_attack_grace_days as i32;
    assert_eq!(state.involved_count(later), 5);
    assert_eq!(state.daily_base(later), 5);
}

#[test]
fn digging_in_costs_a_point_and_makes_a_place_dearer() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    let cap = state.daily_base(DAY);

    // Every territory starts simply held.
    assert_eq!(state.fort_level(1), 1);
    let bare = projected_probability(
        &state,
        &map,
        uid(1),
        1,
        reach_for(&state, &map, uid(1), 1),
        DAY,
    )
    .unwrap();

    // b digs in, which costs them a point like anything else.
    apply_action(
        &mut state,
        &map,
        uid(2),
        RealmAction::Fortify { target: 1 },
        DAY,
        1,
    )
    .expect("dig in");
    assert_eq!(state.fort_level(1), 2);
    assert_eq!(state.points_left(uid(2), DAY), cap - 1);
    let walled = projected_probability(
        &state,
        &map,
        uid(1),
        1,
        reach_for(&state, &map, uid(1), 1),
        DAY,
    )
    .unwrap();
    assert!(walled < bare, "walls cost an attacker something");

    // Deeper is dearer, up to the ruleset's limit.
    for _ in 0..10 {
        let _ = apply_action(
            &mut state,
            &map,
            uid(2),
            RealmAction::Fortify { target: 1 },
            DAY,
            1,
        );
    }
    assert_eq!(state.fort_level(1), state.ruleset.fort_max_level);
    assert_eq!(
        apply_action(
            &mut state,
            &map,
            uid(2),
            RealmAction::Fortify { target: 1 },
            DAY,
            1
        )
        .unwrap_err(),
        ActionRejected::FullyFortified(state.ruleset.fort_max_level)
    );

    // And you only dig on your own ground.
    assert_eq!(
        apply_action(
            &mut state,
            &map,
            uid(1),
            RealmAction::Fortify { target: 1 },
            DAY,
            1
        )
        .unwrap_err(),
        ActionRejected::NotYours
    );
}

#[test]
fn walls_are_a_losing_trade_in_points_and_are_levelled_when_the_place_falls() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));

    let odds_at = |state: &RealmGameState| {
        projected_probability(
            state,
            &map,
            uid(1),
            1,
            reach_for(state, &map, uid(1), 1),
            DAY,
        )
        .unwrap()
    };
    let bare = odds_at(&state);
    // Four points, the most anyone can spend on one place.
    for _ in 1..state.ruleset.fort_max_level {
        let level = state.fort_level(1);
        state.forts.insert(1, level + 1);
    }
    let walled = odds_at(&state);
    let spent = f64::from(state.ruleset.fort_max_level - 1);

    // The attacker needs this many more attempts to get through.
    let extra_attempts = (1.0 / walled) - (1.0 / bare);
    assert!(
        extra_attempts < spent,
        "four points of wall bought {extra_attempts:.1} points of attack — \
         digging in has to be the weaker way to spend a day"
    );
    assert!(extra_attempts > 0.3, "but it has to be worth something");

    // Taking the place levels what was built on it: offence is rewarded
    // with a weak position, so a frontier never sets.
    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    apply_action(
        &mut state,
        &map,
        uid(1),
        attack(1),
        DAY,
        seed_for(true, odds),
    )
    .expect("legal attack");
    assert_eq!(state.ownership.get(&1), Some(&uid(1)));
    assert_eq!(state.fort_level(1), 1, "the walls came down with it");
}

#[test]
fn walls_make_a_place_dearer_to_march_past_and_crumble_with_their_holder() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    // a is boxed into the corner: every way out of 0 crosses b's ground, so
    // there is no going around and the walls have to be paid for.
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    state.ownership.insert(4, uid(2));
    let cost_to = |state: &RealmGameState, day: i32| {
        ReachIndex::build_on(state, &map, uid(1), day)
            .reach(&map, &state.ruleset, 2)
            .cost
    };
    let plain = cost_to(&state, DAY);

    state.forts.insert(1, state.ruleset.fort_max_level);
    let walled = cost_to(&state, DAY);
    assert!(
        walled > plain,
        "a fortified pass with no way around it costs more: {plain} then {walled}"
    );

    // But an abandoned fortress is not a fortress: once its holder has been
    // quiet long enough, the walls and the toll both fade.
    let long_quiet = DAY + 60;
    assert!(
        cost_to(&state, long_quiet) < walled,
        "nobody is manning it any more"
    );
    assert!(
        state.fortification(1, long_quiet) > state.fortification(1, DAY),
        "and its defence has faded with it"
    );
}

#[test]
fn the_attack_key_can_never_spend_a_point_on_walls() {
    // The whole reason digging has its own key: reaching for the attack key
    // on your own ground must not quietly cost you a point.
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    let cap = state.daily_base(DAY);

    // Both of the shapes the attack key can produce are refused on your own
    // land, and neither leaves a mark.
    assert_eq!(
        apply_action(&mut state, &map, uid(1), claim(0), DAY, 1).unwrap_err(),
        ActionRejected::AlreadyYours
    );
    assert_eq!(
        apply_action(&mut state, &map, uid(1), attack(0), DAY, 1).unwrap_err(),
        ActionRejected::AlreadyYours
    );
    assert_eq!(state.fort_level(0), 1, "no wall was built by accident");
    assert_eq!(
        state.points_left(uid(1), DAY),
        cap,
        "and no point was spent"
    );

    // The spade is the only thing that digs.
    apply_action(
        &mut state,
        &map,
        uid(1),
        RealmAction::Fortify { target: 0 },
        DAY,
        1,
    )
    .expect("dig in");
    assert_eq!(state.fort_level(0), 2);
}

#[test]
fn a_realm_without_fortifications_refuses_the_spade() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state
        .ruleset
        .options
        .insert(super::super::rulesets::OPTION_FORTIFY.to_string(), false);
    state.ownership.insert(0, uid(1));
    assert_eq!(
        apply_action(
            &mut state,
            &map,
            uid(1),
            RealmAction::Fortify { target: 0 },
            DAY,
            1
        )
        .unwrap_err(),
        ActionRejected::NoFortifying
    );
}

/// Taking somebody's last territory puts them out — and with nobody left to
/// fight, that ends the realm. The ten territories still lying about
/// unclaimed do not keep it open: mopping up ground nobody is contesting is
/// bookkeeping, not conquest.
#[test]
fn taking_a_last_territory_eliminates_and_ends_the_realm() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));

    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        attack(1),
        DAY,
        seed_for(true, odds),
    )
    .expect("legal attack");

    assert!(out.success);
    assert_eq!(
        state.player(uid(2)).unwrap().status,
        RealmPlayerStatus::Eliminated
    );
    assert_eq!(out.end, ResolveEnd::Won(uid(1)));
    assert!(
        state.ownership.len() < map.territories.len(),
        "and it ended with most of the map still unclaimed"
    );
    let entries = &state.last_day.as_ref().unwrap().entries;
    assert!(
        entries
            .iter()
            .any(|e| matches!(e, LogEntry::Eliminated { .. }))
    );
    assert!(entries.iter().any(|e| matches!(e, LogEntry::Won { .. })));
}

#[test]
fn going_quiet_costs_you_your_grip_not_your_land() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b"), (3, "c")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    state.ownership.insert(2, uid(3));
    let grace = state.ruleset.dormancy_grace_days as i32;

    // c has not acted since the game began; a and b are keeping up — which
    // is what keeps this about one quiet player rather than an abandoned
    // realm, since a realm nobody touches dissolves instead.
    let later = DAY + grace + 4;
    state.player_mut(uid(1)).unwrap().last_action_day = Some(later);
    state.player_mut(uid(2)).unwrap().last_action_day = Some(later);
    state.player_mut(uid(3)).unwrap().last_action_day = Some(DAY);

    let (removed, end) = sweep_idle(&mut state, &map, later);
    // Nobody is removed from a running game any more.
    assert!(removed.is_empty());
    assert_eq!(end, ResolveEnd::Ongoing);
    assert_eq!(
        state.player(uid(3)).unwrap().status,
        RealmPlayerStatus::Alive
    );
    assert_eq!(
        state.ownership.get(&2),
        Some(&uid(3)),
        "a quiet player keeps their land — it has to be taken"
    );

    // But their grip has slipped: the same empire defends with less.
    let decay = state.decay_multiplier(uid(3), later);
    assert!(decay < 1.0 && decay >= state.ruleset.decay_floor);
    assert!(state.effective_strength(uid(3), &map, later) < state.strength(uid(3), &map));
    // Which shows up as better odds for whoever comes for them.
    let reach = reach_for(&state, &map, uid(2), 2);
    let now = projected_probability(&state, &map, uid(2), 2, reach, DAY).unwrap();
    let crumbling = projected_probability(&state, &map, uid(2), 2, reach, later).unwrap();
    assert!(crumbling > now, "an abandoned empire is easier to carve up");
    // And it stops slipping at the floor.
    let forever = DAY + 500;
    assert_eq!(
        state.decay_multiplier(uid(3), forever),
        state.ruleset.decay_floor
    );
}

#[test]
fn a_game_everyone_abandoned_dissolves() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    // Nobody has touched it for a long time.
    let (removed, end) = sweep_idle(&mut state, &map, DAY + 90);
    assert_eq!(removed.len(), 2);
    assert_eq!(end, ResolveEnd::Dissolved, "dead rows do not linger");
}

/// The dissolve clock is five days of complete silence, and one player still
/// playing holds the whole realm open — the point is dead rows, not slow
/// ones.
#[test]
fn the_abandon_clock_is_five_quiet_days_and_one_player_stops_it() {
    let map = map_by_id("testmap").unwrap();
    let quiet_for = |days: i32, second_player_acts: bool| {
        let mut state = base_state(&[(1, "a"), (2, "b")]);
        state.ownership.insert(0, uid(1));
        state.ownership.insert(1, uid(2));
        let now = DAY + days;
        state.player_mut(uid(1)).unwrap().last_action_day = Some(DAY);
        state.player_mut(uid(2)).unwrap().last_action_day =
            Some(if second_player_acts { now } else { DAY });
        sweep_idle(&mut state, &map, now).1
    };
    assert_eq!(
        quiet_for(REALM_ABANDON_DAYS - 1, false),
        ResolveEnd::Ongoing,
        "four quiet days is a weekend, not an abandonment"
    );
    assert_eq!(quiet_for(REALM_ABANDON_DAYS, false), ResolveEnd::Dissolved);
    assert_eq!(
        quiet_for(REALM_ABANDON_DAYS * 4, true),
        ResolveEnd::Ongoing,
        "one player still playing keeps the realm alive"
    );
}

/// A realm nobody ever played — created, maybe joined, never acted in — is
/// the commonest dead row of all, and the quiet clock has to catch it from
/// the day people joined rather than waiting for a first action that never
/// comes.
#[test]
fn a_realm_nobody_ever_played_clears_itself() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    for player in &mut state.players {
        player.last_action_day = None;
        player.joined_day = DAY;
    }
    assert_eq!(
        sweep_idle(&mut state, &map, DAY + REALM_ABANDON_DAYS).1,
        ResolveEnd::Dissolved
    );
}

#[test]
fn missed_days_bank_on_a_ladder_and_then_stop() {
    let map = map_by_id("testmap").unwrap();
    let _ = map;
    let state = base_state(&[(1, "a"), (2, "b")]);
    let base = state.ruleset.actions_per_day(2);
    let standing = |missed: i32| state.standing(uid(1), DAY + missed + 1);

    // Playing daily: just the base, nothing banked.
    assert_eq!(standing(0).banked, 0);
    assert_eq!(standing(0).allowance, base);
    // One missed day comes back nearly whole.
    let one = standing(1);
    assert!(one.banked > 0);
    assert_eq!(
        one.banked,
        (f64::from(base) * state.ruleset.bank_rate_one_missed).floor() as u8
    );
    // A few missed days are worth half each.
    let three = standing(3);
    assert!(three.banked > one.banked);
    // Past the cutoff nothing more banks — a fortnight away is not a blitz.
    let cutoff = i32::from(state.ruleset.bank_missed_cutoff);
    let long_gone = standing(cutoff + 5);
    assert_eq!(long_gone.banked, 0);
    assert_eq!(long_gone.allowance, base, "just the day's own points");
    // And the bank never exceeds its cap.
    let cap = base * state.ruleset.bank_cap_days;
    for missed in 0..10 {
        assert!(standing(missed).banked <= cap);
    }
}

#[test]
fn a_banked_day_can_actually_be_spent() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    // Away for one day, back the next.
    let back = DAY + 2;
    let allowance = state.standing(uid(1), back).allowance;
    let base = state.ruleset.actions_per_day(2);
    assert!(allowance > base, "the missed day banked something");

    for i in 0..allowance {
        let target = (1..map.territories.len() as u16)
            .find(|t| !state.ownership.contains_key(t))
            .expect("free land");
        apply_action(
            &mut state,
            &map,
            uid(1),
            claim(target),
            back,
            u64::from(i) + 3,
        )
        .expect("within the allowance");
    }
    assert_eq!(state.points_left(uid(1), back), 0);
    assert_eq!(
        apply_action(&mut state, &map, uid(1), claim(5), back, 1).unwrap_err(),
        ActionRejected::NoPointsLeft
    );
}

#[test]
fn winning_takes_a_week_of_play_not_an_afternoon() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(1, uid(2));
    // a has played two days; the bar is a week.
    state.player_mut(uid(1)).unwrap().active_days = 2;
    assert!(!state.can_win(uid(1)));
    assert_eq!(
        state.days_to_win(uid(1)),
        state.ruleset.min_active_days_to_win - 2
    );

    // Conquering the only other player does not end it yet: the field is
    // clear, but the week is not in.
    let reach = reach_for(&state, &map, uid(1), 1);
    let odds = projected_probability(&state, &map, uid(1), 1, reach, DAY).unwrap();
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        attack(1),
        DAY,
        seed_for(true, odds),
    )
    .expect("legal attack");
    assert!(out.success);
    assert_eq!(
        state.player(uid(2)).unwrap().status,
        RealmPlayerStatus::Eliminated
    );
    assert_eq!(out.end, ResolveEnd::Ongoing);

    // Taking the whole map does not buy past the bar either — this is what it
    // is for, since a twelve-territory map could be swept in an afternoon
    // with an obliging friend.
    for territory in 0..map.territories.len() as u16 {
        state.ownership.insert(territory, uid(1));
    }
    let (_, end) = sweep_idle(&mut state, &map, DAY + 1);
    assert_eq!(
        end,
        ResolveEnd::Ongoing,
        "the world is held, but the week is not in"
    );

    // Once the days are in, the same position is a win.
    state.player_mut(uid(1)).unwrap().active_days = state.ruleset.min_active_days_to_win;
    let (_, end) = sweep_idle(&mut state, &map, DAY + 1);
    assert_eq!(end, ResolveEnd::Won(uid(1)));
}

#[test]
fn acting_counts_the_day_once() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    state.player_mut(uid(1)).unwrap().active_days = 0;
    state.player_mut(uid(1)).unwrap().last_action_day = None;

    apply_action(&mut state, &map, uid(1), claim(1), DAY, 5).expect("first action of the day");
    assert_eq!(state.player(uid(1)).unwrap().active_days, 1);
    apply_action(&mut state, &map, uid(1), claim(2), DAY, 6).expect("second action, same day");
    assert_eq!(
        state.player(uid(1)).unwrap().active_days,
        1,
        "a day of play is one day however many actions it took"
    );
    apply_action(&mut state, &map, uid(1), claim(3), DAY + 1, 7).expect("next day");
    assert_eq!(state.player(uid(1)).unwrap().active_days, 2);
}

#[test]
fn total_conquest_wins_while_others_are_still_alive() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    for id in 0..map.territories.len() as u16 {
        state.ownership.insert(id, uid(1));
    }
    // b is alive but landless-by-fiat; the sweep sees the whole map is taken.
    state.ownership.insert(11, uid(2));
    let reach = reach_for(&state, &map, uid(1), 11);
    let odds = projected_probability(&state, &map, uid(1), 11, reach, DAY).unwrap();
    let out = apply_action(
        &mut state,
        &map,
        uid(1),
        attack(11),
        DAY,
        seed_for(true, odds),
    )
    .expect("legal attack");
    assert_eq!(out.end, ResolveEnd::Won(uid(1)));
}

#[test]
fn the_same_attempt_rolls_the_same_way() {
    let map = map_by_id("testmap").unwrap();
    let mk = || {
        let mut s = base_state(&[(1, "a"), (2, "b")]);
        s.ownership.insert(0, uid(1));
        s.ownership.insert(11, uid(2));
        s
    };
    let seed = action_seed(uid(9), DAY, 1, uid(1), 1);
    let mut a = mk();
    let mut b = mk();
    let ra = apply_action(&mut a, &map, uid(1), claim(1), DAY, seed).unwrap();
    let rb = apply_action(&mut b, &map, uid(1), claim(1), DAY, seed).unwrap();
    assert_eq!(ra.success, rb.success);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    // A retry after losing the row CAS is a genuinely new roll: the revision
    // moved, so the seed does too.
    assert_ne!(seed, action_seed(uid(9), DAY, 2, uid(1), 1));
}

#[test]
fn the_day_log_rolls_over_and_hands_back_the_old_day() {
    let map = map_by_id("testmap").unwrap();
    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(0, uid(1));
    state.ownership.insert(11, uid(2));
    state.roll_day(DAY, day_seed(uid(9), DAY));
    apply_action(&mut state, &map, uid(1), claim(1), DAY, 11).unwrap();
    assert_eq!(state.last_day.as_ref().unwrap().entries.len(), 1);

    let archived = state
        .roll_day(DAY + 1, day_seed(uid(9), DAY + 1))
        .expect("the finished day comes back for archiving");
    assert_eq!(archived.day, DAY);
    assert_eq!(archived.entries.len(), 1);
    assert_eq!(state.last_day.as_ref().unwrap().day, DAY + 1);
    assert!(state.last_day.as_ref().unwrap().entries.is_empty());
}

#[test]
fn splitmix_shuffle_is_deterministic() {
    let mut a = SplitMix64::new(42);
    let mut b = SplitMix64::new(42);
    let mut xs: Vec<u8> = (0..16).collect();
    let mut ys = xs.clone();
    a.shuffle(&mut xs);
    b.shuffle(&mut ys);
    assert_eq!(xs, ys);
}

#[test]
fn ranking_orders_winner_then_later_exits() {
    let mut state = base_state(&[(1, "a"), (2, "b"), (3, "c")]);
    state.ownership.insert(0, uid(1));
    {
        let p = state.player_mut(uid(2)).unwrap();
        p.status = RealmPlayerStatus::Eliminated;
        p.exit_day = Some(DAY + 3);
        p.exit_territories = 2;
    }
    {
        let p = state.player_mut(uid(3)).unwrap();
        p.status = RealmPlayerStatus::Eliminated;
        p.exit_day = Some(DAY + 1);
        p.exit_territories = 4;
    }
    let ranking = final_ranking(&state, Some(uid(1)), 99);
    assert_eq!(ranking, vec![uid(1), uid(2), uid(3)]);
}

/// A target with no route at all — another landmass, nothing to sail from or
/// nothing to land on — was priced as if it bordered you: cost 1.0, "on your
/// border", full odds, and the attack went through. Two bugs wearing one
/// coat, because the same fallback covered both a boxed-in player and an
/// unreachable target.
#[test]
fn a_target_on_another_landmass_is_out_of_reach_not_next_door() {
    let map = map_by_id("earth").unwrap();
    let find = |name: &str| {
        map.territories
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is on the map"))
    };
    // Bolivia is landlocked, and Mongolia is landlocked on another continent:
    // no land route, no shore to sail from, no shore to land on.
    let bolivia = find("Bolivia");
    let mongolia = find("Mongolia");
    assert!(!bolivia.coastal && !mongolia.coastal, "both are landlocked");

    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(bolivia.id, uid(1));
    state.ownership.insert(mongolia.id, uid(2));

    let reach = reach_for(&state, &map, uid(1), mongolia.id);
    assert!(
        !reach.is_adjacent(),
        "another continent is not your border: {}",
        reach.describe()
    );
    assert!(reach.is_unreachable(), "{}", reach.describe());
    assert_eq!(
        projected_probability(&state, &map, uid(1), mongolia.id, reach, DAY),
        None,
        "odds for a move that cannot be made are worse than no odds"
    );

    let outcome = apply_action(
        &mut state,
        &map,
        uid(1),
        RealmAction::Attack {
            target: mongolia.id,
        },
        DAY,
        7,
    );
    assert_eq!(outcome.err(), Some(ActionRejected::NoRoute));
    assert_eq!(
        state.points_left(uid(1), DAY),
        state.standing(uid(1), DAY).allowance,
        "a refused action costs nothing"
    );
}

/// With landlocked navies off, a player holding only inland ground has no
/// fleet — so a coastal target across the water is not theirs to take, even
/// though the target itself could be landed on.
#[test]
fn an_inland_player_has_no_fleet_when_the_option_is_off() {
    let map = map_by_id("earth").unwrap();
    let find = |name: &str| {
        map.territories
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is on the map"))
    };
    let bolivia = find("Bolivia");
    let madagascar = find("Madagascar");
    let peru = find("Peru");
    assert!(madagascar.coastal && peru.coastal);

    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(bolivia.id, uid(1));
    state.ownership.insert(madagascar.id, uid(2));
    assert!(
        !state.option(crate::app::lobby::realm::rulesets::OPTION_LANDLOCKED_SEA),
        "off by default — this test is about that default"
    );

    let landlocked = reach_for(&state, &map, uid(1), madagascar.id);
    assert!(
        landlocked.is_unreachable(),
        "no coast, no fleet: {}",
        landlocked.describe()
    );

    // Reach the sea and the same target becomes a crossing.
    state.ownership.insert(peru.id, uid(1));
    let with_a_coast = reach_for(&state, &map, uid(1), madagascar.id);
    assert!(
        with_a_coast.by_sea(),
        "a coastal holding is a port: {}",
        with_a_coast.describe()
    );
    assert!(
        !with_a_coast.is_adjacent(),
        "an ocean away is not a border move"
    );
}

/// The bug in one line: France is a land neighbour of Brazil, because French
/// Guiana is. That made a European country read "on your border" from South
/// America, and — worse — made marching *through* it a dry route from one
/// continent to the other, navy or no navy. A border is only a border on the
/// landmass it is actually on.
#[test]
fn an_overseas_part_is_not_a_bridge_between_continents() {
    let map = map_by_id("earth").unwrap();
    let find = |name: &str| {
        map.territories
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is on the map"))
    };
    let brazil = find("Brazil");
    let france = find("France");
    let spain = find("Spain");
    assert!(
        brazil.neighbors.contains(&france.id),
        "the atlas really does say these two share a border"
    );
    assert!(
        france.landmasses.len() > 1,
        "France has ground on more than one landmass"
    );

    let mut state = base_state(&[(1, "a"), (2, "b")]);
    state.ownership.insert(brazil.id, uid(1));
    state.ownership.insert(spain.id, uid(2));
    let index = ReachIndex::build(&state, &map, uid(1));

    // The border itself is real — it is in South America, and so are you.
    let to_france = index.reach(&map, &state.ruleset, france.id);
    assert!(
        to_france.is_adjacent(),
        "French Guiana borders Brazil: {}",
        to_france.describe()
    );

    // Europe does not. Brazil is landlocked for this purpose only in the
    // sense that matters: no walk gets there.
    let to_spain = index.reach(&map, &state.ruleset, spain.id);
    assert!(
        !matches!(to_spain.mode, RouteMode::Land { .. }),
        "Spain must not be walkable from Brazil: {}",
        to_spain.describe()
    );
    assert!(
        to_spain.by_sea(),
        "Brazil has a coast, so Spain is a crossing: {}",
        to_spain.describe()
    );
    assert!(to_spain.cost > 2.0, "an ocean is not one step");
}

/// Joining a world that is already carved up buys a longer shield. The
/// point is not to make a latecomer safe — it is to give them the days they
/// need to turn one square into a position, which somebody who was here at
/// the start never needed.
#[test]
fn the_newcomer_shield_grows_with_how_carved_up_the_world_is() {
    let mut state = base_state(&[(1, "founder"), (2, "early"), (3, "late")]);
    let base = i32::from(state.ruleset.attack_grace_days);
    let most = base + i32::from(state.ruleset.newcomer_shield_max_days);
    assert!(most > base, "the standard ruleset has a shield to grow");

    // Arrived to an empty world: the plain opening truce, as before.
    assert_eq!(state.shield_days(uid(1)), base);

    // Arrived with the world half gone — the join door — gets all of it.
    let door = (state.ruleset.join_max_claimed * 100.0).round() as u8;
    state.player_mut(uid(3)).unwrap().joined_claimed = door;
    assert_eq!(state.shield_days(uid(3)), most);

    // And in between, in between.
    state.player_mut(uid(2)).unwrap().joined_claimed = door / 2;
    let middling = state.shield_days(uid(2));
    assert!(
        middling > base && middling < most,
        "half-way in should be half-way protected, got {middling}"
    );

    // The shield moves the phases along with it; the rampup window keeps its
    // own length rather than being eaten by the longer protection.
    let window = i32::from(state.ruleset.distant_attack_grace_days) - base;
    let joined = state.player(uid(3)).unwrap().joined_day;
    assert_eq!(state.phase(uid(3), joined + most - 1), PlayerPhase::New);
    assert_eq!(state.phase(uid(3), joined + most), PlayerPhase::Rampup);
    assert_eq!(
        state.phase(uid(3), joined + most + window),
        PlayerPhase::Involved
    );

    // A game frozen before any of this existed reads as it always did.
    state.ruleset.newcomer_shield_max_days = 0;
    assert_eq!(state.shield_days(uid(3)), base);
}
