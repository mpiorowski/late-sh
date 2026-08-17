use super::{InvAction, inv_action, is_leave_confirm_pending};
use crate::app::door::lateania::svc::InvView;
use std::time::{Duration, Instant};

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
        desc: "A well-balanced blade.",
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

// ---- combat action-bar click hit-testing ----------------------------------

use super::{ClickAction, hit_at};
use ratatui::layout::Rect;

#[test]
fn a_click_resolves_to_the_chip_under_it() {
    let hits = vec![
        (Rect::new(0, 20, 5, 1), ClickAction::Attack),
        (Rect::new(6, 20, 7, 1), ClickAction::Quaff),
        (Rect::new(14, 20, 6, 1), ClickAction::Flee),
    ];
    assert_eq!(
        hit_at(&hits, 2, 20),
        Some(ClickAction::Attack),
        "inside the attack chip"
    );
    assert_eq!(
        hit_at(&hits, 6, 20),
        Some(ClickAction::Quaff),
        "left edge is inclusive"
    );
    assert_eq!(
        hit_at(&hits, 12, 20),
        Some(ClickAction::Quaff),
        "right edge is exclusive-1"
    );
    assert_eq!(
        hit_at(&hits, 13, 20),
        None,
        "the gap between chips hits nothing"
    );
    assert_eq!(
        hit_at(&hits, 2, 19),
        None,
        "a click on the row above misses"
    );
    assert_eq!(
        hit_at(&hits, 40, 20),
        None,
        "a click past the last chip misses"
    );
}

// ---- the "press Esc twice to leave" confirmation window -------------------

#[test]
fn leave_confirm_is_pending_only_within_its_window() {
    // The bug this guards: a single stray Esc used to log a player straight
    // out of Lateania instantly, with no way to back out of it.
    let now = Instant::now();
    assert!(
        !is_leave_confirm_pending(None, now),
        "no deadline armed at all: not pending"
    );
    assert!(
        is_leave_confirm_pending(Some(now + Duration::from_secs(6)), now),
        "a deadline still ahead of now is pending"
    );
    assert!(
        !is_leave_confirm_pending(Some(now), now),
        "the exact deadline instant itself has already lapsed"
    );
    assert!(
        !is_leave_confirm_pending(Some(now - Duration::from_secs(1)), now),
        "a deadline already in the past is not pending"
    );
}
