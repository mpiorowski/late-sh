use super::strip_frame_changed;
use crate::app::pet::state::PetMood;

#[test]
fn parked_sad_pet_never_pays_a_frame() {
    // Activity 0 parks the pet mid-zone with no blink and a limp tail: the
    // strip is fully static, so no tick may report a change.
    for tick in 0..500 {
        assert!(!strip_frame_changed(PetMood::Sad, tick, 40));
    }
}

#[test]
fn active_pet_changes_on_blink_edges_and_skips_still_ticks() {
    // Blink turns on at tick % 64 == 0 and off at tick % 64 == 3; both edges
    // repaint regardless of where the wander is.
    assert!(strip_frame_changed(PetMood::Happy, 64, 40));
    assert!(strip_frame_changed(PetMood::Happy, 67, 40));

    // The gate only pays for ticks where the art moves: across a whole blink
    // period an ambling pet must have both changed and clean ticks.
    let changed_ticks = (1..=64)
        .filter(|&tick| strip_frame_changed(PetMood::Happy, tick, 40))
        .count();
    assert!(changed_ticks > 0, "an active pet animates");
    assert!(changed_ticks < 64, "an active pet still has clean ticks");
}

#[test]
fn zero_travel_still_blinks() {
    // A strip too narrow to wander still has blink and tail edges.
    assert!(strip_frame_changed(PetMood::Happy, 64, 0));
}
