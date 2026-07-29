use chrono::{TimeZone, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{Perk, Resource};
use super::event::{self, Active, Ctx, Outcome, Phase, Row};
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
