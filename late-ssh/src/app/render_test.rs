use super::{
    HelpHintStyle, app_frame_bottom_titles, app_frame_help_hint_title, app_frame_sponsor_title,
    dashboard_home_selected, line_width, resolve_right_sidebar_enabled,
    room_list_sidebar_enabled, sidebar_enabled, sponsor_line, status_hud_title,
};
use crate::app::common::primitives::Screen;
use late_core::models::user::RightSidebarMode;
use uuid::Uuid;

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
    ));
    assert!(resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Arcade,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Lateania,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Artboard,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::On,
        Screen::Pinstar,
    ));
}

#[test]
fn right_sidebar_off_hides_on_allowed_pages() {
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::Off,
        Screen::Dashboard,
    ));
    assert!(!resolve_right_sidebar_enabled(
        RightSidebarMode::Off,
        Screen::Arcade,
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
fn status_hud_title_hidden_when_empty() {
    assert!(status_hud_title(None, 0, None).is_none());
    assert!(status_hud_title(None, -3, None).is_none());
}

#[test]
fn status_hud_title_renders_right_aligned_pluralized_text() {
    use ratatui::layout::Alignment;

    let one = status_hud_title(None, 1, None).expect("one mention should render");
    assert_eq!(one.alignment, Some(Alignment::Right));
    let text: String = one.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 1 unread mention ");

    let many = status_hud_title(None, 14, None).expect("many mentions should render");
    let text: String = many.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 14 unread mentions ");
}

#[test]
fn status_hud_title_combines_voice_and_mentions() {
    let line =
        status_hud_title(None, 2, Some(" mic #lounge [muted] ")).expect("status should render");
    let text: String = line.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 2 unread mentions | mic #lounge [muted] ");
}

#[test]
fn status_hud_title_renders_balance_right_of_mentions() {
    use ratatui::layout::Alignment;

    let only = status_hud_title(Some(1_500), 0, None).expect("balance should render alone");
    assert_eq!(only.alignment, Some(Alignment::Right));
    let text: String = only.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, " 1500 chips ");

    let combined = status_hud_title(Some(1_500), 2, Some(" mic #lounge [muted] "))
        .expect("balance + voice + mentions should render");
    let text: String = combined.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(
        text,
        " 2 unread mentions | mic #lounge [muted] | 1500 chips "
    );
}

#[test]
fn sponsor_title_drops_optional_segments_before_overlapping_help_hints() {
    let full_width = line_width(&sponsor_line(true, true));
    let url_width = line_width(&sponsor_line(false, true));
    let short_url_width = line_width(&sponsor_line(false, false));

    let full = app_frame_sponsor_title(full_width).expect("full sponsor should fit");
    assert_eq!(
        line_text(&full),
        " thanks for hanging out ☕ https://ko-fi.com/mateuszpiorowski "
    );

    let url_only =
        app_frame_sponsor_title(full_width - 1).expect("url-only sponsor should fit");
    assert_eq!(line_text(&url_only), "https://ko-fi.com/mateuszpiorowski ");

    let short_url =
        app_frame_sponsor_title(url_width - 1).expect("protocol-stripped sponsor should fit");
    assert_eq!(line_text(&short_url), "ko-fi.com/mateuszpiorowski ");

    let hidden = app_frame_sponsor_title(short_url_width - 1);
    assert!(hidden.is_none());
}

#[test]
fn help_hint_title_lists_guide_last() {
    let help = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
    assert_eq!(
        line_text(&help),
        " Settings Ctrl+O · Hub Ctrl+G · Lobby Ctrl+Q · Guide ? "
    );
}

#[test]
fn help_hint_title_compacts_separators_then_ctrl_notation() {
    let dotted = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
    let spaced = app_frame_help_hint_title(HelpHintStyle::SpacedCtrl);
    let caret = app_frame_help_hint_title(HelpHintStyle::SpacedCaret);
    assert_eq!(
        line_text(&spaced),
        " Settings Ctrl+O  Hub Ctrl+G  Lobby Ctrl+Q  Guide ? "
    );
    assert_eq!(
        line_text(&caret),
        " Settings ^O  Hub ^G  Lobby ^Q  Guide ? "
    );

    let (help, sponsor) = app_frame_bottom_titles((line_width(&dotted) + 2) as u16);
    assert_eq!(line_text(&help), line_text(&dotted));
    assert!(sponsor.is_none());

    let (help, sponsor) = app_frame_bottom_titles((line_width(&spaced) + 2) as u16);
    assert_eq!(line_text(&help), line_text(&spaced));
    assert!(sponsor.is_none());

    let (help, sponsor) = app_frame_bottom_titles((line_width(&caret) + 2) as u16);
    assert_eq!(line_text(&help), line_text(&caret));
    assert!(sponsor.is_none());
}
