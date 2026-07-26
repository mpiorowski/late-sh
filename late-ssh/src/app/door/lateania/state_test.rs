use super::{InvAction, inv_action};
use crate::app::door::lateania::svc::InvView;

fn row(slot: Option<&str>, equipped: bool) -> InvView {
    InvView {
        item_id: 1,
        name: "Iron Sword".to_string(),
        rarity: "common".to_string(),
        slot: slot.map(|s| s.to_string()),
        equipped,
        sell_price: 10,
        stats: String::new(),
        compare: String::new(),
        compare_pct: None,
        category: "Weapons",
    }
}

#[test]
fn enter_takes_off_worn_gear() {
    // The bug: the inventory panel lists worn gear, and Enter on it routed to
    // equip, which found the item missing from the pack and returned in
    // silence. There was no way to unequip anything at all.
    assert_eq!(inv_action(&row(Some("weapon"), true)), InvAction::Unequip);
}

#[test]
fn enter_equips_loose_gear_and_uses_consumables() {
    assert_eq!(inv_action(&row(Some("weapon"), false)), InvAction::Equip);
    assert_eq!(inv_action(&row(None, false)), InvAction::Use);
}
