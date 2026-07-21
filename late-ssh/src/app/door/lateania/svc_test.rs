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
    assert_eq!(
        item_category(&ItemKind::Consumable {
            heal: 30,
            restore: 0
        }),
        "Consumables"
    );
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
    assert!(p.weapon_poison.is_some(), "the weapon is coated");
    assert!(!p.inventory.contains(&poison), "the vial is used up");
}

#[test]
fn a_coated_weapon_poisons_the_foe_and_spends_a_charge() {
    let (mut s, mob_id) = engaged_with(MobBehavior::Brute);
    s.players.get_mut(&uid(1)).unwrap().weapon_poison = Some((10, POISON_CHARGES));
    s.tick();
    assert_eq!(
        s.players[&uid(1)].weapon_poison.map(|(_, c)| c),
        Some(POISON_CHARGES - 1),
        "a landed strike spends one poison charge"
    );
    assert!(
        s.mob_dots.get(&mob_id).is_some_and(|d| !d.is_empty()),
        "the struck foe is left with a poison DoT"
    );
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
    use super::super::world::{TASMANIA_SQUARE, features_at};
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    s.players.get_mut(&uid(1)).unwrap().room = TASMANIA_SQUARE;
    let board = features_at(TASMANIA_SQUARE)
        .iter()
        .position(|f| f.kind == FeatureKind::Board)
        .expect("a board stands in the Tasmania square");

    // First examine accepts the next bounty (id 1).
    s.interact(uid(1), board);
    assert!(
        s.players[&uid(1)]
            .board_progress
            .iter()
            .any(|(id, _)| *id == 1),
        "examining the board accepts the next bounty"
    );

    // Force it complete, then claim on the next examine.
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
    let gold_before = s.players[&uid(1)].gold;
    s.interact(uid(1), board);
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
    let board = super::super::world::features_at(super::super::world::TASMANIA_SQUARE)
        .iter()
        .position(|f| f.kind == FeatureKind::Board)
        .expect("board in the square");
    // Take and finish the daily bounty (id 1), then claim it.
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .board_progress
        .push((1, 99));
    s.interact(uid(1), board);
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
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.hp = 1;
    }
    // The player starts in the town square, home of the hearth-cat (Mend boon).
    s.apply_critter_perks(uid(1));
    assert!(s.players[&uid(1)].hp > 1, "the hearth-cat should mend you");
}

#[test]
fn unclassed_player_cannot_move_or_fight() {
    let mut s = world();
    s.join(uid(1));
    s.move_player(uid(1), Dir::South);
    assert_eq!(s.players[&uid(1)].room, s.world.start_room);
    s.engage(uid(1));
    assert!(s.players[&uid(1)].target.is_none());
}

#[test]
fn buying_costs_gold_and_adds_item() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
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
    assert_eq!(s.players[&uid(1)].room, 1);
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
fn continent_waystones_honor_their_walking_gate_titles() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // Standing on Embergate's town waystone (room 1): Kaelmyr's far gate
    // stays sealed until the Yssgar crown is earned.
    assert_eq!(s.players[&uid(1)].room, 1);
    s.travel(uid(1), super::super::world::KAELMYR_BASE);
    assert_eq!(s.players[&uid(1)].room, 1, "a sealed gate refuses the ways");
    s.players
        .get_mut(&uid(1))
        .unwrap()
        .titles
        .push(KAELMYR_GATE_TITLE.to_string());
    s.travel(uid(1), super::super::world::KAELMYR_BASE);
    assert_eq!(
        s.players[&uid(1)].room,
        super::super::world::KAELMYR_BASE,
        "the crowned traveler passes"
    );
    // Open lands need no title, and the town waystone routes home again.
    s.travel(uid(1), super::super::world::LAKES_BASE);
    assert_eq!(s.players[&uid(1)].room, super::super::world::LAKES_BASE);
    s.travel(uid(1), 1);
    assert_eq!(s.players[&uid(1)].room, 1);
}

