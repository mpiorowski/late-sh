use std::collections::BTreeMap;
use std::num::NonZeroU8;

use uuid::Uuid;

use super::*;
use crate::app::door::realm::map::map_by_id;
use crate::app::door::realm::resolver::{DayResult, RealmPlayer, STATE_VERSION};
use crate::app::door::realm::rulesets::CAMPAIGN;

const DAY: i32 = 20_000;

// The testmap is a 4x3 grid of land, ids row-major:
//
//    0  1  2  3
//    4  5  6  7
//    8  9 10 11
//
// with only 5 and 6 inland. Centres are thousands of kilometres apart, so a
// test that wants a landing stretches `sea_reach_km`.

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn nz(n: u8) -> NonZeroU8 {
    NonZeroU8::new(n).expect("non-zero")
}

fn take(target: TerritoryId, points: u8) -> Order {
    Order::Take {
        target,
        points: nz(points),
    }
}

fn fortify(target: TerritoryId, levels: u8) -> Order {
    Order::Fortify {
        target,
        levels: nz(levels),
    }
}

/// A campaign on its day `DAY`: everyone long settled, played enough to
/// win, and back from yesterday with no bank. The budget (seven points a
/// day for two players), the win bar and the opening days are pinned
/// here, so the mechanics
/// read the same whatever the tuned numbers in `rulesets.rs` are; the
/// sanity-point test is the one that reads those.
fn campaign(players: &[(u128, &str)]) -> RealmGameState {
    let mut ruleset: RealmRulesetSnapshot = (&CAMPAIGN).into();
    ruleset.actions_per_day_tiers = vec![(4, 7), (7, 5), (10, 4), (20, 3)];
    ruleset.min_active_days_to_win = 7;
    ruleset.attack_grace_days = 1;
    ruleset.distant_attack_grace_days = 3;
    ruleset.newcomer_shield_max_days = 4;
    RealmGameState {
        version: STATE_VERSION,
        revision: 1,
        ruleset,
        players: players
            .iter()
            .map(|(n, name)| RealmPlayer {
                user_id: uid(*n),
                username: name.to_string(),
                status: RealmPlayerStatus::Alive,
                joined_day: DAY - 30,
                exit_day: None,
                exit_territories: 0,
                actions_day: DAY - 1,
                actions_used: 0,
                actions_allowance: 0,
                last_action_day: Some(DAY - 1),
                active_days: 20,
                joined_claimed: 0,
                color: None,
                orders: Vec::new(),
                dominating_nights: 0,
            })
            .collect(),
        ownership: BTreeMap::new(),
        forts: BTreeMap::new(),
        last_day: Some(DayResult {
            day: DAY,
            seed: 1,
            entries: Vec::new(),
        }),
        start_player_count: players.len() as u8,
        start_day: DAY - 10,
        news: None,
        last_night: None,
    }
}

fn hold(state: &mut RealmGameState, user: u128, territories: &[TerritoryId]) {
    for t in territories {
        state.ownership.insert(*t, uid(user));
    }
}

fn order(state: &mut RealmGameState, map: &WorldMap, user: u128, order: Order) {
    place_order(state, map, uid(user), order, DAY).expect("the order is placed");
}

/// The first night seed whose resolution passes `wanted`, with what it did.
/// Lets a test ask for an outcome ("both rolls succeed") instead of
/// hard-coding a seed that a change to the stream would silently break.
fn night_where(
    state: &RealmGameState,
    map: &WorldMap,
    wanted: impl Fn(&RealmGameState) -> bool,
) -> (RealmGameState, NightReport) {
    for seed in 0..10_000u64 {
        let mut tried = state.clone();
        tried.last_day.as_mut().expect("an open day").seed = seed;
        let report = resolve_night(&mut tried, map);
        if wanted(&tried) {
            return (tried, report);
        }
    }
    panic!("no seed resolved the night the way the test wants");
}

