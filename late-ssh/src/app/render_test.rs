use super::{
    AUTO_RIGHT_SIDEBAR_MIN_COLS, FRAME_TABS, app_frame_bottom_titles, app_frame_sponsor_title,
    chat_title_hint, dashboard_home_selected, line_width, resolve_right_sidebar_enabled,
    sidebar_enabled, sponsor_line, tab_is_lit,
};
use crate::app::common::primitives::Screen;
use late_core::models::user::RightSidebarMode;
use uuid::Uuid;

/// A terminal wide enough that `Auto` keeps every rail, so the `On`/`Off` cases
/// below are unaffected by width.
const WIDE_TERMINAL: u16 = 200;

fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn sidebar_enabled_prefers_settings_draft_while_modal_is_open() {
    assert!(!sidebar_enabled(true, false, true));
    assert!(sidebar_enabled(true, true, false));
}

#[test]
fn sidebar_enabled_uses_saved_profile_when_modal_is_closed() {
    assert!(sidebar_enabled(false, false, true));
    assert!(!sidebar_enabled(false, true, false));
}

#[test]
fn right_sidebar_is_the_arcades_alone() {
    // Home's sidebar is its tiles (`app/zen`, the Home page).
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Dashboard,
        WIDE_TERMINAL,
    ));
    assert!(resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Arcade,
        WIDE_TERMINAL,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Lateania,
        WIDE_TERMINAL,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Artboard,
        WIDE_TERMINAL,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Profiles,
        WIDE_TERMINAL,
    ));
}

#[test]
fn right_sidebar_off_hides_on_allowed_pages() {
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::Off,
        Screen::Dashboard,
        WIDE_TERMINAL,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::Off,
        Screen::Arcade,
        WIDE_TERMINAL,
    ));
}

#[test]
fn auto_sidebar_folds_away_as_the_terminal_narrows() {
    // The Arcade keeps its sidebar on a desktop and a landscape phone; a
    // portrait phone gets the page's full width.
    for cols in [200, AUTO_RIGHT_SIDEBAR_MIN_COLS] {
        assert!(
            resolve_right_sidebar_enabled(RightSidebarMode::Auto, Screen::Arcade, cols),
            "{cols}"
        );
    }
    for cols in [AUTO_RIGHT_SIDEBAR_MIN_COLS - 1, 50, 0] {
        assert!(
            !resolve_right_sidebar_enabled(RightSidebarMode::Auto, Screen::Arcade, cols),
            "{cols}"
        );
    }
}

#[test]
fn explicit_sidebar_modes_ignore_terminal_width() {
    // Only `Auto` consults the width: someone who asked for the sidebar on
    // a narrow terminal keeps it, and `Off` stays off however wide the
    // window gets.
    for cols in [0, 40, 200] {
        assert!(
            resolve_right_sidebar_enabled(RightSidebarMode::On, Screen::Arcade, cols),
            "{cols}"
        );
        assert!(
            !resolve_right_sidebar_enabled(RightSidebarMode::Off, Screen::Arcade, cols),
            "{cols}"
        );
    }
    // Auto still never puts the sidebar on a page that has no sidebar:
    // Home's sidebar is its tiles now.
    for screen in [Screen::Artboard, Screen::Dashboard] {
        assert!(!resolve_right_sidebar_enabled(
            RightSidebarMode::Auto,
            screen,
            200,
        ));
    }
}

#[test]
fn dashboard_home_selected_for_lounge_room_without_synthetic_entry() {
    let lounge = Uuid::from_u128(1);
    assert!(dashboard_home_selected(Some(lounge), Some(lounge), false));
}

#[test]
fn dashboard_home_selected_rejects_synthetic_and_non_lounge_rooms() {
    let lounge = Uuid::from_u128(1);
    let topic = Uuid::from_u128(2);
    assert!(!dashboard_home_selected(Some(lounge), Some(lounge), true));
    assert!(!dashboard_home_selected(Some(lounge), Some(topic), false));
    assert!(!dashboard_home_selected(None, Some(topic), false));
}

/// The sponsor line has one flexible part: its thanks. The link itself is
/// all or nothing.
#[test]
fn sponsor_title_drops_its_thanks_before_its_link() {
    let full_width = line_width(&sponsor_line(true));
    let link_width = line_width(&sponsor_line(false));

    let full = app_frame_sponsor_title(full_width).expect("full sponsor should fit");
    assert_eq!(
        line_text(&full),
        " thanks for hanging out ☕ https://late.sh/thanks "
    );

    // The link keeps the blank cell on both sides: the title is drawn over
    // the bottom border, so a URL flush against `─` gets the glyph linkified
    // along with it.
    let link_only = app_frame_sponsor_title(full_width - 1).expect("the link should fit");
    assert_eq!(line_text(&link_only), " https://late.sh/thanks ");

    assert!(app_frame_sponsor_title(link_width - 1).is_none());
}

