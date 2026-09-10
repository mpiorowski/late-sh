use super::care_bar_spans;
use crate::app::hub::aquarium::state::CareBar;
use late_core::models::aquarium_care::CARE_DAYS;

fn glyphs(bar: CareBar) -> String {
    care_bar_spans(bar)
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn the_care_bar_is_always_fourteen_boxes() {
    // Five fed days: five full, nine empty.
    assert_eq!(glyphs(CareBar::Streak(5)), "■■■■■□□□□□□□□□");
    // Two unfed days: two full (red), the rest empty.
    assert_eq!(glyphs(CareBar::Dry(2)), "■■□□□□□□□□□□□□");
    // A minded tank shows nothing filled.
    assert_eq!(glyphs(CareBar::Minded), "□".repeat(CARE_DAYS as usize));
    // Nothing past fourteen ever widens the title.
    assert_eq!(glyphs(CareBar::Dry(40)), "■".repeat(CARE_DAYS as usize));
}
