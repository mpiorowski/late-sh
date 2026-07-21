use super::*;
use crate::app::door::lateania::items::item;

#[test]
fn every_recipe_has_real_items_and_a_sane_gate() {
    for r in recipes() {
        assert!(
            item(r.output).is_some(),
            "recipe output {} is not a real item",
            r.output
        );
        assert!(r.output_qty >= 1, "a recipe must yield something");
        assert!(
            (1..=super::super::skills::SKILL_MAX_LEVEL).contains(&r.level_req),
            "recipe gate {} out of range",
            r.level_req
        );
        assert!(r.xp > 0, "crafting must grant xp");
        assert!(!r.inputs.is_empty(), "a recipe needs inputs");
        for ingredient in &r.inputs {
            assert!(
                item(ingredient.item).is_some(),
                "recipe input {} is not a real item",
                ingredient.item
            );
            assert!(ingredient.qty >= 1, "an input needs a positive quantity");
        }
    }
}

#[test]
fn every_craft_skill_has_recipes_and_indices_are_stable() {
    for skill in CraftSkill::ALL {
        let idx = recipe_indices_for(skill);
        assert!(!idx.is_empty(), "no recipes for {}", skill.label());
        for &i in &idx {
            assert_eq!(
                recipe(i).unwrap().skill,
                skill,
                "index {i} maps to {skill:?}"
            );
        }
    }
    // 10 recipes per tier x 5 tiers, plus two masterwork sinks.
    assert_eq!(recipes().len(), 52);
}

#[test]
fn masterwork_recipes_are_an_endgame_sink() {
    let mw = recipes()
        .iter()
        .find(|r| r.output == masterwork_id(0))
        .expect("a masterwork blade recipe exists");
    assert!(mw.level_req >= 40, "masterwork demands near-max skill");
    assert!(
        mw.inputs
            .iter()
            .any(|i| i.item == ingot_id(4) && i.qty >= 5),
        "masterwork eats a heap of the top-tier ingot"
    );
}

#[test]
fn chains_link_up_ore_becomes_ingot_becomes_weapon() {
    // The tier-0 sword recipe must consume the tier-0 ingot, which is itself
    // a recipe output - a real chain, not a flat table.
    let sword = recipes()
        .iter()
        .find(|r| r.output == smith_weapon_id(0))
        .expect("a tier-0 sword recipe exists");
    assert!(
        sword.inputs.iter().any(|i| i.item == ingot_id(0)),
        "the sword is forged from ingots"
    );
    assert!(
        recipes().iter().any(|r| r.output == ingot_id(0)),
        "the ingot is itself craftable from ore"
    );
}
