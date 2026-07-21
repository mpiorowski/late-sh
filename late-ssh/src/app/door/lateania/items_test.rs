use super::*;

#[test]
fn item_ids_are_unique() {
    let mut ids: Vec<u32> = ITEMS
        .iter()
        .chain(frontier_items().iter())
        .chain(reaches_items().iter())
        .chain(kaelmyr_items().iter())
        .chain(materials().iter())
        .chain(crafted().iter())
        .chain(fish().iter())
        .map(|i| i.id)
        .collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, ids.len(), "duplicate item id");
}

#[test]
fn materials_form_a_clean_sellable_catalog() {
    assert_eq!(materials().len(), 25, "five skills x five tiers");
    for m in materials() {
        assert!(
            m.id >= MATERIAL_BASE && m.id < MATERIAL_BASE + 100,
            "material {} sits in the 4000 band",
            m.id
        );
        assert!(
            matches!(m.kind, ItemKind::Valuable),
            "raw materials are sellable valuables for now"
        );
        assert!(m.sell_price() >= 1, "materials are worth something");
        // Look-ups resolve through the shared catalog.
        assert!(item(m.id).is_some(), "material {} is not findable", m.id);
    }
}

#[test]
fn every_equippable_item_carries_real_stats() {
    // No dead gear: every wearable item in every catalog must actually
    // grant at least one stat, so nothing is a pure downgrade to bare hands.
    for it in ITEMS
        .iter()
        .chain(frontier_items().iter())
        .chain(reaches_items().iter())
    {
        if matches!(it.kind, ItemKind::Equipment(_)) {
            assert!(
                it.power() > 0,
                "equippable item {} ({}) has no stats",
                it.id,
                it.name
            );
        }
    }
}

#[test]
fn crafted_goods_form_a_clean_catalog() {
    assert_eq!(
        crafted().len(),
        52,
        "ten crafted kinds x five tiers, plus two masterwork sinks"
    );
    for c in crafted() {
        assert!(
            c.id >= CRAFTED_BASE && c.id < CRAFTED_BASE + 300,
            "crafted item {} sits in the 4200 band",
            c.id
        );
        assert!(c.sell_price() >= 1, "crafted goods are worth something");
        assert!(
            item(c.id).is_some(),
            "crafted item {} is not findable",
            c.id
        );
        assert!(
            materials().iter().all(|m| m.id != c.id),
            "crafted item {} collides with a raw material",
            c.id
        );
    }
}

#[test]
fn fish_catalog_is_a_clean_band_of_sell_and_edible_species() {
    let all = fish();
    assert_eq!(all.len() as u32, FISH_COUNT, "forty fish species");
    let mut edible = 0;
    let mut sell_only = 0;
    let mut specials = 0;
    let mut min_price = i64::MAX;
    let mut max_price = 0;
    for f in all {
        assert!(
            f.id >= FISH_BASE && f.id < FISH_BASE + 100,
            "fish {} sits in the 4600 band",
            f.id
        );
        // Clear of every other catalog band.
        assert!(f.id >= 4600, "fish must not collide with materials/crafted");
        assert!(item(f.id).is_some(), "fish {} is not findable", f.id);
        assert!(f.sell_price() >= 1, "every fish is worth something");
        min_price = min_price.min(f.price);
        max_price = max_price.max(f.price);
        match f.kind {
            ItemKind::Consumable { heal, restore } => {
                edible += 1;
                assert!(heal > 0 || restore > 0, "an edible fish must do something");
                if fish_well_fed(f.id).is_some() {
                    specials += 1;
                }
            }
            ItemKind::Valuable => sell_only += 1,
            ItemKind::Equipment(_) => panic!("no fish is equipment"),
        }
    }
    // Roughly a third edible, the rest pure sell loot.
    assert!(
        (10..=16).contains(&edible),
        "about a third of fish should be edible, got {edible}"
    );
    assert_eq!(
        edible + sell_only,
        FISH_COUNT as i32,
        "no third kind of fish"
    );
    assert!(specials >= 3, "a few rare fish carry a well-fed special");
    // A wide price spread: cheap minnows to prized several-hundred-gold catches.
    assert!(
        min_price <= 15,
        "there are a few-gold minnows, got {min_price}"
    );
    assert!(
        max_price >= 500,
        "there are prized catches, got {max_price}"
    );
    // Only fish carry a well-fed special outside the food catalog.
    for f in all {
        if let Some(regen) = fish_well_fed(f.id) {
            assert!(regen > 0 && regen <= 10, "special regen is modest");
            assert!(
                matches!(f.kind, ItemKind::Consumable { .. }),
                "a special fish must be edible"
            );
        }
    }
    assert_eq!(fish_well_fed(FISH_BASE + FISH_COUNT + 1), None);
}

