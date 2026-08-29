use super::*;

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn craft_entry(name: &str, skill: &str) -> CraftEntryView {
    CraftEntryView {
        recipe: 0,
        name: name.to_string(),
        skill: skill.to_string(),
        inputs: String::new(),
        craftable: true,
        reason: String::new(),
    }
}

#[test]
fn item_category_maps_kinds_to_panel_sections() {
    use super::super::items::{ItemKind, Slot};
    assert_eq!(item_category(&ItemKind::Equipment(Slot::Weapon)), "Weapons");
    assert_eq!(item_category(&ItemKind::Equipment(Slot::Chest)), "Armor");
    assert_eq!(item_category(&ItemKind::Equipment(Slot::Ring)), "Armor");
    // An actual heal/restore consumable groups under "Heals"...
    assert_eq!(
        item_category(&ItemKind::Consumable {
            heal: 30,
            restore: 0
        }),
        "Heals"
    );
    assert_eq!(
        item_category(&ItemKind::Consumable {
            heal: 0,
            restore: 20
        }),
        "Heals"
    );
    // ...while a non-heal consumable (a zero-heal Consumable, or a Utility
    // item like a poison) groups under the more general "Consumables",
    // separate from pure sell-fodder "Valuables".
    assert_eq!(
        item_category(&ItemKind::Consumable {
            heal: 0,
            restore: 0
        }),
        "Consumables"
    );
    assert_eq!(item_category(&ItemKind::Utility), "Consumables");
    assert_eq!(item_category(&ItemKind::Valuable), "Valuables");
}

#[test]
fn section_rows_group_and_fold_generically() {
    use std::collections::HashSet;
    // Three items across two categories; first-seen order preserved.
    let cats = ["A", "B", "A"];
    let cat = |i: usize| (format!("p:{}", cats[i]), cats[i].to_string());
    let rows = section_rows(3, cat, &HashSet::new());
    assert_eq!(
        rows,
        vec![
            SectionRow::Header {
                key: "p:A".into(),
                label: "A".into(),
                count: 2,
                collapsed: false
            },
            SectionRow::Item { index: 0 },
            SectionRow::Item { index: 2 },
            SectionRow::Header {
                key: "p:B".into(),
                label: "B".into(),
                count: 1,
                collapsed: false
            },
            SectionRow::Item { index: 1 },
        ]
    );
    // Folding a category hides exactly its items.
    let folded: HashSet<String> = ["p:A".to_string()].into_iter().collect();
    let rows = section_rows(3, cat, &folded);
    assert!(
        !rows
            .iter()
            .any(|r| matches!(r, SectionRow::Item { index } if *index == 0 || *index == 2))
    );
    assert!(
        rows.iter()
            .any(|r| matches!(r, SectionRow::Item { index } if *index == 1))
    );
}

#[test]
fn craft_rows_group_under_collapsible_skill_headers() {
    use std::collections::HashSet;
    let view = CraftView {
        stations: "forge, kitchen".to_string(),
        entries: vec![
            craft_entry("Iron Sword", "Smithing"),
            craft_entry("Iron Shield", "Smithing"),
            craft_entry("Trout Stew", "Cooking"),
        ],
    };
    // Expanded: a header per skill (first-seen order) followed by its recipes.
    let rows = view.rows(&HashSet::new());
    assert_eq!(
        rows,
        vec![
            SectionRow::Header {
                key: "craft:Smithing".into(),
                label: "Smithing".into(),
                count: 2,
                collapsed: false
            },
            SectionRow::Item { index: 0 },
            SectionRow::Item { index: 1 },
            SectionRow::Header {
                key: "craft:Cooking".into(),
                label: "Cooking".into(),
                count: 1,
                collapsed: false
            },
            SectionRow::Item { index: 2 },
        ]
    );
    // Collapsing Smithing hides its recipes but keeps the header (marked
    // collapsed); Cooking is untouched.
    let collapsed: HashSet<String> = ["craft:Smithing".to_string()].into_iter().collect();
    let rows = view.rows(&collapsed);
    assert_eq!(
        rows,
        vec![
            SectionRow::Header {
                key: "craft:Smithing".into(),
                label: "Smithing".into(),
                count: 2,
                collapsed: true
            },
            SectionRow::Header {
                key: "craft:Cooking".into(),
                label: "Cooking".into(),
                count: 1,
                collapsed: false
            },
            SectionRow::Item { index: 2 },
        ]
    );
}

fn world() -> WorldState {
    WorldState::new(uid(999), seed_world())
}

#[test]
fn gathering_a_node_yields_its_material_and_trains_the_skill() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    // Stand at the roadside birch (Woodcutting tier 0, room 600).
    s.players.get_mut(&uid(1)).unwrap().room = 600;
    let before = s.players[&uid(1)].inventory.len();
    s.gather(uid(1));
    let p = &s.players[&uid(1)];
    assert_eq!(p.inventory.len(), before + 1, "a material is taken");
    assert!(
        p.inventory
            .contains(&super::super::items::material_id(0, 0)),
        "the birch log lands in the pack"
    );
    assert_eq!(
        p.skill_xp(GatherSkill::Woodcutting),
        12,
        "woodcutting xp is granted"
    );
}

#[test]
fn a_worked_node_is_depleted_until_it_regrows() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    s.players.get_mut(&uid(1)).unwrap().room = 600;
    s.gather(uid(1));
    let after_one = s.players[&uid(1)].inventory.len();
    s.gather(uid(1)); // still on cooldown
    assert_eq!(
        s.players[&uid(1)].inventory.len(),
        after_one,
        "the same node can't be stripped twice before it regrows"
    );
}

#[test]
fn an_underskilled_node_refuses_to_be_worked() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    // The ironbark (tier 4, room 803) needs Woodcutting 38; a fresh
    // character has no woodcutting training at all.
    s.players.get_mut(&uid(1)).unwrap().room = 803;
    let before = s.players[&uid(1)].inventory.len();
    s.gather(uid(1));
    let p = &s.players[&uid(1)];
    assert_eq!(
        p.inventory.len(),
        before,
        "nothing is taken while under-skilled"
    );
    assert_eq!(
        p.skill_xp(GatherSkill::Woodcutting),
        0,
        "no xp for a node you can't work"
    );
}

#[test]
fn skill_xp_survives_a_save_load_round_trip() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .skills
        .insert(GatherSkill::Mining, 500);
    let saved = s.export_saved(uid(1)).expect("classed characters export");
    let mut s2 = world();
    s2.join(uid(1));
    s2.hydrate(uid(1), &saved);
    assert_eq!(
        s2.players[&uid(1)].skill_xp(GatherSkill::Mining),
        500,
        "mining xp reloads through the save"
    );
}

fn copper_ingot_recipe() -> usize {
    recipe_indices_for(CraftSkill::Smithing)
        .into_iter()
        .find(|&i| recipe(i).unwrap().output == super::super::items::ingot_id(0))
        .expect("a copper ingot recipe exists")
}

#[test]
fn crafting_at_a_station_consumes_inputs_and_makes_the_output() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Stand at Embergate's crafters' row (room 3) with 2 copper ore.
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = 3;
        p.inventory.push(super::super::items::material_id(1, 0));
        p.inventory.push(super::super::items::material_id(1, 0));
    }
    s.craft(uid(1), copper_ingot_recipe());
    let p = &s.players[&uid(1)];
    assert_eq!(
        p.item_count(super::super::items::material_id(1, 0)),
        0,
        "the ore is consumed"
    );
    assert_eq!(
        p.item_count(super::super::items::ingot_id(0)),
        1,
        "an ingot is produced"
    );
    assert!(
        p.craft_xp(CraftSkill::Smithing) > 0,
        "smithing is trained by crafting"
    );
}

#[test]
fn crafting_needs_both_the_station_and_the_materials() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let ri = copper_ingot_recipe();
    // Away from a forge (town square) with no ore: nothing is made.
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.craft(uid(1), ri);
    assert_eq!(
        s.players[&uid(1)].item_count(super::super::items::ingot_id(0)),
        0,
        "no station means no craft"
    );
    // At the forge but still without ore: still nothing, and no xp.
    s.players.get_mut(&uid(1)).unwrap().room = 3;
    s.craft(uid(1), ri);
    assert_eq!(
        s.players[&uid(1)].item_count(super::super::items::ingot_id(0)),
        0,
        "no materials means no craft"
    );
    assert_eq!(
        s.players[&uid(1)].craft_xp(CraftSkill::Smithing),
        0,
        "a failed craft trains nothing"
    );
}

#[test]
fn craft_skill_xp_survives_a_save_load_round_trip() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .craft_skills
        .insert(CraftSkill::Alchemy, 250);
    let saved = s.export_saved(uid(1)).expect("classed characters export");
    let mut s2 = world();
    s2.join(uid(1));
    s2.hydrate(uid(1), &saved);
    assert_eq!(
        s2.players[&uid(1)].craft_xp(CraftSkill::Alchemy),
        250,
        "alchemy xp reloads through the save"
    );
}

#[test]
fn a_poison_coats_the_weapon_instead_of_being_drunk() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let poison = super::super::items::poison_id(2);
    s.players.get_mut(&uid(1)).unwrap().inventory.push(poison);
    s.use_item(uid(1), poison);
    let p = &s.players[&uid(1)];
    assert_eq!(
        p.weapon_coat.map(|(school, _, _)| school),
        Some(DamageType::Poison),
        "the weapon is coated with poison"
    );
    assert!(!p.inventory.contains(&poison), "the vial is used up");
}

#[test]
fn a_coated_weapon_poisons_the_foe_and_spends_a_charge() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    s.players.get_mut(&uid(1)).unwrap().weapon_coat =
        Some((DamageType::Poison, 10, POISON_CHARGES));
    s.tick();
    assert_eq!(
        s.players[&uid(1)].weapon_coat.map(|(_, _, c)| c),
        Some(POISON_CHARGES - 1),
        "a landed strike spends one poison charge"
    );
    assert!(
        s.mob_dots.get(&mob_id).is_some_and(|d| !d.is_empty()),
        "the struck foe is left with a poison DoT"
    );
}

#[test]
fn an_oil_coats_the_weapon_with_its_school_and_replaces_the_last_coat() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let poison = super::super::items::poison_id(2);
    let oil = super::super::items::oil_id(0, 2); // Firebrand Oil
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.inventory.push(poison);
        p.inventory.push(oil);
    }
    s.use_item(uid(1), poison);
    s.use_item(uid(1), oil);
    let p = &s.players[&uid(1)];
    assert_eq!(
        p.weapon_coat,
        Some((DamageType::Fire, OIL_PER_TICK[2], OIL_CHARGES)),
        "the oil takes the one coat slot, replacing the poison"
    );
    assert!(!p.inventory.contains(&oil), "the vial is used up");
}

#[test]
fn the_attack_bar_still_matches_a_real_character() {
    // TIER_ATTACK_BAR is the yardstick the coat curves and the world pass's
    // grind-rate budget are both written against, so it has to be measured
    // rather than remembered: rebuild a real character at each crafting gate,
    // wearing that tier's crafted weapon, and ask the engine what it swings for.
    for t in 0..6usize {
        let lvl = TIER_GATE_LEVEL[t];
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.level = lvl;
        p.base_attack = Class::Warrior.stats_at(lvl).attack;
        // Ability scores are rolled 4d6-drop-lowest at character creation, so
        // the bar is measured at the flat 10s (a +0 modifier). A yardstick that
        // moved with the dice would not be a yardstick.
        p.scores = super::super::stats::AbilityScores::default();
        p.equipped
            .insert(Slot::Weapon, super::super::items::smith_weapon_id(t as u32));
        assert_eq!(
            p.attack(),
            TIER_ATTACK_BAR[t],
            "tier {t} (level {lvl}): the bar moved, so every coat share moved with it"
        );
    }
}

#[test]
fn the_coat_curves_stay_inside_their_share_of_the_bar() {
    // The balance contract for weapon coats, in the only terms that mean
    // anything: what fraction of a real swing the rider is worth. An oil
    // sustains about a fifth of the auto for a whole fight; a poison bursts
    // about a third of it for a handful of strikes. Both are held to a band,
    // and both are held to it at *every* tier, so the curves can never quietly
    // outgrow the attack curve the way they did before.
    for t in 0..6usize {
        let bar = TIER_ATTACK_BAR[t] as f64;
        let oil = OIL_PER_TICK[t] as f64 / bar;
        let poison = POISON_PER_TICK[t] as f64 / bar;
        assert!(
            (0.15..=0.22).contains(&oil),
            "tier {t}: oil rider is {oil:.3} of the bar, outside the sustain band"
        );
        assert!(
            (0.24..=0.32).contains(&poison),
            "tier {t}: poison rider is {poison:.3} of the bar, outside the burst band"
        );
        assert!(
            POISON_PER_TICK[t] > OIL_PER_TICK[t],
            "tier {t}: poison must hit harder per tick than oil, or it is dead content"
        );
    }
    // And the shape that keeps them from being the same item: the poison
    // bursts, the oil lasts. A coat wound is live for its charges plus the
    // trailing POISON_DOT_TICKS after the final swing.
    let ticks = |charges: u8| (charges + POISON_DOT_TICKS - 1) as f64;
    let oil_total = ticks(OIL_CHARGES) * OIL_PER_TICK[5] as f64;
    let poison_total = ticks(POISON_CHARGES) * POISON_PER_TICK[5] as f64;
    assert!(
        poison_total < oil_total,
        "the cheap vial must not out-total the prepared coat"
    );
    assert!(
        poison_total / oil_total > 0.6,
        "but it must stay a real alternative, not a strictly worse one"
    );
}

#[test]
fn a_coat_keeps_one_refreshing_wound_however_many_swings_land() {
    // A coat re-seeds on every landed strike, at the very cadence its DoT
    // ticks. If each strike opened its own wound, POISON_DOT_TICKS of them
    // would be live at once and the rider would be paid three times over -
    // which is not the bar the grind-rate budget is written against. One
    // wound per attacker, refreshed, is the contract.
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    // A neutral foe: `engaged_with` grabs whichever mob the map hands over
    // first, and this test is about how many wounds a coat opens, not about
    // what the foe's profile does to the number on each one.
    if let Some(m) = s.mobs.get_mut(&mob_id) {
        m.spawn.profile = DamageProfile::new(DamageType::Physical, None, None);
    }
    s.players.get_mut(&uid(1)).unwrap().weapon_coat = Some((DamageType::Fire, 10, OIL_CHARGES));
    for _ in 0..5 {
        s.tick();
    }
    let stacks = s.mob_dots.get(&mob_id).map(Vec::as_slice).unwrap_or(&[]);
    assert_eq!(stacks.len(), 1, "five landed swings, one coat wound");
    assert_eq!(stacks[0].per_tick, 10, "ticking for the rider exactly once");
    let view = s.snapshot().players[&uid(1)].clone();
    let foe = view
        .mobs
        .iter()
        .find(|m| m.id == mob_id)
        .expect("the foe is on the panel");
    assert_eq!(
        foe.dot_stacks, 1,
        "and the panel shows one wound, not a growing pile"
    );
}

#[test]
fn ability_dots_still_stack_on_top_of_a_coat() {
    // The other half of the rule: an ability DoT is one wound per cast and
    // keeps stacking (its cooldown is what rations it), and it stacks
    // alongside the coat's single wound rather than refreshing it.
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    s.players.get_mut(&uid(1)).unwrap().weapon_coat = Some((DamageType::Fire, 10, OIL_CHARGES));
    s.tick();
    s.seed_mob_dot(
        uid(1),
        7,
        DamageType::Poison,
        3,
        DotSource::Ability,
        "Venom",
    );
    s.seed_mob_dot(
        uid(1),
        7,
        DamageType::Poison,
        3,
        DotSource::Ability,
        "Venom",
    );
    let stacks = s.mob_dots.get(&mob_id).map(Vec::as_slice).unwrap_or(&[]);
    assert_eq!(stacks.len(), 3, "one coat wound plus two ability wounds");
}

#[test]
fn an_oiled_strike_rides_the_zone_profile() {
    // Against a foe weak to Fire, a fire oil's DoT seeds at 1.5x per tick;
    // against one that resists it, at half. The multiplier is baked in by
    // seed_mob_dot, so the coat is a real matchup lever, not flat flavor.
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    if let Some(m) = s.mobs.get_mut(&mob_id) {
        m.spawn.profile = DamageProfile::new(DamageType::Physical, None, Some(DamageType::Fire));
    }
    s.players.get_mut(&uid(1)).unwrap().weapon_coat = Some((DamageType::Fire, 10, OIL_CHARGES));
    s.tick();
    let seeded = s
        .mob_dots
        .get(&mob_id)
        .and_then(|d| d.first())
        .map(|dot| dot.per_tick);
    assert_eq!(seeded, Some(15), "weak to fire: 10 per tick seeds as 15");
}

#[test]
fn gear_comparison_reads_against_what_is_worn() {
    let mut equipped = HashMap::new();
    equipped.insert(Slot::Weapon, 1000u32); // Rusty Shortsword, +4 atk
    let stronger = item(super::super::items::smith_weapon_id(2)).unwrap(); // Iron Sword, +16
    let cmp = compare_to_worn(&equipped, stronger);
    assert!(cmp.starts_with("vs worn:"), "shows a comparison: {cmp}");
    assert!(cmp.contains("+12 atk"), "16 vs 4 should read +12: {cmp}");
    // A bare slot reads as a new slot; a consumable never compares.
    let empty = HashMap::new();
    assert_eq!(compare_to_worn(&empty, stronger), "new slot");
    let potion = item(super::super::items::potion_id(0)).unwrap();
    assert_eq!(compare_to_worn(&empty, potion), "");
}

#[test]
fn eating_cooked_food_grants_a_well_fed_regen() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let meal = super::super::items::food_id(1);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.inventory.push(meal);
        p.hp = 1;
    }
    s.use_item(uid(1), meal);
    let p = &s.players[&uid(1)];
    assert!(
        p.self_effects
            .iter()
            .any(|e| e.kind == AbilityEffect::HealOverTime && e.remaining > 0),
        "a hot meal leaves a well-fed regen"
    );
}

fn grant_frontier_unlock_titles(s: &mut WorldState, user_id: Uuid) {
    let p = s.players.get_mut(&user_id).expect("player exists");
    for title in FRONTIER_REQUIRED_TITLES {
        if !p.titles.iter().any(|owned| owned == title) {
            p.titles.push(title.to_string());
        }
    }
}

fn dir_to_zone(s: &WorldState, from: RoomId, zone: &str) -> Dir {
    s.world
        .room(from)
        .expect("room exists")
        .exits
        .iter()
        .find_map(|(dir, dest)| {
            s.world
                .room(*dest)
                .is_some_and(|room| room.zone == zone)
                .then_some(*dir)
        })
        .expect("exit to zone exists")
}

/// Put a classed player and a single controlled mob (with `behavior`) into a
/// non-safe Frontier room that has same-zone neighbours to flee to, engage
/// it, and return (state, mob_id). The mob is given a big HP pool so the
/// player's opening strike can't kill it before its behavior resolves.
fn engaged_with(behavior: MobBehavior) -> (WorldState, u32) {
    const ROOM: RoomId = 2001; // Frontier zone 0, interior (non-safe, has exits)
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let mob_id = *s.mobs.keys().next().expect("world has mobs");
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.behavior = behavior;
        m.alive = true;
        m.revealed = true;
        m.current_room = ROOM;
        m.leash_home = ROOM;
        m.hp = 200;
        m.spawn.max_hp = 1000;
        m.spawn.damage = 1; // can't kill the player while we observe
    }
    s.players.get_mut(&uid(1)).unwrap().room = ROOM;
    s.engage(uid(1));
    assert_eq!(s.players[&uid(1)].target, Some(mob_id), "engaged the mob");
    (s, mob_id)
}

