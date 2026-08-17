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
fn terminal_theme_defers_to_the_terminals_own_colors() {
    set_current_by_id("terminal");
    let canvas = BG_CANVAS();
    let body = CHAT_BODY();
    let text = TEXT();
    let faint = TEXT_FAINT();
    let dim = TEXT_DIM();
    let amber = color_rgb(AMBER()).expect("amber resolves to rgb");
    let amber_dim = color_rgb(AMBER_DIM()).expect("amber_dim resolves to rgb");
    set_current_by_id("contrast");

    // The reading pair is the terminal's own default background/foreground:
    // legible on whatever profile the user actually configured, and leaving
    // the background alone is what keeps terminal transparency working.
    assert_eq!(canvas, Color::Reset);
    assert_eq!(body, Color::Reset);
    assert_eq!(text, Color::Reset);

    // Faint and dim sharing Indexed(8) collapsed two tiers onto the one gray
    // that sits closest to a dark background, which is what made secondary
    // text unreadable.
    assert_ne!(faint, dim, "text_faint and text_dim are the same color");

    let sum = |(r, g, b): (u8, u8, u8)| r as u32 + g as u32 + b as u32;
    assert!(
        sum(amber_dim) < sum(amber),
        "amber_dim {amber_dim:?} is brighter than amber {amber:?}"
    );
}

#[test]
fn canvas_relative_blends_darken_when_the_canvas_has_no_rgb() {
    // The terminal palette's canvas is `Color::Reset`, which cannot be read
    // back. Blends used to return their unblended anchor there, so the Sudoku
    // "same number" wash painted as full-strength tan behind the digits.
    set_current_by_id("terminal");
    let wash = SUDOKU_SAME_NUM_BG();
    set_current_by_id("contrast");

    let (r, g, b) = color_rgb(wash).expect("wash resolves to rgb");
    assert!(
        r < 210 && g < 180 && b < 90,
        "wash {wash:?} was not blended toward the canvas"
    );
}

#[test]
fn attention_washes_drop_out_when_the_canvas_has_no_rgb() {
    // The mention/reply washes sit under body text, and on the terminal
    // palette body text is the terminal's own default foreground. No fixed
    // wash color can promise contrast against an unknown foreground (the
    // flat highlight fallback painted solid ANSI-blue bars that swallowed a
    // light profile's dark text), so the wash drops out entirely and the
    // mention/author accents on the text carry the emphasis.
    set_current_by_id("terminal");
    let mention = CHAT_MENTION_BG();
    let reply = CHAT_REPLY_BG();
    set_current_by_id("contrast");

    assert_eq!(mention, Color::Reset);
    assert_eq!(reply, Color::Reset);
}

#[test]
fn selection_inverts_on_reset_canvas_palettes_and_fills_elsewhere() {
    // A Reset canvas means text follows the terminal's own foreground, and
    // no fixed selection fill can guarantee contrast against it, so those
    // palettes invert the terminal's own fg/bg pair instead of painting
    // `bg_selection`. The helper composes via `Style::patch`, which only
    // overwrites `Some` fields, so the swap must carry explicit Reset
    // colors: a call site's pre-set accent fg or board fill would otherwise
    // survive and the swapped pair would not be the terminal's own.
    set_current_by_id("terminal");
    let inverted = Style::default()
        .fg(Color::Indexed(11))
        .bg(Color::Indexed(4))
        .patch(selection_style());
    set_current_by_id("contrast");
    let filled = Style::default()
        .fg(Color::Indexed(11))
        .patch(selection_style());
    let filled_bg = BG_SELECTION();

    assert!(inverted.add_modifier.contains(Modifier::REVERSED));
    assert_eq!(
        inverted.fg,
        Some(Color::Reset),
        "inherited accent fg survived the swap"
    );
    assert_eq!(
        inverted.bg,
        Some(Color::Reset),
        "inherited fill survived the swap"
    );
    assert_eq!(filled.bg, Some(filled_bg));
    assert_eq!(
        filled.fg,
        Some(Color::Indexed(11)),
        "paintable palettes keep the accent fg over the fill"
    );
    assert!(!filled.add_modifier.contains(Modifier::REVERSED));
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