/// The night's one fight on `target`.
fn fight_on(state: &RealmGameState, target: TerritoryId) -> (Option<Uuid>, Vec<TakerRoll>, bool) {
    state
        .last_night
        .as_ref()
        .expect("a night was resolved")
        .entries
        .iter()
        .find_map(|e| match e {
            LogEntry::Fought {
                target: t,
                taken_by,
                takers,
                bounced,
                ..
            } if *t == target => Some((*taken_by, takers.clone(), *bounced)),
            _ => None,
        })
        .expect("a fight on the target")
}

fn every_roll_succeeded(state: &RealmGameState, target: TerritoryId) -> bool {
    fight_on(state, target).1.iter().all(|r| r.success)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{actual:.4} is not about {expected}"
    );
}

#[test]
fn the_starting_numbers_hit_the_sanity_points() {
    let rs: RealmRulesetSnapshot = (&CAMPAIGN).into();
    let medium = 0.5;
    let bare = garrison(&rs, 1.0, medium);
    let walls_3 = 1.0 + 2.0 * rs.garrison_per_wall;
    let walled = garrison(&rs, walls_3, medium);
    // One point from a walls-1 front of two onto medium free land.
    close(
        odds(
            &rs,
            force(&rs, 1, 2.0, false, false),
            bare * rs.neutral_factor,
        ),
        0.74,
    );
    // Three points from the same front onto bare medium land with two bare
    // neighbours behind it.
    close(
        odds(
            &rs,
            force(&rs, 3, 2.0, false, false),
            bare + rs.support_factor * (bare + bare),
        ),
        0.69,
    );
    // Five points onto walls 3 with two walls-3 neighbours: a fair fight
    // from a bare front, a better bet from a walled one.
    let fortress = walled + rs.support_factor * (walled + walled);
    close(odds(&rs, force(&rs, 5, 2.0, false, false), fortress), 0.61);
    close(
        odds(&rs, force(&rs, 5, 2.0 * walls_3, false, false), fortress),
        0.74,
    );
}

#[test]
fn the_odds_are_clamped_and_climb_with_every_point() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5, 6]);
    state.forts.insert(5, 5);
    let at = |points: u8| {
        projection(&state, &map, uid(1), 5, points, DAY)
            .expect("a take on a border")
            .probability
    };
    let climb: Vec<f64> = (1..=20).map(at).collect();
    assert!(climb.windows(2).all(|w| w[0] <= w[1]), "{climb:?}");
    assert_eq!(at(1), state.ruleset.prob_floor);
    // Twenty points onto walls 5 with a neighbour behind is a good bet, not
    // a certainty; force enough meets the ceiling and never passes it.
    assert!(at(20) < state.ruleset.prob_ceil, "{}", at(20));
    assert_eq!(at(u8::MAX), state.ruleset.prob_ceil);
}

#[test]
fn a_wall_raised_tonight_holds_tonight() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5]);
    let before = projection(&state, &map, uid(1), 5, 3, DAY).unwrap();
    // The holder sees their own wall in the odds against them at once.
    order(&mut state, &map, 2, fortify(5, 2));
    order(&mut state, &map, 1, take(5, 3));
    let (night, _) = night_where(&state, &map, |s| !every_roll_succeeded(s, 5));
    let (_, rolls, _) = fight_on(&night, 5);
    // The taker's board could not see it, and the night does.
    assert_eq!(rolls[0].shown, before.probability);
    assert!(rolls[0].probability < rolls[0].shown, "{rolls:?}");
    assert_eq!(night.fort_level(5), 3);
}

#[test]
fn a_neighbour_under_attack_lends_no_support() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4, 2]);
    hold(&mut state, 2, &[5, 6]);
    let supported = projection(&state, &map, uid(1), 5, 3, DAY).unwrap();
    // A's own take on 6 is on A's board straight away.
    order(&mut state, &map, 1, take(6, 1));
    let cut = projection(&state, &map, uid(1), 5, 3, DAY).unwrap();
    assert!(cut.defence < supported.defence);
    assert!(cut.probability > supported.probability);

    // Somebody else's take on 6 only shows at the night.
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5, 6]);
    hold(&mut state, 3, &[7]);
    order(&mut state, &map, 1, take(5, 3));
    order(&mut state, &map, 3, take(6, 1));
    resolve_night(&mut state, &map);
    let (_, rolls, _) = fight_on(&state, 5);
    assert!(rolls[0].probability > rolls[0].shown, "{rolls:?}");
}