#[test]
fn skirmisher_flees_when_wounded_and_breaks_the_lock() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Skirmisher);
    let start = s.mobs[&mob_id].current_room;
    // Wound it below a third so the flee condition trips.
    s.mobs.get_mut(&mob_id).unwrap().hp = 100; // < 1000/3
    s.tick();
    assert_ne!(
        s.mobs[&mob_id].current_room, start,
        "a wounded skirmisher should flee to another room"
    );
    assert_eq!(
        s.players[&uid(1)].target,
        None,
        "fleeing breaks the player's target lock"
    );
}

#[test]
fn abilities_scale_with_spell_power_and_the_auto_swings_by_calling() {
    // A level-1 Mage holding a Mythril Arming Sword (+34 attack): attack
    // rating 5 + 34 = 39. Mage weights are auto 50 / spell 60, so the sword
    // swings for 19, spell power is 23, and Firebolt (magnitude 16, a Strike
    // at 100% of spell power) lands for 16 + 23 = 39, +20% Arcane Mastery = 46.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    let mob_id = *s.mobs.keys().next().expect("world has mobs");
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = 2001;
        m.leash_home = 2001;
        m.hp = 100_000;
        m.spawn.max_hp = 100_000;
        m.spawn.damage = 1;
        m.spawn.profile = DamageProfile::physical();
    }
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = 2001;
        p.scores = super::super::stats::AbilityScores::default();
        p.equipped.insert(Slot::Weapon, 1010);
    }
    s.engage_mob(uid(1), mob_id);
    s.use_ability(uid(1), 1);
    assert_eq!(
        s.mobs[&mob_id].hp,
        100_000 - 46,
        "Firebolt = (16 + 23) * 1.2"
    );
    s.tick();
    assert_eq!(
        s.mobs[&mob_id].hp,
        100_000 - 46 - 19,
        "the auto swings for half the rating"
    );
}

#[test]
fn a_draught_needs_a_breath_between_gulps() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.hp = 1;
        p.inventory = vec![1300, 1300, 1300]; // three Minor Healing Draughts (40)
    }
    s.use_item(uid(1), 1300);
    assert_eq!(s.players[&uid(1)].hp, 41);
    s.use_item(uid(1), 1300);
    assert_eq!(s.players[&uid(1)].hp, 41, "the second gulp is refused");
    assert_eq!(
        s.players[&uid(1)].inventory.len(),
        2,
        "and nothing is spent"
    );
    for _ in 0..QUAFF_COOLDOWN_TICKS {
        s.tick();
    }
    s.use_item(uid(1), 1300);
    assert_eq!(
        s.players[&uid(1)].inventory.len(),
        1,
        "a breath later it goes down"
    );
    assert!(s.players[&uid(1)].hp > 41);
}

#[test]
fn a_companion_bites_off_its_owners_rating() {
    // A level-1 Warrior holding a Mythril Arming Sword (attack rating 6 + 34 =
    // 40, a full swing for a martial) with a fresh Emberdrake (base bite 20,
    // loyalty 0): the companion bites for its own 20 plus PET_COEF_PCT (20%)
    // of the owner's rating, 28, so the tick takes 40 + 28 off the foe.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let mob_id = *s.mobs.keys().next().expect("world has mobs");
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = 2001;
        m.leash_home = 2001;
        m.hp = 100_000;
        m.spawn.max_hp = 100_000;
        m.spawn.damage = 1;
        m.spawn.profile = DamageProfile::physical();
    }
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = 2001;
        p.scores = super::super::stats::AbilityScores::default();
        p.equipped.insert(Slot::Weapon, 1010);
        let drake = super::super::pets::pet_species_by_key("emberdrake").expect("stable species");
        p.pet = Some(super::super::pets::Pet::new(drake, 0));
    }
    s.engage_mob(uid(1), mob_id);
    s.tick();
    assert_eq!(
        s.mobs[&mob_id].hp,
        100_000 - 40 - 28,
        "the swing (40) and the companion's bite (20 + 20% of 40)"
    );
}

#[test]
fn fleeing_costs_a_parting_blow_and_the_foe_recovers() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Sentinel);
    s.mobs.get_mut(&mob_id).unwrap().spawn.damage = 10;
    let hp_before = s.players[&uid(1)].hp;
    s.flee(uid(1));
    let p = &s.players[&uid(1)];
    assert_eq!(p.target, None, "the lock is dropped");
    assert_eq!(
        p.hp,
        hp_before - 10,
        "the foe strikes at a fleeing back (naked Warrior, no armor)"
    );
    let m = &s.mobs[&mob_id];
    assert_eq!(
        m.hp, m.spawn.max_hp,
        "a foe left with nobody fighting it recovers on the spot"
    );
}

#[test]
fn a_stunned_foe_cannot_strike_at_a_fleeing_back() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Sentinel);
    s.mobs.get_mut(&mob_id).unwrap().spawn.damage = 10;
    s.mob_stuns.insert(mob_id, 2);
    s.mob_dots.insert(
        mob_id,
        vec![MobDot {
            owner: uid(1),
            per_tick: 5,
            remaining: 3,
            source: DotSource::Ability,
        }],
    );
    let hp_before = s.players[&uid(1)].hp;
    s.flee(uid(1));
    assert_eq!(
        s.players[&uid(1)].hp,
        hp_before,
        "a reeling foe gets no blow"
    );
    let m = &s.mobs[&mob_id];
    assert_eq!(m.hp, m.spawn.max_hp);
    assert!(
        !s.mob_stuns.contains_key(&mob_id),
        "the stun does not outlive the fight it was cast in"
    );
    assert!(!s.mob_dots.contains_key(&mob_id), "nor do the wounds");
}

#[test]
fn a_shorter_stun_does_not_cut_a_longer_one_short() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Sentinel);
    s.mob_stuns.insert(mob_id, 4);
    let p = s.players.get_mut(&uid(1)).unwrap();
    p.level = 12; // unlocks Shield Bash (slot 4), a 1-tick stun
    p.resource = 999;
    s.use_ability(uid(1), 4);
    assert!(s.mobs[&mob_id].alive, "the bash only dazes the sentinel");
    assert_eq!(
        s.mob_stuns.get(&mob_id).copied(),
        Some(4),
        "a fresh 1-tick daze must not shorten the 4 ticks already on the foe"
    );
}

#[test]
fn a_wounded_foe_nobody_fights_recovers_after_a_few_ticks() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Sentinel);
    // The attacker is simply gone mid-fight (death, disconnect): no flee, the
    // lock just vanishes.
    s.players.get_mut(&uid(1)).unwrap().target = None;
    s.mob_stuns.insert(mob_id, 5);
    for _ in 1..MOB_RESET_TICKS {
        s.tick();
    }
    assert_eq!(s.mobs[&mob_id].hp, 200, "a short grace keeps the wounds");
    s.tick();
    let m = &s.mobs[&mob_id];
    assert_eq!(m.hp, m.spawn.max_hp, "then the foe recovers in full");
    assert!(!s.mob_stuns.contains_key(&mob_id));
}

#[test]
fn a_foe_someone_else_still_fights_keeps_its_wounds_when_you_flee() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Sentinel);
    let room = s.players[&uid(1)].room;
    s.join(uid(2));
    s.choose_class(uid(2), Class::Rogue);
    s.players.get_mut(&uid(2)).unwrap().room = room;
    s.engage_mob(uid(2), mob_id);
    assert_eq!(s.players[&uid(2)].target, Some(mob_id));
    s.flee(uid(1));
    assert_eq!(s.mobs[&mob_id].hp, 200, "still in a fight with someone");
    for _ in 0..MOB_RESET_TICKS + 1 {
        s.tick();
    }
    assert!(
        s.mobs[&mob_id].hp < 200,
        "the second fighter keeps grinding it down, no recovery"
    );
}

#[test]
fn summoner_calls_an_add_into_the_fight() {
    let (mut s, _mob_id) = engaged_with(MobBehavior::Summoner);
    let before = s.mobs.len();
    s.tick();
    assert!(
        s.mobs.keys().any(|id| *id >= SUMMON_ID_START),
        "summoner should have spawned a runtime add"
    );
    assert!(s.mobs.len() > before, "the add joins the mob roster");
}

#[test]
fn world_clock_cycles_through_day_phases_and_weather() {
    assert_eq!(TimeOfDay::from_ticks(0), TimeOfDay::Dawn);
    assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS), TimeOfDay::Day);
    assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 2), TimeOfDay::Dusk);
    assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 3), TimeOfDay::Night);
    assert_eq!(TimeOfDay::from_ticks(PHASE_TICKS * 4), TimeOfDay::Dawn);
    // The dark hits harder than the day.
    assert_eq!(TimeOfDay::Day.mob_damage_pct(), 100);
    assert!(TimeOfDay::Night.mob_damage_pct() > 100);
    // Weather rolls over as the clock advances.
    assert_ne!(
        Weather::from_ticks(0),
        Weather::from_ticks(WEATHER_TICKS * 2)
    );
    // Every phase gets its own glyph, and "dark" lines up with dusk/night
    // exactly (the UI colours the clock as a danger cue from this flag).
    let phases = [
        TimeOfDay::Dawn,
        TimeOfDay::Day,
        TimeOfDay::Dusk,
        TimeOfDay::Night,
    ];
    let glyphs: std::collections::HashSet<&str> = phases.iter().map(|p| p.glyph()).collect();
    assert_eq!(glyphs.len(), 4, "every phase has a distinct glyph");
    assert!(!TimeOfDay::Dawn.is_dark());
    assert!(!TimeOfDay::Day.is_dark());
    assert!(TimeOfDay::Dusk.is_dark());
    assert!(TimeOfDay::Night.is_dark());
}

#[test]
fn stray_feeding_day_boundary_is_spelled_out_in_real_time() {
    // The bug: "come back tomorrow" left players guessing when "tomorrow"
    // actually starts, and easy to confuse with the much faster in-game
    // Dawn/Day/Dusk/Night clock (a ~16-minute cycle, not a real day).
    let countdown = time_until_next_utc_day();
    assert!(
        countdown.ends_with('m'),
        "always at least a minutes component: {countdown}"
    );
    // Never a full day or more, and never negative/absurd - it counts down
    // to the *next* midnight, always less than 24h away.
    if let Some(h) = countdown.split('h').next()
        && countdown.contains('h')
    {
        let hours: i64 = h.trim().parse().expect("leading hours are numeric");
        assert!((0..24).contains(&hours), "got {countdown}");
    }
}

#[test]
fn world_boss_waits_for_frontier_unlock_titles() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    s.world_ticks = WORLD_BOSS_FIRST_TICK - 1;
    s.next_world_boss_tick = WORLD_BOSS_FIRST_TICK;
    s.tick();
    assert_eq!(
        s.world_boss, None,
        "world boss should not wake before the living-dark seals"
    );
    assert!(
        s.next_world_boss_tick > WORLD_BOSS_FIRST_TICK,
        "failed wake should reschedule instead of retrying every tick"
    );
}

#[test]
fn world_boss_rises_on_schedule_and_is_announced() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    grant_frontier_unlock_titles(&mut s, uid(1));
    s.world_ticks = WORLD_BOSS_FIRST_TICK - 1;
    s.next_world_boss_tick = WORLD_BOSS_FIRST_TICK;
    s.tick();
    assert_eq!(
        s.world_boss,
        Some(WORLD_BOSS_ID),
        "a world boss should rise"
    );
    let boss = s
        .mobs
        .get(&WORLD_BOSS_ID)
        .expect("world boss joins the roster");
    assert!(boss.spawn.boss, "it is a boss");
    assert!(matches!(boss.behavior, MobBehavior::Hunter), "it hunts");
    assert!(
        boss.spawn.loot.iter().any(|id| (3000..3200).contains(id)),
        "post-unlock world boss should drop Frontier catalog loot"
    );
    assert!(
        is_frontier_room(boss.current_room)
            || s.world
                .room(boss.current_room)
                .is_some_and(|room| is_living_dark_zone(room.zone)),
        "world boss should spawn in endgame regions"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("rises")),
        "the rising is announced server-wide"
    );
}

#[test]
fn board_bounty_accepts_then_pays_out_on_claim() {
    use super::super::world::TASMANIA_SQUARE;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = TASMANIA_SQUARE;
    // Bounty 1 hunts the Catacombs, which sit behind the Archdemon's gate;
    // this test exercises the accept/claim flow, so open the gate first
    // (sealed-posting behavior has its own test).
    s.award_title(uid(1), FRONTIER_GATE_TITLE.to_string(), 1);

    // The board's picker lists bounty 1 as available before it's taken.
    let entries = s.board_entries(uid(1), TASMANIA_SQUARE);
    let posting = entries
        .iter()
        .find(|e| e.quest_id == 1)
        .expect("bounty 1 is posted and available");
    assert!(!posting.ready, "not accepted yet, so not claimable");
    assert!(!posting.blurb.is_empty(), "the picker shows the blurb");
    assert!(!posting.objective.is_empty(), "and the objective");

    s.accept_board_quest(uid(1), 1);
    assert!(
        s.players[&uid(1)]
            .board_progress
            .iter()
            .any(|(id, _)| *id == 1),
        "accepting from the picker takes the bounty"
    );
    // Once accepted, the picker no longer offers it again as a fresh posting.
    assert!(
        !s.board_entries(uid(1), TASMANIA_SQUARE)
            .iter()
            .any(|e| e.quest_id == 1 && !e.ready),
        "an already-accepted bounty isn't offered again"
    );

    // Force it complete: the picker now shows it as ready to claim.
    for e in s
        .players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .iter_mut()
    {
        if e.0 == 1 {
            e.1 = 99;
        }
    }
    let ready = s
        .board_entries(uid(1), TASMANIA_SQUARE)
        .into_iter()
        .find(|e| e.quest_id == 1)
        .expect("the finished bounty is still listed");
    assert!(ready.ready, "a finished bounty shows as ready to claim");

    let gold_before = s.players[&uid(1)].gold;
    s.claim_board_quest(uid(1), 1);
    // Quest 1 is a Daily, so a claim records a cooldown rather than a
    // permanent done-flag.
    assert!(
        s.players[&uid(1)]
            .quest_cooldowns
            .iter()
            .any(|(id, _)| *id == 1),
        "claiming the daily records its cooldown"
    );
    assert_eq!(
        s.players[&uid(1)].gold,
        gold_before + 120,
        "the reward is paid on claim"
    );
    assert!(
        !s.players[&uid(1)]
            .board_progress
            .iter()
            .any(|(id, _)| *id == 1),
        "a claimed bounty leaves the active list"
    );
}

#[test]
fn accepting_a_bounty_not_offered_by_the_picker_is_a_no_op() {
    // A stale or tampered quest_id (already accepted elsewhere, on cooldown,
    // or simply never offered) must not be acceptable out of band - only
    // what the picker actually lists.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .push((1, 0));
    let before = s.players[&uid(1)].board_progress.clone();

    s.accept_board_quest(uid(1), 1); // already in progress
    assert_eq!(
        s.players[&uid(1)].board_progress,
        before,
        "accepting an already-active bounty changes nothing"
    );

    s.claim_board_quest(uid(1), 1); // not yet ready
    assert_eq!(
        s.players[&uid(1)].board_progress,
        before,
        "claiming an unfinished bounty changes nothing"
    );
}

#[test]
fn quest_journal_rows_carry_a_real_description() {
    // The bug: a quest's name alone doesn't say what it actually asks for -
    // players couldn't remember what a bounty was after the one-time
    // accept-time log line scrolled off. Every row must now say so directly.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .push((1, 0));

    let quests = s.snapshot().players[&uid(1)].quests.clone();
    let bounty = quests
        .iter()
        .find(|q| q.name.starts_with("Still the Restless Dead"))
        .expect("the accepted bounty appears in the journal");
    assert!(
        bounty.desc.contains("Skeletons walk the crypt"),
        "the bounty's blurb should be in its description: {:?}",
        bounty.desc
    );
    assert!(
        bounty.desc.contains("slay 5 of Skeleton-kind"),
        "the mechanical objective should be in its description too: {:?}",
        bounty.desc
    );

    // A fresh character sees no Frontier rows at all - twenty endgame quests
    // used to drown the journal from level 1. They appear once the gate
    // titles are held, each with a description.
    assert!(
        !quests.iter().any(|q| q.kind == QuestKind::Frontier),
        "a locked Frontier lists no zone quests"
    );
    for title in FRONTIER_REQUIRED_TITLES {
        s.award_title(uid(1), title.to_string(), 1);
    }
    let view = s.snapshot().players[&uid(1)].clone();
    assert!(view.frontier_open, "the gate titles open the Frontier");
    let frontier = view
        .quests
        .iter()
        .find(|q| q.kind == QuestKind::Frontier)
        .expect("an open Frontier lists its zone quests");
    assert!(
        !frontier.desc.is_empty(),
        "Frontier quests get a description too, not just board bounties"
    );
}

#[test]
fn reach_bounty_completes_on_entering_the_zone() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    // Hold the "Into the Dark" reach bounty (id 3 -> The Sunken Catacombs).
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .push((3, 0));
    s.players.get_mut(&uid(1)).unwrap().room = 5001; // a Catacombs room
    s.describe_room(uid(1));
    let prog = s.players[&uid(1)]
        .board_progress
        .iter()
        .find(|(id, _)| *id == 3)
        .map(|(_, p)| *p)
        .expect("reach bounty still tracked");
    assert!(
        prog >= 1,
        "entering the catacombs completes the reach bounty"
    );
}

#[test]
fn escort_completes_on_reaching_its_destination_zone() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().escort = Some(EscortState {
        quest_id: 10,
        name: "Brother Aldric",
        dest_zone: "The Sunken Catacombs",
        hp: 80,
        max_hp: 80,
    });
    let gold_before = s.players[&uid(1)].gold;
    s.players.get_mut(&uid(1)).unwrap().room = 5001; // a Catacombs room
    s.describe_room(uid(1));
    assert!(
        s.players[&uid(1)].escort.is_none(),
        "the escort completes on arrival"
    );
    assert!(
        s.players[&uid(1)].board_done.contains(&10),
        "quest 10 is done"
    );
    assert_eq!(
        s.players[&uid(1)].gold,
        gold_before + 220,
        "the escort reward is paid"
    );
}

#[test]
fn escort_is_lost_when_the_escortee_is_slain() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().escort = Some(EscortState {
        quest_id: 10,
        name: "Brother Aldric",
        dest_zone: "The Sunken Catacombs",
        hp: 3,
        max_hp: 80,
    });
    // generation is 0, so roll = raw % 100; raw=10 -> 10 < 35 -> a hit lands.
    s.wound_escort(uid(1), 10);
    assert!(
        s.players[&uid(1)].escort.is_none(),
        "a slain escortee ends the escort"
    );
}