#[test]
fn continent_waystone_titles_match_the_walking_gates() {
    for (label, room, required) in super::super::world::CONTINENT_WAYSTONES {
        if super::super::world::is_kaelmyr_room(*room) {
            assert_eq!(*required, Some(KAELMYR_GATE_TITLE), "{label}");
        } else if super::super::world::is_reaches_room(*room) {
            assert_eq!(*required, Some(REACHES_GATE_TITLE), "{label}");
        } else {
            assert_eq!(*required, None, "{label} is an open land");
        }
    }
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
fn buying_a_companion_costs_gold_and_sets_a_pet() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
    // A fresh adventurer stands in Embergate's square, which has a stable.
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

// The forest gate (entrance) room of Broceliande zone 0, where the easiest
// tameable beasts roam.
fn broceliande_beast_room() -> RoomId {
    super::super::world::BROCELIANDE_BASE
}

#[test]
fn taming_a_beast_makes_it_your_companion_and_trains_the_trade() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Ranger);
    // Stand where the easiest beasts (level 1) gather, and be well-trained so
    // the roll is a near-sure thing; force success by maxing the odds.
    s.players.get_mut(&uid(1)).unwrap().room = broceliande_beast_room();
    s.players.get_mut(&uid(1)).unwrap().taming_xp = super::super::skills::xp_for_skill_level(50);
    // The easiest beast in the room is the first tameable species.
    let beasts = super::super::taming::beasts_at(broceliande_beast_room());
    assert!(!beasts.is_empty(), "beasts roam the first forest gate");
    let before_xp = s.players[&uid(1)].taming_xp;
    // Try a few times; a master's odds cap at 95%, so one of a handful lands.
    for _ in 0..40 {
        if s.players[&uid(1)].pet.is_some() {
            break;
        }
        s.tame(uid(1), 0);
    }
    let p = &s.players[&uid(1)];
    assert!(p.pet.is_some(), "a successful tame yields a companion");
    assert!(
        p.pet.unwrap().species.is_tameable(),
        "the companion is a tamed wild beast"
    );
    assert!(p.taming_xp > before_xp, "taming trains Animal Taming xp");
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
fn rogue_opening_strike_is_flagged_then_consumed() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Rogue);
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
    // Move to a combat room with a mob (room 6, goblin) and engage.
    s.move_player(uid(1), Dir::South);
    s.move_player(uid(1), Dir::South);
    s.engage(uid(1));

    s.tick();

    let log = &s.players[&uid(1)].log;
    assert!(
        log.iter()
            .any(|line| line.kind == LogKind::Combat && line.text.starts_with("You strike ")),
        "auto-attacks should be visible in the combat log"
    );
}

#[test]
fn movement_keeps_a_compact_travel_line_in_recent_log() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Mage);
    s.move_player(uid(1), Dir::North);

    assert!(
        s.players[&uid(1)]
            .log
            .iter()
            .any(|line| line.kind == LogKind::Travel
                && line.text == "Arrived at Embergate - The Gilded Flagon."),
        "movement should leave a compact room-visit breadcrumb"
    );
}

#[test]
fn warrior_does_not_arm_opening_strike() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior);
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
fn final_bosses_map_to_lifetime_achievements() {
    let archdemon = boss_achievement_for("the Archdemon Mal'gareth")
        .expect("authored final boss should grant an achievement");
    let archdemon_payout = archdemon.payout.expect("archdemon pays chips");
    assert_eq!(archdemon_payout.reward_key, LATEANIA_ARCHDEMON_REWARD_KEY);
    assert_eq!(
        archdemon_payout.ledger_reason,
        LATEANIA_ARCHDEMON_LEDGER_REASON
    );
    assert_eq!(archdemon.award_category, LATEANIA_ARCHDEMON_AWARD_CATEGORY);

    let frontier_king = boss_achievement_for("the King Who Was Promised Nothing")
        .expect("last Frontier boss should grant an achievement");
    let king_payout = frontier_king.payout.expect("frontier king pays chips");
    assert_eq!(king_payout.reward_key, LATEANIA_FRONTIER_KING_REWARD_KEY);
    assert_eq!(
        king_payout.ledger_reason,
        LATEANIA_FRONTIER_KING_LEDGER_REASON
    );
    assert_eq!(
        frontier_king.award_category,
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY
    );

    let yssgar = boss_achievement_for("Yssgar, the Sundering Deep")
        .expect("the Reaches' crowned boss should grant an achievement");
    assert!(
        yssgar.payout.is_none(),
        "Yssgar's badge is the whole prize; no chip payout"
    );
    assert_eq!(
        yssgar.award_category,
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY
    );

    let kaethyr = boss_achievement_for("Kaethyr Ascendant, Who Sang the God Awake")
        .expect("Kaelmyr's last boss should grant an achievement");
    assert!(
        kaethyr.payout.is_none(),
        "Kaethyr's badge is the whole prize; no chip payout"
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
fn ability_scores_change_derived_stats() {
    let mut s = world();
    s.join(uid(1));
    s.choose_class(uid(1), Class::Warrior); // STR is the warrior's key score
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.scores.strength = 10;
        p.scores.constitution = 10;
    }
    let base_attack = s.players[&uid(1)].attack();
    let base_hp = s.players[&uid(1)].max_hp();
    if let Some(p) = s.players.get_mut(&uid(1)) {
        p.scores.strength = 18; // +4
        p.scores.constitution = 18; // +4
    }
    assert!(
        s.players[&uid(1)].attack() > base_attack,
        "STR raises attack"
    );
    assert!(s.players[&uid(1)].max_hp() > base_hp, "CON raises max HP");
}