#[test]
fn a_landing_is_weaker_than_a_march_and_encirclement_is_stronger() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    state.ruleset.sea_reach_km = 40_000.0;
    // 0 does not border 2: a landing.
    hold(&mut state, 1, &[0]);
    let landing = projection(&state, &map, uid(1), 2, 3, DAY).unwrap();
    assert!(landing.landing);
    assert_eq!(landing.from, 0);
    // 1 does: a march.
    let mut marching = state.clone();
    marching.ownership.clear();
    hold(&mut marching, 1, &[1]);
    let march = projection(&marching, &map, uid(1), 2, 3, DAY).unwrap();
    assert!(!march.landing);
    assert!(landing.force < march.force);
    assert!(landing.probability < march.probability);

    // 1 and 4 are all of 0's neighbours; 0 and 2 are two of 1's three.
    let mut ring = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut ring, 1, &[1, 4]);
    let encircled = projection(&ring, &map, uid(1), 0, 2, DAY).unwrap();
    let mut pair = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut pair, 1, &[0, 2]);
    let flanked = projection(&pair, &map, uid(1), 1, 2, DAY).unwrap();
    assert!(encircled.encircled && !flanked.encircled);
    assert_eq!(encircled.front, flanked.front);
    close(
        encircled.force,
        flanked.force * ring.ruleset.encircle_factor,
    );
}

#[test]
fn bigger_land_is_harder_to_take() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    // 5 borders both Eldwyn (4) and tiny Gyre (6).
    hold(&mut state, 1, &[5]);
    assert!(map.territory(4).unwrap().weight > map.territory(6).unwrap().weight);
    let big = projection(&state, &map, uid(1), 4, 1, DAY).unwrap();
    let small = projection(&state, &map, uid(1), 6, 1, DAY).unwrap();
    assert!(big.probability < small.probability);
}

#[test]
fn walls_crumble_back_towards_one_as_their_holder_goes_quiet() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5]);
    state.forts.insert(5, 5);
    let manned = projection(&state, &map, uid(1), 5, 3, DAY).unwrap();
    state.players[1].last_action_day = Some(DAY - 40);
    let abandoned = projection(&state, &map, uid(1), 5, 3, DAY).unwrap();
    let rs = &state.ruleset;
    let weight = map.territory(5).unwrap().weight;
    assert_eq!(
        manned.defence,
        garrison(rs, 1.0 + 4.0 * rs.garrison_per_wall, weight)
    );
    // At the decay floor, four levels of wall keep the floor's share.
    assert_eq!(
        abandoned.defence,
        garrison(
            rs,
            1.0 + 4.0 * rs.garrison_per_wall * rs.decay_floor,
            weight
        )
    );
}