#[test]
fn daily_bounty_goes_on_cooldown_then_returns_after_a_day() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = super::super::world::TASMANIA_SQUARE;
    // Take and finish the daily bounty (id 1), then claim it.
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .push((1, 99));
    s.claim_board_quest(uid(1), 1);
    assert!(
        s.players[&uid(1)]
            .quest_cooldowns
            .iter()
            .any(|(id, _)| *id == 1),
        "claiming a daily records its cooldown"
    );
    assert!(
        !s.players[&uid(1)].board_done.contains(&1),
        "a daily is never permanently done"
    );
    let q1 = board_quest(1).unwrap();
    let claimed_at = s.players[&uid(1)]
        .quest_cooldowns
        .iter()
        .find_map(|(id, at)| (*id == 1).then_some(*at))
        .expect("daily claim timestamp");
    assert!(
        !s.board_quest_available_at(&s.players[&uid(1)], q1, claimed_at),
        "a freshly-claimed daily is unavailable"
    );
    assert!(
        s.board_quest_available_at(&s.players[&uid(1)], q1, claimed_at + DAY_SECS),
        "the daily returns once a day has passed"
    );
}

#[test]
fn druid_regenerates_health_each_tick() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Druid);
    s.players.get_mut(&uid(1)).unwrap().hp = 1;
    s.tick();
    assert!(
        s.players[&uid(1)].hp > 1,
        "Nature's Renewal should mend the Druid each tick"
    );
}

#[test]
fn necromancer_harvests_health_and_souls_on_a_kill() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Necromancer);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.hp = 5;
        p.resource = 0;
    }
    let mob_id = *s.mobs.keys().next().expect("world has mobs");
    s.kill_mob(uid(1), mob_id);
    let p = &s.players[&uid(1)];
    assert!(p.hp > 5, "Soul Harvest restores health on a kill");
    assert!(p.resource > 0, "Soul Harvest restores Souls on a kill");
}

#[test]
fn spiritmaster_siphons_health_and_souls_on_a_kill() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Spiritmaster);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.hp = 5;
        p.resource = 0;
    }
    let mob_id = *s.mobs.keys().next().expect("world has mobs");
    s.kill_mob(uid(1), mob_id);
    let p = &s.players[&uid(1)];
    assert!(p.hp > 5, "Spirit Siphon restores health on a kill");
    assert!(p.resource > 0, "Spirit Siphon restores Souls on a kill");
}

#[test]
fn beastlord_pack_bond_toughens_the_companion() {
    // The same incoming blow splashes less onto a Beastlord's companion than
    // onto an ordinary owner's - Pack Bond makes the beast hardier.
    let species = super::super::pets::pet_species_by_key("war_hound").unwrap();
    let mut plain = world();
    plain.join(uid(1));
    plain.choose_class(uid(1), Class::Ranger);
    plain.players.get_mut(&uid(1)).unwrap().pet = Some(super::super::pets::Pet::new(species, 0));
    plain.wound_pet(uid(1), 100);
    let plain_hp = plain.players[&uid(1)].pet.unwrap().hp;

    let mut bond = world();
    bond.join(uid(2));
    bond.choose_class(uid(2), Class::Beastlord);
    bond.players.get_mut(&uid(2)).unwrap().pet = Some(super::super::pets::Pet::new(species, 0));
    bond.wound_pet(uid(2), 100);
    let bond_hp = bond.players[&uid(2)].pet.unwrap().hp;

    assert!(
        bond_hp > plain_hp,
        "Pack Bond should soften the wound splash ({bond_hp} vs {plain_hp})"
    );
}

#[test]
fn all_classes_can_be_chosen_with_sane_stats() {
    for (i, class) in Class::ALL.iter().enumerate() {
        let mut s = world();
        let u = uid(i as u128 + 1);
        s.join(u);
        s.choose_class(u, *class);
        let p = &s.players[&u];
        assert_eq!(p.class, Some(*class), "class applied");
        assert!(p.max_hp() > 0, "{class:?} has health");
        assert!(p.max_resource > 0, "{class:?} has a resource pool");
        assert_eq!(p.hp, p.max_hp(), "{class:?} starts at full health");
    }
}

#[test]
fn archetype_is_gated_to_level_ten_then_persists_and_tunes_stats() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Too early: the choice is refused below the eligibility level.
    s.players.get_mut(&uid(1)).unwrap().level = ARCHETYPE_LEVEL - 1;
    s.choose_archetype(uid(1), 1); // Juggernaut (tank) at level 9
    assert!(
        s.players[&uid(1)].archetype.is_none(),
        "no archetype before the gate level"
    );
    // At the gate, the view offers exactly the two Warrior paths.
    s.players.get_mut(&uid(1)).unwrap().level = ARCHETYPE_LEVEL;
    let choices = s.snapshot().players[&uid(1)].archetype_choices.clone();
    assert_eq!(choices.len(), 2, "two paths offered at the gate");

    let hp_before = s.players[&uid(1)].max_hp();
    s.choose_archetype(uid(1), 1); // Juggernaut: tank, +12% max HP
    let chosen = s.players[&uid(1)].archetype.expect("archetype committed");
    assert_eq!(chosen.key, "juggernaut");
    assert!(
        s.players[&uid(1)].max_hp() > hp_before,
        "the tank max-HP bonus takes effect immediately"
    );
    // Locked in: a second attempt is a no-op.
    s.choose_archetype(uid(1), 0);
    assert_eq!(s.players[&uid(1)].archetype.unwrap().key, "juggernaut");
    // Once chosen, the offer list is empty so the gate releases.
    assert!(s.snapshot().players[&uid(1)].archetype_choices.is_empty());
}

#[test]
fn tank_archetype_mitigates_incoming_damage() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let p = s.players.get_mut(&uid(1)).unwrap();
    p.level = ARCHETYPE_LEVEL;
    // Strip armor so the only difference measured is archetype mitigation.
    let base_hp = 500;
    p.base_max_hp = base_hp;
    p.hp = base_hp;
    s.strike_player(uid(1), 100, DamageType::Physical, "test");
    let plain = base_hp - s.players[&uid(1)].hp;

    // Reset and pick the tank path, then take the identical blow.
    s.players.get_mut(&uid(1)).unwrap().hp = base_hp;
    s.choose_archetype(uid(1), 1); // Juggernaut (tank, 22% mitigation)
    s.players.get_mut(&uid(1)).unwrap().hp = base_hp;
    s.strike_player(uid(1), 100, DamageType::Physical, "test");
    let tanked = base_hp - s.players[&uid(1)].hp;
    assert!(
        tanked < plain,
        "tank archetype should reduce the hit ({tanked} vs {plain})"
    );
}

#[test]
fn monk_iron_body_blunts_physical_but_not_elemental() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Monk);
    let p = s.players.get_mut(&uid(1)).unwrap();
    let base_hp = 500;
    p.base_max_hp = base_hp;
    p.hp = base_hp;
    // A physical blow is blunted by Iron Body...
    s.strike_player(uid(1), 100, DamageType::Physical, "test");
    let physical = base_hp - s.players[&uid(1)].hp;
    // ...while an elemental blow of the same size lands in full.
    s.players.get_mut(&uid(1)).unwrap().hp = base_hp;
    s.strike_player(uid(1), 100, DamageType::Fire, "test");
    let fire = base_hp - s.players[&uid(1)].hp;
    assert!(
        physical < fire,
        "Iron Body should reduce physical but not fire ({physical} vs {fire})"
    );
}

#[test]
fn level_up_announces_concrete_gains_and_milestones() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.level = 1;
        p.xp = xp_for_level(5); // exactly enough for level 5
        // Pin scores to neutral so the final max-HP assertion isolates the
        // milestone bonus from a random (possibly negative) CON roll.
        p.scores = AbilityScores::default();
    }
    s.check_level_up(uid(1));
    assert_eq!(s.players[&uid(1)].level, 5);
    let texts: Vec<String> = s.players[&uid(1)]
        .log
        .iter()
        .map(|l| l.text.clone())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("Level 5 reached")),
        "each level is announced"
    );
    assert!(
        texts.iter().any(|t| t.contains("max HP")),
        "the concrete stat gain is shown"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("Milestone") && t.contains("Blooded")),
        "the fifth level is a named milestone"
    );
    // The milestone HP bonus is real and folded into max health.
    assert!(s.players[&uid(1)].max_hp() > Class::Warrior.stats_at(5).max_hp);
}

#[test]
fn join_then_choose_class_sets_stats() {
    let mut s = world();
    assert!(s.join(uid(1)));
    assert!(!s.is_classed(uid(1)));
    s.choose_class(uid(1), Class::Mage);
    assert!(s.is_classed(uid(1)));
    let p = s.players.get(&uid(1)).unwrap();
    assert_eq!(p.class, Some(Class::Mage));
    assert!(p.max_resource > 0);
    assert_eq!(p.hp, p.max_hp());
}

#[test]
fn recall_returns_to_the_town_square() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let home = s.world.start_room;
    s.move_player(uid(1), Dir::North); // 1 -> 2, off the square
    assert_ne!(s.players[&uid(1)].room, home, "should have left the square");
    s.recall(uid(1));
    assert_eq!(
        s.players[&uid(1)].room,
        home,
        "recall returns to the square"
    );
}

#[test]
fn waypoint_can_be_set_and_warped_to_from_anywhere() {
    // Community-reported pain point: the far run between Embergate and the
    // Frontier's deep levels for healing/resurrecting a downed pet. A player
    // should be able to mark a spot and warp back to it later, from anywhere.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let deep = super::super::world::frontier_entrance_room() + 400;
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = deep;
        p.gold = 1_000;
    }
    s.set_waypoint(uid(1));
    assert_eq!(
        s.players[&uid(1)].waypoint,
        Some(deep),
        "the waypoint marks the room it was set in"
    );

    // Now walk away (back to the square) and warp back.
    let home = s.world.start_room;
    s.players.get_mut(&uid(1)).unwrap().room = home;
    let gold_before = s.players[&uid(1)].gold;
    s.warp_to_waypoint(uid(1));
    assert_eq!(
        s.players[&uid(1)].room,
        deep,
        "warping returns to the marked waypoint, not just the town square"
    );
    assert_eq!(
        s.players[&uid(1)].gold,
        gold_before - WAYPOINT_WARP_COST,
        "warping to a waypoint costs gold, unlike the free word of recall"
    );
}

#[test]
fn warp_to_waypoint_refuses_without_one_set_or_without_enough_gold() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let start = s.players[&uid(1)].room;
    s.warp_to_waypoint(uid(1));
    assert_eq!(
        s.players[&uid(1)].room,
        start,
        "no waypoint set: nothing happens"
    );

    let deep = super::super::world::frontier_entrance_room() + 400;
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = deep;
        p.gold = 0;
    }
    s.set_waypoint(uid(1));
    s.players.get_mut(&uid(1)).unwrap().room = s.world.start_room;
    s.warp_to_waypoint(uid(1));
    assert_eq!(
        s.players[&uid(1)].room,
        s.world.start_room,
        "not enough gold: the warp is refused"
    );
}

#[test]
fn first_dungeon_descent_requires_elder_treant_title() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = FIRST_DUNGEON_GATE_FROM;

    s.move_player(uid(1), Dir::Down);
    assert_eq!(s.players[&uid(1)].room, FIRST_DUNGEON_GATE_FROM);
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("Elder Treant")),
        "gate should point the player at the first boss"
    );

    s.players
        .get_mut(&uid(1))
        .unwrap()
        .titles
        .push(FIRST_DUNGEON_GATE_TITLE.to_string());
    s.move_player(uid(1), Dir::Down);
    assert_eq!(s.players[&uid(1)].room, FIRST_DUNGEON_GATE_TO);
}

#[test]
fn living_dark_regions_require_archdemon_title() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = super::super::world::TASMANIA_SQUARE;
    let dir = dir_to_zone(
        &s,
        super::super::world::TASMANIA_SQUARE,
        "The Sunken Catacombs",
    );

    s.move_player(uid(1), dir);
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::TASMANIA_SQUARE
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("Archdemon Mal'gareth")),
        "gate should point players at the Archdemon first"
    );

    s.players
        .get_mut(&uid(1))
        .unwrap()
        .titles
        .push(FRONTIER_GATE_TITLE.to_string());
    s.move_player(uid(1), dir);
    assert_eq!(
        s.world.room(s.players[&uid(1)].room).map(|room| room.zone),
        Some("The Sunken Catacombs")
    );
}

#[test]
fn frontier_entrance_requires_archdemon_title_then_confirming_move() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let home = s.world.start_room;
    s.players.get_mut(&uid(1)).unwrap().room = home;

    s.move_player(uid(1), Dir::Down);
    assert_eq!(
        s.players[&uid(1)].room,
        home,
        "Frontier should be locked before the Archdemon falls"
    );
    assert!(!s.players[&uid(1)].frontier_descent_pending);
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("Archdemon Mal'gareth")),
        "gate should point the player at the authored final boss"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("three living-dark seals")),
        "gate should mention the full Frontier unlock chain"
    );

    s.players
        .get_mut(&uid(1))
        .unwrap()
        .titles
        .push(FRONTIER_GATE_TITLE.to_string());
    s.move_player(uid(1), Dir::Down);
    assert_eq!(
        s.players[&uid(1)].room,
        home,
        "Frontier should still be locked before the living-dark bosses fall"
    );
    assert!(!s.players[&uid(1)].frontier_descent_pending);
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("living-dark seals")),
        "gate should point the player at the three side regions"
    );

    grant_frontier_unlock_titles(&mut s, uid(1));
    s.move_player(uid(1), Dir::Down);
    assert_eq!(
        s.players[&uid(1)].room,
        home,
        "first descent should warn without moving"
    );
    assert!(s.players[&uid(1)].frontier_descent_pending);
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.text.contains("older, meaner country")),
        "warning should explain the Frontier danger"
    );

    s.move_player(uid(1), Dir::Down);
    assert_eq!(s.players[&uid(1)].room, frontier_entrance_room());
    assert!(!s.players[&uid(1)].frontier_descent_pending);
}

#[test]
fn frontier_warning_clears_when_moving_elsewhere() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = s.world.start_room;
    grant_frontier_unlock_titles(&mut s, uid(1));

    s.move_player(uid(1), Dir::Down);
    assert!(s.players[&uid(1)].frontier_descent_pending);
    s.move_player(uid(1), Dir::South);
    assert_eq!(s.players[&uid(1)].room, 5);
    assert!(!s.players[&uid(1)].frontier_descent_pending);
}

#[test]
fn town_square_exit_labels_mark_frontier_as_dangerous() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = s.world.start_room;

    let snap = s.snapshot();
    let view = snap.players.get(&uid(1)).expect("player view");
    assert!(
        view.exits.iter().any(|(dir, label)| {
            *dir == Dir::Down && label.as_str() == "down (dangerous Frontier)"
        }),
        "Town Square should visibly mark the Frontier exit"
    );
}

#[test]
fn following_pulls_a_companion_along() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    // uid(1) follows the only other adventurer in the square.
    s.follow_toggle(uid(1));
    assert_eq!(s.players[&uid(1)].following, Some(uid(2)));
    // When uid(2) walks north, uid(1) is dragged along to the same room.
    s.move_player(uid(2), Dir::North);
    let dest = s.players[&uid(2)].room;
    assert_eq!(s.players[&uid(1)].room, dest);
    // Toggling again stops the follow.
    s.follow_toggle(uid(1));
    assert_eq!(s.players[&uid(1)].following, None);
}

#[test]
fn follow_to_rejects_target_no_longer_in_room() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);

    s.move_player(uid(2), Dir::North);
    s.follow_to(uid(1), uid(2));

    assert_eq!(s.players[&uid(1)].following, None);
}

#[test]
fn stop_follow_clears_absent_target() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);

    s.follow_to(uid(1), uid(2));
    assert_eq!(s.players[&uid(1)].following, Some(uid(2)));
    if let Some(p) = s.players.get_mut(&uid(2)) {
        p.room = 2;
    }
    s.stop_follow(uid(1));

    assert_eq!(s.players[&uid(1)].following, None);
}

#[test]
fn hunting_small_game_grants_xp_then_cools_down() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    let before = s.players[&uid(1)].xp;
    // Room 600 (the Greatroad) hosts a fat marsh-rat (Game).
    assert!(s.try_hunt(uid(1), 600), "should catch the game");
    assert!(s.players[&uid(1)].xp > before, "hunting grants xp");
    // It has slipped away, so an immediate second hunt finds nothing.
    assert!(!s.try_hunt(uid(1), 600), "game is on cooldown");
}

#[test]
fn a_boon_creature_mends_on_arrival() {
    // The bug: Mend used to be a small partial heal, so fully healing meant
    // walking in and out of the room over and over. It should just heal you
    // all the way in one visit.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let max = s.players[&uid(1)].max_hp();
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.hp = 1;
        // Room 1, the town square, is home to the hearth-cat (Mend boon).
        p.room = 1;
    }
    s.apply_critter_perks(uid(1));
    assert_eq!(
        s.players[&uid(1)].hp,
        max,
        "the hearth-cat should mend you all the way, not partway"
    );
}

#[test]
fn unclassed_player_cannot_move_or_fight() {
    let mut s = world();
    s.join(uid(1));
    let start = s.players[&uid(1)].room;
    s.move_player(uid(1), Dir::South);
    assert_eq!(s.players[&uid(1)].room, start);
    s.engage(uid(1));
    assert!(s.players[&uid(1)].target.is_none());
}

#[test]
fn buying_costs_gold_and_adds_item() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores::default();
    // Walk to the smith (room 3, east of square).
    s.move_player(uid(1), Dir::East);
    assert_eq!(s.players[&uid(1)].room, 3);
    let before = s.players[&uid(1)].gold;
    s.buy(uid(1), 1001); // Iron Longsword, 80g
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, before - 80);
    assert!(p.inventory.contains(&1001));
}

#[test]
fn waystone_travel_teleports_between_portals() {
    use super::super::archipelago::{island_entrance, village_room};
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Room 1 (Embergate square) has the town waystone.
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.travel(uid(1), village_room(0));
    assert_eq!(
        s.players[&uid(1)].room,
        village_room(0),
        "steps through to Lantern Cove"
    );
    // From a village waystone, hop to an island landing.
    s.travel(uid(1), island_entrance(3));
    assert_eq!(s.players[&uid(1)].room, island_entrance(3));
}

#[test]
fn travel_needs_a_waystone_and_a_real_destination() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Walk to a plain room with no portal, then try to travel: refused.
    s.move_player(uid(1), Dir::North); // the Gilded Flagon (room 2), no portal
    let here = s.players[&uid(1)].room;
    s.travel(uid(1), super::super::archipelago::village_room(0));
    assert_eq!(s.players[&uid(1)].room, here, "no waystone, no travel");
}

#[test]
fn the_ways_only_carry_you_where_you_have_already_stood() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Stand on Embergate's town waystone the way a character walking out of
    // Wayfarer's Hollow does: arriving is what marks the square visited, and
    // no waystone can be used without standing on it first.
    let p = s.players.get_mut(&uid(1)).unwrap();
    p.room = 1;
    Arc::make_mut(&mut p.visited).insert(1);
    s.travel(uid(1), super::super::world::LAKES_BASE);
    assert_eq!(
        s.players[&uid(1)].room,
        1,
        "an ungated land you have never walked to is not on the network yet"
    );
    // Having stood at the landing once, it answers forever after.
    let p = s.players.get_mut(&uid(1)).unwrap();
    Arc::make_mut(&mut p.visited).insert(super::super::world::LAKES_BASE);
    s.travel(uid(1), super::super::world::LAKES_BASE);
    assert_eq!(s.players[&uid(1)].room, super::super::world::LAKES_BASE);
    // And the way home is always open, since that is where you began.
    s.travel(uid(1), 1);
    assert_eq!(s.players[&uid(1)].room, 1);
}

