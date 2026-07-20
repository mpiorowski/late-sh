// Crafting recipes for Lateania.
//
// A recipe turns a set of input items (raw materials from gathering, or refined
// intermediates) into an output item, training a craft skill (`skills::CraftSkill`).
// Recipes chain - ore -> ingot -> weapon - so the maker's economy has depth. This
// module is data only: the service (`svc::craft`) resolves the recipe, checks the
// station/skill/materials, consumes the inputs and grants the output plus xp.
//
// Output ids come from the `items` crafted-goods catalog; input ids come from the
// raw-material catalog (`items::material_id`) or lower-tier crafted intermediates.

use std::sync::OnceLock;

use super::items::{
    food_id, ingot_id, leather_armor_id, leather_id, masterwork_id, material_id, plank_id,
    poison_id, potion_id, smith_armor_id, smith_weapon_id, wood_weapon_id,
};
use super::skills::CraftSkill;

/// One input line of a recipe: an item id and how many are consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ingredient {
    pub item: u32,
    pub qty: u32,
}

const fn ing(item: u32, qty: u32) -> Ingredient {
    Ingredient { item, qty }
}

// Raw-material ids by gathering skill index (see `GatherSkill::index`): logs=0,
// ore=1, fish=2, herbs=3, hides=4.
const fn ore(t: u32) -> u32 {
    material_id(1, t)
}
const fn log(t: u32) -> u32 {
    material_id(0, t)
}
const fn hide(t: u32) -> u32 {
    material_id(4, t)
}
const fn herb(t: u32) -> u32 {
    material_id(3, t)
}
const fn fish(t: u32) -> u32 {
    material_id(2, t)
}

/// A crafting recipe: inputs -> output, gated behind a craft skill and level.
#[derive(Clone, Debug)]
pub struct Recipe {
    /// The item produced.
    pub output: u32,
    /// How many of the output a single craft yields (usually 1).
    pub output_qty: u32,
    pub skill: CraftSkill,
    pub level_req: i32,
    pub xp: i32,
    pub inputs: Vec<Ingredient>,
}

/// Skill-level gate per tier (matches the gathering node gates).
const LEVEL_REQ: [i32; 5] = [1, 8, 16, 26, 38];
/// Xp for refining a raw material into an intermediate (the cheaper step).
const REFINE_XP: [i32; 5] = [8, 20, 45, 100, 200];
/// Xp for crafting a finished good (matches the gather xp curve).
const CRAFT_XP: [i32; 5] = [12, 30, 70, 150, 320];

fn build_recipes() -> Vec<Recipe> {
    use CraftSkill::*;
    let mut r = Vec::new();
    for t in 0..5u32 {
        let ti = t as usize;
        let (gate, refine, craft) = (LEVEL_REQ[ti], REFINE_XP[ti], CRAFT_XP[ti]);

        // ---- Refining: 2 raw -> 1 intermediate --------------------------
        r.push(Recipe {
            output: ingot_id(t),
            output_qty: 1,
            skill: Smithing,
            level_req: gate,
            xp: refine,
            inputs: vec![ing(ore(t), 2)],
        });
        r.push(Recipe {
            output: plank_id(t),
            output_qty: 1,
            skill: Woodworking,
            level_req: gate,
            xp: refine,
            inputs: vec![ing(log(t), 2)],
        });
        r.push(Recipe {
            output: leather_id(t),
            output_qty: 1,
            skill: Leatherworking,
            level_req: gate,
            xp: refine,
            inputs: vec![ing(hide(t), 2)],
        });

        // ---- Smithing: weapon (ingots + a plank grip) and plate ---------
        r.push(Recipe {
            output: smith_weapon_id(t),
            output_qty: 1,
            skill: Smithing,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(ingot_id(t), 3), ing(plank_id(t), 1)],
        });
        r.push(Recipe {
            output: smith_armor_id(t),
            output_qty: 1,
            skill: Smithing,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(ingot_id(t), 4)],
        });

        // ---- Woodworking: a bow (planks + a leather grip) ---------------
        r.push(Recipe {
            output: wood_weapon_id(t),
            output_qty: 1,
            skill: Woodworking,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(plank_id(t), 3), ing(leather_id(t), 1)],
        });

        // ---- Leatherworking: light armor --------------------------------
        r.push(Recipe {
            output: leather_armor_id(t),
            output_qty: 1,
            skill: Leatherworking,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(leather_id(t), 3)],
        });

        // ---- Alchemy: a healing draught and a coating poison ------------
        r.push(Recipe {
            output: potion_id(t),
            output_qty: 1,
            skill: Alchemy,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(herb(t), 2)],
        });
        r.push(Recipe {
            output: poison_id(t),
            output_qty: 1,
            skill: Alchemy,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(herb(t), 3)],
        });

        // ---- Cooking: a restorative meal --------------------------------
        r.push(Recipe {
            output: food_id(t),
            output_qty: 1,
            skill: Cooking,
            level_req: gate,
            xp: craft,
            inputs: vec![ing(fish(t), 1)],
        });
    }

    // ---- Masterwork: the endgame smithing sinks -------------------------
    // Made from a heap of the very best materials at near-max Smithing; the
    // gear step above every tiered craftable, and a real material sink.
    r.push(Recipe {
        output: masterwork_id(0),
        output_qty: 1,
        skill: Smithing,
        level_req: 45,
        xp: 600,
        inputs: vec![
            ing(ingot_id(4), 8),
            ing(plank_id(4), 2),
            ing(leather_id(4), 2),
        ],
    });
    r.push(Recipe {
        output: masterwork_id(1),
        output_qty: 1,
        skill: Smithing,
        level_req: 45,
        xp: 600,
        inputs: vec![ing(ingot_id(4), 10), ing(leather_id(4), 3)],
    });
    r
}

/// Every recipe, built once and leaked to 'static for the service.
pub fn recipes() -> &'static [Recipe] {
    static RECIPES: OnceLock<Vec<Recipe>> = OnceLock::new();
    RECIPES.get_or_init(build_recipes)
}

/// The recipe at a global index (the stable id the UI passes back to craft).
pub fn recipe(index: usize) -> Option<&'static Recipe> {
    recipes().get(index)
}

/// Global indices of the recipes worked at a given craft skill's station, in
/// table order.
pub fn recipe_indices_for(skill: CraftSkill) -> Vec<usize> {
    recipes()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.skill == skill)
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
#[path = "crafting_test.rs"]
mod crafting_test;

