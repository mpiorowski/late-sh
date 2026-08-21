use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{Blueprint, Building, Job, Perk, Resource};
use super::model::{Deck, Expedition, GRID, Game, WorldMap};
use super::world::{self, Direction, Step};
use super::world_data::{self, Tile};

fn generated() -> WorldMap {
    let mut rng = StdRng::seed_from_u64(42);
    world::generate(false, &mut rng)
}

/// The same map, for an account that has already flown out once.
fn generated_for_veteran() -> WorldMap {
    let mut rng = StdRng::seed_from_u64(42);
    world::generate(true, &mut rng)
}

#[test]
fn the_map_is_built_to_upstreams_shape() {
    // A veteran's map, because it is the only one that carries every
    // landmark: the battleship is deliberately absent from a first run's, and
    // that gate has its own test below.
    let map = generated_for_veteran();
    let radius = world_data::RADIUS;
    assert_eq!(
        map.tile(radius, radius),
        Tile::Village,
        "home is the centre"
    );
    assert_eq!(map.tiles.len(), GRID as usize);

    // Every landmark that promises a count has one, in its ring.
    for tile in Tile::ALL {
        let Some(landmark) = tile.landmark() else {
            continue;
        };
        if landmark.num == 0 {
            continue;
        }
        let mut found = 0;
        for y in 0..GRID {
            for x in 0..GRID {
                if map.tile(x, y) != tile {
                    continue;
                }
                found += 1;
                let distance = (x - radius).abs() + (y - radius).abs();
                assert!(
                    distance >= landmark.min_radius && distance <= landmark.max_radius * 2,
                    "{} sits {distance} out, outside its ring",
                    landmark.label
                );
            }
        }
        assert_eq!(
            found, landmark.num,
            "expected {} of {}",
            landmark.num, landmark.label
        );
    }

    // The village square is lit, and the far corner is not.
    assert!(map.seen(radius, radius));
    assert!(!map.seen(0, 0));
}

/// A trip standing at the village with a full pack.
fn trip(game: &mut Game, meat: i64, water: i64) -> Expedition {
    game.world = Some(generated());
    let mut trip = Expedition {
        x: world_data::VILLAGE_POS.0,
        y: world_data::VILLAGE_POS.1,
        hp: game.max_health(),
        water,
        map: game.world.clone().unwrap(),
        ..Expedition::default()
    };
    trip.add(Resource::CuredMeat, meat);
    trip
}

/// Walk `steps` times, back and forth so the trip stays near home.
fn wander(game: &mut Game, trip: &mut Expedition, steps: usize) -> Vec<(Step, Vec<String>)> {
    let mut rng = StdRng::seed_from_u64(9);
    let mut results = Vec::new();
    for step in 0..steps {
        let direction = match step % 2 {
            0 => Direction::East,
            _ => Direction::West,
        };
        let mut out = Vec::new();
        // Stepping back onto the village would end the trip, so start by
        // moving out one square and oscillate beyond it.
        let outcome = world::step(game, trip, direction, &mut rng, &mut out);
        results.push((outcome, out));
    }
    results
}

#[test]
fn hunger_warns_once_and_then_kills() {
    let mut game = Game::new(false);
    let mut expedition = trip(&mut game, 1, 100);
    // Two moves per strip of meat: the first eats the last strip, the next
    // warns, the one after that is fatal.
    expedition.x += 1;
    let steps = wander(&mut game, &mut expedition, 6);
    let lines: Vec<&str> = steps
        .iter()
        .flat_map(|(_, out)| out.iter().map(String::as_str))
        .collect();
    assert!(
        lines.contains(&world_data::MSG_MEAT_OUT),
        "expected the out-of-meat line, got {lines:?}"
    );
    assert!(
        lines.contains(&world_data::MSG_STARVING),
        "expected the starvation warning, got {lines:?}"
    );
    assert!(
        steps.iter().any(|(step, _)| *step == Step::Died),
        "starvation has to be fatal in the end"
    );
    assert_eq!(
        game.starved, 1,
        "the count is what eventually teaches a perk"
    );
}