#[test]
fn every_refusal_is_free_and_says_why() {
    let map = map_by_id("testmap").unwrap();
    let mut base = campaign(&[(1, "a"), (2, "b")]);
    base.ruleset.sea_reach_km = 40_000.0;
    hold(&mut base, 1, &[0, 1]);
    hold(&mut base, 2, &[3, 7]);
    let refused = |state: &RealmGameState, user: u128, order: Order| {
        let mut tried = state.clone();
        let before = serde_json::to_value(&tried).unwrap();
        let rejected = place_order(&mut tried, &map, uid(user), order, DAY).unwrap_err();
        assert_eq!(
            serde_json::to_value(&tried).unwrap(),
            before,
            "{rejected:?}"
        );
        assert!(!rejected.message().is_empty());
        rejected
    };
    let joined = |state: &mut RealmGameState, user: usize, days_ago: i32| {
        state.players[user].joined_day = DAY - days_ago;
    };

    assert_eq!(refused(&base, 9, take(2, 1)), OrderRejected::NotPlaying);
    assert_eq!(
        refused(&base, 1, take(99, 1)),
        OrderRejected::UnknownTerritory
    );
    assert_eq!(refused(&base, 1, take(0, 1)), OrderRejected::AlreadyYours);
    assert_eq!(refused(&base, 1, fortify(3, 1)), OrderRejected::NotYours);
    let mut walls_off = base.clone();
    walls_off
        .ruleset
        .options
        .insert(OPTION_FORTIFY.to_string(), false);
    assert_eq!(
        refused(&walls_off, 1, fortify(0, 1)),
        OrderRejected::NoFortifying
    );
    let mut walled = base.clone();
    walled.forts.insert(0, 4);
    assert_eq!(
        refused(&walled, 1, fortify(0, 2)),
        OrderRejected::FullyFortified { level: 4, max: 5 }
    );
    // 6 is inland and borders nothing of theirs.
    assert_eq!(refused(&base, 1, take(6, 1)), OrderRejected::NoFront);
    assert_eq!(
        refused(&base, 1, take(2, 8)),
        OrderRejected::NotEnoughPoints { left: 7 }
    );

    let mut fresh = base.clone();
    joined(&mut fresh, 0, 0);
    assert_eq!(
        refused(&fresh, 1, take(3, 1)),
        OrderRejected::AttacksLocked(1)
    );
    let mut ramping = base.clone();
    joined(&mut ramping, 0, 1);
    // 3 is a sea crossing from 0 and 1.
    assert_eq!(
        refused(&ramping, 1, take(3, 1)),
        OrderRejected::LandingsLocked(2)
    );
    let mut newcomer = base.clone();
    joined(&mut newcomer, 1, 0);
    assert_eq!(
        refused(&newcomer, 1, take(3, 1)),
        OrderRejected::TargetIsNew(1)
    );
    let mut green = base.clone();
    joined(&mut green, 1, 1);
    assert_eq!(
        refused(&green, 1, take(3, 1)),
        OrderRejected::TargetIsRamping(2)
    );
    // While a land neighbour may still come for them.
    let mut neighbour = green.clone();
    hold(&mut neighbour, 1, &[2]);
    place_order(&mut neighbour, &map, uid(1), take(3, 1), DAY).expect("a border take");

    let mut nothing = base.clone();
    assert_eq!(
        withdraw_order(&mut nothing, uid(1), 2, DAY),
        Err(OrderRejected::NoSuchOrder)
    );
}

#[test]
fn a_replaced_or_withdrawn_order_hands_its_points_back() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[0]);
    let placed = place_order(&mut state, &map, uid(1), take(1, 3), DAY).unwrap();
    assert_eq!(placed.points_left, 4);
    assert!(placed.projection.is_some());
    // Ordering the same target again replaces the order and its points.
    let placed = place_order(&mut state, &map, uid(1), take(1, 5), DAY).unwrap();
    assert_eq!(placed.points_left, 2);
    // Which can go past what is left, up to what the old order held.
    assert_eq!(state.player(uid(1)).unwrap().orders, vec![take(1, 5)]);
    place_order(&mut state, &map, uid(1), take(4, 2), DAY).unwrap();
    assert_eq!(state.points_left(uid(1), DAY), 0);
    assert_eq!(withdraw_order(&mut state, uid(1), 1, DAY), Ok(5));
    assert_eq!(state.player(uid(1)).unwrap().orders, vec![take(4, 2)]);
}

