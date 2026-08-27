use super::{
    AUTO_RIGHT_SIDEBAR_MIN_COLS, AUTO_ROOM_LIST_MIN_COLS, HelpHintStyle, StatusHud,
    StatusHudInputs, app_frame_bottom_titles, app_frame_help_hint_title, app_frame_sponsor_title,
    dashboard_home_selected, line_width, resolve_right_sidebar_enabled, resolve_room_list_enabled,
    room_list_sidebar_enabled, sidebar_enabled, sponsor_line, status_hud_title,
};
use crate::app::common::primitives::Screen;
use crate::app::pot::state::PotView;
use late_core::models::user::{RightSidebarMode, RoomListMode};
use uuid::Uuid;

/// A terminal wide enough that `Auto` keeps every rail, so the `On`/`Off` cases
/// below are unaffected by width.
const WIDE_TERMINAL: u16 = 200;

/// Enough border room that `status_hud_title` never degrades a segment, so
/// the cases below test content rather than fitting.
const WIDE_HUD_BORDER: u16 = 200;

/// A HUD with no left title competing for the border row, sized so nothing
/// degrades. Cases that exercise fitting pass `border_width`/`title_width`
/// themselves.
fn hud(
    balance: Option<i64>,
    unread: i64,
    voice_badge: Option<&str>,
    pomodoro_badge: Option<&str>,
) -> Option<StatusHud> {
    status_hud_title(StatusHudInputs {
        balance,
        unread,
        voice_badge,
        pomodoro_badge,
        pot: None,
        border_width: WIDE_HUD_BORDER,
        title_width: 0,
    })
}

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
fn status_hud_title_hidden_when_empty() {
    assert!(hud(None, 0, None, None).is_none());
    assert!(hud(None, -3, None, None).is_none());
}

#[test]
fn status_hud_title_renders_right_aligned_pluralized_text() {
    use ratatui::layout::Alignment;

    let one = hud(None, 1, None, None).expect("one mention should render");
    assert_eq!(one.line.alignment, Some(Alignment::Right));
    assert_eq!(line_text(&one.line), " 1 unread mention ");
    assert_eq!(one.mentions_width, " 1 unread mention ".len() as u16);

    let many = hud(None, 14, None, None).expect("many mentions should render");
    assert_eq!(line_text(&many.line), " 14 unread mentions ");
}

#[test]
fn status_hud_title_combines_voice_and_mentions() {
    let combined = hud(None, 2, Some(" mic #lounge [muted] "), None).expect("status should render");
    assert_eq!(
        line_text(&combined.line),
        " mic #lounge [muted] | 2 unread mentions "
    );
    // Only the mentions segment is clickable, so its width stops at the text
    // and its offset starts past the voice badge.
    assert_eq!(combined.mentions_width, " 2 unread mentions ".len() as u16);
    assert_eq!(
        combined.mentions_offset,
        " mic #lounge [muted] |".len() as u16
    );
}

#[test]
fn status_hud_title_renders_balance_right_of_mentions() {
    use ratatui::layout::Alignment;

    let only = hud(Some(1_500), 0, None, None).expect("balance should render alone");
    assert_eq!(only.line.alignment, Some(Alignment::Right));
    assert_eq!(line_text(&only.line), " 1500 chips ");
    assert_eq!(only.mentions_width, 0);

    let combined = hud(Some(1_500), 2, Some(" mic #lounge [muted] "), None)
        .expect("balance + voice + mentions should render");
    assert_eq!(
        line_text(&combined.line),
        " mic #lounge [muted] | 2 unread mentions | 1500 chips "
    );
}

/// The pomodoro badge shows the HUD on its own, and leads every other
/// segment, so a running countdown always sits at the left end of the HUD.
#[test]
fn status_hud_title_renders_pomodoro_left_of_every_other_segment() {
    let only = hud(None, 0, None, Some("24:59 deep work"))
        .expect("a running pomodoro alone should render the HUD");
    assert_eq!(line_text(&only.line), " 24:59 deep work ");
    assert_eq!(only.mentions_width, 0);

    let combined = hud(
        Some(1_500),
        2,
        Some(" mic #lounge [muted] "),
        Some("05:00 Pomodoro"),
    )
    .expect("every segment should render");
    assert_eq!(
        line_text(&combined.line),
        " 05:00 Pomodoro | mic #lounge [muted] | 2 unread mentions | 1500 chips "
    );
    assert_eq!(combined.mentions_width, " 2 unread mentions ".len() as u16);
    // The two badges ahead of it push the clickable mentions rect right.
    assert_eq!(
        combined.mentions_offset,
        " 05:00 Pomodoro | mic #lounge [muted] |".len() as u16
    );
}

