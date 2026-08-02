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
fn dim_and_faint_text_never_matches_a_highlight_background() {
    // Rows can be overlaid with a selection/highlight background while
    // keeping their existing (possibly dim/faint) foreground color, so if a
    // palette assigns a highlight background the same color as its dim/faint
    // text, that text goes invisible on selection (the Terminal palette's
    // Indexed(8) collision this guards against).
    for option in OPTIONS {
        set_current_by_id(option.id);
        let dim = TEXT_DIM();
        let faint = TEXT_FAINT();
        let selection = BG_SELECTION();
        let highlight = BG_HIGHLIGHT();
        assert_ne!(
            dim, selection,
            "{}: text_dim matches bg_selection",
            option.id
        );
        assert_ne!(
            faint, selection,
            "{}: text_faint matches bg_selection",
            option.id
        );
        assert_ne!(
            dim, highlight,
            "{}: text_dim matches bg_highlight",
            option.id
        );
        assert_ne!(
            faint, highlight,
            "{}: text_faint matches bg_highlight",
            option.id
        );
    }
    set_current_by_id("contrast");
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
