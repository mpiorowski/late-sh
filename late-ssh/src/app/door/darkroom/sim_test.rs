use chrono::{DateTime, Duration, TimeZone, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{self, Building, Fire, Resource};
use super::model::{Builder, Game, View};
use super::pace::{self, DAILY_CREDIT_FLOOR_SECS, DAILY_CREDIT_SECS};
use super::sim;

fn clock() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

/// A game whose clock has already been primed, as the load path does.
fn started(now: DateTime<Utc>) -> Game {
    let mut game = Game::new(false);
    game.last_settled = now.timestamp();
    game
}

/// Push the world forward by `secs` and hand back what it announced.
fn advance(
    game: &mut Game,
    view: View,
    now: &mut DateTime<Utc>,
    session_start: DateTime<Utc>,
    secs: i64,
) -> Vec<String> {
    *now += Duration::seconds(secs);
    let mut rng = StdRng::seed_from_u64(7);
    sim::settle(game, view, *now, session_start, &mut rng).messages
}

#[test]
fn lighting_the_fire_brings_the_stranger_and_opens_the_forest() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);

    game.light_fire();
    assert_eq!(game.fire, Fire::Burning);
    assert_eq!(game.builder, Builder::Approaching);

    // She takes one builder-state step to arrive.
    let arrival = advance(
        &mut game,
        View::Room,
        &mut now,
        session_start,
        i64::from(data::BUILDER_STATE_DELAY),
    );
    assert_eq!(game.builder, Builder::Collapsed);
    assert!(
        arrival
            .iter()
            .any(|m| m.contains("a ragged stranger stumbles through the door")),
        "expected the arrival line, got {arrival:?}"
    );

    // Then the wood runs out, and with it the forest opens up.
    let out_of_wood = advance(
        &mut game,
        View::Room,
        &mut now,
        session_start,
        i64::from(data::NEED_WOOD_DELAY),
    );
    assert!(game.forest_unlocked);
    assert_eq!(game.store(Resource::Wood), data::UNLOCK_FOREST_WOOD);
    assert!(
        out_of_wood.iter().any(|m| m == "the wood is running out"),
        "expected the wood warning, got {out_of_wood:?}"
    );
}

#[test]
fn a_warm_room_walks_the_stranger_to_her_feet() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.light_fire();

    // Long enough for the room to warm and both shivering steps to land, but
    // well short of the fire burning out. The room's clocks keep running while
    // the player is outside, exactly as upstream's timers do.
    advance(&mut game, View::Outside, &mut now, session_start, 200);
    assert_eq!(game.builder, Builder::Sleeping);

    // She only gets up when somebody is in the room to see it.
    advance(&mut game, View::Outside, &mut now, session_start, 1);
    assert_eq!(game.builder, Builder::Sleeping);
    advance(&mut game, View::Room, &mut now, session_start, 1);
    assert_eq!(game.builder, Builder::Helping);
}

#[test]
fn a_gatherer_pays_at_the_slowed_rate() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.population = 1;

    let one_payout = i64::from(pace::slowed(data::INCOME_DELAY));
    advance(
        &mut game,
        View::Outside,
        &mut now,
        session_start,
        one_payout - 1,
    );
    assert_eq!(
        game.store(Resource::Wood),
        0,
        "a payout landed early, so the slowdown is not being applied"
    );

    advance(&mut game, View::Outside, &mut now, session_start, 1);
    assert_eq!(game.store(Resource::Wood), 1);
}

#[test]
fn the_metal_trades_stall_when_their_inputs_run_dry() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.raise(Building::Steelworks);
    game.raise(Building::Armoury);
    game.population = 2;
    game.workers.insert(data::Job::Steelworker, 1);
    game.workers.insert(data::Job::Armourer, 1);
    // Exactly one round of inputs for each: the steelworker's iron and coal,
    // and the sulphur the armourer needs to go with the steel that makes.
    game.set_store(Resource::Iron, 1);
    game.set_store(Resource::Coal, 1);
    game.set_store(Resource::Sulphur, 1);

    let one_payout = i64::from(pace::slowed(data::INCOME_DELAY));
    advance(
        &mut game,
        View::Outside,
        &mut now,
        session_start,
        one_payout,
    );
    assert_eq!(game.store(Resource::Iron), 0);
    assert_eq!(game.store(Resource::Coal), 0);
    assert_eq!(game.store(Resource::Sulphur), 0);
    assert_eq!(
        game.store(Resource::Bullets),
        1,
        "the steel made this round should have gone straight into a bullet"
    );
    assert_eq!(game.store(Resource::Steel), 0);

    // With the inputs gone both trades stall rather than going into debt.
    advance(
        &mut game,
        View::Outside,
        &mut now,
        session_start,
        one_payout,
    );
    assert_eq!(game.store(Resource::Bullets), 1);
    assert_eq!(game.store(Resource::Steel), 0);
    assert_eq!(game.store(Resource::Iron), 0);
}