#[test]
fn the_first_order_freezes_the_allowance_and_keeps_the_bank() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[0]);
    // Two days missed: three quarters and a half of a day banked.
    state.players[0].last_action_day = Some(DAY - 3);
    let opening = state.standing(uid(1), DAY);
    assert_eq!(opening.allowance, 7 + 8);
    order(&mut state, &map, 1, take(1, 1));
    let player = state.player(uid(1)).unwrap();
    assert_eq!(player.actions_allowance, 15);
    assert_eq!(player.last_action_day, Some(DAY));
    assert_eq!(player.active_days, 21);
    // The gap has closed, and the bank it paid for is still there.
    assert_eq!(state.points_left(uid(1), DAY), 14);
    withdraw_order(&mut state, uid(1), 1, DAY).unwrap();
    assert_eq!(state.points_left(uid(1), DAY), 15);
    assert_eq!(state.player(uid(1)).unwrap().active_days, 21);
}

#[test]
fn the_stronger_success_takes_a_contested_territory() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[6]);
    hold(&mut state, 3, &[5]);
    order(&mut state, &map, 1, take(5, 1));
    order(&mut state, &map, 2, take(5, 6));
    let (night, report) = night_where(&state, &map, |s| every_roll_succeeded(s, 5));
    let (taken_by, rolls, bounced) = fight_on(&night, 5);
    assert_eq!(taken_by, Some(uid(2)));
    assert!(!bounced);
    assert!(rolls[1].force > rolls[0].force);
    assert_eq!(night.ownership.get(&5), Some(&uid(2)));
    assert_eq!(report.taken, 1);
}

#[test]
fn an_exact_tie_bounces_and_the_holder_keeps_it() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[6]);
    hold(&mut state, 3, &[5]);
    state.forts.insert(5, 2);
    order(&mut state, &map, 1, take(5, 4));
    order(&mut state, &map, 2, take(5, 4));
    let (night, report) = night_where(&state, &map, |s| every_roll_succeeded(s, 5));
    let (taken_by, rolls, bounced) = fight_on(&night, 5);
    assert_eq!(rolls[0].force, rolls[1].force);
    assert!(bounced);
    assert_eq!(taken_by, None);
    assert_eq!(night.ownership.get(&5), Some(&uid(3)));
    assert_eq!(night.fort_level(5), 2, "a bounce leaves the walls standing");
    assert_eq!(report.bounced, 1);
}

#[test]
fn free_land_two_players_tie_on_stays_free() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[6]);
    order(&mut state, &map, 1, take(5, 1));
    order(&mut state, &map, 2, take(5, 1));
    let (night, _) = night_where(&state, &map, |s| every_roll_succeeded(s, 5));
    assert!(fight_on(&night, 5).2);
    assert_eq!(night.ownership.get(&5), None);
}

#[test]
fn a_take_on_land_its_holder_let_go_of_is_a_claim() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5]);
    hold(&mut state, 3, &[11]);
    order(&mut state, &map, 1, take(5, 2));
    // The holder withdraws during the day.
    state.exit_player(uid(2), RealmPlayerStatus::Left, DAY);
    let (night, _) = night_where(&state, &map, |s| every_roll_succeeded(s, 5));
    let holder = night
        .last_night
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .find_map(|e| match e {
            LogEntry::Fought { holder, .. } => Some(*holder),
            _ => None,
        });
    assert_eq!(holder, Some(None));
    assert_eq!(night.ownership.get(&5), Some(&uid(1)));
}