/// The sponsor's link is set aside before the bar gets any room: however many
/// segments are switched on, the bar is the one that drops them.
#[test]
fn sponsor_link_keeps_its_place_however_full_the_status_bar_is() {
    use crate::app::statusline::data::StatusData;
    use late_core::models::statusline::{StatusComponentSetting, default_statusline_components};
    use ratatui::layout::Rect;

    let everything_on: Vec<StatusComponentSetting> = default_statusline_components()
        .into_iter()
        .map(|setting| StatusComponentSetting {
            enabled: true,
            auto_hide: false,
            ..setting
        })
        .collect();
    let data = StatusData {
        clock_24: "14:32",
        clock_ampm: "2:32 pm",
        station_name: "chillsynth",
        ..StatusData::default()
    };
    let area = Rect::new(0, 0, 120, 40);

    let (bar, sponsor) = app_frame_bottom_titles(&everything_on, &data, area);

    let sponsor = sponsor.expect("the sponsor link survives a full bar");
    assert!(line_text(&sponsor).contains("https://late.sh/thanks"));
    let bar = bar.expect("the bar keeps what fits beside the sponsor");
    assert!(
        line_width(&bar.line) + line_width(&sponsor) <= usize::from(area.width - 2),
        "the two titles share the row without overlapping"
    );
    assert!(line_text(&bar.line).contains("Settings"));
}

/// With room to spare the sponsor line carries its thanks too.
#[test]
fn sponsor_line_adds_its_thanks_when_the_status_bar_leaves_room() {
    use crate::app::statusline::data::StatusData;
    use late_core::models::statusline::default_statusline_components;
    use ratatui::layout::Rect;

    let (bar, sponsor) = app_frame_bottom_titles(
        &default_statusline_components(),
        &StatusData::default(),
        Rect::new(0, 0, 200, 40),
    );

    assert!(bar.is_some());
    assert_eq!(
        line_text(&sponsor.expect("sponsor")),
        " thanks for hanging out ☕ https://late.sh/thanks "
    );
}

/// No segment is special: on a terminal too narrow for the Keyhints beside
/// the sponsor's link, the hints are dropped like anything else and the
/// narrower segments after them still paint.
#[test]
fn keyhints_too_wide_for_the_row_are_dropped_like_any_other_segment() {
    use crate::app::statusline::data::StatusData;
    use late_core::models::statusline::default_statusline_components;
    use ratatui::layout::Rect;

    let (bar, sponsor) = app_frame_bottom_titles(
        &default_statusline_components(),
        &StatusData::default(),
        Rect::new(0, 0, 80, 24),
    );

    assert_eq!(
        line_text(&sponsor.expect("sponsor link")),
        " https://late.sh/thanks "
    );
    let bar = line_text(&bar.expect("the narrower segments still fit").line);
    assert!(!bar.contains("Settings"), "{bar:?}");
    assert!(bar.contains("unread 0"), "{bar:?}");
}

/// The lit tab is the page a screen belongs to, the one Tab steps back to:
/// a screen off the cycle lights exactly that tab, and Zen lights none.
#[test]
fn the_lit_tab_is_the_page_the_screen_belongs_to() {
    let lit = |screen: Screen| -> Vec<Screen> {
        FRAME_TABS
            .iter()
            .map(|(tab, _)| *tab)
            .filter(|tab| tab_is_lit(*tab, screen))
            .collect()
    };
    for (screen, tab) in [
        (Screen::Games, Screen::Games),
        (Screen::Nethack, Screen::Games),
        (Screen::Darkroom, Screen::Games),
        (Screen::Realm, Screen::Games),
        (Screen::DailyMatch, Screen::Dashboard),
        (Screen::HouseTable, Screen::Dashboard),
        (Screen::Scratchpad, Screen::Dashboard),
        (Screen::Nightcap, Screen::Clubhouse),
        (Screen::City, Screen::Clubhouse),
    ] {
        assert_eq!(lit(screen), vec![tab], "{screen:?}");
    }
    assert_eq!(lit(Screen::Zen), Vec::<Screen>::new());
}

/// The frame title's word for a game's shut chat: dim with nothing new,
/// bright with the count once somebody has talked, nothing while there is no
/// chat to open.
#[test]
fn the_chat_title_hint_counts_what_was_said_while_the_chat_was_shut() {
    use crate::app::common::theme;
    let text = |unread: Option<i64>| {
        chat_title_hint(unread).map(|span| (span.content.to_string(), span.style.fg))
    };

    assert_eq!(
        text(Some(0)),
        Some(("\u{b7} F4 chat ".to_string(), Some(theme::TEXT_DIM())))
    );
    assert_eq!(
        text(Some(3)),
        Some(("\u{b7} F4 chat (3 new) ".to_string(), Some(theme::AMBER())))
    );
    assert_eq!(text(None), None);
}