#[test]
fn a_gate_title_alone_does_not_open_the_ways() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    // Crowned Bane of Yssgar, but never once through the wound below his
    // chamber: the title is permission to walk in, not to skip the walk.
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .titles
        .push(KAELMYR_GATE_TITLE.to_string());
    s.travel(uid(1), super::super::world::KAELMYR_BASE);
    assert_eq!(
        s.players[&uid(1)].room,
        1,
        "the Ways carry no progression rules of their own"
    );
}

#[test]
fn the_archipelago_answers_without_a_title_or_a_prior_visit() {
    use super::super::archipelago::{island_entrance, village_room};
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    // Villages and island landings have no directional exits at all, so a
    // visited rule would orphan them. They stay open to a level 1, Lv100
    // island bosses and all.
    s.travel(uid(1), village_room(0));
    assert_eq!(s.players[&uid(1)].room, village_room(0));
    s.travel(uid(1), island_entrance(3));
    assert_eq!(s.players[&uid(1)].room, island_entrance(3));
}

#[test]
fn retreat_slips_to_the_nearest_haven_only_out_of_combat() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Drop the adventurer a few cells deep into the first Frontier zone.
    s.players.get_mut(&uid(1)).unwrap().room = 2_003;
    assert!(
        !s.world.room(2_003).unwrap().safe,
        "test premise: an unsafe maze cell"
    );
    // Mid-fight, retreat refuses.
    s.players.get_mut(&uid(1)).unwrap().target = Some(999);
    s.retreat_to_haven(uid(1));
    assert_eq!(s.players[&uid(1)].room, 2_003, "no retreating mid-fight");
    // Out of combat it ends at the closest safe room: the zone's own gate.
    s.players.get_mut(&uid(1)).unwrap().target = None;
    s.retreat_to_haven(uid(1));
    let room = s.players[&uid(1)].room;
    assert!(s.world.room(room).unwrap().safe, "retreat ends in a haven");
    assert_eq!(room, 2_000, "the nearest haven is the zone's entrance");
    // Already safe: retreating again goes nowhere.
    s.retreat_to_haven(uid(1));
    assert_eq!(s.players[&uid(1)].room, 2_000);
}

#[test]
fn class_cannot_be_changed_once_chosen() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // A second choice must be ignored - no re-classing mid-adventure.
    s.choose_class(uid(1), Class::Mage);
    assert_eq!(
        s.players[&uid(1)].class,
        Some(Class::Warrior),
        "class is locked in once chosen"
    );
}

#[test]
fn sell_batch_dumps_junk_but_keeps_upgrades_and_potions() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.move_player(uid(1), Dir::East); // to the smithy (room 3), a merchant
    assert_eq!(s.players[&uid(1)].room, 3);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.equipped.clear();
        // A weak weapon worn, a stronger one loose (an upgrade to keep), a
        // weaker one loose (junk), and a potion (must survive).
        p.equipped.insert(Slot::Weapon, 1001); // Iron Longsword
        p.inventory = vec![1004, 1000, 1300]; // strong wpn, weak wpn, potion
        p.gold = 0;
    }
    s.sell_batch(uid(1), SellBatch::NonUpgrades);
    let p = &s.players[&uid(1)];
    assert!(p.inventory.contains(&1300), "keeps the potion");
    assert!(!p.inventory.contains(&1000), "sells the weaker weapon");
    assert!(
        p.inventory.contains(&1004) || p.equipped.values().any(|v| *v == 1004),
        "keeps the upgrade weapon"
    );
    assert!(p.gold > 0, "selling junk earns gold");
}

#[test]
fn poisons_survive_every_batch_sell_mode_and_group_under_consumables() {
    // The bug: poisons were classified `Valuable`, so any of the three
    // batch-sell hotkeys (not just "sell all") could wipe them out right
    // alongside pure sell-fodder gems and raw materials, and they showed up
    // grouped with junk in the "Valuables" category instead of with the
    // other pack items you actually use.
    let poison = super::super::items::poison_id(1);
    assert_eq!(
        item_category(&item(poison).unwrap().kind),
        "Consumables",
        "a poison should not be grouped with pure sell-fodder valuables"
    );

    for kind in [SellBatch::All, SellBatch::Common, SellBatch::NonUpgrades] {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        s.players.get_mut(&uid(1)).unwrap().room = 1;
        s.move_player(uid(1), Dir::East); // the smithy (room 3), a merchant
        s.players.get_mut(&uid(1)).unwrap().inventory = vec![poison];

        s.sell_batch(uid(1), kind);
        assert!(
            s.players[&uid(1)].inventory.contains(&poison),
            "{kind:?} must never dump a poison"
        );
    }
}

#[test]
fn buying_a_companion_costs_gold_and_sets_a_pet() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Embergate's square (room 1) has a stable.
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.players.get_mut(&uid(1)).unwrap().gold = 1000;
    s.buy_pet(uid(1), "war_hound");
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, 1000 - 120, "the war hound's price is spent");
    assert_eq!(
        p.pet.map(|pet| pet.species.key),
        Some("war_hound"),
        "the companion is now at your heel"
    );
    // Too poor for the pricey drake: the purchase is refused.
    s.players.get_mut(&uid(1)).unwrap().gold = 10;
    s.buy_pet(uid(1), "emberdrake");
    assert_eq!(
        s.players[&uid(1)].pet.map(|p| p.species.key),
        Some("war_hound"),
        "an unaffordable purchase changes nothing"
    );
}

#[test]
fn a_companion_piles_onto_your_target_in_combat() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    // Give the fighter a companion (the stable is back in town).
    let species = super::super::pets::pet_species_by_key("dire_wolf").unwrap();
    s.players.get_mut(&uid(1)).unwrap().pet = Some(super::super::pets::Pet::new(species, 0));
    let before = s.mobs[&mob_id].hp;
    s.tick();
    let after = s.mobs[&mob_id].hp;
    assert!(
        after <= before - species.base_attack,
        "the companion's bite adds to the damage dealt"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("tears into")),
        "the companion's attack is logged"
    );
}

#[test]
fn a_companion_is_downed_when_its_owner_is_battered() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let species = super::super::pets::pet_species_by_key("moor_hawk").unwrap();
    s.players.get_mut(&uid(1)).unwrap().pet = Some(super::super::pets::Pet::new(species, 0));
    // Give the owner a deep health pool so they survive the barrage; the pet
    // shares each survivable blow and is eventually beaten down.
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.base_max_hp = 10_000;
        p.hp = 10_000;
    }
    for _ in 0..10 {
        s.strike_player(uid(1), 40, DamageType::Physical, "a test foe");
    }
    let pet = s.players[&uid(1)].pet.expect("still owns the pet");
    assert!(!s.players[&uid(1)].dead, "the owner survives the barrage");
    assert!(pet.downed, "a battered companion is downed (hp={})", pet.hp);
    assert_eq!(pet.hp, 0);
}

#[test]
fn feeding_at_a_stable_revives_and_strengthens_a_companion() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let species = super::super::pets::pet_species_by_key("war_hound").unwrap();
    let mut pet = super::super::pets::Pet::new(species, 0);
    pet.downed = true;
    pet.hp = 0;
    s.players.get_mut(&uid(1)).unwrap().pet = Some(pet);
    s.players.get_mut(&uid(1)).unwrap().gold = 500;
    s.feed_pet(uid(1)); // Embergate square has a stable
    let pet = s.players[&uid(1)].pet.unwrap();
    assert!(!pet.downed, "feeding rouses a downed companion");
    assert_eq!(pet.hp, pet.max_hp(), "and heals it to full");
    assert!(pet.loyalty_xp > 0, "and raises its loyalty");
    assert_eq!(s.players[&uid(1)].gold, 500 - PET_FEED_COST);
}

#[test]
fn feeding_works_anywhere_not_just_at_a_stable() {
    // Reported pain point: a pet going down mid-fight deep in the Frontier
    // used to be stuck downed until a long walk back to a capital's Stable.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let species = super::super::pets::pet_species_by_key("war_hound").unwrap();
    let mut pet = super::super::pets::Pet::new(species, 0);
    pet.downed = true;
    pet.hp = 0;
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.pet = Some(pet);
        p.gold = 500;
        p.room = super::super::world::frontier_entrance_room() + 400; // deep in the Frontier
    }
    s.feed_pet(uid(1));
    let pet = s.players[&uid(1)].pet.unwrap();
    assert!(
        !pet.downed,
        "feeding revives a downed companion far from any stable"
    );
    assert_eq!(pet.hp, pet.max_hp());
}

// The forest gate (entrance) room of Broceliande zone 0, where the easiest
// tameable beasts roam.
fn broceliande_beast_room() -> RoomId {
    super::super::world::BROCELIANDE_BASE
}

#[test]
fn taming_a_beast_makes_it_your_companion_and_trains_the_trade() {
    let mut s = world();
    let user_id = uid(1);
    s.join(user_id);
    s.choose_class(user_id, Class::Ranger);
    s.players.get_mut(&user_id).unwrap().room = broceliande_beast_room();
    // The easiest beast in the room is the first tameable species.
    let beasts = super::super::taming::beasts_at(broceliande_beast_room());
    assert!(!beasts.is_empty(), "beasts roam the first forest gate");
    let beast_index = beasts[0].species;
    let species = super::super::taming::beast_species(beast_index);
    let cooldown_key = (user_id, beast_index);
    let baseline_xp = super::super::skills::xp_for_skill_level(species.tame_level);
    s.players.get_mut(&user_id).unwrap().taming_xp = baseline_xp;

    // Exercise both real RNG outcomes. The old retry loop was flaky because a
    // failed roll records a 30-second cooldown, so its immediate retries never
    // rolled again. After proving that behavior, remove only that transient test
    // token; after success, reset its side effects if a failure is still needed.
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut attempt_count = 0;
    while success_count == 0 || failure_count == 0 {
        attempt_count += 1;
        assert!(
            attempt_count <= 10_000,
            "real taming RNG did not produce both outcomes: {success_count} successes, {failure_count} failures"
        );

        let before_xp = s.players[&user_id].taming_xp;
        s.tame(user_id, 0);

        if let Some(pet) = s.players[&user_id].pet {
            success_count += 1;
            assert_eq!(pet.species.key, species.key);
            assert!(
                pet.species.is_tameable(),
                "the companion is a tamed wild beast"
            );
            assert_eq!(
                s.players[&user_id].taming_xp,
                before_xp + tame_xp(species) as i64,
                "taming trains Animal Taming xp"
            );
            assert!(!s.tame_cooldowns.contains_key(&cooldown_key));
            assert!(
                s.players[&user_id]
                    .log
                    .iter()
                    .any(|line| line.text.contains("You've earned its trust!"))
            );

            if failure_count == 0 {
                let player = s.players.get_mut(&user_id).unwrap();
                player.pet = None;
                player.taming_xp = baseline_xp;
            }
        } else {
            failure_count += 1;
            assert_eq!(s.players[&user_id].taming_xp, before_xp);
            let failed_at = *s
                .tame_cooldowns
                .get(&cooldown_key)
                .expect("a failed tame records the tamer-beast cooldown");
            assert!(failed_at.elapsed() < TAME_COOLDOWN);
            assert!(
                s.players[&user_id]
                    .log
                    .last()
                    .is_some_and(|line| line.text.contains("shies, then bolts"))
            );

            s.tame(user_id, 0);
            assert!(s.players[&user_id].pet.is_none());
            assert_eq!(s.players[&user_id].taming_xp, before_xp);
            assert_eq!(s.tame_cooldowns.get(&cooldown_key), Some(&failed_at));
            assert!(
                s.players[&user_id]
                    .log
                    .last()
                    .unwrap()
                    .text
                    .contains("still wary")
            );

            if success_count == 0 {
                assert_eq!(s.tame_cooldowns.remove(&cooldown_key), Some(failed_at));
            }
        }
    }

    assert!(success_count > 0);
    assert!(failure_count > 0);
}

#[test]
fn an_underskilled_tamer_cannot_take_a_great_beast() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    // Deep in Broceliande the great beasts need a near-master tamer; a fresh
    // Ranger has no taming training, so the attempt is refused outright.
    let deep =
        super::super::world::BROCELIANDE_BASE + 19 * super::super::world::BROCELIANDE_ZONE_STRIDE;
    s.players.get_mut(&uid(1)).unwrap().room = deep;
    let beasts = super::super::taming::beasts_at(deep);
    assert!(!beasts.is_empty(), "great beasts roam the deep gate");
    s.tame(uid(1), 0);
    let p = &s.players[&uid(1)];
    assert!(p.pet.is_none(), "an under-level tamer takes nothing");
    assert_eq!(p.taming_xp, 0, "and earns no taming xp");
    assert!(
        p.log.iter().any(|l| l.text.contains("beyond your skill")),
        "the refusal explains the level gate"
    );
}

#[test]
fn a_leveled_companions_auto_skills_fire_in_combat() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    // A well-fed, high-loyalty companion has unlocked its auto-skills.
    let species = super::super::pets::pet_species_by_key("dire_wolf").unwrap();
    let pet = super::super::pets::Pet::new(species, super::super::pets::LOYALTY_PER_LEVEL * 5);
    assert!(pet.level() >= 3, "the fed companion has unlocked skills");
    s.players.get_mut(&uid(1)).unwrap().pet = Some(pet);
    // Give the foe a big pool so it survives to show the extra hits.
    s.mobs.get_mut(&mob_id).unwrap().hp = 5000;
    s.mobs.get_mut(&mob_id).unwrap().spawn.max_hp = 5000;
    // Run a few rounds so a skill comes off cooldown and fires.
    let mut fired = false;
    for _ in 0..5 {
        s.tick();
        if s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("Savage Bite") || l.text.contains("rips into"))
        {
            fired = true;
            break;
        }
    }
    assert!(fired, "a leveled companion's auto-skill fires in combat");
}

#[test]
fn buying_a_deed_claims_a_home_and_only_one_per_name() {
    use super::super::housing::{HOUSING_BASE, TIERS};
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Stand at the clerk in Hearthward Close.
    s.players.get_mut(&uid(1)).unwrap().room = HOUSING_BASE;
    s.players.get_mut(&uid(1)).unwrap().gold = 50_000;
    s.buy_deed(uid(1), 0); // the Wattle Hut
    assert_eq!(s.owned_plot(uid(1)), Some(0), "the hut deed is held");
    assert_eq!(
        s.players[&uid(1)].gold,
        50_000 - TIERS[0].price,
        "the deed price is spent"
    );
    // One home to a name: a second deed is refused.
    s.buy_deed(uid(1), 4);
    assert_eq!(s.owned_plot(uid(1)), Some(0), "still only the hut");
}

#[test]
fn furniture_can_be_placed_only_in_a_home_you_own() {
    use super::super::housing::{HOUSING_BASE, plot_base};
    let mut s = world();
    // Owner claims the hut (plot 0).
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = HOUSING_BASE;
    s.players.get_mut(&uid(1)).unwrap().gold = 50_000;
    s.buy_deed(uid(1), 0);
    let hut = plot_base(0);
    s.players.get_mut(&uid(1)).unwrap().room = hut;
    s.buy_furniture(uid(1), "oak_stool");
    assert_eq!(
        s.house_furniture.get(&hut).map(|v| v.len()),
        Some(1),
        "the stool is set down in the owner's home"
    );

    // A visitor may walk in (shared world) but cannot furnish it.
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.players.get_mut(&uid(2)).unwrap().room = hut;
    s.players.get_mut(&uid(2)).unwrap().gold = 50_000;
    s.buy_furniture(uid(2), "carved_armchair");
    assert_eq!(
        s.house_furniture.get(&hut).map(|v| v.len()),
        Some(1),
        "a visitor cannot place furniture in someone else's home"
    );
}

#[test]
fn saved_house_furniture_is_replaced_and_deduped_on_load() {
    use super::super::housing::{HOUSING_BASE, plot_base};
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = HOUSING_BASE;
    s.players.get_mut(&uid(1)).unwrap().gold = 50_000;
    s.buy_deed(uid(1), 0);
    let hut = plot_base(0);
    s.players.get_mut(&uid(1)).unwrap().room = hut;
    s.buy_furniture(uid(1), "oak_stool");

    let mut saved = s.export_saved(uid(1)).expect("character is saveable");
    saved.house_furniture.push((hut, "oak_stool".to_string()));

    s.hydrate(uid(1), &saved);
    s.hydrate(uid(1), &saved);

    assert_eq!(
        s.house_furniture.get(&hut).map(|v| v.len()),
        Some(1),
        "loading the same save must not append duplicate furniture"
    );
    assert_eq!(
        s.export_saved(uid(1))
            .expect("character is saveable")
            .house_furniture
            .len(),
        1,
        "exported save must stay deduped"
    );
}

#[test]
fn appearance_cycles_wrap_and_compose_the_bio() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Cycling the Build field forward changes the composed bio.
    let before = appearance::compose_bio(&s.players[&uid(1)].appearance);
    s.cycle_appearance(uid(1), 0, 1);
    let after = appearance::compose_bio(&s.players[&uid(1)].appearance);
    assert_ne!(before, after, "cycling a field changes the bio");
    // Cycling back returns to the original selection (wrapping arithmetic).
    s.cycle_appearance(uid(1), 0, -1);
    assert_eq!(s.players[&uid(1)].appearance[0], 0, "cycle wraps cleanly");
    // An out-of-range field is ignored, not a panic.
    s.cycle_appearance(uid(1), 99, 1);
}

#[test]
fn the_sundered_reaches_adds_twenty_new_bosses() {
    let s = world();
    let reaches_bosses = s
        .mobs
        .values()
        .filter(|m| super::super::world::is_reaches_room(m.spawn.home) && m.spawn.boss)
        .count();
    assert_eq!(reaches_bosses, 20, "one boss per Reaches zone");
}

#[test]
fn every_capital_has_a_stable() {
    use super::super::world::{MATLATESH_SQUARE, MELVANALA_SQUARE, TASMANIA_SQUARE};
    for square in [1, TASMANIA_SQUARE, MELVANALA_SQUARE, MATLATESH_SQUARE] {
        assert!(
            features_at(square)
                .iter()
                .any(|f| f.kind == FeatureKind::Stable),
            "capital room {square} should have a stable"
        );
    }
}

#[test]
fn bank_toggles_between_deposit_and_withdraw_all_gold() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;

    // Find the banker's grille by kind - feature indices shift as scenery
    // (e.g. a stable) is added to the square.
    let bank = features_at(s.players[&uid(1)].room)
        .iter()
        .position(|f| f.kind == FeatureKind::Bank)
        .expect("the town square has a bank");

    s.interact(uid(1), bank);
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, 0);
    assert_eq!(p.banked_gold, STARTING_GOLD);

    s.interact(uid(1), bank);
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, STARTING_GOLD);
    assert_eq!(p.banked_gold, 0);
}

#[test]
fn normal_death_loses_carried_gold_but_not_banked_gold() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.gold = 1000;
        p.banked_gold = 500;
    }

    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");

    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, 800);
    assert_eq!(p.banked_gold, 500);
    assert!(p.respawn_at.is_some());
    assert!(
        p.log
            .iter()
            .any(|line| line.text.contains("lose 200 carried gold")),
        "death log should explain the gold loss"
    );
}