#[test]
fn a_take_fizzles_when_the_board_turns_against_it() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    // 5 borders 1 and 4; 8 borders only 4 of A's.
    hold(&mut state, 1, &[1, 4]);
    hold(&mut state, 2, &[11]);
    order(&mut state, &map, 1, take(5, 1));
    order(&mut state, &map, 1, take(8, 1));
    // A newcomer is put down on the free land A meant to take...
    state.players[2].joined_day = DAY;
    hold(&mut state, 3, &[5]);
    // ...and the ground under A's other take is gone by nightfall.
    state.ownership.remove(&4);
    let report = resolve_night(&mut state, &map);
    let fizzles: Vec<(TerritoryId, FizzleReason)> = state
        .last_night
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .filter_map(|e| match e {
            LogEntry::Fizzled { action, reason, .. } => Some((action.target(), *reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        fizzles,
        vec![
            (5, FizzleReason::TargetProtected),
            (8, FizzleReason::NoFront)
        ]
    );
    assert_eq!(report.fizzled, 2);
    assert_eq!(state.ownership.get(&5), Some(&uid(3)));
    // The points were spent when the orders were written.
    assert_eq!(state.points_left(uid(1), DAY), 5);
}

#[test]
fn a_fall_names_whoever_took_the_last_ground() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5]);
    hold(&mut state, 3, &[11]);
    order(&mut state, &map, 1, take(5, 5));
    let (night, _) = night_where(&state, &map, |s| every_roll_succeeded(s, 5));
    assert_eq!(
        night.player(uid(2)).unwrap().status,
        RealmPlayerStatus::Eliminated
    );
    assert!(night.last_night.as_ref().unwrap().entries.iter().any(|e| matches!(
        e,
        LogEntry::Eliminated { user_id, by_user_id } if *user_id == uid(2) && *by_user_id == uid(1)
    )));
}

#[test]
fn the_night_orders_are_drained_and_its_entries_kept_apart() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[11]);
    // A line the day already had stays on the day, off the night.
    state.log(
        DAY,
        1,
        LogEntry::Spawned {
            user_id: uid(2),
            target: 11,
        },
    );
    order(&mut state, &map, 1, take(5, 1));
    order(&mut state, &map, 1, fortify(4, 1));
    let revision = state.revision;
    resolve_night(&mut state, &map);
    assert!(state.players.iter().all(|p| p.orders.is_empty()));
    let day = state.last_day.as_ref().unwrap();
    let night = state.last_night.as_ref().unwrap();
    assert_eq!(night.day, DAY);
    assert_eq!(day.entries.len(), night.entries.len() + 1);
    assert!(matches!(
        night.entries[0],
        LogEntry::Fortified {
            target: 4,
            level: 2,
            ..
        }
    ));
    assert!(state.revision > revision);
}

/// Hold all but `left` of the testmap for player 1, the rest for player 2.
fn split_world(state: &mut RealmGameState, left: &[TerritoryId]) {
    state.ownership.clear();
    for t in 0..12u16 {
        let owner = if left.contains(&t) { 2 } else { 1 };
        hold(state, owner, &[t]);
    }
}

#[test]
fn three_nights_dominating_win_and_a_night_below_starts_over() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    // A holds most of the world; B keeps a corner.
    split_world(&mut state, &[3]);
    assert!(share(&state, &map, uid(1)) >= state.ruleset.domination_share(2));
    let night = |state: &mut RealmGameState, day: i32| {
        state.roll_day(day, day as u64);
        resolve_night(state, &map).end
    };
    assert_eq!(night(&mut state, DAY), ResolveEnd::Ongoing);
    assert_eq!(night(&mut state, DAY + 1), ResolveEnd::Ongoing);
    assert_eq!(state.player(uid(1)).unwrap().dominating_nights, 2);
    assert_eq!(state.player(uid(2)).unwrap().dominating_nights, 0);

    // A night under the share resets the run.
    let mut slipped = state.clone();
    split_world(&mut slipped, &[0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(night(&mut slipped, DAY + 2), ResolveEnd::Ongoing);
    assert_eq!(slipped.player(uid(1)).unwrap().dominating_nights, 0);

    assert_eq!(
        night(&mut state, DAY + 2),
        ResolveEnd::Won {
            winner: uid(1),
            kind: WinKind::Dominated
        }
    );
    assert!(
        state
            .last_night
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .any(|e| matches!(
                e,
                LogEntry::Won { user_id, kind: WinKind::Dominated } if *user_id == uid(1)
            ))
    );
}

#[test]
fn the_clock_gives_the_world_to_the_biggest_share_that_can_win() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    // The season's last day is tomorrow, and nobody dominates.
    state.ruleset.season_days = 11;
    assert_eq!(season_last_day(&state), Some(DAY + 1));
    hold(&mut state, 1, &[8, 9]);
    hold(&mut state, 2, &[2, 3]);
    hold(&mut state, 3, &[0]);
    assert!(share(&state, &map, uid(1)) > share(&state, &map, uid(2)));
    let night = |state: &mut RealmGameState, day: i32| {
        state.roll_day(day, day as u64);
        resolve_night(state, &map).end
    };
    assert_eq!(night(&mut state.clone(), DAY), ResolveEnd::Ongoing);
    let mut played = state.clone();
    assert_eq!(
        night(&mut played, DAY + 1),
        ResolveEnd::Won {
            winner: uid(1),
            kind: WinKind::SeasonOver
        }
    );
    // The biggest share has not played the days: the next one has.
    let mut late = state.clone();
    late.players[0].active_days = 1;
    assert_eq!(
        night(&mut late, DAY + 1),
        ResolveEnd::Won {
            winner: uid(2),
            kind: WinKind::SeasonOver
        }
    );
    // Nobody has: the world is nobody's.
    let mut idle = state.clone();
    for p in idle.players.iter_mut() {
        p.active_days = 1;
    }
    assert_eq!(night(&mut idle, DAY + 1), ResolveEnd::Dissolved);
}

