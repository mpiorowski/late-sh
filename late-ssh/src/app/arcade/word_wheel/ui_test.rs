use super::*;

/// A click lands on the letter the frame drew, or the cell either side of
/// it; a spoke or the gap between letters is no letter.
#[test]
fn a_click_lands_on_the_wheel_letter_drawn() {
    let area = Rect::new(0, 0, 100, 40);
    let wheel = layout(game_content_area(area, true, SHOW_GAME_BOTTOM_BAR)).wheel;
    let at = |column: u16, row: u16| hit_test(area, wheel.x + column, wheel.y + row);
    assert_eq!(
        [
            at(9, 2),
            at(8, 2),
            at(9, 0),
            at(15, 1),
            at(16, 1),
            at(3, 3),
            at(12, 1),
            at(9, 1),
            at(0, 0),
        ],
        [
            Some(WheelCell::Hub),
            Some(WheelCell::Hub),
            Some(WheelCell::Rim(0)),
            Some(WheelCell::Rim(1)),
            Some(WheelCell::Rim(1)),
            Some(WheelCell::Rim(4)),
            None,
            None,
            None,
        ]
    );
}
