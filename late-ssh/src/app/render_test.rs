use super::{
    AUTO_RIGHT_SIDEBAR_MIN_COLS, AUTO_ROOM_LIST_MIN_COLS, app_frame_bottom_titles,
    app_frame_sponsor_title, dashboard_home_selected, line_width, resolve_right_sidebar_enabled,
    resolve_room_list_enabled, room_list_sidebar_enabled, sidebar_enabled, sponsor_line,
};
use crate::app::common::primitives::Screen;
use late_core::models::user::{RightSidebarMode, RoomListMode};
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
fn right_sidebar_is_only_available_on_first_three_pages() {
    assert!(resolve_right_sidebar_enabled(
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
fn auto_rails_fold_away_as_the_terminal_narrows() {
    // A desktop keeps both rails; a landscape phone drops the room rail but
    // keeps the ambient sidebar; a portrait phone gets the chat's full width.
    for cols in [200, AUTO_ROOM_LIST_MIN_COLS] {
        assert!(
            resolve_room_list_enabled(RoomListMode::Auto, cols),
            "{cols}"
        );
        assert!(
            resolve_right_sidebar_enabled(RightSidebarMode::Auto, Screen::Dashboard, cols),
            "{cols}"
        );
    }
    for cols in [AUTO_ROOM_LIST_MIN_COLS - 1, AUTO_RIGHT_SIDEBAR_MIN_COLS] {
        assert!(
            !resolve_room_list_enabled(RoomListMode::Auto, cols),
            "{cols}"
        );
        assert!(
            resolve_right_sidebar_enabled(RightSidebarMode::Auto, Screen::Dashboard, cols),
            "{cols}"
        );
    }
    for cols in [AUTO_RIGHT_SIDEBAR_MIN_COLS - 1, 50, 0] {
        assert!(
            !resolve_room_list_enabled(RoomListMode::Auto, cols),
            "{cols}"
        );
        assert!(
            !resolve_right_sidebar_enabled(RightSidebarMode::Auto, Screen::Dashboard, cols),
            "{cols}"
        );
    }
}

#[test]
fn explicit_rail_modes_ignore_terminal_width() {
    // Only `Auto` consults the width: someone who asked for a rail on a narrow
    // terminal keeps it, and `Off` stays off however wide the window gets.
    for cols in [0, 40, 200] {
        assert!(resolve_room_list_enabled(RoomListMode::On, cols), "{cols}");
        assert!(
            !resolve_room_list_enabled(RoomListMode::Off, cols),
            "{cols}"
        );
        assert!(
            resolve_right_sidebar_enabled(RightSidebarMode::On, Screen::Dashboard, cols),
            "{cols}"
        );
    }
    // Auto still never puts the sidebar on a page that has no sidebar.
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::Auto,
        Screen::Artboard,
        200,
    ));
}

#[test]
fn room_list_sidebar_enabled_prefers_settings_draft_while_modal_is_open() {
    assert!(!room_list_sidebar_enabled(true, false, true));
    assert!(room_list_sidebar_enabled(true, true, false));
}

#[test]
fn room_list_sidebar_enabled_uses_saved_profile_when_modal_is_closed() {
    assert!(room_list_sidebar_enabled(false, false, true));
    assert!(!room_list_sidebar_enabled(false, true, false));
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
        " thanks for hanging out ☕ https://ko-fi.com/mateuszpiorowski "
    );

    // The link keeps the blank cell on both sides: the title is drawn over
    // the bottom border, so a URL flush against `─` gets the glyph linkified
    // along with it.
    let link_only = app_frame_sponsor_title(full_width - 1).expect("the link should fit");
    assert_eq!(
        line_text(&link_only),
        " https://ko-fi.com/mateuszpiorowski "
    );

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
    assert!(line_text(&sponsor).contains("https://ko-fi.com/mateuszpiorowski"));
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
        " thanks for hanging out ☕ https://ko-fi.com/mateuszpiorowski "
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
        " https://ko-fi.com/mateuszpiorowski "
    );
    let bar = line_text(&bar.expect("the narrower segments still fit").line);
    assert!(!bar.contains("Settings"), "{bar:?}");
    assert!(bar.contains("unread 0"), "{bar:?}");
}