#[test]
fn ten_starvations_teach_a_slow_metabolism() {
    let mut game = Game::new(false);
    game.starved = world_data::STARVED_PERK_AT - 1;
    let mut expedition = trip(&mut game, 0, 100);
    expedition.x += 1;
    let steps = wander(&mut game, &mut expedition, 8);
    assert!(steps.iter().any(|(step, _)| *step == Step::Died));
    assert!(
        game.has_perk(Perk::SlowMetabolism),
        "the tenth starvation is the one that teaches it"
    );
}

#[test]
fn danger_turns_on_at_eight_squares_without_iron() {
    let mut game = Game::new(false);
    let mut expedition = trip(&mut game, 100, 100);
    let mut rng = StdRng::seed_from_u64(4);
    let mut warned = false;
    for _ in 0..8 {
        let mut out = Vec::new();
        world::step(
            &mut game,
            &mut expedition,
            Direction::East,
            &mut rng,
            &mut out,
        );
        warned |= out.iter().any(|line| line == world_data::MSG_DANGER);
    }
    assert!(expedition.distance() >= 8);
    assert!(warned, "eight squares out unarmoured has to say so");
    assert!(expedition.danger);
}

#[test]
fn coming_home_hands_over_the_mines_and_banks_the_pack() {
    let mut game = Game::new(false);
    let mut expedition = trip(&mut game, 5, 10);
    expedition.cleared.insert(Building::IronMine);
    expedition.found_ship = true;
    expedition.add(Resource::Iron, 7);
    expedition.add(Resource::BoneSpear, 1);

    let mut out = Vec::new();
    world::go_home(&mut game, expedition, &mut out);

    assert_eq!(game.building_count(Building::IronMine), 1);
    assert!(
        game.seen_jobs.contains(&Job::IronMiner),
        "a mine that stands has to open its trade"
    );
    assert!(game.ship.is_some(), "the ship tab opens on a safe return");
    assert_eq!(
        game.store(Resource::Iron),
        7,
        "loot lands in the store room"
    );
    assert_eq!(game.store(Resource::CuredMeat), 5);
    // Meat and weapons stay packed for next time; ore does not.
    assert_eq!(game.outfit.get(&Resource::CuredMeat).copied(), Some(5));
    assert_eq!(game.outfit.get(&Resource::BoneSpear).copied(), Some(1));
    assert_eq!(game.outfit.get(&Resource::Iron).copied(), None);
    assert!(game.expedition.is_none());
}

#[test]
fn dying_out_there_drops_everything() {
    let mut game = Game::new(false);
    let expedition = trip(&mut game, 5, 10);
    game.expedition = Some(expedition);
    game.outfit.insert(Resource::CuredMeat, 5);

    let mut out = Vec::new();
    world::die(&mut game, &mut out);

    assert!(game.expedition.is_none());
    assert!(game.outfit.is_empty(), "the pack is lost with the wanderer");
    assert_eq!(game.embark_cooldown, world_data::DEATH_COOLDOWN);
    assert_eq!(game.store(Resource::CuredMeat), 0, "nothing is banked");
}

#[test]
fn a_parked_trip_round_trips_through_the_save() {
    let mut game = Game::new(false);
    let mut expedition = trip(&mut game, 3, 7);
    expedition.x += 4;
    expedition.map.set_visited(expedition.x, expedition.y);
    game.expedition = Some(expedition);

    let blob = super::persist::to_json(&game);
    let loaded = super::persist::from_json(&blob);
    let parked = loaded.expedition.expect("the trip is parked, not lost");

    assert_eq!(parked.x, world_data::VILLAGE_POS.0 + 4);
    assert_eq!(parked.water, 7);
    assert_eq!(parked.carrying(Resource::CuredMeat), 3);
    assert!(parked.map.visited(parked.x, parked.y));
    assert_eq!(parked.map.tiles.len(), GRID as usize);
}