#[test]
fn time_before_the_session_connected_is_worth_nothing() {
    // The save was last touched a day ago; this session connected a minute ago.
    let last_settled = clock();
    let session_start = last_settled + Duration::hours(24);
    let mut now = session_start + Duration::seconds(60);

    let mut game = started(last_settled);
    game.population = 1;

    let mut rng = StdRng::seed_from_u64(7);
    let settled = sim::settle(&mut game, View::Outside, now, session_start, &mut rng);
    assert_eq!(
        settled.credited, 60,
        "credit reached back past the session's connect time"
    );

    // And from here on, connected time does count.
    now += Duration::seconds(60);
    let settled = sim::settle(&mut game, View::Outside, now, session_start, &mut rng);
    assert_eq!(settled.credited, 60);
}

#[test]
fn the_daily_floor_fast_forwards_a_village_but_not_a_bare_room() {
    let session_start = clock();
    let mut now = session_start;

    // A villaged save visited for one second banks the whole floor...
    let mut village = started(now);
    village.buildings.insert(Building::Hut, 1);
    village.population = 2;
    now += Duration::seconds(1);
    let mut rng = StdRng::seed_from_u64(7);
    let settled = sim::settle(&mut village, View::Outside, now, session_start, &mut rng);
    assert_eq!(settled.credited, DAILY_CREDIT_FLOOR_SECS);
    assert_eq!(settled.withheld, 0, "the floor must not underflow withheld");

    // ...while a hutless room only gets the second that passed.
    let mut room = started(session_start);
    let settled = sim::settle(
        &mut room,
        View::Room,
        session_start + Duration::seconds(1),
        session_start,
        &mut rng,
    );
    assert_eq!(settled.credited, 1);
}

#[test]
fn arrivals_are_drawn_from_upstreams_three_delays() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.buildings.insert(Building::Hut, 1);
    // Consume the daily floor first, so the settle below credits exactly one
    // second and the arrival clock still holds its fresh draw.
    game.pace.grant(now, 0, DAILY_CREDIT_FLOOR_SECS);

    // The first villaged second primes the arrival clock from the draw.
    advance(&mut game, View::Outside, &mut now, session_start, 1);
    let expected: Vec<u32> = [30, 90, 150].iter().map(|s| pace::slowed(*s)).collect();
    assert!(
        expected.contains(&game.pop_timer),
        "arrival delay {} is not one of upstream's draws {expected:?}",
        game.pop_timer
    );
}

#[test]
fn a_trap_haul_reads_like_upstream() {
    let one = [(Resource::Fur, 2)];
    assert_eq!(sim::haul_message(&one), "the traps contain scraps of fur");

    let two = [(Resource::Fur, 1), (Resource::Meat, 1)];
    assert_eq!(
        sim::haul_message(&two),
        "the traps contain scraps of fur and bits of meat"
    );

    let three = [
        (Resource::Fur, 1),
        (Resource::Meat, 2),
        (Resource::Charm, 1),
    ];
    assert_eq!(
        sim::haul_message(&three),
        "the traps contain scraps of fur, bits of meat and a crudely made charm"
    );
}

#[test]
fn cooldowns_keep_running_after_the_daily_allowance_is_spent() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.pace.grant(now, DAILY_CREDIT_SECS, 0);
    game.gather_cooldown = data::GATHER_DELAY;

    now += Duration::seconds(i64::from(data::GATHER_DELAY));
    let mut rng = StdRng::seed_from_u64(7);
    let settled = sim::settle(&mut game, View::Outside, now, session_start, &mut rng);

    assert_eq!(
        settled.credited, 0,
        "the daily cap should have withheld all of it"
    );
    assert!(settled.withheld > 0);
    assert_eq!(
        game.gather_cooldown, 0,
        "a pacing rule must never leave a button stuck"
    );
}

#[test]
fn the_thieves_take_what_is_there_and_it_is_all_booked_as_stolen() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.thieves = super::model::Thieves::Active;
    // Seven wood against a ten-wood skim: upstream drains it to zero and
    // books the seven that were actually there, instead of skipping the
    // round the way a starved trade would.
    game.set_store(Resource::Wood, 7);
    game.set_store(Resource::Fur, 100);
    game.set_store(Resource::Meat, 0);

    advance(
        &mut game,
        View::Room,
        &mut now,
        session_start,
        i64::from(pace::slowed(data::INCOME_DELAY)),
    );

    assert_eq!(game.store(Resource::Wood), 0);
    assert_eq!(game.stolen.get(&Resource::Wood).copied(), Some(7));
    assert_eq!(game.store(Resource::Fur), 95);
    assert_eq!(game.stolen.get(&Resource::Fur).copied(), Some(5));
    assert_eq!(game.stolen.get(&Resource::Meat).copied(), None);
}
