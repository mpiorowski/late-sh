use chrono::{TimeZone, Utc};
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::data::{Perk, Resource};
use super::event::{self, Active, Ctx, Event, Loot, Outcome, Phase, Row, Scene};
use super::model::{Expedition, Game, Thieves, View};
use super::world_data::Weapon;

fn game() -> Game {
    let mut game = Game::new(false);
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

// ---------------------------------------------------------------------------
// The ravaged battleship's fights
// ---------------------------------------------------------------------------

/// One of the battleship's fight scenes, opened at full enemy health.
fn battleship_fight(event_key: &str, scene_key: &str) -> Active {
    let event = find(&super::scenes_executioner::EXECUTIONER, event_key);
    let scene = event.scene(scene_key).expect("scene is in the event");
    let enemy_hp = scene.combat.expect("scene is a fight").health;
    Active::resume(event, scene, enemy_hp)
}

#[test]
fn a_shielded_enemy_heals_the_hit_it_takes_and_the_shield_breaks() {
    let mut game = game();
    let mut trip = armed_trip(&mut game);
    trip.add(Resource::SteelSword, 1);
    trip.hp = 500;
    let mut rng = StdRng::seed_from_u64(11);
    let mut out = Vec::new();
    // The unstable prototype throws a shield up every five seconds.
    let mut active = battleship_fight("executioner-engineering", "7");
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };

    // Swings miss one time in five, so rather than pinning a single hit this
    // drives the fight and holds the invariant across all of it: while the
    // shield is up, nothing the wanderer does can take the enemy's health
    // down, and a landed hit puts it up and breaks the shield.
    let mut shielded_seen = false;
    let mut healed_through_a_shield = false;
    let mut hurt_through_a_shield = false;
    let mut broke_a_shield = false;
    for _ in 0..60 {
        let Some(before) = active.fight() else {
            break;
        };
        let was_shielded =
            before.enemy_status.map(|held| held.status) == Some(event::Status::Shield);
        let hp_before = before.enemy_hp;
        shielded_seen |= was_shielded;

        active.press(
            Row::Attack(Weapon::SteelSword),
            &mut ctx,
            &mut rng,
            &mut out,
        );

        if let Some(after) = active.fight()
            && was_shielded
        {
            healed_through_a_shield |= after.enemy_hp > hp_before;
            hurt_through_a_shield |= after.enemy_hp < hp_before;
            broke_a_shield |= after.enemy_status.is_none();
        }
        active.tick(2.0, &mut ctx, &mut rng, &mut out);
    }

    assert!(shielded_seen, "the prototype shields itself every 5s");
    assert!(
        !hurt_through_a_shield,
        "a shielded enemy must never lose health to a hit"
    );
    assert!(
        healed_through_a_shield,
        "a shielded hit has to heal it instead"
    );
    assert!(broke_a_shield, "and the shield breaks on that one hit");
}

#[test]
fn the_unstable_automaton_goes_off_before_it_gives_up_its_blueprint() {
    let mut game = game();
    let mut trip = armed_trip(&mut game);
    trip.add(Resource::Grenade, 40);
    trip.hp = 200;
    let mut rng = StdRng::seed_from_u64(5);
    let mut out = Vec::new();
    let mut active = battleship_fight("executioner-medical", "8");
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };

    // Check the phase straight after the swing that kills it: ticking on
    // would run the blast's own clock out and hide the whole point.
    for _ in 0..60 {
        active.press(Row::Attack(Weapon::Grenade), &mut ctx, &mut rng, &mut out);
        if !matches!(active.phase, Phase::Fighting(_)) {
            break;
        }
        active.tick(5.0, &mut ctx, &mut rng, &mut out);
    }
    assert!(
        matches!(active.phase, Phase::Exploding { .. }),
        "it does not fall over, it winds up: {:?}",
        active.phase
    );

    let hp_before = ctx.trip.as_ref().expect("on a trip").hp;
    // The blast lands whether or not anyone is still watching.
    active.tick(
        super::world_data::EXPLOSION_DELAY,
        &mut ctx,
        &mut rng,
        &mut out,
    );
    assert!(
        matches!(active.phase, Phase::Spoils { .. }),
        "and then the fight is over"
    );
    assert_eq!(
        ctx.trip.as_ref().expect("on a trip").hp,
        hp_before - 30,
        "thirty damage, taken after the kill"
    );
    assert!(
        active
            .loot
            .iter()
            .any(|row| row.item == Resource::GlowstoneBlueprint),
        "the blueprint is still on the floor afterwards"
    );
}

