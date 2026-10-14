use super::{
    navigation::{ClickTarget, Selection},
    state::*,
    svc::CalendarService,
    ui::*,
};
use crate::app::common::theme;
use chrono::{Duration, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::*,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use uuid::Uuid;

fn viewer() -> Uuid {
    Uuid::from_u128(0xA)
}

fn event(id: u128, hour: u32, board: bool, going: i64) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, hour, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(id),
        owner_id: (!board).then_some(viewer()),
        creator_id: if board {
            Uuid::from_u128(0xB)
        } else {
            viewer()
        },
        creator_name: if board { "mat".into() } else { "me".into() },
        title: if board {
            "日本語 café 🗓 Movie night".into()
        } else {
            "Dentist".into()
        },
        description: "Bring snacks".into(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + Duration::hours(2)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + Duration::hours(2),
        going,
        revision: 1,
    }
}

fn state() -> CalendarState {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        viewer(),
    );
    s.loading = false;
    s.selected = "2026-10-02".parse().unwrap();
    s
}

fn render(s: &CalendarState, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, frame.area(), s);
            draw_modal(frame, frame.area(), s);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn rendered_text(buffer: &Buffer, area: Rect) -> String {
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn whole(buffer: &Buffer) -> String {
    rendered_text(buffer, *buffer.area())
}

fn action_area(s: &CalendarState, action: Action) -> Option<Rect> {
    s.hits
        .borrow()
        .iter()
        .find(|hit| hit.action == action)
        .map(|hit| hit.area)
}

const READABILITY_THEMES: [&str; 5] = ["late", "github-light", "contrast", "mono-ink", "terminal"];

/// Themes are thread-local; restore the calling test's palette even on panic.
struct RestoreTheme(&'static str);

impl RestoreTheme {
    fn new() -> Self {
        Self(
            theme::OPTIONS
                .iter()
                .find(|option| option.kind == theme::current_kind())
                .unwrap()
                .id,
        )
    }
}

impl Drop for RestoreTheme {
    fn drop(&mut self) {
        theme::set_current_by_id(self.0);
    }
}

/// The page: a header with every control as a hit target, a month grid with
/// the day's events previewed and the count of who is in, and the agenda
/// beside it on a wide terminal.
#[tokio::test]
async fn the_page_shows_the_board_and_the_viewers_own_events_together() {
    let mut s = state();
    let post = event(1, 21, true, 6);
    let mine = event(2, 9, false, 0);
    s.events = vec![post.clone(), mine.clone()];
    let buffer = render(&s, 160, 36);
    let text = whole(&buffer);
    assert!(text.contains("October 2026"), "{text}");
    assert!(text.contains("UTC"));
    for action in [
        Action::New,
        Action::Today,
        Action::Previous,
        Action::Next,
        Action::View,
        Action::Board,
    ] {
        assert!(
            action_area(&s, action.clone()).is_some(),
            "{action:?} has no hit"
        );
    }
    assert!(text.contains("Board shown"));
    assert!(text.contains("Movie night"));
    assert!(
        text.contains("21:00 ·6 日"),
        "the count leads the preview: {text}"
    );
    assert!(text.contains("Dentist"));
    assert!(text.contains("by mat"), "the agenda names the poster");
    assert!(text.contains("just you"), "and marks the viewer's own");
    assert!(action_area(&s, Action::EventAt(post.id, s.selected)).is_some());
    assert!(action_area(&s, Action::Event(mine.id)).is_some());
    // The foot says what the rules allow on the marked event: in or out on
    // another's post, edit and delete on the viewer's own; an empty day
    // offers a new event.
    s.select_date(s.selected + Duration::days(20));
    let text = whole(&render(&s, 160, 36));
    assert!(text.contains("Enter new event here"), "{text}");
    s.select_event(post.id, None);
    let text = whole(&render(&s, 160, 36));
    assert!(
        text.contains("i I'm in") && !text.contains("e edit"),
        "{text}"
    );
    s.select_event(mine.id, None);
    let text = whole(&render(&s, 160, 36));
    assert!(text.contains("e edit · d delete"), "{text}");
    s.select_event(post.id, None);

    s.show_board = false;
    let text = whole(&render(&s, 160, 36));
    assert!(text.contains("Board hidden"));

    // Nothing of the upcoming panel survives on this page.
    assert!(!text.contains("Upcoming"));
}

/// Details offer what the viewer may do: in or out for anyone on a board
/// post, edit for its poster, delete for its poster or staff, and the
/// count in words.
#[tokio::test]
async fn details_offer_the_viewers_actions_and_count_who_is_in() {
    let mut s = state();
    let post = event(1, 21, true, 6);
    s.events.push(post.clone());
    s.selection = Selection::Event(post.id);
    s.push_modal(Modal::Details(post.clone()));
    let text = whole(&render(&s, 100, 30));
    assert!(text.contains("On the board"));
    assert!(text.contains("6 in"), "{text}");
    assert!(text.contains("Bring snacks"));
    assert!(action_area(&s, Action::Rsvp).is_some());
    assert!(text.contains("I'm in"));
    assert!(
        action_area(&s, Action::Edit).is_none(),
        "not the viewer's post"
    );
    assert!(action_area(&s, Action::Delete).is_none());

    s.rsvps.push(post.id);
    let text = whole(&render(&s, 100, 30));
    assert!(text.contains("6 in, you included"), "{text}");
    assert!(text.contains("I'm out"));

    s.staff = true;
    render(&s, 100, 30);
    assert!(
        action_area(&s, Action::Delete).is_some(),
        "staff take posts down"
    );
    assert!(
        action_area(&s, Action::Edit).is_none(),
        "but never edit them"
    );

    let mine = event(2, 9, false, 0);
    s.modal = Some(Modal::Details(mine));
    let text = whole(&render(&s, 100, 30));
    assert!(text.contains("Your event"));
    assert!(text.contains("just you"));
    assert!(action_area(&s, Action::Rsvp).is_none());
    assert!(action_area(&s, Action::Edit).is_some());
    assert!(action_area(&s, Action::Delete).is_some());
}

/// Taking down a stranger's post says whose it was, so a moderator reads
/// what they are about to do.
#[tokio::test]
async fn deleting_a_strangers_post_names_the_poster() {
    let mut s = state();
    s.staff = true;
    let post = event(1, 21, true, 2);
    s.events.push(post.clone());
    s.push_modal(Modal::Delete(post));
    let text = whole(&render(&s, 100, 30));
    assert!(text.contains("Posted by mat"), "{text}");
    assert!(action_area(&s, Action::Save).is_some());
    assert!(action_area(&s, Action::Cancel).is_some());
}

#[tokio::test]
async fn context_menu_clamps_to_content_and_replaces_underlying_hits() {
    let mut s = state();
    let post = event(1, 21, true, 0);
    s.events.push(post.clone());
    s.open_context_menu(ClickTarget::Event(post.id), (118, 34));
    let buffer = render(&s, 120, 36);
    let area = s.context_menu.as_ref().unwrap().area.get();
    assert!(area.right() <= 120 && area.bottom() <= 36);
    assert!(
        s.hits
            .borrow()
            .iter()
            .all(|hit| matches!(hit.action, Action::MenuChoice(_))),
        "only the menu takes clicks while it is up"
    );
    let text = rendered_text(&buffer, area);
    assert!(text.contains("Open") && text.contains("I'm in"), "{text}");
}

#[tokio::test]
async fn compact_month_keeps_crowded_day_count_intact() {
    let mut s = state();
    s.events = (1..=12).map(|n| event(n, 9, true, 0)).collect();
    // Sixteen rows less the header and the foot is under the compact
    // grid's fourteen: one row per week, the count on the day's cell.
    let buffer = render(&s, 48, 16);
    let text = whole(&buffer);
    assert!(text.contains("2+12"), "{text}");
    assert!(action_area(&s, Action::Date(s.selected)).is_some());
}

#[tokio::test]
async fn the_list_groups_the_month_by_day() {
    let mut s = state();
    s.view = CalendarView::List;
    let post = event(1, 21, true, 3);
    let mine = event(2, 9, false, 0);
    s.events = vec![post.clone(), mine.clone()];
    s.selection = Selection::Event(post.id);
    let text = whole(&render(&s, 100, 30));
    assert!(text.contains("Friday, October 02, 2026"), "{text}");
    assert!(text.contains("09:00 Dentist"));
    assert!(text.contains("21:00"), "{text}");
    assert!(
        text.contains("21:00 ·3 日"),
        "the count leads the title: {text}"
    );
    assert!(text.contains("by mat"));
    assert!(action_area(&s, Action::Event(post.id)).is_some());
}

/// The terminal owns the page's background: "Sync terminal background"
/// paints it to the theme's canvas through OSC 11, and with the setting off
/// it stays the user's own. A cell painted with the theme's canvas ignores
/// that choice, so no view may lay the canvas down itself. Today's tint is a
/// fill of its own and stays.
#[tokio::test]
async fn the_page_leaves_the_background_to_the_terminal() {
    let _restore = RestoreTheme::new();
    theme::set_current_by_id("late");
    let canvas = theme::BG_CANVAS();
    let today_tint = theme::blend_toward(canvas, theme::BORDER_ACTIVE(), 0.16);
    let mut s = state();
    s.selected = s.today();
    let post = event(1, 21, true, 2);
    s.events.push(post.clone());

    let mut screens = Vec::new();
    for view in [CalendarView::Month, CalendarView::List] {
        s.view = view;
        screens.push((format!("{view:?}"), render(&s, 120, 40)));
    }
    s.push_modal(Modal::Details(post));
    screens.push(("details".to_string(), render(&s, 120, 40)));

    for (screen, buffer) in &screens {
        let painted: Vec<(u16, u16)> = buffer
            .area()
            .positions()
            .filter(|position| buffer[*position].bg == canvas)
            .map(|position| (position.x, position.y))
            .collect();
        assert_eq!(
            painted,
            Vec::new(),
            "{screen}: cells painted with the canvas"
        );
    }
    let month = &screens[0].1;
    assert!(
        month
            .area()
            .positions()
            .any(|position| month[position].bg == today_tint),
        "today keeps its tint"
    );
}

#[tokio::test]
async fn all_palettes_have_visible_legible_selection() {
    let _restore = RestoreTheme::new();
    for id in READABILITY_THEMES {
        theme::set_current_by_id(id);
        let style = selection_style();
        if theme::BG_CANVAS() == ratatui::style::Color::Reset {
            continue;
        }
        let fill = style.bg.unwrap();
        let fg = style.fg.unwrap();
        assert!(
            theme::contrast_ratio(fg, fill).is_some_and(|ratio| ratio >= 4.5),
            "{id}: selected text must read against its fill"
        );
        if theme::current_kind() == theme::ThemeKind::Contrast {
            assert!(
                theme::contrast_ratio(fill, theme::BG_CANVAS()).is_some_and(|ratio| ratio >= 3.0),
                "{id}: the selection surface must stand off the canvas"
            );
        }
    }
}
