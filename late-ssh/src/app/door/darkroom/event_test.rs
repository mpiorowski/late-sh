use chrono::{TimeZone, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{Perk, Resource};
use super::event::{self, Active, Ctx, Event, Loot, Outcome, Phase, Row, Scene};
use super::model::{Expedition, Game, Thieves, View};
use super::world_data::Weapon;

fn game() -> Game {
    let mut game = Game::new();
    game.last_settled = 1_800_000_000;
    game
}

/// A context over the village, with no trip in progress.
macro_rules! village {
    ($game:expr) => {
        Ctx {
            game: &mut $game,
            trip: None,
            view: View::Room,
            now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        }
    };
}

fn find(pool: &'static [event::Event], key: &str) -> &'static event::Event {
    pool.iter()
        .find(|event| event.key == key)
        .expect("event is in the pool")
}

#[test]
fn a_button_refuses_until_its_cost_is_covered() {
    let mut game = game();
    game.set_store(Resource::Fur, 99);
    let mut rng = StdRng::seed_from_u64(3);
    let mut out = Vec::new();
    let nomad = find(&super::scenes_village::POOL, "nomad");

    let mut ctx = village!(game);
    let mut active = Active::start(nomad, &mut ctx, &mut rng, &mut out);
    // "buy scales" wants 100 fur, and 99 is not 100.
    assert!(!active.row_ready(Row::Button(0), &ctx.look()));
    assert_eq!(
        active.press(Row::Button(0), &mut ctx, &mut rng, &mut out),
        Outcome::Continue
    );
    assert_eq!(
        ctx.game.store(Resource::Fur),
        99,
        "a refusal spends nothing"
    );

    ctx.game.set_store(Resource::Fur, 100);
    assert!(active.row_ready(Row::Button(0), &ctx.look()));
    active.press(Row::Button(0), &mut ctx, &mut rng, &mut out);
    assert_eq!(ctx.game.store(Resource::Fur), 0);
    assert_eq!(ctx.game.store(Resource::Scales), 1);
    // The nomad stays put: upstream's trade buttons have no next scene.
    assert_eq!(active.scene.key, "start");
}

#[test]
fn hanging_the_thief_returns_what_was_taken() {
    let mut game = game();
    game.thieves = Thieves::Active;
    game.stolen.insert(Resource::Wood, 120);
    game.set_store(Resource::Wood, 10);
    let mut rng = StdRng::seed_from_u64(11);
    let mut out = Vec::new();
    let thief = find(&super::scenes_village::POOL, "thief");

    let mut ctx = village!(game);
    let mut active = Active::start(thief, &mut ctx, &mut rng, &mut out);
    // Button 0 is "hang him".
    active.press(Row::Button(0), &mut ctx, &mut rng, &mut out);

    assert_eq!(active.scene.key, "hang");
    assert_eq!(ctx.game.thieves, Thieves::Dealt, "the skim has to stop");
    assert_eq!(ctx.game.store(Resource::Wood), 130);
    assert!(ctx.game.stolen.is_empty());

    // Sparing him teaches sneaking instead, and hands nothing back.
    game.thieves = Thieves::Active;
    game.stolen.insert(Resource::Wood, 120);
    game.set_store(Resource::Wood, 10);
    let mut ctx = village!(game);
    let mut active = Active::start(thief, &mut ctx, &mut rng, &mut out);
    active.press(Row::Button(1), &mut ctx, &mut rng, &mut out);
    assert!(ctx.game.has_perk(Perk::Stealthy));
    assert_eq!(ctx.game.store(Resource::Wood), 10);
}

#[test]
fn a_weighted_branch_follows_the_roll() {
    // Noises outside: `{0.3: 'stuff', 1: 'nothing'}`. A low roll finds the
    // bundle of sticks, a high one finds nothing at all.
    let noises = find(&super::scenes_village::POOL, "noises outside");
    let mut low = None;
    let mut high = None;
    for seed in 0..64u64 {
        let mut game = game();
        game.set_store(Resource::Wood, 50);
        let mut rng = StdRng::seed_from_u64(seed);
        let mut out = Vec::new();
        let mut ctx = village!(game);
        let mut active = Active::start(noises, &mut ctx, &mut rng, &mut out);
        active.press(Row::Button(0), &mut ctx, &mut rng, &mut out);
        match active.scene.key {
            "stuff" => low = Some(ctx.game.store(Resource::Wood)),
            "nothing" => high = Some(ctx.game.store(Resource::Wood)),
            other => panic!("unexpected scene {other}"),
        }
        if low.is_some() && high.is_some() {
            break;
        }
    }
    assert_eq!(low, Some(150), "the sticks are worth 100 wood");
    assert_eq!(high, Some(50), "and the other branch is worth nothing");
}

/// A trip standing on a fight, with a spear and a strip of meat.
fn armed_trip(game: &mut Game) -> Expedition {
    game.set_store(Resource::CuredMeat, 10);
    let mut trip = Expedition {
        hp: 10,
        water: 10,
        ..Expedition::default()
    };
    trip.add(Resource::BoneSpear, 1);
    trip.add(Resource::CuredMeat, 2);
    trip
}

#[test]
fn a_fight_spends_ammo_and_ends_when_the_enemy_drops() {
    let mut game = game();
    let mut trip = armed_trip(&mut game);
    trip.add(Resource::Rifle, 1);
    trip.add(Resource::Bullets, 1);
    let beast = find(&super::scenes_encounters::ENCOUNTERS, "snarling beast");
    let mut rng = StdRng::seed_from_u64(1);
    let mut out = Vec::new();
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut active = Active::start(beast, &mut ctx, &mut rng, &mut out);
    assert!(matches!(active.phase, Phase::Fighting(_)));

    // The rifle costs a bullet per shot, and once they are gone it is dead
    // weight.
    active.press(Row::Attack(Weapon::Rifle), &mut ctx, &mut rng, &mut out);
    assert_eq!(ctx.quantity(Resource::Bullets), 0);
    assert!(!active.row_ready(Row::Attack(Weapon::Rifle), &ctx.look()));

    // Five health, two damage a stab: keep stabbing (past the cooldown) until
    // it falls.
    for _ in 0..30 {
        if matches!(active.phase, Phase::Spoils { .. }) {
            break;
        }
        active.tick(2.0, &mut ctx, &mut rng, &mut out);
        active.press(Row::Attack(Weapon::BoneSpear), &mut ctx, &mut rng, &mut out);
    }
    assert!(
        matches!(active.phase, Phase::Spoils { .. }),
        "the beast should be dead by now"
    );
    assert!(
        out.iter().any(|line| line == "the snarling beast is dead"),
        "expected the death line, got {out:?}"
    );
}

#[test]
fn loot_is_bounded_by_what_the_pack_can_hold() {
    let mut game = game();
    let mut trip = armed_trip(&mut game);
    // Ten units of space, and the spear and meat already fill four of it.
    let beast = find(&super::scenes_encounters::ENCOUNTERS, "snarling beast");
    let mut rng = StdRng::seed_from_u64(5);
    let mut out = Vec::new();
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut active = Active::start(beast, &mut ctx, &mut rng, &mut out);
    for _ in 0..40 {
        if matches!(active.phase, Phase::Spoils { .. }) {
            break;
        }
        active.tick(2.0, &mut ctx, &mut rng, &mut out);
        active.press(Row::Attack(Weapon::BoneSpear), &mut ctx, &mut rng, &mut out);
    }
    // Fill the pack, then try to keep taking.
    let capacity = ctx.game.capacity();
    active.press(Row::TakeAll, &mut ctx, &mut rng, &mut out);
    let load = ctx.trip.as_ref().map(|trip| trip.load()).unwrap_or(0.0);
    assert!(
        load <= capacity,
        "took {load} into a pack that holds {capacity}"
    );
    // Leaving is allowed once the second-long pause after a fight is up, full
    // pack or not.
    assert!(
        !active.row_ready(Row::Leave, &ctx.look()),
        "the leave row waits out upstream's cooldown"
    );
    active.tick(1.5, &mut ctx, &mut rng, &mut out);
    assert!(active.row_ready(Row::Leave, &ctx.look()));
}

#[test]
fn a_delayed_reward_is_scheduled_and_not_paid_early() {
    let mut game = game();
    game.set_store(Resource::Wood, 1000);
    let wanderer = find(&super::scenes_village::POOL, "wanderer wood");
    let mut out = Vec::new();
    // Seeded so the roll lands under the 0.5 chance at least once.
    let mut scheduled = false;
    for seed in 0..32u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut game = game.clone();
        let mut ctx = village!(game);
        let mut active = Active::start(wanderer, &mut ctx, &mut rng, &mut out);
        active.press(Row::Button(0), &mut ctx, &mut rng, &mut out);
        assert_eq!(ctx.game.store(Resource::Wood), 900, "the cart takes 100");
        if let Some(reward) = ctx.game.pending_rewards.first() {
            assert_eq!(reward.amount, 300);
            assert_eq!(reward.due, 1_800_000_000 + 60);
            scheduled = true;
            break;
        }
    }
    assert!(scheduled, "the wanderer never once promised to come back");
}

#[test]
fn the_unarmed_master_punches_twice_as_fast() {
    let beast = find(&super::scenes_encounters::ENCOUNTERS, "snarling beast");
    let mut rng = StdRng::seed_from_u64(9);
    let mut out = Vec::new();

    let mut cooldown_after_punch = |game: &mut Game| {
        // Nothing in the pack, so fists are the only row.
        let mut trip = Expedition {
            hp: 10,
            water: 10,
            ..Expedition::default()
        };
        let mut ctx = Ctx {
            game,
            trip: Some(&mut trip),
            view: View::World,
            now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
        };
        let mut active = Active::start(beast, &mut ctx, &mut rng, &mut out);
        active.press(Row::Attack(Weapon::Fists), &mut ctx, &mut rng, &mut out);
        active
            .fight()
            .and_then(|fight| fight.weapon_cooldown.get(&Weapon::Fists).copied())
            .expect("the punch went on cooldown")
    };

    let mut game = game();
    assert_eq!(cooldown_after_punch(&mut game), 2.0);
    game.add_perk(Perk::UnarmedMaster);
    assert_eq!(
        cooldown_after_punch(&mut game),
        1.0,
        "upstream halves the fists cooldown for the unarmed master"
    );
}

#[test]
fn drop_items_to_take_heavy_loot() {
    let mut game = game();
    // Default capacity is 10.0. Fill it with 10 CuredMeat (weight 1.0 each).
    let mut trip = Expedition {
        hp: 10,
        water: 10,
        outfit: [(Resource::CuredMeat, 10)].into_iter().collect(),
        ..Expedition::default()
    };
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut rng = StdRng::seed_from_u64(0);
    let mut out = Vec::new();

    // An event offering an IronSword (weight 3.0).
    static LOOT: [Loot; 1] = [Loot {
        item: Resource::IronSword,
        min: 1,
        max: 1,
        chance: 1.0,
    }];
    static SCENE: Scene = Scene {
        key: "start",
        loot: &LOOT,
        ..Scene::EMPTY
    };
    static EVENT: Event = Event {
        key: "test loot drop",
        title: "test",
        available: &[],
        scenes: &[SCENE],
    };

    let mut active = Active::start(&EVENT, &mut ctx, &mut rng, &mut out);
    assert_eq!(active.loot.len(), 1);
    assert_eq!(active.loot[0].item, Resource::IronSword);
    assert_eq!(ctx.look().free_space(), 0.0);

    // Row::Take(0) should be ready because dropping 3 cured meat covers the sword (weight 3.0).
    assert!(active.row_ready(Row::Take(0), &ctx.look()));

    // Pressing Take(0) when it doesn't fit transitions to Phase::DropFor { loot_index: 0 }.
    let outcome = active.press(Row::Take(0), &mut ctx, &mut rng, &mut out);
    assert_eq!(outcome, Outcome::Continue);
    assert!(matches!(active.phase, Phase::DropFor { loot_index: 0 }));

    // Check rows offered in DropFor phase.
    let rows = active.rows(&ctx.look());
    assert_eq!(
        rows,
        vec![
            Row::Drop {
                item: Resource::CuredMeat,
                count: 3
            },
            Row::DropCancel
        ]
    );

    // Press Row::Drop.
    let outcome = active.press(
        Row::Drop {
            item: Resource::CuredMeat,
            count: 3,
        },
        &mut ctx,
        &mut rng,
        &mut out,
    );
    assert_eq!(outcome, Outcome::Continue);
    assert!(matches!(active.phase, Phase::Story));

    // Verify outfit and loot state.
    let trip = ctx.trip.as_ref().unwrap();
    assert_eq!(trip.carrying(Resource::CuredMeat), 7);
    assert_eq!(trip.carrying(Resource::IronSword), 1);
    assert_eq!(trip.load(), 10.0);

    // Ground loot should now contain the dropped CuredMeat (3 left).
    let dropped = active.loot.iter().find(|l| l.item == Resource::CuredMeat);
    assert!(dropped.is_some());
    assert_eq!(dropped.unwrap().left, 3);
}

#[test]
fn cancel_drop_restores_spoils_phase_without_changes() {
    let mut game = game();
    let mut trip = Expedition {
        hp: 10,
        water: 10,
        outfit: [(Resource::CuredMeat, 10)].into_iter().collect(),
        ..Expedition::default()
    };
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut rng = StdRng::seed_from_u64(0);
    let mut out = Vec::new();

    static LOOT: [Loot; 1] = [Loot {
        item: Resource::IronSword,
        min: 1,
        max: 1,
        chance: 1.0,
    }];
    static SCENE: Scene = Scene {
        key: "start",
        loot: &LOOT,
        ..Scene::EMPTY
    };
    static EVENT: Event = Event {
        key: "test loot drop cancel",
        title: "test",
        available: &[],
        scenes: &[SCENE],
    };

    let mut active = Active::start(&EVENT, &mut ctx, &mut rng, &mut out);
    active.press(Row::Take(0), &mut ctx, &mut rng, &mut out);
    assert!(matches!(active.phase, Phase::DropFor { loot_index: 0 }));

    active.press(Row::DropCancel, &mut ctx, &mut rng, &mut out);
    assert!(matches!(active.phase, Phase::Story));

    let trip = ctx.trip.as_ref().unwrap();
    assert_eq!(trip.carrying(Resource::CuredMeat), 10);
    assert_eq!(trip.carrying(Resource::IronSword), 0);
    assert_eq!(active.loot[0].left, 1);
}

#[test]
fn unusable_ranged_weapon_without_ammo_falls_back_to_fists() {
    let mut game = game();
    let mut trip = Expedition {
        hp: 10,
        water: 10,
        ..Expedition::default()
    };
    trip.add(Resource::Rifle, 1);
    assert_eq!(trip.carrying(Resource::Bullets), 0);

    let beast = find(&super::scenes_encounters::ENCOUNTERS, "snarling beast");
    let mut rng = StdRng::seed_from_u64(1);
    let mut out = Vec::new();
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };

    let weapons = event::available_weapons(&ctx.look());
    assert!(weapons.contains(&Weapon::Rifle));
    assert!(weapons.contains(&Weapon::Fists));

    let mut active = Active::start(beast, &mut ctx, &mut rng, &mut out);
    assert!(matches!(active.phase, Phase::Fighting(_)));
    assert!(!active.row_ready(Row::Attack(Weapon::Rifle), &ctx.look()));
    assert!(active.row_ready(Row::Attack(Weapon::Fists), &ctx.look()));

    active.press(Row::Attack(Weapon::Fists), &mut ctx, &mut rng, &mut out);
    assert_eq!(
        active
            .fight()
            .and_then(|fight| fight.weapon_cooldown.get(&Weapon::Fists).copied()),
        Some(2.0)
    );
}