#[test]
fn the_medic_turns_venomous_once_and_its_bite_keeps_working() {
    let mut game = game();
    let mut trip = armed_trip(&mut game);
    trip.add(Resource::SteelSword, 1);
    trip.hp = 2_000;
    let mut rng = StdRng::seed_from_u64(2);
    let mut out = Vec::new();
    let mut active = battleship_fight("executioner-medical", "5-1");
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };

    // Eighty health, venomous at forty: swing until it crosses the line.
    for _ in 0..20 {
        if active
            .fight()
            .is_some_and(|fight| fight.enemy_hp <= 40 && fight.enemy_hp > 0)
        {
            break;
        }
        active.press(
            Row::Attack(Weapon::SteelSword),
            &mut ctx,
            &mut rng,
            &mut out,
        );
        active.tick(2.0, &mut ctx, &mut rng, &mut out);
    }
    assert_eq!(
        active
            .fight()
            .expect("still fighting")
            .enemy_status
            .map(|held| held.status),
        Some(event::Status::Venomous),
        "half dead, it starts spitting"
    );

    // Sit through its swings until one lands, and check it leaves a bleed.
    for _ in 0..60 {
        if active.fight().is_some_and(|fight| fight.bleed.is_some()) {
            break;
        }
        active.tick(1.0, &mut ctx, &mut rng, &mut out);
    }
    assert!(
        active.fight().is_some_and(|fight| fight.bleed.is_some()),
        "a venomous hit has to keep working after it lands"
    );

    let hp_before = ctx.trip.as_ref().expect("on a trip").hp;
    active.tick(super::world_data::DOT_TICK, &mut ctx, &mut rng, &mut out);
    assert!(
        ctx.trip.as_ref().expect("on a trip").hp < hp_before,
        "the bleed ticks on its own"
    );
}

/// Walk the engineering wing's `start` scene until the weighted branch lands
/// on the burning junction (`1-3`), whose two buttons are both cost-gated.
fn reach_burning_junction(ctx: &mut Ctx<'_>, out: &mut Vec<String>) -> Active {
    let engineering = find(
        &super::scenes_executioner::EXECUTIONER,
        "executioner-engineering",
    );
    for seed in 0..64u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut active = Active::start(engineering, ctx, &mut rng, out);
        active.press(Row::Button(0), ctx, &mut rng, out);
        if active.scene.key == "1-3" {
            return active;
        }
    }
    panic!("the weighted branch never landed on the burning junction");
}

/// The burning junction's buttons both cost (5 water / 10 hp) and the scene
/// has no leave, faithfully to upstream. A browser player who can afford
/// neither can refresh the page; over SSH the modal swallows Esc, so a modal
/// with zero pressable rows would hold the session hostage until the
/// connection dropped.
#[test]
fn the_burning_junction_always_leaves_a_way_out() {
    let mut game = game();
    let mut trip = Expedition {
        hp: 4,
        water: 0,
        ..Expedition::default()
    };
    let mut rng = StdRng::seed_from_u64(0);
    let mut out = Vec::new();
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut active = reach_burning_junction(&mut ctx, &mut out);

    assert!(
        !active.row_ready(Row::Button(0), &ctx.look()),
        "no water to extinguish with"
    );
    assert!(
        !active.row_ready(Row::Button(1), &ctx.look()),
        "not enough blood to rush through with"
    );
    let rows = active.rows(&ctx.look());
    assert!(
        rows.contains(&Row::Leave),
        "a modal with nothing pressable must offer leave, got {rows:?}"
    );
    assert!(active.row_ready(Row::Leave, &ctx.look()));
    assert_eq!(
        active.press(Row::Leave, &mut ctx, &mut rng, &mut out),
        Outcome::Done
    );
}

/// Upstream lets the hp cost land on exactly-enough health and leaves the
/// wanderer standing at 0 hp, alive until something else hits them. Same rule
/// as the stim: over SSH a button that silently ends the run reads as a bug,
/// so the cost refuses when paying it would take the last of the health.
#[test]
fn rushing_through_the_flames_can_never_be_the_killing_blow() {
    let mut game = game();
    let mut trip = Expedition {
        hp: 10,
        water: 0,
        ..Expedition::default()
    };
    let mut rng = StdRng::seed_from_u64(0);
    let mut out = Vec::new();
    let mut ctx = Ctx {
        game: &mut game,
        trip: Some(&mut trip),
        view: View::World,
        now: Utc.timestamp_opt(1_800_000_000, 0).unwrap(),
    };
    let mut active = reach_burning_junction(&mut ctx, &mut out);
    assert!(
        !active.row_ready(Row::Button(1), &ctx.look()),
        "10 hp is exactly the cost, and the cost may not kill"
    );

    // One point more and the wanderer limps through alive.
    ctx.trip.as_mut().expect("on a trip").hp = 11;
    assert!(active.row_ready(Row::Button(1), &ctx.look()));
    let rows = active.rows(&ctx.look());
    assert!(
        !rows.contains(&Row::Leave),
        "a solvent wanderer gets no way out but through, got {rows:?}"
    );
    assert_eq!(
        active.press(Row::Button(1), &mut ctx, &mut rng, &mut out),
        Outcome::Continue
    );
    assert_eq!(ctx.trip.as_ref().expect("on a trip").hp, 1);
}

/// The elevator bank is the only way into the three wings, so a button that
/// says one deck and opens another would be silent and unrecoverable. Both
/// halves come off `Deck`; this is what pins that they still line up.
#[test]
fn every_elevator_button_opens_the_wing_it_is_labelled_with() {
    let antechamber = find(
        &super::scenes_executioner::EXECUTIONER,
        "executioner-antechamber",
    );
    let start = antechamber.scene("start").expect("the bank of elevators");

    for deck in super::model::Deck::ALL {
        let button = start
            .buttons
            .iter()
            .find(|button| button.text == deck.label())
            .unwrap_or_else(|| panic!("no button for the {} deck", deck.label()));
        assert_eq!(
            button.next,
            event::Next::Event(deck.scene()),
            "the {} button opens the wrong wing",
            deck.label()
        );
        assert!(
            super::scenes_executioner::by_key(deck.scene()).is_some(),
            "the {} deck names an event that does not exist",
            deck.label()
        );
    }
}
