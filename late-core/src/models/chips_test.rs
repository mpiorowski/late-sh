use crate::models::chips::*;

#[test]
fn difficulty_bonus_mapping() {
    assert_eq!(difficulty_bonus("easy"), 100);
    assert_eq!(difficulty_bonus("medium"), 250);
    assert_eq!(difficulty_bonus("mid"), 250);
    assert_eq!(difficulty_bonus("hard"), 500);
    assert_eq!(difficulty_bonus("draw-1"), 250);
    assert_eq!(difficulty_bonus("draw-3"), 500);
    assert_eq!(difficulty_bonus("unknown"), 100);
}

#[test]
fn constants() {
    assert_eq!(CHIP_FLOOR, 100);
    assert_eq!(INITIAL_CHIP_BALANCE, 1_000);
}
