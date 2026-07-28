use super::data::{Building, Resource, Temperature};
use super::model::{BuildOutcome, Builder, Game};

/// A game whose builder is up and holding enough wood for a trap.
fn ready_to_build() -> Game {
    let mut game = Game::new();
    game.builder = Builder::Helping;
    game.set_store(Resource::Wood, 200);
    game
}

#[test]
fn a_cold_room_refuses_to_build() {
    // Upstream `Room.build`: "builder just shivers" whenever the room is Cold
    // or worse, before costs are even looked at.
    let mut game = ready_to_build();
    game.temperature = Temperature::Cold;
    assert_eq!(game.build(Building::Trap), BuildOutcome::TooCold);
    assert_eq!(
        game.store(Resource::Wood),
        200,
        "a refused build must spend nothing"
    );

    game.temperature = Temperature::Mild;
    assert_eq!(
        game.build(Building::Trap),
        BuildOutcome::Built(Building::Trap)
    );
    assert_eq!(game.store(Resource::Wood), 190);
}

#[test]
fn the_outside_title_follows_the_hut_count() {
    let mut game = Game::new();
    let expect = [
        (0, "A Silent Forest"),
        (1, "A Lonely Hut"),
        (4, "A Tiny Village"),
        (5, "A Modest Village"),
        (8, "A Modest Village"),
        (9, "A Large Village"),
        (14, "A Large Village"),
        (15, "A Raucous Village"),
    ];
    for (huts, title) in expect {
        game.buildings.insert(Building::Hut, huts);
        assert_eq!(game.outside_title(), title, "at {huts} huts");
    }
}

#[test]
fn traps_split_into_baited_and_bare() {
    let mut game = Game::new();
    game.buildings.insert(Building::Trap, 5);

    game.set_store(Resource::Bait, 2);
    assert_eq!(game.trap_rows(), (3, 2));

    // More bait than traps only baits what stands.
    game.set_store(Resource::Bait, 9);
    assert_eq!(game.trap_rows(), (0, 5));
}

#[test]
fn income_per_tick_totals_every_source() {
    let mut game = Game::new();
    game.builder = Builder::Helping;
    game.population = 4;
    game.workers.insert(super::data::Job::Hunter, 2);
    game.workers.insert(super::data::Job::Charcutier, 1);

    let income = game.income_per_tick();
    // builder 2 + one gatherer 1 - charcutier 5.
    assert_eq!(income.get(&Resource::Wood).copied(), Some(-2.0));
    // two hunters at half a fur each.
    assert_eq!(income.get(&Resource::Fur).copied(), Some(1.0));
    // two hunters at half a meat, minus the charcutier's five.
    assert_eq!(income.get(&Resource::Meat).copied(), Some(-4.0));
    assert_eq!(income.get(&Resource::CuredMeat).copied(), Some(1.0));
}
