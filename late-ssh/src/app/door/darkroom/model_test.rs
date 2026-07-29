use super::data::{self, Building, Craftable, Resource, Temperature, TradeGood};
use super::model::{BuildOutcome, Builder, BuyOutcome, CraftOutcome, Game};

/// A game whose builder is up and holding enough wood for a trap.
fn ready_to_build() -> Game {
    let mut game = Game::new();
    game.builder = Builder::Helping;
    game.set_store(Resource::Wood, 200);
    game
}

/// The same, with a workshop standing and a room warm enough to work in.
fn ready_to_craft() -> Game {
    let mut game = ready_to_build();
    game.temperature = Temperature::Mild;
    game.raise(Building::Workshop);
    game
}

fn craftable(item: Resource) -> &'static Craftable {
    data::CRAFTABLES
        .iter()
        .find(|craftable| craftable.item == item)
        .expect("craftable is in the table")
}

fn trade_good(good: Resource) -> &'static TradeGood {
    data::TRADE_GOODS
        .iter()
        .find(|trade| trade.good == good)
        .expect("good is in the table")
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
fn crafting_refuses_whole_and_spends_nothing() {
    let mut game = ready_to_craft();
    let spear = craftable(Resource::BoneSpear);

    // Wood is there, teeth are not.
    assert_eq!(game.craft(spear), CraftOutcome::Missing(Resource::Teeth));
    assert_eq!(
        game.store(Resource::Wood),
        200,
        "a refused craft must spend nothing"
    );

    game.set_store(Resource::Teeth, 5);
    assert_eq!(
        game.craft(spear),
        CraftOutcome::Crafted(Resource::BoneSpear)
    );
    assert_eq!(game.store(Resource::Wood), 100);
    assert_eq!(game.store(Resource::Teeth), 0);
    assert_eq!(game.store(Resource::BoneSpear), 1);

    // Upgrades are one apiece; weapons are not.
    let waterskin = craftable(Resource::Waterskin);
    game.set_store(Resource::Leather, 200);
    assert_eq!(
        game.craft(waterskin),
        CraftOutcome::Crafted(Resource::Waterskin)
    );
    assert_eq!(
        game.craft(waterskin),
        CraftOutcome::AtMaximum(Resource::Waterskin)
    );
    assert_eq!(
        game.store(Resource::Leather),
        150,
        "the refused second waterskin must not cost leather"
    );
}

#[test]
fn craft_rows_wait_for_the_workshop_and_then_stay() {
    // Materials in hand, builder up, but no workshop: nothing on offer.
    let mut game = ready_to_build();
    game.temperature = Temperature::Mild;
    game.set_store(Resource::Leather, 50);
    game.refresh_item_options();
    assert!(
        !game.craft_available(craftable(Resource::Waterskin)),
        "the workshop is what unlocks crafting"
    );

    game.raise(Building::Workshop);
    game.refresh_item_options();
    assert!(game.craft_available(craftable(Resource::Waterskin)));

    // And the row latches: spending the leather does not take it away.
    game.set_store(Resource::Leather, 0);
    game.refresh_item_options();
    assert!(game.craft_available(craftable(Resource::Waterskin)));
}

#[test]
fn the_trading_post_only_sells_what_has_been_seen() {
    let mut game = Game::new();
    game.set_store(Resource::Fur, 500);
    game.set_store(Resource::Scales, 30);
    game.set_store(Resource::Teeth, 20);

    // No post, no shop.
    game.refresh_item_options();
    assert!(!game.buy_available(trade_good(Resource::Compass)));

    game.raise(Building::TradingPost);
    game.refresh_item_options();
    assert!(
        game.buy_available(trade_good(Resource::Scales)),
        "scales have been held, so they are on the shelf"
    );
    assert!(
        !game.buy_available(trade_good(Resource::Iron)),
        "iron has never been seen, so upstream does not offer it yet"
    );
    assert!(
        game.buy_available(trade_good(Resource::Compass)),
        "the compass is the one thing offered sight unseen, and it opens the path"
    );
}

#[test]
fn buying_refuses_whole_and_the_compass_is_a_one_off() {
    let mut game = Game::new();
    game.raise(Building::TradingPost);
    game.set_store(Resource::Fur, 500);
    game.set_store(Resource::Scales, 30);
    game.set_store(Resource::Teeth, 20);
    let compass = trade_good(Resource::Compass);

    assert_eq!(game.buy(compass), BuyOutcome::Bought(Resource::Compass));
    assert_eq!(game.store(Resource::Fur), 100);
    assert_eq!(game.store(Resource::Scales), 10);
    assert_eq!(game.store(Resource::Teeth), 10);

    assert_eq!(game.buy(compass), BuyOutcome::AtMaximum(Resource::Compass));
    assert_eq!(
        game.store(Resource::Fur),
        100,
        "a refused purchase must spend nothing"
    );

    // Fur is short for scales now, and the refusal names what is missing.
    assert_eq!(
        game.buy(trade_good(Resource::Scales)),
        BuyOutcome::Missing(Resource::Fur)
    );
    assert_eq!(game.store(Resource::Fur), 100);
}

#[test]
fn the_builder_never_offers_a_mine() {
    let mut game = ready_to_craft();
    assert_eq!(
        game.build(Building::IronMine),
        BuildOutcome::NotOffered(Building::IronMine),
        "mines come from the wasteland, not from the build list"
    );
    assert_eq!(game.building_count(Building::IronMine), 0);

    // The world granting one opens its trade instead.
    game.raise(Building::IronMine);
    assert!(game.seen_jobs.contains(&super::data::Job::IronMiner));
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

#[test]
fn embarking_needs_meat_packed_and_still_on_the_shelf() {
    let mut game = Game::new();
    game.set_store(Resource::CuredMeat, 10);
    assert!(
        !game.can_embark(),
        "a full store room means nothing until it is packed"
    );
    game.outfit.insert(Resource::CuredMeat, 2);
    assert!(game.can_embark());
    // The loadout is a plan: if the store room has since been emptied, there
    // is nothing to actually take.
    game.set_store(Resource::CuredMeat, 0);
    assert!(!game.can_embark());
}