#[test]
fn equipping_a_weapon_raises_attack() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let base = s.players[&uid(1)].attack();
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006); // greatsword +16
    s.equip(uid(1), 1006);
    assert!(s.players[&uid(1)].attack() > base);
}

#[test]
fn taking_off_gear_returns_it_to_the_pack_and_drops_its_stats() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let base = s.players[&uid(1)].attack();
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006); // greatsword +16
    s.equip(uid(1), 1006);
    assert!(s.players[&uid(1)].attack() > base, "worn gear counts");

    s.unequip(uid(1), 1006);
    let p = &s.players[&uid(1)];
    assert!(
        p.inventory.contains(&1006),
        "the greatsword goes back in the pack"
    );
    assert!(
        p.equipped.values().all(|id| *id != 1006),
        "and is no longer worn"
    );
    assert_eq!(p.attack(), base, "its attack bonus comes off with it");
}

#[test]
fn taking_off_gear_you_are_not_wearing_does_nothing() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006);
    let before = s.players[&uid(1)].inventory.clone();
    s.unequip(uid(1), 1006); // in the pack, not on the body
    assert_eq!(
        s.players[&uid(1)].inventory,
        before,
        "a pack item is not duplicated by taking it off"
    );
}

#[test]
fn worn_gear_cannot_be_sold_out_from_under_you() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Stand at Embergate's merchant so the sell path gets past the shop gate.
    s.players.get_mut(&uid(1)).unwrap().room = 3;
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006);
    s.equip(uid(1), 1006);
    let gold = s.players[&uid(1)].gold;

    s.sell(uid(1), 1006);
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, gold, "no silent sale of worn gear");
    assert!(p.equipped.values().any(|id| *id == 1006), "still worn");
    assert!(
        p.log.iter().any(|l| l.text.contains("take off")),
        "and the refusal says why: {:?}",
        p.log.iter().map(|l| &l.text).collect::<Vec<_>>()
    );
}

#[test]
fn inventory_and_shop_rows_carry_the_items_own_description() {
    // Item.desc existed but was never plumbed into any view - inventory and
    // shop rows showed stats with no flavor/description text at all.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 3; // Embergate's merchant
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006);
    let expected = item(1006).unwrap().desc;
    assert!(!expected.is_empty(), "the fixture item has real desc text");

    let view = s.snapshot().players[&uid(1)].clone();
    let inv_row = view
        .inventory
        .iter()
        .find(|it| it.item_id == 1006)
        .expect("the item appears in the inventory view");
    assert_eq!(inv_row.desc, expected);

    let shop_row = view
        .shop
        .expect("a merchant stands here")
        .entries
        .iter()
        .find(|e| e.item_id == 1006)
        .map(|e| e.desc);
    if let Some(shop_desc) = shop_row {
        assert_eq!(
            shop_desc, expected,
            "the shop row should carry the same desc"
        );
    }
}

#[test]
fn a_loose_duplicate_of_worn_gear_can_still_be_sold() {
    // The bug: selling required unequipping first even when a second, loose
    // copy of the same item sat right there in the pack.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 3;
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores::default();
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006);
    s.equip(uid(1), 1006);
    // A second copy, still loose in the pack.
    s.players.get_mut(&uid(1)).unwrap().inventory.push(1006);
    let gold = s.players[&uid(1)].gold;
    let price = item(1006).unwrap().sell_price();

    s.sell(uid(1), 1006);
    let p = &s.players[&uid(1)];
    assert_eq!(p.gold, gold + price, "the loose copy sold");
    assert!(
        p.equipped.values().any(|id| *id == 1006),
        "the worn copy stayed on"
    );
    assert!(
        !p.inventory.contains(&1006),
        "the loose copy left the pack: {:?}",
        p.inventory
    );
}

// ---- character slots (multiple saved characters per account) -------------

#[test]
fn empty_slot_summary_reads_as_unoccupied() {
    let summary = SlotSummary::empty(3);
    assert_eq!(summary.slot, 3);
    assert!(!summary.occupied);
    assert_eq!(summary.class, None);
    assert_eq!(summary.level, 0);
}

#[test]
fn slot_summary_from_a_save_reads_its_class_and_level() {
    let saved = SavedCharacter::from_json(&serde_json::json!({
        "class": "warrior",
        "level": 14,
    }))
    .expect("well-formed blob parses");

    let summary = SlotSummary::from_saved(2, &saved);
    assert_eq!(summary.slot, 2);
    assert!(summary.occupied);
    assert_eq!(summary.class, Some(Class::Warrior));
    assert_eq!(summary.level, 14);
}

#[test]
fn slot_summary_before_a_class_is_chosen_still_reads_as_occupied() {
    // A character mid-tutorial (joined, never picked a class) has a real save
    // to resume or delete, even with no class to show yet.
    let saved = SavedCharacter::from_json(&serde_json::json!({ "level": 1 }))
        .expect("well-formed blob parses");

    let summary = SlotSummary::from_saved(0, &saved);
    assert!(summary.occupied);
    assert_eq!(summary.class, None);
}

#[test]
fn rogue_opening_strike_is_flagged_then_consumed() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Rogue);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    // Move to a combat room with a mob (room 6, goblin) and engage.
    s.move_player(uid(1), Dir::South);
    s.move_player(uid(1), Dir::South);
    s.engage(uid(1));
    assert!(s.players[&uid(1)].opening_strike, "rogue arms opening crit");
    // One tick resolves the auto-attack and consumes the opening strike.
    s.tick();
    assert!(!s.players[&uid(1)].opening_strike, "opening crit is spent");
}

#[test]
fn combat_tick_logs_player_auto_attack() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    // Move to a combat room with a mob (room 6, goblin) and engage.
    s.move_player(uid(1), Dir::South);
    s.move_player(uid(1), Dir::South);
    s.engage(uid(1));

    s.tick();

    let log = &s.players[&uid(1)].log;
    assert!(
        log.iter().any(|line| line.kind == LogKind::Combat
            && (line.text.starts_with("You strike ") || line.text.starts_with("You crush into "))),
        "auto-attacks should be visible in the combat log"
    );
}

#[test]
fn field_mode_logs_discoveries_only_and_classic_logs_every_arrival() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = 1;
        // Room 1 is "known land" for this test's purposes, same as it was
        // when a fresh join placed a character there directly.
        std::sync::Arc::make_mut(&mut p.visited).insert(1);
    }

    // First footfall in field mode (rpg on by default): a discovery line plus
    // the room's prose, which has nowhere else to live on the field layout.
    s.move_player(uid(1), Dir::North);
    let flagon_desc = s
        .world
        .room(s.players[&uid(1)].room)
        .expect("player stands in a real room")
        .desc
        .to_string();
    let travel_texts = |s: &WorldState| -> Vec<String> {
        s.players[&uid(1)]
            .log
            .iter()
            .filter(|l| l.kind == LogKind::Travel)
            .map(|l| l.text.clone())
            .collect()
    };
    let after_discovery = travel_texts(&s);
    assert!(
        after_discovery.contains(&"You find Embergate - The Gilded Flagon.".to_string()),
        "first footfall should announce the discovery: {after_discovery:?}"
    );
    assert!(
        after_discovery.contains(&flagon_desc),
        "the discovery should carry the room's prose into the feed"
    );

    // Stepping back through known land in field mode says nothing: the @ on
    // the field and the Here panel already tell the story.
    s.move_player(uid(1), Dir::South);
    assert_eq!(
        travel_texts(&s),
        after_discovery,
        "a field-mode revisit must not add travel lines"
    );

    // Classic mode keeps the per-step breadcrumb, discovery or not.
    s.players.get_mut(&uid(1)).unwrap().rpg_mode = false;
    s.move_player(uid(1), Dir::North);
    assert_eq!(
        travel_texts(&s).last().map(String::as_str),
        Some("Arrived at Embergate - The Gilded Flagon."),
        "classic mode should keep its room-visit breadcrumb"
    );
}

#[test]
fn warrior_does_not_arm_opening_strike() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.move_player(uid(1), Dir::South);
    s.move_player(uid(1), Dir::South);
    s.engage(uid(1));
    assert!(
        !s.players[&uid(1)].opening_strike,
        "only rogues get the crit"
    );
}

#[test]
fn warrior_survives_first_lethal_blow() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    assert_eq!(
        s.players[&uid(1)].hp,
        1,
        "Unbreakable should save the warrior"
    );
    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    assert!(s.players[&uid(1)].respawn_at.is_some(), "second blow falls");
}

#[test]
fn a_lethal_blow_leaves_a_lingering_corpse_not_an_instant_temple_trip() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage); // no Warrior death-save
    let where_fell = s.players[&uid(1)].room;
    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    let p = &s.players[&uid(1)];
    assert!(p.dead, "the player is a corpse");
    assert_eq!(p.hp, 0, "a corpse has no health");
    assert_eq!(p.room, where_fell, "the corpse stays where it fell");
    assert!(
        p.respawn_at.is_some(),
        "an auto-release deadline is armed, not an instant temple trip"
    );
    assert_ne!(
        p.room, TEMPLE_ROOM,
        "death no longer blinks you to the temple"
    );
}

#[test]
fn releasing_sends_a_corpse_to_the_temple_restored() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    assert!(s.players[&uid(1)].dead);
    s.release_to_temple(uid(1));
    let p = &s.players[&uid(1)];
    assert!(!p.dead, "release clears the corpse state");
    assert_eq!(p.room, TEMPLE_ROOM, "you wake at the temple");
    assert_eq!(p.hp, p.max_hp(), "restored to full");
    assert!(p.respawn_at.is_none());
}

#[test]
fn a_healer_resurrects_a_corpse_in_place_but_others_cannot() {
    let mut s = world();
    // Caster who can rez (Cleric), victim (Mage), and an incapable bystander
    // (Rogue) - all gathered in one room.
    s.join(uid(1));
    s.choose_class(uid(1), Class::Cleric);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.join(uid(3));
    s.choose_class(uid(3), Class::Rogue);
    let room = s.players[&uid(1)].room;
    for who in [uid(2), uid(3)] {
        s.players.get_mut(&who).unwrap().room = room;
    }
    s.strike_player(uid(2), 9999, DamageType::Physical, "a test foe");
    assert!(s.players[&uid(2)].dead, "the mage is a corpse");

    // The Rogue has no rite: the corpse stays fallen.
    assert!(!Class::Rogue.can_resurrect());
    s.resurrect_nearest(uid(3));
    assert!(
        s.players[&uid(2)].dead,
        "an incapable class cannot resurrect"
    );

    // The Cleric revives the mage where it lies (not at the temple).
    s.players.get_mut(&uid(1)).unwrap().resource = s.players[&uid(1)].max_resource;
    s.resurrect_nearest(uid(1));
    let v = &s.players[&uid(2)];
    assert!(!v.dead, "the mage lives again");
    assert!(v.hp > 0, "revived with some health");
    assert!(v.hp < v.max_hp(), "but not to full");
    assert_eq!(v.room, room, "raised where it fell, not the temple");
    assert_ne!(v.room, TEMPLE_ROOM);
}

#[test]
fn slaying_a_foe_grants_a_themed_title() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    s.grant_title(uid(1), "a frost-bound wretch", false, 4);
    s.grant_title(uid(1), "the Barrow King", true, 21);
    // Re-slaying the same foe must not duplicate its title.
    s.grant_title(uid(1), "a frost-bound wretch", false, 4);
    let titles = s.players[&uid(1)].titles.clone();
    assert!(
        titles.iter().any(|t| t == "Wretchbane"),
        "lesser foe -> ...bane"
    );
    assert!(
        titles.iter().any(|t| t == "Bane of the Barrow King"),
        "boss -> Bane of ..."
    );
    assert_eq!(titles.iter().filter(|t| *t == "Wretchbane").count(), 1);
}

#[test]
fn only_bosses_grant_titles_on_a_real_kill() {
    // 426 distinct regular foes used to each mint their own "...bane" title on
    // first kill, burying the handful that mean anything under a wall of
    // trash titles. Only a boss kill should add one now.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let regular_id = *s
        .mobs
        .iter()
        .find(|(_, m)| !m.spawn.boss)
        .map(|(id, _)| id)
        .expect("world has a regular mob");
    s.kill_mob(uid(1), regular_id);
    assert!(
        s.players[&uid(1)].titles.is_empty(),
        "a regular kill should grant no title"
    );
    let boss_id = *s
        .mobs
        .iter()
        .find(|(_, m)| m.spawn.boss)
        .map(|(id, _)| id)
        .expect("world has a boss");
    s.kill_mob(uid(1), boss_id);
    let titles = s.players[&uid(1)].titles.clone();
    assert!(
        !titles.is_empty(),
        "a boss kill should grant at least its themed title"
    );
    assert!(
        titles.iter().any(|t| t.starts_with("Bane of ")),
        "a boss kill should grant its \"Bane of ...\" title; got {titles:?}"
    );
}

#[test]
fn final_bosses_map_to_lifetime_achievements() {
    let archdemon = boss_achievement_for("the Archdemon Mal'gareth")
        .expect("authored final boss should grant an achievement");
    let archdemon_payout = archdemon.payout.expect("archdemon pays chips");
    assert_eq!(archdemon_payout.reward_key, LATEANIA_ARCHDEMON_REWARD_KEY);
    assert_eq!(
        archdemon_payout.chip_move,
        ChipMove::LateaniaArchdemonDefeat
    );
    assert_eq!(archdemon.award_category, LATEANIA_ARCHDEMON_AWARD_CATEGORY);

    let frontier_king = boss_achievement_for("the King Who Was Promised Nothing")
        .expect("last Frontier boss should grant an achievement");
    let king_payout = frontier_king.payout.expect("frontier king pays chips");
    assert_eq!(king_payout.reward_key, LATEANIA_FRONTIER_KING_REWARD_KEY);
    assert_eq!(king_payout.chip_move, ChipMove::LateaniaFrontierKingDefeat);
    assert_eq!(
        frontier_king.award_category,
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY
    );

    let yssgar = boss_achievement_for("Yssgar, the Sundering Deep")
        .expect("the Reaches' crowned boss should grant an achievement");
    let yssgar_payout = yssgar.payout.expect("yssgar pays chips");
    assert_eq!(yssgar_payout.reward_key, LATEANIA_SUNDERING_DEEP_REWARD_KEY);
    assert_eq!(
        yssgar_payout.chip_move,
        ChipMove::LateaniaSunderingDeepDefeat
    );
    assert_eq!(
        yssgar.award_category,
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY
    );

    let kaethyr = boss_achievement_for("Kaethyr Ascendant, Who Sang the God Awake")
        .expect("Kaelmyr's last boss should grant an achievement");
    let kaethyr_payout = kaethyr.payout.expect("kaethyr pays chips");
    assert_eq!(
        kaethyr_payout.reward_key,
        LATEANIA_KAETHYR_ASCENDANT_REWARD_KEY
    );
    assert_eq!(
        kaethyr_payout.chip_move,
        ChipMove::LateaniaKaethyrAscendantDefeat
    );
    assert_eq!(
        kaethyr.award_category,
        LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY
    );

    assert!(boss_achievement_for("the Elder Treant").is_none());
    assert!(
        boss_achievement_for("Kaethyr the Unquenched, Ashen King of Kaelmyr").is_none(),
        "only the Ascendant form at the Sundering Wound carries the crown"
    );
}

#[test]
fn reach_and_escort_quest_zones_exist_in_the_world() {
    let w = seed_world();
    let zones: std::collections::HashSet<&str> = w.rooms.values().map(|r| r.zone).collect();
    for q in BOARD_QUESTS {
        match q.objective {
            Objective::Reach { zone } => assert!(
                zones.contains(zone),
                "quest {} targets zone {zone:?} which no room carries",
                q.id
            ),
            Objective::Escort { dest_zone, .. } => assert!(
                zones.contains(dest_zone),
                "quest {} escorts to zone {dest_zone:?} which no room carries",
                q.id
            ),
            _ => {}
        }
    }
}

#[test]
fn sea_gate_requires_the_frontier_kings_bane() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let gate_dir = *s
        .world
        .room(super::super::world::MATLATESH_SQUARE)
        .expect("Matlatesh square exists")
        .exits
        .iter()
        .find(|(_, dest)| super::super::world::is_reaches_room(**dest))
        .expect("Matlatesh carries the sea-gate")
        .0;
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.room = super::super::world::MATLATESH_SQUARE;
    }

    // Without the King's bane the gate refuses, even on a second press.
    s.move_player(uid(1), gate_dir);
    s.move_player(uid(1), gate_dir);
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::MATLATESH_SQUARE,
        "sea-gate should hold without the King's bane"
    );

    // With the title, the first press warns and the second passes.
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.titles.push(REACHES_GATE_TITLE.to_string());
    }
    s.move_player(uid(1), gate_dir);
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::MATLATESH_SQUARE,
        "first press should only warn"
    );
    s.move_player(uid(1), gate_dir);
    assert!(
        super::super::world::is_reaches_room(s.players[&uid(1)].room),
        "second press should pass the sea-gate"
    );
}

#[test]
fn loading_saved_character_reconciles_level_from_xp() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    let mut saved = s.export_saved(uid(1)).expect("character saves");
    saved.level = 1;
    saved.xp = xp_for_level(5);

    s.hydrate(uid(1), &saved);
    let p = &s.players[&uid(1)];
    assert_eq!(p.level, 5, "saved xp should drive restored level");
    assert_eq!(p.base_attack, Class::Mage.stats_at(5).attack);

    let snap = s.snapshot();
    let view = snap.players.get(&uid(1)).expect("player view");
    assert_eq!(view.level, 5);
    assert!(
        view.abilities.iter().any(|a| a.name == "Frost Nova"),
        "restored level should update unlocked skills"
    );
}

#[test]
fn gold_math_keeps_rewards_and_death_loss_predictable() {
    assert_eq!(gold_for_kill(80, false), 19);
    assert_eq!(gold_for_kill(352, true), 80);
    assert_eq!(carried_gold_death_loss(0), 0);
    assert_eq!(carried_gold_death_loss(1), 1);
    assert_eq!(carried_gold_death_loss(1000), 200);
}

#[test]
fn veteran_resurrects_in_place_then_falls_when_spent() {
    let mut s = world();
    s.join(uid(1));
    s.set_veteran(uid(1), true);
    s.choose_class(uid(1), Class::Mage); // mage has no Warrior death-save
    assert_eq!(s.players[&uid(1)].resurrection_cap, VETERAN_RESURRECTIONS);
    for expected_left in (0..VETERAN_RESURRECTIONS).rev() {
        s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
        let p = &s.players[&uid(1)];
        assert!(p.respawn_at.is_none(), "veteran rises where they fall");
        assert_eq!(p.hp, p.max_hp(), "revived at full health");
        assert_eq!(p.resurrections_left, expected_left);
    }
    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    assert!(
        s.players[&uid(1)].respawn_at.is_some(),
        "out of charges, falls"
    );
}

#[test]
fn a_capital_fountain_restores_vitals_and_revives() {
    let mut s = world();
    s.join(uid(1));
    s.set_veteran(uid(1), true);
    s.choose_class(uid(1), Class::Mage);
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.room = 620; // Tasmania's Harborgate Square (safe capital)
        p.hp = 1;
        p.resource = 0;
        p.resurrections_left = 0;
    }
    let fountain = super::super::world::features_at(620)
        .iter()
        .position(|f| f.kind == FeatureKind::Fountain)
        .expect("the square has a fountain");
    s.interact(uid(1), fountain);
    let p = &s.players[&uid(1)];
    assert_eq!(p.hp, p.max_hp(), "fountain heals to full");
    assert_eq!(p.resource, p.max_resource, "fountain restores resource");
    assert_eq!(
        p.resurrections_left, p.resurrection_cap,
        "fountain refreshes resurrection charges"
    );
}

