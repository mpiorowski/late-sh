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
    assert_eq!(materials().len(), 30, "five skills x six tiers");
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
        62,
        "ten crafted kinds x six tiers, plus two masterwork sinks"
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
fn shops_offer_gold_sinks_without_selling_legendary_gear() {
    // Gold shops should still give a player somewhere to sink surplus gold
    // (a pricey consumable, a handful of desirable Epic pieces) - but
    // Legendary-tier EQUIPMENT was moved out of every shop entirely, because a
    // plain gold purchase (Mythril Arming Sword, the Masterwork armor,
    // Dragonbone Reliquary) used to outclass early Frontier drops, which made
    // a "brutal" zone hand out a downgrade. Legendary consumables (a top-shelf
    // potion) are unaffected - they're spent, not equipped, so they don't
    // create the same power-creep problem.
    let all_stock: Vec<_> = SHOPS
        .iter()
        .flat_map(|shop| shop.stock.iter().filter_map(|id| item(*id)))
        .collect();
    let costly: Vec<_> = all_stock.iter().filter(|it| it.price >= 700).collect();
    assert!(
        costly.len() >= 4,
        "shops should offer enough desirable expensive stock to sink gold into"
    );
    assert!(
        costly
            .iter()
            .any(|it| matches!(it.kind, ItemKind::Consumable { .. })),
        "shops should include a repeatable expensive consumable"
    );
    assert!(
        all_stock
            .iter()
            .any(|it| matches!(it.kind, ItemKind::Consumable { .. }) && it.price >= 2_000),
        "the top-end gold sink is a premium repeatable consumable (>= 2,000g), \
         since Legendary equipment left the shops"
    );
    assert!(
        all_stock
            .iter()
            .all(|it| !matches!(it.kind, ItemKind::Equipment(_)) || it.rarity != Rarity::Legendary),
        "no shop should sell Legendary equipment - that tier is earned, not bought"
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
fn outfitter_sells_real_upgrades_across_every_slot() {
    // Legs and Feet used to have exactly one shop item each (the Common
    // starter piece) and nothing past it - no upgrade path at all for those
    // two slots. Every wearable slot should now have a real ladder.
    let outfitter = SHOPS
        .iter()
        .find(|shop| shop.shop_name == "The Outfitter's Stall")
        .expect("outfitter shop exists");
    let stock: Vec<_> = outfitter.stock.iter().filter_map(|id| item(*id)).collect();

    for slot in [Slot::Head, Slot::Chest, Slot::Legs, Slot::Hands, Slot::Feet] {
        let in_slot: Vec<_> = stock.iter().filter(|it| it.slot() == Some(slot)).collect();
        assert!(
            in_slot.len() >= 4,
            "{slot:?} should have a real upgrade ladder in the outfitter, got {}",
            in_slot.len()
        );
        let cheapest = in_slot.iter().map(|it| it.power()).min().unwrap();
        let priciest = in_slot.iter().map(|it| it.power()).max().unwrap();
        assert!(
            priciest > cheapest * 2,
            "{slot:?}'s best shop piece should clearly outclass its starter piece ({cheapest} -> {priciest})"
        );
    }
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
fn frontier_tier_one_beats_every_shop_slot() {
    // The reported bug, pinned down: "the Frontier has worse loot than can be
    // bought at a shop." A player braving the Frontier's very first zone
    // should never come out with a downgrade over what plain gold already
    // bought them in town, in any slot.
    let shop_ceiling_power = |slot: Slot| -> i32 {
        SHOPS
            .iter()
            .flat_map(|shop| shop.stock.iter().filter_map(|id| item(*id)))
            .filter(|it| it.slot() == Some(slot))
            .map(Item::power)
            .max()
            .unwrap_or(0)
    };
    let tier1: Vec<&Item> = frontier_loot(0).iter().filter_map(|id| item(*id)).collect();
    for slot in Slot::WEARABLE {
        let piece = tier1
            .iter()
            .find(|it| it.slot() == Some(slot))
            .unwrap_or_else(|| panic!("Frontier tier 1 must cover every wearable slot: {slot:?}"));
        let ceiling = shop_ceiling_power(slot);
        // Real headroom, not a squeak: a one-point win over a shop piece (or
        // one that trades away armor) still reads as a sidegrade in the field.
        assert!(
            piece.power() >= ceiling + 5,
            "Frontier tier-1 {slot:?} (power {}) should clear the shop ceiling (power {ceiling}) with headroom",
            piece.power()
        );
    }
}

#[test]
fn every_trinket_and_ring_carries_a_visible_bonus() {
    // Reported bug: trinkets that show no bonus you can actually see. Every
    // Trinket/Ring across every catalog - hand-authored, and the three
    // generated realms - must carry a nonzero stat and a non-empty
    // stat_summary(), so the inventory/shop panels always show something.
    let all_catalogs: Vec<&Item> = ITEMS
        .iter()
        .chain(frontier_items().iter())
        .chain(reaches_items().iter())
        .chain(kaelmyr_items().iter())
        .collect();
    let mut checked = 0;
    for it in all_catalogs {
        if !matches!(it.slot(), Some(Slot::Trinket) | Some(Slot::Ring)) {
            continue;
        }
        checked += 1;
        assert!(
            it.mods.attack != 0 || it.mods.max_hp != 0 || it.mods.armor != 0,
            "{} ({:?}) has no visible stat bonus",
            it.name,
            it.slot()
        );
        assert!(
            !it.stat_summary().is_empty(),
            "{} has an empty stat summary",
            it.name
        );
    }
    assert!(
        checked >= 20,
        "expected a real body of trinkets/rings to check, got {checked}"
    );
}

#[test]
fn crafted_gear_climbs_every_tier_and_clears_the_shop_ceiling() {
    // Every crafted weapon/armor line (smith sword, war-bow, plate, leather
    // jerkin) should be a decent, strictly-improving upgrade tier over tier -
    // no flat or backwards steps - and the top tier (skill 100, the trade's
    // own endgame) should clear what plain gold can buy in the same slot.
    type CraftLine = (fn(u32) -> u32, Slot);
    let lines: [CraftLine; 4] = [
        (smith_weapon_id, Slot::Weapon),
        (wood_weapon_id, Slot::Weapon),
        (smith_armor_id, Slot::Chest),
        (leather_armor_id, Slot::Chest),
    ];
    for (id_fn, slot) in lines {
        let mut prev = 0;
        for tier in 0..6u32 {
            let it = item(id_fn(tier)).unwrap_or_else(|| panic!("tier {tier} exists"));
            assert!(
                it.power() > prev,
                "{} (tier {tier}) should beat the previous tier ({} -> {})",
                it.name,
                prev,
                it.power()
            );
            prev = it.power();
        }
        let shop_ceiling = SHOPS
            .iter()
            .flat_map(|shop| shop.stock.iter().filter_map(|id| item(*id)))
            .filter(|it| it.slot() == Some(slot))
            .map(Item::power)
            .max()
            .unwrap_or(0);
        assert!(
            prev > shop_ceiling,
            "the top crafted tier in {slot:?} (power {prev}) should clear the shop ceiling (power {shop_ceiling})"
        );
    }
}

#[test]
fn wildbound_adds_108_regional_finds_with_unique_resolvable_ids() {
    let finds = regional_finds();
    assert_eq!(finds.len(), 108, "14+20+20 zones x 2 pieces each");
    let mut ids: Vec<u32> = finds.iter().map(|it| it.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, ids.len(), "duplicate regional-find id");
    for it in finds {
        assert!(
            item(it.id).is_some(),
            "{} should resolve through item()",
            it.name
        );
        assert!(it.power() > 0, "{} has no visible bonus", it.name);
        assert_ne!(
            it.rarity,
            Rarity::Common,
            "a real find should never be Common"
        );
    }
}

#[test]
fn sunderlakes_and_broceliande_finds_stay_under_the_frontier_ceiling() {
    // Peaceful/moderate continents: their new finds should never outclass the
    // Frontier's own top tier, so exploring them for gear stays a nice bonus
    // rather than eclipsing the actual endgame.
    let frontier_ceiling = |slot: Slot| -> i32 {
        (0..super::super::items::FRONTIER_TIERS)
            .flat_map(|t| frontier_loot(t).iter().filter_map(|id| item(*id)))
            .filter(|it| it.slot() == Some(slot))
            .map(Item::power)
            .max()
            .unwrap_or(0)
    };
    for zone in 0..14 {
        for id in sunderlakes_find_ids(zone) {
            let it = item(id).expect("sunderlakes find resolves");
            let Some(slot) = it.slot() else { continue };
            assert!(
                it.power() < frontier_ceiling(slot),
                "{} (power {}) should stay under the Frontier's own ceiling",
                it.name,
                it.power()
            );
        }
    }
    for zone in 0..20 {
        for id in broceliande_find_ids(zone) {
            let it = item(id).expect("broceliande find resolves");
            let Some(slot) = it.slot() else { continue };
            assert!(
                it.power() < frontier_ceiling(slot),
                "{} (power {}) should stay under the Frontier's own ceiling",
                it.name,
                it.power()
            );
        }
    }
}

#[test]
fn archipelago_finds_outclass_kaelmyrs_deepest_tier() {
    // The deadliest ground in the world should genuinely outclass even
    // Kaelmyr's own top tier, not just tie it.
    let kaelmyr_ceiling = |slot: Slot| -> i32 {
        kaelmyr_loot(KAELMYR_TIERS - 1)
            .iter()
            .filter_map(|id| item(*id))
            .filter(|it| it.slot() == Some(slot))
            .map(Item::power)
            .max()
            .unwrap_or(0)
    };
    for isle in 0..20 {
        for id in archipelago_find_ids(isle) {
            let it = item(id).expect("archipelago find resolves");
            let Some(slot) = it.slot() else { continue };
            assert!(
                it.power() > kaelmyr_ceiling(slot),
                "{} (power {}) should outclass Kaelmyr's deepest tier (power {})",
                it.name,
                it.power(),
                kaelmyr_ceiling(slot)
            );
        }
    }
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

#[test]
fn generated_loot_covers_the_previously_dropped_gear_slots() {
    // Offsets 3/5/7 in each tier block are Legs/Feet/Trinket; the old loot table
    // skipped them, so those two-plus slots never progressed from drops. Guard
    // that every tier now drops the full gear block (offsets 0..=7).
    let tables = super::generated_loot_tables(super::FRONTIER_ITEM_BASE, super::FRONTIER_TIERS);
    assert_eq!(tables.len(), super::FRONTIER_TIERS);
    for (t, table) in tables.iter().enumerate() {
        let base = super::FRONTIER_ITEM_BASE + t as u32 * 10;
        for offset in 0u32..8 {
            assert!(
                table.contains(&(base + offset)),
                "tier {t} loot table missing gear offset {offset}"
            );
        }
    }
}
