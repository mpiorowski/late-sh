use super::*;
#[test]
fn normalize_unknown_theme_to_default() {
    assert_eq!(normalize_id("wat"), "contrast");
}

#[test]
fn cycle_theme_wraps() {
    let first = OPTIONS
        .first()
        .expect("theme options should not be empty")
        .id;
    let last = OPTIONS
        .last()
        .expect("theme options should not be empty")
        .id;

    assert_eq!(cycle_id(last, true), first);
    assert_eq!(cycle_id(first, false), last);
}

#[test]
fn text_brightness_adjustment_lightens_and_darkens_primary_text() {
    assert_eq!(
        adjust_color_lightness(Color::Rgb(100, 150, 200), 5),
        Color::Rgb(201, 218, 236)
    );
    assert_eq!(
        adjust_color_lightness(Color::Rgb(100, 150, 200), -5),
        Color::Rgb(40, 60, 80)
    );
    assert_eq!(
        adjust_color_lightness(Color::Rgb(100, 150, 200), 0),
        Color::Rgb(100, 150, 200)
    );

    set_current_by_id("late");
    set_text_brightness_adjustment(0);
    assert_eq!(TEXT(), Color::Rgb(175, 158, 138));
    assert_eq!(TEXT_BRIGHT(), Color::Rgb(200, 182, 158));
    assert_eq!(CHAT_BODY(), Color::Rgb(190, 178, 165));

    set_text_brightness_adjustment(-5);
    assert_eq!(TEXT(), Color::Rgb(70, 63, 55));
    assert_eq!(TEXT_BRIGHT(), Color::Rgb(80, 73, 63));
    assert_eq!(CHAT_BODY(), Color::Rgb(76, 71, 66));

    set_text_brightness_adjustment(5);
    assert_eq!(TEXT(), Color::Rgb(227, 221, 214));
    assert_eq!(TEXT_BRIGHT(), Color::Rgb(236, 229, 221));
    assert_eq!(CHAT_BODY(), Color::Rgb(232, 228, 224));

    set_text_brightness_adjustment(0);
    set_current_by_id("late");
}

#[test]
fn every_theme_group_has_distinct_bit() {
    let mut mask = 0u32;
    for group in ThemeGroup::ALL {
        let bit = group.bit();
        assert_ne!(bit, 0);
        assert_eq!(mask & bit, 0);
        mask |= bit;
    }
}