/// The pot sits right before the chips, so the prize reads against the
/// viewer's own balance, and it is the first segment the border sheds:
/// countdown first, then the whole badge, before the pomodoro gives up its
/// label.
#[test]
fn status_hud_title_renders_pot_before_chips_and_sheds_it_first() {
    let pot = PotView {
        size: 84_200,
        ticket_count: 842,
        my_tickets: 5,
        draws_in: "3h12m".to_string(),
        open: true,
    };
    let with_pot = |border_width: u16| {
        status_hud_title(StatusHudInputs {
            balance: Some(1_500),
            unread: 2,
            voice_badge: Some(" mic #lounge [muted] "),
            pomodoro_badge: Some("05:00 Pomodoro"),
            pot: Some(&pot),
            border_width,
            title_width: 0,
        })
        .map(|hud| line_text(&hud.line))
    };
    let full = " 05:00 Pomodoro | mic #lounge [muted] | 2 unread mentions | pot 84,200 · 3h12m | 1500 chips ";
    let without_clock =
        " 05:00 Pomodoro | mic #lounge [muted] | 2 unread mentions | pot 84,200 | 1500 chips ";
    let without_pot = " 05:00 Pomodoro | mic #lounge [muted] | 2 unread mentions | 1500 chips ";
    let width = |text: &str| text.chars().count() as u16 + 2;

    assert_eq!(with_pot(WIDE_HUD_BORDER).as_deref(), Some(full));
    assert_eq!(
        with_pot(width(full) - 1).as_deref(),
        Some(without_clock),
        "one cell short drops the countdown, not the pot"
    );
    assert_eq!(
        with_pot(width(without_clock) - 1).as_deref(),
        Some(without_pot),
        "too tight for the size drops the pot before the pomodoro label"
    );

    // Alone with the chips it still reads pot first, balance last, and a
    // pot with nothing else keeps the HUD alive on its own.
    let alone = status_hud_title(StatusHudInputs {
        balance: Some(1_500),
        unread: 0,
        voice_badge: None,
        pomodoro_badge: None,
        pot: Some(&pot),
        border_width: WIDE_HUD_BORDER,
        title_width: 0,
    })
    .expect("pot + chips should render");
    assert_eq!(line_text(&alone.line), " pot 84,200 · 3h12m | 1500 chips ");
    assert_eq!(alone.mentions_width, 0);
    let only = status_hud_title(StatusHudInputs {
        balance: None,
        unread: 0,
        voice_badge: None,
        pomodoro_badge: None,
        pot: Some(&pot),
        border_width: WIDE_HUD_BORDER,
        title_width: 0,
    })
    .expect("pot alone should render");
    assert_eq!(line_text(&only.line), " pot 84,200 · 3h12m ");
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

#[test]
fn help_hint_title_lists_exit_last() {
    let help = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
    assert_eq!(
        line_text(&help),
        " Settings Ctrl+O · Lobby Ctrl+G · Shop /shop · Guide ? · Exit qq "
    );
}

#[test]
fn help_hint_title_compacts_separators_then_ctrl_notation() {
    let dotted = app_frame_help_hint_title(HelpHintStyle::DottedCtrl);
    let spaced = app_frame_help_hint_title(HelpHintStyle::SpacedCtrl);
    let caret = app_frame_help_hint_title(HelpHintStyle::SpacedCaret);
    assert_eq!(
        line_text(&spaced),
        " Settings Ctrl+O  Lobby Ctrl+G  Shop /shop  Guide ?  Exit qq "
    );
    assert_eq!(
        line_text(&caret),
        " Settings ^O  Lobby ^G  Shop /shop  Guide ?  Exit qq "
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

/// The HUD is painted over the left title, so a badge that does not fit the
/// spare border room must shed its label and then itself, rather than eating
/// the page tabs. Only the countdown degrades: the three older segments keep
/// their long-standing behavior.
#[test]
fn status_hud_title_degrades_pomodoro_to_fit_the_border() {
    let full = " 05:00 Pomodoro | mic #lounge [muted] | 2 unread mentions | 1500 chips ";
    let without_label = " 05:00 | mic #lounge [muted] | 2 unread mentions | 1500 chips ";
    let without_badge = " mic #lounge [muted] | 2 unread mentions | 1500 chips ";
    // A left title the HUD must not paint over, so the spare-room subtraction
    // is exercised rather than bypassed by a zero-width title.
    const TABS: u16 = 20;
    // Terminal width that leaves the HUD exactly `spare` cells: the two border
    // corners and the left title come off the top row first.
    let at_spare = |spare: u16| {
        status_hud_title(StatusHudInputs {
            balance: Some(1_500),
            unread: 2,
            voice_badge: Some(" mic #lounge [muted] "),
            pomodoro_badge: Some("05:00 Pomodoro"),
            pot: None,
            border_width: spare + 2 + TABS,
            title_width: TABS,
        })
    };
    let text_at = |spare: u16| at_spare(spare).map(|hud| line_text(&hud.line));

    assert_eq!(text_at(full.len() as u16).as_deref(), Some(full));
    assert_eq!(
        text_at(full.len() as u16 - 1).as_deref(),
        Some(without_label),
        "one cell short of the label drops the label, not the countdown"
    );
    assert_eq!(
        text_at(without_label.len() as u16 - 1).as_deref(),
        Some(without_badge),
        "too tight for even MM:SS drops the badge"
    );
    // Whatever is shown, the mentions hit-test rect still points at the text:
    // it starts past whatever the countdown has left ahead of it, and past the
    // voice badge alone once the countdown is dropped.
    for (spare, expected) in [
        (
            full.len() as u16,
            " 05:00 Pomodoro | mic #lounge [muted] |".len() as u16,
        ),
        (
            without_label.len() as u16,
            " 05:00 | mic #lounge [muted] |".len() as u16,
        ),
        (
            without_label.len() as u16 - 1,
            " mic #lounge [muted] |".len() as u16,
        ),
    ] {
        let hud = at_spare(spare).expect("hud should render");
        assert_eq!(hud.mentions_width, " 2 unread mentions ".len() as u16);
        assert_eq!(hud.mentions_offset, expected);
        let text = line_text(&hud.line);
        assert_eq!(
            &text[hud.mentions_offset as usize..][..hud.mentions_width as usize],
            " 2 unread mentions "
        );
    }
}

/// A left title wider than the whole border row must not underflow the spare
/// calculation into a huge budget: the badge is dropped, not force-fitted.
#[test]
fn status_hud_title_drops_pomodoro_when_the_title_outgrows_the_border() {
    let squeezed = status_hud_title(StatusHudInputs {
        balance: Some(1_500),
        unread: 0,
        voice_badge: None,
        pomodoro_badge: Some("05:00 Pomodoro"),
        pot: None,
        border_width: 10,
        title_width: 40,
    })
    .expect("chips keep the hud alive");
    assert_eq!(line_text(&squeezed.line), " 1500 chips ");
}

/// A pomodoro with nothing else in the HUD still needs its dividers right: no
/// leading `|` when it is the first segment, and one before the next segment.
#[test]
fn status_hud_title_places_pomodoro_dividers_without_mentions() {
    let alone = hud(None, 0, None, Some("05:00 focus")).expect("pomodoro alone should render");
    assert_eq!(line_text(&alone.line), " 05:00 focus ");

    let with_voice = hud(None, 0, Some(" mic #lounge [muted] "), Some("05:00 focus"))
        .expect("pomodoro + voice should render");
    assert_eq!(
        line_text(&with_voice.line),
        " 05:00 focus | mic #lounge [muted] "
    );

    // Dropping the badge entirely must not leave a stray divider behind.
    let dropped = status_hud_title(StatusHudInputs {
        balance: None,
        unread: 0,
        voice_badge: Some(" mic #lounge [muted] "),
        pomodoro_badge: Some("05:00 focus"),
        pot: None,
        border_width: 7,
        title_width: 0,
    })
    .expect("voice should still render");
    assert_eq!(line_text(&dropped.line), " mic #lounge [muted] ");
}