#[test]
fn talking_to_a_villager_speaks_their_line_and_the_room_announces_them() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1; // Embergate's Town Square
    let idx = super::super::world::features_at(1)
        .iter()
        .position(|f| f.kind == FeatureKind::Villager)
        .expect("the town square has a villager");
    let name = super::super::world::features_at(1)[idx].name;

    s.interact(uid(1), idx);
    let log = &s.players[&uid(1)].log;
    assert!(
        log.iter()
            .any(|l| l.text.contains(name) && l.text.contains("says:")),
        "talking to a villager should speak their line"
    );
    assert!(
        !log.iter().any(|l| l.text.contains("for a moment")),
        "the vague 'you ask X for a moment' preamble reads as an unfulfilled \
         action ('...and? that's it?') and should be gone - the dialogue \
         line above is the whole interaction, not a placeholder for one"
    );

    // The room description announces them up front, not hidden in a menu.
    s.describe_room(uid(1));
    let log = &s.players[&uid(1)].log;
    assert!(
        log.iter()
            .any(|l| l.text.contains(name) && l.text.contains("waiting for a question")),
        "a villager should always be announced, waiting for a question"
    );
}

#[test]
fn every_ability_score_moves_the_number_it_promises() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 3; // the smith
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores::default();
    let p = &s.players[&uid(1)];
    let (attack, swing, spell, hp, regen) = (
        p.attack(),
        p.swing(),
        p.spell_power(),
        p.max_hp(),
        p.regen(),
    );
    let sword = item(1001).unwrap(); // Iron Longsword, 80g
    let (buy, sell) = (p.buy_price(sword), p.sell_price(sword));
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores {
        strength: 18,
        dexterity: 18,
        constitution: 18,
        intelligence: 18,
        wisdom: 18,
        charisma: 18,
    };
    let p = &s.players[&uid(1)];
    let level = p.level;
    assert_eq!(
        p.attack(),
        attack,
        "no score touches the attack rating itself"
    );
    assert_eq!(p.swing(), swing + swing * 8 / 100, "STR: +8% on the swing");
    assert_eq!(
        p.spell_power(),
        spell + spell * 8 / 100,
        "INT: +8% spell power"
    );
    assert_eq!(
        p.max_hp(),
        hp + 4 * (4 + level / 2),
        "CON: +4 per modifier point at level 1"
    );
    assert_eq!(p.regen(), regen + 4, "WIS: +4 resource a tick");
    assert_eq!(p.buy_price(sword), buy - buy * 12 / 100, "CHA: 12% off");
    assert_eq!(
        p.sell_price(sword),
        sell + sell * 12 / 100,
        "CHA: 12% on top of a sale"
    );
    assert_eq!(p.scores.crit_pct(), 8, "DEX: 8% of swings crit");
    let before = p.gold;
    s.buy(uid(1), 1001);
    assert_eq!(
        s.players[&uid(1)].gold,
        before - (80 - 80 * 12 / 100),
        "the discount is what the shop actually charges"
    );
}

#[test]
fn a_point_every_fourth_level_is_placed_from_the_point_screen() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores::default();
    assert_eq!(s.players[&uid(1)].score_points(), 0);
    s.players.get_mut(&uid(1)).unwrap().xp = xp_for_level(8);
    s.check_level_up(uid(1));
    assert_eq!(
        s.players[&uid(1)].score_points(),
        2,
        "levels 4 and 8 each earn one"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("attribute point is yours to place")),
        "the level-up feed says a point was earned"
    );

    let snap = s.snapshot();
    let view = snap.players.get(&uid(1)).expect("player view");
    assert_eq!(view.score_points, 2);
    let offer: Vec<(&str, &str, Option<&str>)> = view
        .score_offer
        .iter()
        .map(|r| (r.label.as_str(), r.now.as_str(), r.after.as_deref()))
        .collect();
    assert_eq!(
        offer,
        vec![
            ("STR", "swings hit for +0%", Some("swings hit for +0%")),
            ("DEX", "no crits, no glances", Some("no crits, no glances")),
            ("CON", "+0 max HP at level 8", Some("+0 max HP at level 8")),
            ("INT", "spell power +0%", Some("spell power +0%")),
            (
                "WIS",
                "+0 resource every tick",
                Some("+0 resource every tick")
            ),
            (
                "CHA",
                "shops 0% cheaper, sells 0% dearer, taming +0%",
                Some("shops 0% cheaper, sells 0% dearer, taming +0%")
            ),
        ],
        "the screen shows every score with its reading now and after the point"
    );

    s.spend_score_point(uid(1), 0);
    s.spend_score_point(uid(1), 0);
    let p = &s.players[&uid(1)];
    assert_eq!(p.scores.strength, 12);
    assert_eq!(p.score_points(), 0);
    assert_eq!(
        p.swing(),
        p.attack() + p.attack() * 2 / 100,
        "and the swing moved with it"
    );
    let snap = s.snapshot();
    assert!(
        snap.players[&uid(1)].score_offer.is_empty(),
        "nothing left to place, the screen closes"
    );

    // A third point cannot go past the cap: it is kept, and the row says so.
    s.players.get_mut(&uid(1)).unwrap().xp = xp_for_level(12);
    s.check_level_up(uid(1));
    s.players.get_mut(&uid(1)).unwrap().scores.strength = 20;
    let snap = s.snapshot();
    assert!(
        snap.players[&uid(1)].score_offer.is_empty(),
        "the archetype crossroads at level 10 comes first"
    );
    s.choose_archetype(uid(1), 0);
    let snap = s.snapshot();
    assert_eq!(
        snap.players[&uid(1)].score_offer[0].after,
        None,
        "STR at the cap"
    );
    s.spend_score_point(uid(1), 0);
    let p = &s.players[&uid(1)];
    assert_eq!(p.scores.strength, 20);
    assert_eq!(
        p.score_points(),
        1,
        "the point is still there to place elsewhere"
    );
    assert!(
        p.log
            .iter()
            .any(|l| l.text.contains("already at its peak of 20"))
    );
}

/// A point can only be placed on a score below the cap, and the point screen
/// takes every key until it is placed. A character whose six scores are all
/// at the cap (a farmed roll, deep into the levels: 25 points are earned by
/// 100 and a 96+ roll leaves fewer slots than that) used to be held on that
/// screen for good, since no key could place the point and rejoining showed
/// it again. Points past what the scores can hold are simply not there.
#[test]
fn a_character_with_every_score_at_the_cap_is_never_held_at_the_point_screen() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores {
        strength: 20,
        dexterity: 20,
        constitution: 20,
        intelligence: 20,
        wisdom: 20,
        charisma: 19,
    };
    s.players.get_mut(&uid(1)).unwrap().xp = xp_for_level(100);
    s.check_level_up(uid(1));
    assert_eq!(s.players[&uid(1)].level, 100);
    // The level-10 crossroads comes first and keeps the point screen closed.
    s.choose_archetype(uid(1), 0);
    assert_eq!(
        s.players[&uid(1)].score_points(),
        1,
        "25 earned, but only one slot left to put a point in"
    );
    let snap = s.snapshot();
    assert_eq!(
        snap.players[&uid(1)].score_offer.len(),
        6,
        "the screen offers that one"
    );

    s.spend_score_point(uid(1), 5);
    let p = &s.players[&uid(1)];
    assert_eq!(p.scores.charisma, 20);
    assert_eq!(p.score_points(), 0, "nothing left that could be placed");
    let snap = s.snapshot();
    assert!(
        snap.players[&uid(1)].score_offer.is_empty(),
        "the screen closes and the character can play"
    );
    assert_eq!(snap.players[&uid(1)].score_points, 0);

    // Leaving and coming back re-derives the same answer from the save.
    let saved = s.export_saved(uid(1)).expect("character saves");
    s.leave(uid(1));
    s.join(uid(1));
    s.hydrate(uid(1), &saved);
    assert_eq!(s.players[&uid(1)].score_points(), 0);
    assert!(s.snapshot().players[&uid(1)].score_offer.is_empty());
}

/// The point screen takes the keys, but a corpse's only key is release. The
/// view used to draw the point screen over the corpse with the release hint
/// hidden behind it; a corpse sees the corpse, and places the point once up.
#[test]
fn a_corpse_with_a_point_pending_sees_the_corpse_not_the_point_screen() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage); // no Warrior death-save
    s.players.get_mut(&uid(1)).unwrap().scores = AbilityScores::default();
    s.players.get_mut(&uid(1)).unwrap().xp = xp_for_level(4);
    s.check_level_up(uid(1));
    assert_eq!(s.snapshot().players[&uid(1)].score_offer.len(), 6);

    s.strike_player(uid(1), 9999, DamageType::Physical, "a test foe");
    assert!(s.players[&uid(1)].dead);
    let snap = s.snapshot();
    assert!(
        snap.players[&uid(1)].score_offer.is_empty(),
        "the corpse view wins"
    );
    assert_eq!(
        snap.players[&uid(1)].score_points,
        1,
        "the point is still owed"
    );

    s.release_to_temple(uid(1));
    assert!(!s.players[&uid(1)].dead);
    assert_eq!(
        s.snapshot().players[&uid(1)].score_offer.len(),
        6,
        "and offered once risen"
    );
}

#[test]
fn a_character_saved_before_points_existed_has_them_all_to_place() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    let mut saved = s.export_saved(uid(1)).expect("character saves");
    saved.xp = xp_for_level(40);
    saved.level = 40;
    assert_eq!(saved.score_points_spent, 0);
    s.hydrate(uid(1), &saved);
    assert_eq!(
        s.players[&uid(1)].score_points(),
        10,
        "ten points back-paid at level 40"
    );

    // A save claiming more spent than the level ever earned is clamped, never negative.
    saved.score_points_spent = 30;
    s.hydrate(uid(1), &saved);
    assert_eq!(s.players[&uid(1)].score_points(), 0);
    assert_eq!(s.players[&uid(1)].score_points_spent, 10);
}

#[test]
fn quaff_drinks_the_smallest_potion_that_covers_the_wound() {
    use super::super::items::potion_id;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let (small, mid, big) = (potion_id(0), potion_id(2), potion_id(4)); // heal 25 / 75 / 180
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.base_max_hp = 200;
        p.inventory.extend([small, mid, big]);
        let max = p.max_hp();
        p.hp = max - 60; // missing 60: 25 is too small, 75 and 180 both cover
    }
    s.quaff_best(uid(1));
    let p = &s.players[&uid(1)];
    assert!(
        !p.inventory.contains(&mid),
        "should drink the 75 potion (smallest that covers 60)"
    );
    assert!(
        p.inventory.contains(&small) && p.inventory.contains(&big),
        "should leave the too-small and the oversized potion untouched"
    );
}

#[test]
fn quaff_falls_back_to_the_biggest_when_nothing_covers() {
    use super::super::items::potion_id;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let (small, big) = (potion_id(0), potion_id(3)); // heal 25 / 120
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.base_max_hp = 400;
        p.inventory.extend([small, big]);
        p.hp = 1; // missing ~399: neither potion covers it, so take the biggest
    }
    s.quaff_best(uid(1));
    let p = &s.players[&uid(1)];
    assert!(
        !p.inventory.contains(&big),
        "should drink the biggest available potion"
    );
    assert!(p.inventory.contains(&small), "should keep the small one");
}

#[test]
fn quaff_at_full_health_drinks_nothing() {
    use super::super::items::potion_id;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let potion = potion_id(2);
    s.players.get_mut(&uid(1)).unwrap().inventory.push(potion);
    s.quaff_best(uid(1));
    assert!(
        s.players[&uid(1)].inventory.contains(&potion),
        "a full-health quaff must not waste a potion"
    );
}

#[test]
fn clicking_a_foe_locks_onto_that_exact_foe() {
    const ROOM: RoomId = 2001; // Frontier interior: non-safe, so fighting is allowed
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Put two distinct foes in the room.
    let ids: Vec<u32> = s.mobs.keys().copied().take(2).collect();
    assert_eq!(ids.len(), 2, "world seeds at least two mobs to place");
    for &id in &ids {
        let m = s.mobs.get_mut(&id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = ROOM;
        m.leash_home = ROOM;
    }
    s.players.get_mut(&uid(1)).unwrap().room = ROOM;
    // A click on the second foe's row locks onto that exact foe, not the first.
    s.engage_mob(uid(1), ids[1]);
    assert_eq!(
        s.players[&uid(1)].target,
        Some(ids[1]),
        "targets the clicked foe"
    );
}

#[test]
fn clicking_a_vanished_foe_falls_back_to_whatever_is_here() {
    const ROOM: RoomId = 2001;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let present = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&present).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = ROOM;
        m.leash_home = ROOM;
    }
    s.players.get_mut(&uid(1)).unwrap().room = ROOM;
    // Clicking a foe id that isn't here (slain, fled, stale row) still engages
    // whatever is present rather than dead-ending.
    s.engage_mob(uid(1), 424_242);
    assert_eq!(
        s.players[&uid(1)].target,
        Some(present),
        "falls back to the foe that's actually here"
    );
}

#[test]
fn no_fighting_in_a_safe_haven_even_by_click() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Start room is a safe haven; a click must not start a fight there.
    let start = s.players[&uid(1)].room;
    assert!(s.world.room(start).is_some_and(|r| r.safe), "start is safe");
    s.engage_mob(uid(1), 424_242);
    assert_eq!(
        s.players[&uid(1)].target,
        None,
        "no target taken in a safe haven"
    );
}

#[test]
fn nearby_foes_lists_foes_in_neighbouring_rooms() {
    const HERE: RoomId = 2001; // Frontier interior
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let there = s
        .world
        .room(HERE)
        .and_then(|r| r.exits.values().next().copied())
        .expect("the room has an exit to a neighbour");
    s.players.get_mut(&uid(1)).unwrap().room = HERE;
    let mob_id = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = there;
    }
    let snap = s.snapshot();
    let view = &snap.players[&uid(1)];
    assert!(
        view.nearby_foes.contains(&there),
        "a foe in the next room shows on the live field"
    );
    assert!(
        !view.nearby_foes.contains(&HERE),
        "your own room is not listed as a nearby foe (that's the @ tile)"
    );
}

#[test]
fn a_foe_beyond_the_cell_window_stays_off_the_field() {
    const HERE: RoomId = 722; // Matlatesh - The Caravanserai
    const NEAR: RoomId = 608; // The Greatroad, one exit out of Matlatesh
    const FAR: RoomId = 30055; // Duskmire Wood, its own reserved block
    // The field's hint lists are scoped by the cell window alone: since the
    // unfold (worldmap's `zone_interleaves` pin keeps it that way), what sits
    // within a few cells really is a few moves away, and whole other lands
    // sit in reserved blocks hundreds of columns off, so a foe in one can
    // never land "near" by accident of the embedding.
    let coords = crate::app::door::lateania::worldmap::world_coords();
    let (here, far, near) = (coords[&HERE], coords[&FAR], coords[&NEAR]);
    assert!(
        near.z == here.z && (near.x - here.x).abs() <= 16 && (near.y - here.y).abs() <= 12,
        "fixture assumption broke: room {NEAR} should sit inside the field's cell window"
    );
    assert!(
        far.z != here.z || (far.x - here.x).abs() > 16 || (far.y - here.y).abs() > 12,
        "fixture assumption broke: room {FAR} should sit outside the field's cell window"
    );

    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = HERE;
    // Clear the field so the only foe anywhere is the one we place.
    for m in s.mobs.values_mut() {
        m.alive = false;
    }
    let mob_id = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = FAR;
    }
    assert!(
        !s.snapshot().players[&uid(1)].nearby_foes.contains(&FAR),
        "a foe in another land, outside the cell window, must not be marked"
    );

    // One exit out is genuinely near, and still shows.
    s.mobs.get_mut(&mob_id).unwrap().current_room = NEAR;
    assert!(
        s.snapshot().players[&uid(1)].nearby_foes.contains(&NEAR),
        "a foe in the zone next door still shows on the field"
    );
}

#[test]
fn rpg_mode_off_skips_the_field_hint_lists() {
    const HERE: RoomId = 2001;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let there = s
        .world
        .room(HERE)
        .and_then(|r| r.exits.values().next().copied())
        .expect("the room has an exit");
    s.players.get_mut(&uid(1)).unwrap().room = HERE;
    let mob_id = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = there;
    }
    // With the field hidden, the hint lists are dead weight; the snapshot
    // must not spend the window scan on them.
    s.players.get_mut(&uid(1)).unwrap().rpg_mode = false;
    let snap = s.snapshot();
    let view = &snap.players[&uid(1)];
    assert!(
        view.nearby_foes.is_empty() && view.nearby_players.is_empty(),
        "no field, no nearby hints"
    );
    // The room's own mob list is combat UI, not a field hint: still there.
    assert!(
        s.players.get_mut(&uid(1)).map(|p| p.room = there).is_some()
            && !s.snapshot().players[&uid(1)].mobs.is_empty(),
        "the in-room mob list is unaffected by rpg_mode"
    );
}

#[test]
fn a_hidden_foe_is_not_leaked_onto_the_field() {
    const HERE: RoomId = 2001;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let there = s
        .world
        .room(HERE)
        .and_then(|r| r.exits.values().next().copied())
        .expect("the room has an exit");
    s.players.get_mut(&uid(1)).unwrap().room = HERE;
    // Clear the field so the only foe near us is the one we control.
    for m in s.mobs.values_mut() {
        m.alive = false;
    }
    let mob_id = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = false; // still hidden in the fog
        m.current_room = there;
    }
    assert!(
        !s.snapshot().players[&uid(1)].nearby_foes.contains(&there),
        "an unrevealed foe must not be spoiled on the field"
    );
    // Once revealed, it shows.
    s.mobs.get_mut(&mob_id).unwrap().revealed = true;
    assert!(
        s.snapshot().players[&uid(1)].nearby_foes.contains(&there),
        "a revealed foe next door shows on the field"
    );
}

#[test]
fn nearby_players_lists_adventurers_in_neighbouring_rooms() {
    const HERE: RoomId = 2001;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let there = s
        .world
        .room(HERE)
        .and_then(|r| r.exits.values().next().copied())
        .expect("the room has an exit");
    s.players.get_mut(&uid(1)).unwrap().room = HERE;
    // A second adventurer standing in the next room.
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.players.get_mut(&uid(2)).unwrap().room = there;
    let snap = s.snapshot();
    assert!(
        snap.players[&uid(1)].nearby_players.contains(&there),
        "another adventurer next door shows on the field"
    );
    assert!(
        !snap.players[&uid(1)].nearby_players.contains(&HERE),
        "you don't count yourself"
    );
}