#[test]
fn power_ranks_gear_and_is_zero_for_non_gear() {
    let sword = ITEMS
        .iter()
        .find(|it| matches!(it.kind, ItemKind::Equipment(Slot::Weapon)))
        .expect("a weapon exists");
    assert!(sword.power() > 0);
    assert!(
        ITEMS
            .iter()
            .filter(|it| matches!(it.kind, ItemKind::Consumable { .. }))
            .all(|it| it.power() == 0),
        "consumables have no gear-power"
    );
}

#[test]
fn every_shop_sells_real_items() {
    for shop in SHOPS {
        assert!(!shop.stock.is_empty(), "{} has no stock", shop.shop_name);
        for id in shop.stock {
            assert!(item(*id).is_some(), "shop sells missing item {id}");
        }
    }
}

#[test]
fn shops_offer_late_gold_sinks() {
    let costly: Vec<_> = SHOPS
        .iter()
        .flat_map(|shop| shop.stock.iter().filter_map(|id| item(*id)))
        .filter(|it| it.price >= 1_500)
        .collect();
    assert!(
        costly.len() >= 6,
        "shops should offer enough expensive late-game stock"
    );
    assert!(
        costly
            .iter()
            .any(|it| matches!(it.kind, ItemKind::Consumable { .. })),
        "shops should include a repeatable expensive consumable"
    );
}

#[test]
fn apothecary_consumables_scale_into_late_recovery() {
    let minor = item(1300).expect("minor draught exists");
    let potion = item(1301).expect("healing potion exists");
    let greater = item(1302).expect("greater elixir exists");
    let renewal = item(1304).expect("renewal elixir exists");
    let phoenix = item(1305).expect("phoenix tonic exists");

    let healing = |it: &Item| match it.kind {
        ItemKind::Consumable { heal, restore } => (heal, restore),
        _ => panic!("expected consumable"),
    };

    assert!(healing(minor).0 < healing(potion).0);
    assert!(healing(potion).0 < healing(greater).0);
    assert!(healing(renewal).0 >= 180 && healing(renewal).1 >= 120);
    assert!(healing(phoenix).0 >= 400 && healing(phoenix).1 >= 200);
}

#[test]
fn outfitter_sells_real_head_and_hand_upgrades() {
    let outfitter = SHOPS
        .iter()
        .find(|shop| shop.shop_name == "The Outfitter's Stall")
        .expect("outfitter shop exists");
    let stock: Vec<_> = outfitter.stock.iter().filter_map(|id| item(*id)).collect();

    assert!(
        stock
            .iter()
            .any(|it| it.slot() == Some(Slot::Head) && it.price >= 2_000),
        "outfitter should sell a late-game helm"
    );
    assert!(
        stock
            .iter()
            .any(|it| it.slot() == Some(Slot::Hands) && it.price >= 2_000),
        "outfitter should sell late-game gloves"
    );
}

#[test]
fn frontier_loot_includes_head_and_hands() {
    let slots: Vec<_> = frontier_loot(0)
        .iter()
        .filter_map(|id| item(*id).and_then(Item::slot))
        .collect();
    assert!(slots.contains(&Slot::Head), "frontier should drop helms");
    assert!(
        slots.contains(&Slot::Hands),
        "frontier should drop gauntlets"
    );
}

#[test]
fn equipment_reports_its_slot() {
    for it in ITEMS {
        if let ItemKind::Equipment(slot) = it.kind {
            assert_eq!(it.slot(), Some(slot));
        } else {
            assert_eq!(it.slot(), None);
        }
    }
}

#[test]
fn sell_price_is_never_zero() {
    for it in ITEMS {
        assert!(it.sell_price() >= 1, "{} sells for nothing", it.name);
    }
}

