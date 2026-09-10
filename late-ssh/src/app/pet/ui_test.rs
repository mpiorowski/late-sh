use std::cell::Cell;

use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use tokio::sync::broadcast;

use super::{PET_STRIP_HEIGHT, PetTravel, PetView, draw_pet_box, frame_changed};
use crate::app::activity::event::ActivityEvent;
use crate::app::pet::state::{PetMood, PetState};
use crate::test_helpers::new_test_db;

const WIDE: PetTravel = PetTravel { x: 40, y: 0 };

#[test]
fn parked_sad_pet_never_pays_a_frame() {
    // A hungry pet sits on the floor with no blink and a limp tail: the box
    // is fully static, so no tick may report a change.
    for tick in 0..500 {
        assert!(!frame_changed(PetMood::Sad, tick, WIDE));
    }
}

#[test]
fn fed_pet_changes_on_blink_edges_and_skips_still_ticks() {
    // Blink turns on at tick % 64 == 0 and off at tick % 64 == 3; both edges
    // repaint regardless of where the stroll is.
    assert!(frame_changed(PetMood::Happy, 64, WIDE));
    assert!(frame_changed(PetMood::Happy, 67, WIDE));

    // The gate only pays for ticks where the art moves: across a whole blink
    // period a strolling pet must have both changed and clean ticks.
    let changed_ticks = (1..=64)
        .filter(|&tick| frame_changed(PetMood::Happy, tick, WIDE))
        .count();
    assert!(changed_ticks > 0, "a fed pet animates");
    assert!(changed_ticks < 64, "a fed pet still has clean ticks");
}

#[test]
fn zero_travel_still_blinks() {
    // A box too small to roam still has blink and tail edges.
    assert!(frame_changed(
        PetMood::Happy,
        64,
        PetTravel { x: 0, y: 0 }
    ));
}

/// Draw the box at `tick` and report where the pet landed.
fn pet_rect_at(state: &mut PetState, tick: usize, area: Rect) -> Rect {
    state.tick(tick);
    let pet_rect = Cell::new(None);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            draw_pet_box(
                frame,
                area,
                &PetView {
                    state,
                    pet_rect_slot: Some(&pet_rect),
                    bowl_rect_slot: None,
                    travel_slot: None,
                },
            );
        })
        .unwrap();
    pet_rect.get().expect("the box drew the pet")
}

#[tokio::test]
async fn hungry_pet_sits_on_the_floor_and_a_fed_pet_roams_the_whole_box() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-roams-box").await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = crate::app::pet::svc::PetService::new(test_db.db.clone(), tx);
    let cat = svc.ensure_cat(user.id).await.expect("ensure cat");
    let mut state = PetState::new(user.id, svc, cat);
    let area = Rect::new(0, 0, 60, 12);
    let floor_y = area.bottom() - PET_STRIP_HEIGHT;

    // Hungry: parked on the floor, and still there a thousand ticks later.
    assert_eq!(pet_rect_at(&mut state, 5, area).y, floor_y);
    assert_eq!(pet_rect_at(&mut state, 1005, area), pet_rect_at(&mut state, 5, area));

    // Fed: over a long stroll the pet reaches the top and the far right of
    // its box, and comes back down to the floor; nothing pins it anywhere.
    state.feed();
    let mut top = u16::MAX;
    let mut bottom = 0;
    let mut right = 0;
    for tick in (0..6000).step_by(7) {
        let rect = pet_rect_at(&mut state, tick, area);
        top = top.min(rect.y);
        bottom = bottom.max(rect.y);
        right = right.max(rect.right());
        assert!(rect.bottom() <= area.bottom(), "the pet stays inside its box");
    }
    assert_eq!(top, area.y, "the stroll reaches the top of the box");
    assert_eq!(bottom, floor_y, "the stroll comes back to the floor");
    assert!(
        right > area.width / 2,
        "the stroll crosses into the right half (reached {right})"
    );
}