#[test]
fn clearing_a_dungeon_leaves_an_outpost_and_a_road() {
    let mut game = Game::new(false);
    let mut expedition = trip(&mut game, 5, 10);
    expedition.x += 3;
    expedition
        .map
        .set_tile(expedition.x, expedition.y, Tile::Cave);

    world::clear_dungeon(&mut expedition);

    assert_eq!(
        expedition.map.tile(expedition.x, expedition.y),
        Tile::Outpost
    );
    let road = (0..GRID)
        .flat_map(|x| (0..GRID).map(move |y| (x, y)))
        .any(|(x, y)| expedition.map.tile(x, y) == Tile::Road);
    assert!(road, "a cleared dungeon has to connect to the village");
}

#[test]
fn embarking_packs_no_more_than_the_store_room_holds() {
    let mut game = Game::new(false);
    game.world = Some(generated());
    game.set_store(Resource::CuredMeat, 2);
    game.outfit.insert(Resource::CuredMeat, 5);

    let trip = world::embark(&mut game);

    assert_eq!(
        trip.carrying(Resource::CuredMeat),
        2,
        "a stale loadout must not conjure supplies"
    );
    assert_eq!(game.store(Resource::CuredMeat), 0);
}

/// Whether the battleship is anywhere on a map.
fn has_battleship(map: &WorldMap) -> bool {
    (0..GRID)
        .flat_map(|x| (0..GRID).map(move |y| (x, y)))
        .any(|(x, y)| map.tile(x, y) == Tile::Battleship)
}

#[test]
fn the_battleship_is_only_drawn_for_an_account_that_has_flown_out_before() {
    assert!(
        !has_battleship(&generated()),
        "a first run must never meet the wreck"
    );
    assert!(
        has_battleship(&generated_for_veteran()),
        "a veteran's map carries it"
    );
}

#[test]
fn the_battleship_can_be_dropped_into_a_map_drawn_without_it() {
    // Whoever earns the unlock partway through a run should not have to throw
    // that run away to see the wreck.
    let mut map = generated();
    let mut rng = StdRng::seed_from_u64(7);

    assert!(world::place_battleship(&mut map, &mut rng));
    assert!(has_battleship(&map));

    // And it is never dropped twice: a second load must leave the map alone.
    assert!(
        !world::place_battleship(&mut map, &mut rng),
        "a map that already has the wreck must be left untouched"
    );
    let count = (0..GRID)
        .flat_map(|x| (0..GRID).map(move |y| (x, y)))
        .filter(|(x, y)| map.tile(*x, *y) == Tile::Battleship)
        .count();
    assert_eq!(count, 1);
}

#[test]
fn a_visit_opens_on_the_intro_until_the_wreck_has_been_powered_up() {
    let mut game = Game::new(true);
    let mut expedition = trip(&mut game, 5, 10);

    assert_eq!(
        world::battleship_scene(&expedition),
        "executioner-intro",
        "the first arrival explores the wreck"
    );

    expedition.battleship.entered = true;
    assert_eq!(
        world::battleship_scene(&expedition),
        "executioner-antechamber",
        "every arrival after that steps into the elevators"
    );
}

#[test]
fn coming_home_commits_the_battleship_and_redeems_what_the_decks_gave_up() {
    let mut game = Game::new(true);
    let mut expedition = trip(&mut game, 5, 10);
    expedition.battleship.entered = true;
    expedition.battleship.decks.insert(Deck::Engineering);
    expedition.add(Resource::HypoBlueprint, 1);
    expedition.add(Resource::AlienAlloy, 3);
    let mut out = Vec::new();

    world::go_home(&mut game, expedition, &mut out);

    assert!(game.battleship.entered);
    assert!(game.battleship.decks.contains(&Deck::Engineering));
    assert!(game.fabricator, "the strange device opens the fabricator");
    assert!(
        game.blueprints.contains(&Blueprint::Hypo),
        "a blueprint carried home teaches the fabricator"
    );
    assert_eq!(
        game.store(Resource::HypoBlueprint),
        0,
        "the blueprint itself is consumed, never shelved"
    );
    assert_eq!(game.store(Resource::AlienAlloy), 3, "the alloy is not");
}