#[test]
fn reaches_loot_outclasses_the_deepest_frontier_tier() {
    // The Reaches continue the Frontier's power curve: entry-tier Reaches
    // gear must beat the Frontier's top tier, and the whole catalog must
    // resolve through item(id) in the 3200..3400 range.
    let frontier_top = item(3000 + 19 * 10).expect("deepest frontier blade exists");
    let reaches_entry = item(REACHES_ITEM_BASE).expect("first reaches blade exists");
    assert!(
        reaches_entry.mods.attack > frontier_top.mods.attack,
        "reaches entry gear should out-damage the deepest frontier gear"
    );
    for tier in 0..REACHES_TIERS as u32 {
        for i in 0..10 {
            let id = REACHES_ITEM_BASE + tier * 10 + i;
            assert!(item(id).is_some(), "reaches item {id} should resolve");
            assert!(
                id < REACHES_ITEM_BASE + 200,
                "reaches ids must stay in 3200..3400"
            );
        }
    }
}

#[test]
fn kaelmyr_loot_outclasses_the_deepest_reaches_tier() {
    // Kaelmyr continues the curve one continent past the Reaches: entry-tier
    // Kaelmyr gear must beat the Reaches' top tier, and the whole catalog must
    // resolve through item(id) in the 3400..3600 band.
    let reaches_top = item(REACHES_ITEM_BASE + 19 * 10).expect("deepest reaches blade exists");
    let kaelmyr_entry = item(KAELMYR_ITEM_BASE).expect("first kaelmyr blade exists");
    assert!(
        kaelmyr_entry.mods.attack > reaches_top.mods.attack,
        "kaelmyr entry gear should out-damage the deepest reaches gear"
    );
    for tier in 0..KAELMYR_TIERS as u32 {
        for i in 0..10 {
            let id = KAELMYR_ITEM_BASE + tier * 10 + i;
            assert!(item(id).is_some(), "kaelmyr item {id} should resolve");
            assert!(
                (KAELMYR_ITEM_BASE..KAELMYR_ITEM_BASE + 200).contains(&id),
                "kaelmyr ids must stay in 3400..3600"
            );
        }
    }
}

#[test]
fn kaelmyr_relics_state_they_are_not_combat_items() {
    for tier in 0..KAELMYR_TIERS {
        let id = KAELMYR_ITEM_BASE + (tier as u32) * 10 + 9;
        let relic = item(id).expect("kaelmyr relic should exist");
        assert_eq!(relic.kind, ItemKind::Valuable);
        assert!(
            relic.desc.contains("no combat use"),
            "{} should explain its lack of combat use",
            relic.name
        );
    }
}

#[test]
fn reaches_relics_state_they_are_not_combat_items() {
    for tier in 0..REACHES_TIERS {
        let id = REACHES_ITEM_BASE + (tier as u32) * 10 + 9;
        let relic = item(id).expect("reaches relic should exist");
        assert_eq!(relic.kind, ItemKind::Valuable);
        assert!(
            relic.desc.contains("no combat use"),
            "{} should explain its lack of combat use",
            relic.name
        );
    }
}

#[test]
fn valuables_explain_their_sell_use() {
    for it in ITEMS
        .iter()
        .chain(frontier_items().iter())
        .chain(reaches_items().iter())
        .chain(kaelmyr_items().iter())
    {
        if it.kind == ItemKind::Valuable {
            let summary = it.stat_summary();
            assert!(
                summary.contains("valuable") && summary.contains("sell"),
                "{} should explain that it is sell loot, got {summary:?}",
                it.name
            );
            assert!(
                summary.contains(&format!("{}g", it.sell_price())),
                "{} should show its sell value, got {summary:?}",
                it.name
            );
        }
    }
}

#[test]
fn frontier_relics_state_they_are_not_combat_items() {
    for tier in 0..FRONTIER_TIERS {
        let id = 3000 + (tier as u32) * 10 + 9;
        let relic = item(id).expect("frontier relic should exist");
        assert_eq!(relic.kind, ItemKind::Valuable);
        assert!(
            relic.desc.contains("no combat use"),
            "{} should explain its lack of combat use",
            relic.name
        );
    }
}
