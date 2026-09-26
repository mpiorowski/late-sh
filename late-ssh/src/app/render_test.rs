use super::{
    AUTO_RIGHT_SIDEBAR_MIN_COLS, AUTO_ROOM_LIST_MIN_COLS, app_frame_sponsor_title,
    dashboard_home_selected, line_width, resolve_right_sidebar_enabled, resolve_room_list_enabled,
    room_list_sidebar_enabled, sidebar_enabled, sponsor_line,
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

#[test]
fn sponsor_title_drops_optional_segments_to_fit_its_available_width() {
    let full_width = line_width(&sponsor_line(true, true));
    let url_width = line_width(&sponsor_line(false, true));
    let short_url_width = line_width(&sponsor_line(false, false));

    let full = app_frame_sponsor_title(full_width).expect("full sponsor should fit");
    assert_eq!(
        line_text(&full),
        " thanks for hanging out ☕ https://ko-fi.com/mateuszpiorowski "
    );

    // Each fallback keeps the blank cell on both sides of the link: the title
    // is drawn over the bottom border, so a URL flush against `─` gets the
    // glyph linkified along with it.
    let url_only = app_frame_sponsor_title(full_width - 1).expect("url-only sponsor should fit");
    assert_eq!(line_text(&url_only), " https://ko-fi.com/mateuszpiorowski ");

    let short_url =
        app_frame_sponsor_title(url_width - 1).expect("protocol-stripped sponsor should fit");
    assert_eq!(line_text(&short_url), " ko-fi.com/mateuszpiorowski ");

    let hidden = app_frame_sponsor_title(short_url_width - 1);
    assert!(hidden.is_none());
}