#[test]
fn an_equal_share_at_the_clock_goes_by_the_ranking() {
    let map = map_by_id("testmap").unwrap();
    // Two players with nothing at all are as level as shares get: the tie
    // goes to the territory count, then to the seeded jitter, which is the
    // order `final_ranking` puts them in.
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    state.ruleset.season_days = 10;
    let seed = state.last_day.as_ref().unwrap().seed;
    let first = final_ranking(&state, None, seed)[0];
    assert_eq!(
        resolve_night(&mut state, &map).end,
        ResolveEnd::Won {
            winner: first,
            kind: WinKind::SeasonOver
        }
    );
}

#[test]
fn the_last_one_standing_still_needs_the_days_in() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[5]);
    order(&mut state, &map, 1, take(5, 5));
    let mut quick = state.clone();
    quick.players[0].active_days = 1;
    let (_, report) = night_where(&quick, &map, |s| !s.is_alive(uid(2)));
    assert_eq!(report.end, ResolveEnd::Ongoing);
    let (_, report) = night_where(&state, &map, |s| !s.is_alive(uid(2)));
    assert_eq!(
        report.end,
        ResolveEnd::Won {
            winner: uid(1),
            kind: WinKind::LastStanding
        }
    );
}

#[test]
fn one_more_fight_elsewhere_does_not_change_this_ones_roll() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4]);
    hold(&mut state, 2, &[0]);
    hold(&mut state, 3, &[11]);
    order(&mut state, &map, 1, take(5, 2));
    let mut alone = state.clone();
    resolve_night(&mut alone, &map);
    order(&mut state, &map, 3, take(10, 1));
    order(&mut state, &map, 2, take(1, 1));
    resolve_night(&mut state, &map);
    assert_eq!(fight_on(&alone, 5).1, fight_on(&state, 5).1);
}

#[test]
fn the_same_night_resolves_the_same_way() {
    let map = map_by_id("testmap").unwrap();
    let mut state = campaign(&[(1, "a"), (2, "b"), (3, "c")]);
    hold(&mut state, 1, &[4, 0]);
    hold(&mut state, 2, &[6, 7]);
    hold(&mut state, 3, &[9, 10]);
    order(&mut state, &map, 1, take(5, 3));
    order(&mut state, &map, 1, take(1, 2));
    order(&mut state, &map, 2, take(5, 3));
    order(&mut state, &map, 2, fortify(6, 1));
    order(&mut state, &map, 3, take(5, 2));
    order(&mut state, &map, 3, take(8, 1));
    let mut once = state.clone();
    let mut twice = state.clone();
    resolve_night(&mut once, &map);
    resolve_night(&mut twice, &map);
    assert_eq!(
        serde_json::to_value(&once).unwrap(),
        serde_json::to_value(&twice).unwrap()
    );
}