// Wildbound riding: mounted movement strides multiple rooms per keypress.
#[test]
fn a_mounted_step_strides_the_full_length_of_the_road() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Find a straight 6-room east chain inside one region (no gateways).
    let chain = {
        let mut found: Option<Vec<RoomId>> = None;
        'scan: for &start in s.world.rooms.keys() {
            let mut chain = vec![start];
            let mut cur = start;
            for _ in 0..5 {
                let Some(&next) = s.world.room(cur).and_then(|r| r.exits.get(&Dir::East)) else {
                    continue 'scan;
                };
                // Stay well inside one id band so no progression gate triggers.
                if next.abs_diff(start) > 300 {
                    continue 'scan;
                }
                chain.push(next);
                cur = next;
            }
            found = Some(chain);
            break;
        }
        found.expect("the world has a straight six-room east road somewhere")
    };
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = chain[0];
        p.base_max_hp = 5000; // survive anything roaming the road
        p.hp = 5000;
        let serpent = super::super::taming::tameable_by_key("wb_worldserpent").unwrap();
        p.pet = Some(Pet::new(serpent, 0));
    }
    s.toggle_mount(uid(1));
    assert!(s.players[&uid(1)].mounted, "saddled up");
    s.move_player(uid(1), Dir::East);
    let landed = s.players[&uid(1)].room;
    assert_eq!(
        landed, chain[5],
        "a stride-5 mount covers five rooms in one step"
    );
}

#[test]
fn you_cannot_ride_the_unrideable_and_combat_grounds_you() {
    const ROOM: RoomId = 2001;
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // No pet at all: refused.
    s.toggle_mount(uid(1));
    assert!(!s.players[&uid(1)].mounted);
    // A hare is not a horse: refused.
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        let hare = super::super::taming::tameable_by_key("wt_hare").unwrap();
        p.pet = Some(Pet::new(hare, 0));
    }
    s.toggle_mount(uid(1));
    assert!(!s.players[&uid(1)].mounted, "a hare cannot carry a rider");
    // A palfrey can - but starting a fight puts you back on your feet.
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        let palfrey = super::super::taming::tameable_by_key("wb_palfrey").unwrap();
        p.pet = Some(Pet::new(palfrey, 0));
        p.room = ROOM;
    }
    s.toggle_mount(uid(1));
    assert!(s.players[&uid(1)].mounted);
    let mob_id = *s.mobs.keys().next().unwrap();
    {
        let m = s.mobs.get_mut(&mob_id).unwrap();
        m.alive = true;
        m.revealed = true;
        m.current_room = ROOM;
    }
    s.engage(uid(1));
    assert!(
        !s.players[&uid(1)].mounted,
        "combat slides you out of the saddle"
    );
}

#[test]
fn feeding_a_stray_daily_wins_it_over_as_a_companion() {
    // Genesys: five consecutive days of feeding a wild adoptable critter wins
    // it over as a stray, alongside any pet already kept - never replacing it.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    {
        let p = s.players.get_mut(&uid(1)).unwrap();
        p.room = 1; // Embergate's Town Square, home to "a scruffy stray dog"
        let species = super::super::pets::pet_species_by_key("war_hound").unwrap();
        p.pet = Some(super::super::pets::Pet::new(species, 0)); // a healthy owned pet
        p.gold = 1_000;
    }

    s.feed_pet(uid(1));
    assert!(
        s.players[&uid(1)].stray_bond.is_some(),
        "the first feeding starts a bond"
    );
    assert!(
        s.players[&uid(1)].stray.is_none(),
        "not won over on day one"
    );

    // Roll four more days by rewinding "last fed" a day at a time.
    for _ in 0..4 {
        let (idx, streak, day) = s.players[&uid(1)].stray_bond.unwrap();
        s.players.get_mut(&uid(1)).unwrap().stray_bond = Some((idx, streak, day - 1));
        s.feed_pet(uid(1));
    }

    assert!(
        s.players[&uid(1)].stray.is_some(),
        "five consecutive days should win the stray over"
    );
    assert!(
        s.players[&uid(1)].stray_bond.is_none(),
        "the bond clears once adopted"
    );
    assert!(
        s.players[&uid(1)].pet.is_some(),
        "the stray joins on top of the pet the player already had, not instead of it"
    );
}

#[test]
fn a_stray_bond_resets_if_a_day_is_missed_and_wont_double_feed_same_day() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;

    s.feed_pet(uid(1));
    let (idx, streak, day) = s.players[&uid(1)].stray_bond.unwrap();
    assert_eq!(streak, 1);

    // Same-day re-feed: no change.
    s.feed_pet(uid(1));
    assert_eq!(s.players[&uid(1)].stray_bond, Some((idx, 1, day)));

    // Skip two days instead of one: the streak resets to 1, not 2.
    s.players.get_mut(&uid(1)).unwrap().stray_bond = Some((idx, streak, day - 2));
    s.feed_pet(uid(1));
    assert_eq!(
        s.players[&uid(1)].stray_bond.map(|(_, s, _)| s),
        Some(1),
        "missing a day should reset the streak, not continue it"
    );
}

#[test]
fn a_zone_boss_bounty_pays_by_the_zones_target_level_not_the_number_over_its_head() {
    // The level over a boss's head reads by its bite and moves whenever the
    // ladder is retuned. The bounty is a one-time payout; it must key off the
    // level the zone is pitched at (a straight line from the living dark's
    // exit at L40 to the King at L55), so a display retune never repriced it.
    for (zone, target) in [(0usize, 40), (19usize, 55)] {
        let mut s = world();
        s.join(uid(1));
        s.choose_class(uid(1), Class::Warrior);
        let (_, boss) = super::super::world::frontier_zone_info(zone).expect("zone exists");
        let boss_id = *s
            .mobs
            .iter()
            .find(|(_, m)| m.spawn.name == boss)
            .map(|(id, _)| id)
            .expect("the zone boss is fielded");
        let xp_before = s.players[&uid(1)].xp;
        let gold_before = s.players[&uid(1)].gold;
        s.kill_mob(uid(1), boss_id);
        let p = &s.players[&uid(1)];
        // The kill itself pays the boss's own xp and purse; the bounty is what
        // lands on top of that.
        let boss_xp = s.mobs[&boss_id].spawn.xp;
        let boss_gold = gold_for_kill(boss_xp, true) as i64;
        assert_eq!(
            p.xp - xp_before - boss_xp as i64,
            80 + target * 24,
            "zone {zone} bounty xp keys off L{target}"
        );
        assert_eq!(
            p.gold - gold_before - boss_gold,
            35 + target * 6,
            "zone {zone} bounty gold keys off L{target}"
        );
    }
}

#[test]
fn say_defaults_to_the_room_and_ignores_other_rooms() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.join(uid(3));
    s.choose_class(uid(3), Class::Ranger);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.players.get_mut(&uid(2)).unwrap().room = 1; // same room
    s.players.get_mut(&uid(3)).unwrap().room = 3; // different room, same zone (Embergate)

    s.say(uid(1), "hello there");

    let log1 = &s.players[&uid(1)].log;
    assert!(log1.iter().any(|l| l.text == "You say: hello there"));
    let log2 = &s.players[&uid(2)].log;
    assert!(log2.iter().any(|l| l.text == "Someone says: hello there"));
    let log3 = &s.players[&uid(3)].log;
    assert!(
        !log3.iter().any(|l| l.text.contains("hello there")),
        "a bare say should not reach a different room, even in the same zone"
    );
}

#[test]
fn zone_say_reaches_the_whole_zone_but_not_other_zones() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.join(uid(3));
    s.choose_class(uid(3), Class::Ranger);
    s.players.get_mut(&uid(1)).unwrap().room = 1; // Embergate
    s.players.get_mut(&uid(2)).unwrap().room = 3; // Embergate, a different room
    s.players.get_mut(&uid(3)).unwrap().room = 620; // Tasmania - a different zone entirely

    s.say(uid(1), "/zone anyone nearby?");

    let log1 = &s.players[&uid(1)].log;
    assert!(
        log1.iter()
            .any(|l| l.text == "You say to the zone: anyone nearby?")
    );
    let log2 = &s.players[&uid(2)].log;
    assert!(
        log2.iter()
            .any(|l| l.text == "Someone says to the zone: anyone nearby?"),
        "a different room in the same zone should hear it"
    );
    let log3 = &s.players[&uid(3)].log;
    assert!(
        !log3.iter().any(|l| l.text.contains("anyone nearby")),
        "a different zone should never hear it"
    );

    // The short "/z" form works the same way (cooldown reset: this test is
    // about scope parsing, not the broadcast brake).
    s.players.get_mut(&uid(1)).unwrap().log.clear();
    s.players.get_mut(&uid(2)).unwrap().log.clear();
    s.players.get_mut(&uid(1)).unwrap().last_broadcast = None;
    s.say(uid(1), "/z short form works too");
    assert!(
        s.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text.contains("short form works too"))
    );
}

#[test]
fn world_say_reaches_every_adventurer_in_lateania() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.players.get_mut(&uid(1)).unwrap().room = 1; // Embergate
    s.players.get_mut(&uid(2)).unwrap().room = 620; // Tasmania - a different zone

    s.say(uid(1), "/world hail, all of Lateania");

    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text == "You say to all of Lateania: hail, all of Lateania")
    );
    assert!(
        s.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text == "Someone says to all of Lateania: hail, all of Lateania"),
        "world scope should reach every player, any zone"
    );

    // The short "/w" form works the same way (cooldown reset: this test is
    // about scope parsing, not the broadcast brake).
    s.players.get_mut(&uid(1)).unwrap().log.clear();
    s.players.get_mut(&uid(2)).unwrap().log.clear();
    s.players.get_mut(&uid(1)).unwrap().last_broadcast = None;
    s.say(uid(1), "/w short form too");
    assert!(
        s.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text.contains("short form too"))
    );
}

#[test]
fn broadcasts_are_held_by_a_cooldown_but_room_speech_is_not() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.players.get_mut(&uid(1)).unwrap().room = 1; // Embergate
    s.players.get_mut(&uid(2)).unwrap().room = 620; // Tasmania, hears world only

    s.say(uid(1), "/world first call");
    s.say(uid(1), "/world second call");
    let log2 = &s.players[&uid(2)].log;
    assert!(log2.iter().any(|l| l.text.contains("first call")));
    assert!(
        !log2.iter().any(|l| l.text.contains("second call")),
        "a second broadcast inside the cooldown window is held"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("give the echo a breath")),
        "the held speaker is told why nothing went out"
    );

    // Room speech never trips the broadcast brake.
    s.say(uid(1), "hello room");
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text == "You say: hello room")
    );

    // Once the window has passed, the next broadcast goes out.
    s.players.get_mut(&uid(1)).unwrap().last_broadcast = Some(Instant::now() - BROADCAST_COOLDOWN);
    s.say(uid(1), "/world third call");
    assert!(
        s.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text.contains("third call")),
        "an expired cooldown lets the next broadcast through"
    );
}

#[test]
fn a_scope_marker_with_no_message_says_nothing() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let before = s.players[&uid(1)].log.len();
    s.say(uid(1), "/zone ");
    s.say(uid(1), "/world    ");
    assert_eq!(
        s.players[&uid(1)].log.len(),
        before,
        "an empty message after the scope marker should say nothing"
    );
}

#[test]
fn a_word_that_merely_starts_with_z_or_w_is_not_mistaken_for_a_scope_marker() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.players.get_mut(&uid(2)).unwrap().room = 620; // a different zone

    s.say(uid(1), "/zealous about this fight");

    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text == "You say: /zealous about this fight"),
        "\"/zealous\" is a word, not the /z marker, and should say to the room verbatim"
    );
    assert!(
        !s.players[&uid(2)]
            .log
            .iter()
            .any(|l| l.text.contains("zealous")),
        "a merely-similar word should never widen the scope past the room"
    );
}

/// Any real Wildbound Waste field room, for pvp tests that don't care which.
fn any_pvp_room(world: &super::super::world::World) -> RoomId {
    world
        .rooms
        .values()
        .find(|r| r.pvp)
        .map(|r| r.id)
        .expect("the Wildbound Waste has at least one pvp room")
}

#[test]
fn engage_player_only_works_on_pvp_ground() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    // Both start in Embergate's safe square: no duelling allowed here.
    s.players.get_mut(&uid(2)).unwrap().room = s.players[&uid(1)].room;
    s.engage_player(uid(1), uid(2));
    assert_eq!(
        s.players[&uid(1)].pvp_target,
        None,
        "safe ground refuses a duel"
    );

    // Move both onto real pvp ground: the duel locks on and the victim
    // auto-retaliates since they weren't already fighting anything.
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.engage_player(uid(1), uid(2));
    assert_eq!(s.players[&uid(1)].pvp_target, Some(uid(2)));
    assert_eq!(
        s.players[&uid(2)].pvp_target,
        Some(uid(1)),
        "an unengaged victim rounds on their attacker"
    );
}

#[test]
fn a_pvp_duel_blocks_movement_and_recall_like_any_fight() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.engage_player(uid(1), uid(2));
    assert!(s.players[&uid(1)].in_combat());

    let room_before = s.players[&uid(1)].room;
    s.recall(uid(1));
    assert_eq!(
        s.players[&uid(1)].room,
        room_before,
        "recall must not work mid-duel"
    );
}

#[test]
fn winning_a_pvp_duel_credits_gold_xp_a_kill_and_a_title() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    // Stack the fight hopelessly in the attacker's favour: a one-shot kill on
    // the first combat round, no Warrior death-save or veteran charge to
    // interrupt it.
    {
        let victim = s.players.get_mut(&uid(2)).unwrap();
        victim.hp = 1;
        victim.death_save_used = true;
        victim.resurrections_left = 0;
        victim.gold = 100;
    }
    let attacker_xp_before = s.players[&uid(1)].xp;
    let attacker_gold_before = s.players[&uid(1)].gold;
    s.engage_player(uid(1), uid(2));
    s.tick();

    let victim = &s.players[&uid(2)];
    assert!(victim.dead, "the outmatched victim should have fallen");
    let lost_gold = 100 - victim.gold;
    assert!(lost_gold > 0, "a real death loses carried gold");

    let attacker = &s.players[&uid(1)];
    assert_eq!(attacker.pvp_kills, 1);
    assert_eq!(
        attacker.gold,
        attacker_gold_before + lost_gold,
        "the victim's lost gold becomes the spoils"
    );
    assert!(attacker.xp > attacker_xp_before, "a pvp kill grants xp");
    assert!(
        attacker.titles.iter().any(|t| t == "Blooded"),
        "a first pvp kill earns the Blooded title, got {:?}",
        attacker.titles
    );
}

#[test]
fn an_offensive_ability_strikes_a_pvp_target() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(1)).unwrap().resource = 999;
    s.players.get_mut(&uid(2)).unwrap().hp = 200;
    s.engage_player(uid(1), uid(2));

    // Slot 1 is Cleave (Strike) for a level-1 Warrior.
    let before = s.players[&uid(2)].hp;
    s.use_ability(uid(1), 1);
    assert!(
        s.players[&uid(2)].hp < before,
        "Cleave should damage the pvp target directly, not just the auto-attack"
    );
}

#[test]
fn locking_onto_a_mob_breaks_off_the_duel_so_abilities_hit_the_mob() {
    let mut s = world();
    // A Wildbound field room holding a revealed, living foe: contested ground
    // and the Waste's own roster share the same rooms, so a duel and a mob
    // fight are both one keypress away at any moment.
    let (mob_id, pvp_room) = s
        .mobs
        .values()
        .find(|m| m.alive && m.revealed && s.world.room(m.current_room).is_some_and(|r| r.pvp))
        .map(|m| (m.spawn.id, m.current_room))
        .expect("the Wildbound Waste fields mobs on contested ground");
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(1)).unwrap().resource = 999;
    s.players.get_mut(&uid(2)).unwrap().hp = 200;

    s.engage_player(uid(1), uid(2));
    s.engage_mob(uid(1), mob_id);

    let rival_hp = s.players[&uid(2)].hp;
    let mob_hp = s.mobs[&mob_id].hp;
    s.use_ability(uid(1), 1); // Cleave: Strike

    assert!(
        s.mobs[&mob_id].hp < mob_hp,
        "an ability should land on the foe just targeted"
    );
    assert_eq!(
        s.players[&uid(2)].hp,
        rival_hp,
        "and never on the rival the duel was broken off with"
    );
}

#[test]
fn a_damage_over_time_ability_seeds_a_pvp_dot_that_ticks_via_strike_player() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(1)).unwrap().resource = 999;
    s.players.get_mut(&uid(1)).unwrap().level = 4; // unlocks Rend (slot 2)
    s.players.get_mut(&uid(2)).unwrap().hp = 200;
    s.engage_player(uid(1), uid(2));

    s.use_ability(uid(1), 2); // Rend: DamageOverTime
    assert!(
        s.pvp_dots.contains_key(&uid(2)),
        "Rend should seed a pvp dot on the victim"
    );
    let before = s.players[&uid(2)].hp;
    s.tick();
    assert!(
        s.players[&uid(2)].hp < before,
        "the dot should tick real damage into the victim via strike_player"
    );
}

#[test]
fn a_stun_ability_skips_the_stunned_players_next_swing() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(1)).unwrap().resource = 999;
    s.players.get_mut(&uid(1)).unwrap().level = 12; // unlocks Shield Bash (slot 4)
    s.players.get_mut(&uid(2)).unwrap().hp = 500;
    s.players.get_mut(&uid(2)).unwrap().max_resource = 500;
    s.engage_player(uid(1), uid(2));
    // Victim rounds on the attacker too (auto-retaliation), so give them an
    // ability roster of their own to prove their swing is actually skipped.
    s.players.get_mut(&uid(2)).unwrap().level = 1;

    s.use_ability(uid(1), 4); // Shield Bash: Stun
    assert!(
        s.pvp_stuns.get(&uid(2)).copied().unwrap_or(0) > 0,
        "Shield Bash should stun the pvp victim"
    );
    let attacker_hp_before = s.players[&uid(1)].hp;
    s.tick();
    assert_eq!(
        s.players[&uid(1)].hp,
        attacker_hp_before,
        "a stunned adventurer should not land their own swing this round"
    );
}

#[test]
fn a_companions_bite_and_auto_skills_reach_a_pvp_target() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    // Buy the companion at Embergate's stable (room 1) before heading to the
    // pvp ground, same as any real player would.
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    s.players.get_mut(&uid(1)).unwrap().gold = 1000;
    s.buy_pet(uid(1), "war_hound");
    assert!(s.players[&uid(1)].pet.is_some(), "the companion is set");

    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().hp = 500;
    s.engage_player(uid(1), uid(2));

    let before = s.players[&uid(2)].hp;
    s.tick();
    let after_owner_and_pet = s.players[&uid(2)].hp;
    assert!(
        after_owner_and_pet < before,
        "the owner's own blow should land"
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("tears into your rival")),
        "the pet's bite against the pvp target should be logged, got {:?}",
        s.players[&uid(1)]
            .log
            .iter()
            .map(|l| &l.text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_brand_new_character_spawns_in_the_tutorial_and_can_recall_to_embergate() {
    let mut s = world();
    s.join(uid(1));
    // Joining alone (before a class is chosen) already places the character
    // in Wayfarer's Hollow, not `World::start_room` directly.
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::tutorial_start_room()
    );
    s.choose_class(uid(1), Class::Warrior);
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::tutorial_start_room()
    );
    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|l| l.text.contains("Wayfarer's Hollow") && l.text.contains('r')),
        "the welcome message should mention the tutorial and the recall key"
    );
    // The existing recall (r) already works from anywhere: it's the "leave
    // for town with a key anytime" the tutorial promises.
    s.recall(uid(1));
    assert_eq!(s.players[&uid(1)].room, s.world.start_room);
}

