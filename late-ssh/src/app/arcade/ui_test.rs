use crate::app::arcade::ui::*;
use ratatui::layout::Rect;

#[test]
fn centered_rect_centers_inside_larger_area() {
    let area = Rect::new(2, 3, 80, 24);
    let centered = centered_rect(area, 30, 10);

    assert_eq!(centered, Rect::new(27, 10, 30, 10));
}

#[test]
fn centered_rect_clamps_to_available_area() {
    let area = Rect::new(2, 3, 80, 24);
    let centered = centered_rect(area, 100, 40);

    assert_eq!(centered, area);
}
