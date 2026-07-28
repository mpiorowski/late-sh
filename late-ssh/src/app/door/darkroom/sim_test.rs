use chrono::{DateTime, Duration, TimeZone, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{self, Fire, Resource};
use super::model::{Builder, Game, View};
use super::pace::{self, DAILY_CREDIT_SECS};
use super::sim;

fn clock() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

/// A game whose clock has already been primed, as the load path does.
fn started(now: DateTime<Utc>) -> Game {
    let mut game = Game::new();
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
    advance(&mut game, View::Outside, &mut now, session_start, one_payout - 1);
    assert_eq!(
        game.store(Resource::Wood),
        0,
        "a payout landed early, so the slowdown is not being applied"
    );

    advance(&mut game, View::Outside, &mut now, session_start, 1);
    assert_eq!(game.store(Resource::Wood), 1);
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
fn cooldowns_keep_running_after_the_daily_allowance_is_spent() {
    let session_start = clock();
    let mut now = session_start;
    let mut game = started(now);
    game.pace.grant(now, DAILY_CREDIT_SECS);
    game.gather_cooldown = data::GATHER_DELAY;

    now += Duration::seconds(i64::from(data::GATHER_DELAY));
    let mut rng = StdRng::seed_from_u64(7);
    let settled = sim::settle(&mut game, View::Outside, now, session_start, &mut rng);

    assert_eq!(settled.credited, 0, "the daily cap should have withheld all of it");
    assert!(settled.withheld > 0);
    assert_eq!(
        game.gather_cooldown, 0,
        "a pacing rule must never leave a button stuck"
    );
}