#[test]
fn a_returning_character_reloads_where_they_saved_not_the_tutorial() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = 1;
    let saved = s.export_saved(uid(1)).expect("classed character exports");
    assert_eq!(
        saved.room, 1,
        "the saved room is wherever they actually stood"
    );

    // A fresh session reloads that exact room, not the tutorial.
    let mut s2 = world();
    s2.join(uid(1));
    s2.hydrate(uid(1), &saved);
    assert_eq!(s2.players[&uid(1)].room, 1);
}

// Backtick hops out of the world (autosave + leave) and back in, so a hop from
// anywhere but a town must put the character back where they stood: relocating
// them to Embergate's square is a free recall out of the wilds (and out of the
// Waste's pvp rooms) that nobody asked for.
#[test]
fn a_returning_character_reloads_in_an_unsafe_room_too() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    const ROOM: RoomId = 2001; // Frontier zone 0, interior: not a haven
    assert!(
        !s.world.room(ROOM).unwrap().safe,
        "test premise: an unsafe room"
    );
    s.players.get_mut(&uid(1)).unwrap().room = ROOM;
    let saved = s.export_saved(uid(1)).expect("classed character exports");

    // The hop out leaves the world; the hop back rejoins and rehydrates.
    s.leave(uid(1));
    s.join(uid(1));
    s.hydrate(uid(1), &saved);
    assert_eq!(
        s.players[&uid(1)].room,
        ROOM,
        "hopping back in must not teleport the character to the start room"
    );
}

#[test]
fn leaderboard_ranks_by_level_pvp_kills_and_gold_and_skips_unclassed() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Mage);
    s.join(uid(3)); // never classed - must not appear on any board

    {
        let p1 = s.players.get_mut(&uid(1)).unwrap();
        p1.level = 10;
        p1.pvp_kills = 3;
        p1.gold = 50;
        p1.banked_gold = 0;
    }
    {
        let p2 = s.players.get_mut(&uid(2)).unwrap();
        p2.level = 20;
        p2.pvp_kills = 1;
        p2.gold = 10;
        p2.banked_gold = 500;
    }

    let board = s.build_leaderboard();
    assert_eq!(board.by_level.len(), 2, "unclassed players are excluded");
    assert_eq!(
        board.by_level[0].user_id,
        uid(2),
        "higher level ranks first"
    );
    assert_eq!(
        board.by_pvp_kills[0].user_id,
        uid(1),
        "more pvp kills ranks first"
    );
    assert_eq!(
        board.by_gold[0].user_id,
        uid(2),
        "carried + banked gold ranks first"
    );
    assert_eq!(board.by_gold[0].value, 510, "gold is carried plus banked");
    assert!(
        board
            .by_level
            .iter()
            .chain(&board.by_pvp_kills)
            .chain(&board.by_gold)
            .all(|e| e.user_id != uid(3)),
        "an unclassed character never appears on any board"
    );
}

#[test]
fn leaderboard_caps_at_ten_entries() {
    let mut s = world();
    for i in 0..15u128 {
        s.join(uid(100 + i));
        s.choose_class(uid(100 + i), Class::Warrior);
        s.players.get_mut(&uid(100 + i)).unwrap().level = i as i32;
    }
    let board = s.build_leaderboard();
    assert_eq!(board.by_level.len(), 10, "top ten only, not all fifteen");
    assert_eq!(board.by_level[0].level, 14, "the highest level leads");
}

#[test]
fn a_player_never_gets_dropped_from_the_world_for_going_idle() {
    // There used to be a 10-minute inactivity kick. It's gone: only an
    // explicit leave (closing the session) removes a player now.
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    for _ in 0..50 {
        s.tick();
    }
    assert!(
        s.players.contains_key(&uid(1)),
        "no amount of ticking without action should ever drop a present player"
    );
}

// ---- slot binding (which saved character a session is actually playing) ----
//
// These drive the real service against a real database, because the defect
// they pin lives in the join/leave/persist plumbing, not in `WorldState`.

/// Drive `join_task` and wait for the character to actually materialize.
async fn join_and_wait(svc: &LateaniaService, user_id: Uuid, session_id: Uuid, slot: i16) {
    svc.select_slot(user_id, slot);
    svc.join_task(user_id, session_id);
    crate::test_helpers::wait_until(
        || async { svc.is_user_present(user_id) },
        "the character joins the world",
    )
    .await;
}

/// Pick a class and wait for it to land in the snapshot, so the character is
/// exportable (unclassed characters are never persisted).
async fn class_up_and_wait(svc: &LateaniaService, user_id: Uuid, class: Class) {
    svc.choose_class_task(user_id, class);
    crate::test_helpers::wait_until(
        || async {
            svc.snapshot_rx
                .borrow()
                .players
                .get(&user_id)
                .is_some_and(|p| p.class_name == class.name())
        },
        "the class choice reaches the snapshot",
    )
    .await;
}

/// The class key stored in one character slot, or None if the slot is empty.
async fn saved_class(db: &late_core::db::Db, user_id: Uuid, slot: i16) -> Option<String> {
    let client = db.get().await.expect("db client");
    let blob = MudCharacter::load(&client, user_id, slot)
        .await
        .expect("mud_characters loads")?;
    SavedCharacter::from_json(&blob)
        .expect("a stored blob parses")
        .class
}

#[tokio::test]
async fn a_second_session_picking_another_slot_cannot_overwrite_the_live_character() {
    let db = crate::test_helpers::new_test_db().await;
    // A real account row: `mud_characters.user_id` is a foreign key, so a
    // synthetic uuid would make every save fail silently.
    let client = db.db.get().await.expect("db client");
    let user = late_core::models::user::User::create(
        &client,
        late_core::models::user::UserParams {
            fingerprint: "slot-binding-fp".to_string(),
            username: "slotbinder".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("test account")
    .id;
    let app = crate::test_helpers::make_app(db.db.clone(), user, "slot-binding");
    let svc = app.lateania_service.clone();

    // Slot 1 gets a Mage, then logs out: that save is what must survive.
    join_and_wait(&svc, user, uid(11), 1).await;
    class_up_and_wait(&svc, user, Class::Mage).await;
    svc.leave_task(user, uid(11));
    crate::test_helpers::wait_until(
        || async { saved_class(&db.db, user, 1).await == Some(Class::Mage.as_key().to_string()) },
        "the Mage's logout save reaches slot 1",
    )
    .await;

    // Slot 0 gets a Warrior, and this is the character actually in the world.
    join_and_wait(&svc, user, uid(10), 0).await;
    class_up_and_wait(&svc, user, Class::Warrior).await;

    // A second connection on the same account opens the landing and picks
    // slot 1. It attaches to the live character (one world identity per
    // account) - it must not redirect that character's saves at slot 1.
    join_and_wait(&svc, user, uid(12), 1).await;

    svc.flush_all()
        .await
        .expect("flush persists live characters");

    assert_eq!(
        saved_class(&db.db, user, 1).await.as_deref(),
        Some(Class::Mage.as_key()),
        "the idle slot's Mage must survive another session merely selecting it"
    );
    assert_eq!(
        saved_class(&db.db, user, 0).await.as_deref(),
        Some(Class::Warrior.as_key()),
        "the live character keeps saving to the slot it was loaded from"
    );
}

#[test]
fn starter_chain_walks_a_new_player_to_the_first_gate() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Fresh characters start on stage 0 (reach Embergate), and the join log
    // always has a next-step line to announce.
    assert_eq!(s.players[&uid(1)].starter_stage, 0);
    let view = s.snapshot().players[&uid(1)].clone();
    assert!(
        next_step_for(s.players[&uid(1)].starter_stage, &s.players[&uid(1)].titles).is_some(),
        "a fresh character always has a next step"
    );
    assert!(
        view.quests.iter().any(|q| q.kind == QuestKind::Starter),
        "the journal pins the active starter step"
    );

    let gold_before = s.players[&uid(1)].gold;
    s.players.get_mut(&uid(1)).unwrap().room = 1; // Embergate's square
    s.describe_room(uid(1));
    assert_eq!(
        s.players[&uid(1)].starter_stage,
        1,
        "reaching Embergate completes First Steps"
    );
    assert!(s.players[&uid(1)].gold > gold_before, "the step pays out");

    // Stage 1: three kills on the King's Road (the scrawny goblin is homed
    // there). Revive it between kills; only the count matters.
    s.players.get_mut(&uid(1)).unwrap().room = 6;
    for _ in 0..3 {
        s.kill_mob(uid(1), 1);
        if let Some(m) = s.mobs.get_mut(&1) {
            m.alive = true;
            m.hp = m.spawn.max_hp;
        }
    }
    assert_eq!(
        s.players[&uid(1)].starter_stage,
        2,
        "three road kills complete The Open Road"
    );

    s.players.get_mut(&uid(1)).unwrap().room = 11; // Whisperwood's threshold
    s.describe_room(uid(1));
    assert_eq!(s.players[&uid(1)].starter_stage, 3);

    s.players.get_mut(&uid(1)).unwrap().room = 28; // the Treant's grove
    s.kill_mob(uid(1), 13); // the Elder Treant
    assert_eq!(
        s.players[&uid(1)].starter_stage,
        4,
        "slaying the Elder Treant completes its step"
    );

    s.players.get_mut(&uid(1)).unwrap().room = 31; // Duskhollow's first cave
    s.describe_room(uid(1));
    assert_eq!(
        s.players[&uid(1)].starter_stage as usize,
        STARTER_QUESTS.len(),
        "descending into Duskhollow completes the chain"
    );

    // With the chain done the journal drops the starter row and the join-log
    // next step hands over to the Long Road (the Treant is down; the Archdemon
    // is the current milestone).
    let view = s.snapshot().players[&uid(1)].clone();
    assert!(
        !view.quests.iter().any(|q| q.kind == QuestKind::Starter),
        "no starter row once the chain is complete"
    );
    let p = &s.players[&uid(1)];
    let next = next_step_for(p.starter_stage, &p.titles).expect("the Long Road takes over");
    assert!(
        next.contains("Archdemon"),
        "next step names the Archdemon: {next}"
    );
}

#[test]
fn kills_off_the_road_do_not_advance_the_road_stage() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().starter_stage = 1; // The Open Road
    // A kill made while standing in Whisperwood is not road work.
    s.players.get_mut(&uid(1)).unwrap().room = 11;
    s.kill_mob(uid(1), 10);
    let p = &s.players[&uid(1)];
    assert_eq!(p.starter_stage, 1);
    assert_eq!(p.starter_kills, 0, "an off-road kill counts for nothing");
}

#[test]
fn veteran_saves_skip_the_starter_chain_on_load() {
    // A pre-v19 save (version 0) past level 10 has long outgrown the tutorial
    // chain; one still early keeps it.
    let mut s = world();
    s.join(uid(1));
    let veteran = SavedCharacter::from_json(&serde_json::json!({"class": "warrior", "level": 12}))
        .expect("parses");
    s.hydrate(uid(1), &veteran);
    assert_eq!(
        s.players[&uid(1)].starter_stage as usize,
        STARTER_QUESTS.len(),
        "a veteran is not handed the tutorial chain"
    );

    // The novice reloads in Wayfarer's Hollow (room 40000): stage 0 ("reach
    // Embergate") must survive the load. A save sitting in Embergate itself
    // would - correctly - complete that stage the moment it lands.
    let mut s = world();
    s.join(uid(2));
    let novice = SavedCharacter::from_json(
        &serde_json::json!({"class": "warrior", "level": 3, "room": 40000}),
    )
    .expect("parses");
    s.hydrate(uid(2), &novice);
    assert_eq!(
        s.players[&uid(2)].starter_stage,
        0,
        "an early character keeps the chain"
    );
}

#[test]
fn sealed_board_postings_cannot_be_accepted() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = super::super::world::TASMANIA_SQUARE;

    // Quest 1 hunts the Sunken Catacombs, gated on the Archdemon's fall: the
    // posting reads sealed and accepting it is refused.
    let entries = s.board_entries(uid(1), super::super::world::TASMANIA_SQUARE);
    let posting = entries
        .iter()
        .find(|e| e.quest_id == 1)
        .expect("the bounty is posted");
    assert!(posting.locked, "the posting reads sealed");
    assert!(posting.suggested_level > 0, "it carries a level hint");
    assert!(!posting.hint.is_empty(), "it says where the work is");
    s.accept_board_quest(uid(1), 1);
    assert!(
        s.players[&uid(1)].board_progress.is_empty(),
        "a sealed posting cannot be accepted"
    );

    // The gate title unseals it.
    s.award_title(uid(1), FRONTIER_GATE_TITLE.to_string(), 1);
    let entries = s.board_entries(uid(1), super::super::world::TASMANIA_SQUARE);
    assert!(
        entries.iter().any(|e| e.quest_id == 1 && !e.locked),
        "the gate title unseals the posting"
    );
    s.accept_board_quest(uid(1), 1);
    assert!(
        s.players[&uid(1)]
            .board_progress
            .iter()
            .any(|(id, _)| *id == 1),
        "an unsealed posting accepts normally"
    );
}

#[test]
fn the_long_road_matches_the_real_gates_and_tracks_titles() {
    // Drift guard: every gate title the world actually checks appears on the
    // Long Road, derived through the same title_for the kill path uses.
    let road_titles: Vec<String> = LONG_ROAD.iter().map(|m| title_for(m.boss, true)).collect();
    let gates = [
        FIRST_DUNGEON_GATE_TITLE,
        FRONTIER_GATE_TITLE,
        CATACOMBS_GATE_TITLE,
        THORNWOOD_GATE_TITLE,
        CAVERNS_GATE_TITLE,
        REACHES_GATE_TITLE,
        KAELMYR_GATE_TITLE,
    ];
    for gate in gates {
        assert!(
            road_titles.iter().any(|t| t == gate),
            "the Long Road is missing the gate title {gate}"
        );
    }
    // Every milestone boss is a real spawn, so the road can actually be walked
    // - and every milestone resolves a lair room, so Enter in the journal can
    // track it on the compass/map.
    let w = seed_world();
    for m in LONG_ROAD {
        assert!(
            w.spawns.iter().any(|sp| sp.name == m.boss),
            "Long Road boss {} does not exist in the world",
            m.boss
        );
    }
    let targets = road_targets(&w);
    for (m, t) in LONG_ROAD.iter().zip(&targets) {
        assert!(t.is_some(), "no lair room resolved for {}", m.boss);
    }
    // Fresh titles: nothing done, exactly the first milestone current.
    let road = road_view(&[], &targets);
    assert!(road.iter().all(|s| !s.done));
    assert!(road[0].current);
    assert_eq!(road.iter().filter(|s| s.current).count(), 1);
    // The Treant down: it checks off and the Archdemon becomes current.
    let road = road_view(&[FIRST_DUNGEON_GATE_TITLE.to_string()], &targets);
    assert!(road[0].done);
    assert!(road[1].current);
}

#[test]
fn physical_walls_never_gate_the_long_road_past_the_treant() {
    // This is a solo game: a Physical-locked class must be able to walk the
    // whole mandatory road without another player. Physical-resist bosses may
    // guard optional prizes, but on the Long Road only the Elder Treant wears
    // one - and he sits at the low band where a tier-0 oil's flat rider
    // out-punches the resist, so he teaches the coat instead of gating on it.
    let w = seed_world();
    for m in LONG_ROAD {
        let spawn = w
            .spawns
            .iter()
            .find(|sp| sp.name == m.boss)
            .expect("road boss exists");
        if m.boss == "the Elder Treant" {
            assert_eq!(spawn.profile.resist, Some(DamageType::Physical));
            continue;
        }
        assert_ne!(
            spawn.profile.resist,
            Some(DamageType::Physical),
            "{} resists Physical on the mandatory road",
            m.boss
        );
    }
}

#[test]
fn every_quest_target_and_zone_is_real() {
    let w = seed_world();
    for q in STARTER_QUESTS {
        assert!(
            w.room(q.target).is_some(),
            "starter target room {} missing",
            q.target
        );
        match q.goal {
            StarterGoal::Reach { zone } | StarterGoal::SlayIn { zone, .. } => {
                assert!(
                    w.rooms.values().any(|r| r.zone == zone),
                    "starter zone {zone} does not exist"
                );
            }
            StarterGoal::SlayNamed { name_contains } => {
                assert!(
                    w.spawns.iter().any(|sp| sp.name.contains(name_contains)),
                    "no spawn matches {name_contains}"
                );
            }
        }
    }
    for z in 0..super::super::world::frontier_zone_count() {
        assert!(
            w.room(super::super::world::frontier_zone_entrance(z))
                .is_some(),
            "frontier zone {z} entrance missing"
        );
    }
}

#[test]
fn the_top_poison_tier_is_no_longer_a_clone_of_the_fourth() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    let vial = super::super::items::poison_id(5); // Voidvenom
    s.players.get_mut(&uid(1)).unwrap().inventory.push(vial);
    s.use_item(uid(1), vial);
    assert_eq!(
        s.players[&uid(1)].weapon_coat,
        Some((DamageType::Poison, POISON_PER_TICK[5], POISON_CHARGES)),
        "tier 5 continues the per-tick curve instead of clamping to tier 4's"
    );
    assert!(
        POISON_PER_TICK[5] > POISON_PER_TICK[4],
        "and the curve really does keep climbing at the top"
    );
}

#[test]
fn the_active_coat_shows_on_the_player_view() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().weapon_coat = Some((DamageType::Fire, 21, 8));
    let view = s.snapshot().players[&uid(1)].clone();
    assert_eq!(
        view.coat.as_deref(),
        Some("fire coat x8"),
        "the battle panels read the coat from the view"
    );
}

#[test]
fn a_coated_weapon_works_in_a_duel_too() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.join(uid(2));
    s.choose_class(uid(2), Class::Warrior);
    let pvp_room = any_pvp_room(&s.world);
    s.players.get_mut(&uid(1)).unwrap().room = pvp_room;
    s.players.get_mut(&uid(2)).unwrap().room = pvp_room;
    s.engage_player(uid(1), uid(2));
    s.players.get_mut(&uid(1)).unwrap().weapon_coat = Some((DamageType::Fire, 10, OIL_CHARGES));
    s.tick();
    assert!(
        s.pvp_dots.get(&uid(2)).is_some_and(|d| d.iter().any(|dot| {
            dot.owner == uid(1) && dot.per_tick == 10 && dot.school == DamageType::Fire
        })),
        "the landed duel swing seeds the coat's school DoT on the rival"
    );
    assert_eq!(
        s.players[&uid(1)].weapon_coat.map(|(_, _, c)| c),
        Some(OIL_CHARGES - 1),
        "the duel swing spends one coat charge"
    );
    // And the one-wound rule holds in a duel too. It matters more here than
    // against a mob: a pvp dot is *not* pre-scaled, so each tick is charged
    // against the victim's armor live, and a stacking coat would have made a
    // 12-charge vial worth a third of an endgame duel all by itself.
    // Both duellists need the hit points to still be trading blows five
    // swings in, or the wound count below only proves someone died.
    for id in [uid(1), uid(2)] {
        let p = s.players.get_mut(&id).unwrap();
        p.base_max_hp = 4000;
        p.hp = p.max_hp();
    }
    for _ in 0..4 {
        s.tick();
    }
    assert!(
        s.players[&uid(1)].pvp_target == Some(uid(2)),
        "the duel is still running"
    );
    let stacks = s.pvp_dots.get(&uid(2)).map(Vec::as_slice).unwrap_or(&[]);
    assert_eq!(
        stacks.iter().filter(|d| d.owner == uid(1)).count(),
        1,
        "five landed duel swings, one coat wound"
    );
}
