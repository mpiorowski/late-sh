use super::{
    navigation::{ClickTarget, ContextMenu, MenuAction, Selection},
    state::*,
    svc::CalendarService,
    ui::*,
};
use crate::app::common::theme;
use chrono::{Datelike, Duration, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::*,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use uuid::Uuid;
fn event(start: u32, end: u32) -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 2, start, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 10, 2, end, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::now_v7(),
        owner_id: None,
        creator_id: Uuid::nil(),
        creation_tier: CreationTier::Admin,
        mod_editable: false,
        title: "日本語 café 🗓 Calendar".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(end),
        },
        creator_timezone: "UTC".into(),
        notice_lead_seconds: None,
        notice_start: start,
        notice_end: end,
        revision: 1,
    }
}
#[test]
fn calendar_overlaps_use_separate_lanes_and_continue_overnight() {
    let events = vec![event(9, 12), event(10, 11), event(11, 13)];
    let date = "2026-10-02".parse().unwrap();
    let pieces = segments(&events, date, chrono_tz::UTC);
    assert_eq!(
        pieces.iter().map(|p| p.lane).collect::<Vec<_>>(),
        vec![0, 1, 1]
    );
    let mut overnight = event(23, 23);
    if let EventTiming::Timed { end, .. } = &mut overnight.timing {
        *end = Some(Utc.with_ymd_and_hms(2026, 10, 3, 2, 0, 0).unwrap());
    }
    let pieces = segments(&[overnight.clone()], date, chrono_tz::UTC);
    assert!(pieces[0].after);
    assert_eq!(pieces[0].end, 1440);
    let pieces = segments(&[overnight], date + Duration::days(1), chrono_tz::UTC);
    assert!(pieces[0].before);
    assert_eq!(pieces[0].start, 0);
}

#[test]
fn calendar_adjacent_short_events_use_distinct_rendered_rows_or_lanes() {
    let mut first = event(9, 10);
    let mut second = event(9, 10);
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
    first.timing = EventTiming::Timed {
        start,
        end: Some(start + Duration::minutes(15)),
    };
    second.timing = EventTiming::Timed {
        start: start + Duration::minutes(15),
        end: Some(start + Duration::minutes(30)),
    };
    let pieces = segments(&[first, second], start.date_naive(), chrono_tz::UTC);
    assert_ne!(pieces[0].lane, pieces[1].lane);
}

#[tokio::test]
async fn calendar_day_cards_fill_width_and_preserve_both_short_events() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.view = CalendarView::Day;
    s.selected = "2026-10-02".parse().unwrap();
    let mut first = event(9, 10);
    first.title = "First quarter hour".into();
    let mut second = event(9, 10);
    second.title = "Second quarter hour".into();
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap();
    first.timing = EventTiming::Timed {
        start,
        end: Some(start + Duration::minutes(15)),
    };
    second.timing = EventTiming::Timed {
        start: start + Duration::minutes(15),
        end: Some(start + Duration::minutes(30)),
    };
    s.events = vec![first.clone(), second.clone()];
    s.selection = Selection::Event(first.id);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    let first_rect = action_area(&s, Action::EventAt(first.id, s.selected));
    let second_rect = action_area(&s, Action::EventAt(second.id, s.selected));
    assert!(first_rect.width > 40);
    assert_eq!(first_rect.intersection(second_rect).area(), 0);
    for (rect, title) in [(first_rect, &first.title), (second_rect, &second.title)] {
        assert!(rendered_text(terminal.backend().buffer(), rect).contains(title));
    }
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|hit| matches!(hit.action, Action::Slot(_, 540)))
    );

    s.events.truncate(1);
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    assert!(action_area(&s, Action::EventAt(first.id, s.selected)).width > 100);
}

#[tokio::test]
async fn calendar_all_palettes_have_visible_legible_selection() {
    let _restore = RestoreTheme::new();
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    for option in theme::OPTIONS {
        theme::set_current_by_id(option.id);
        let style = selection_style();
        if theme::BG_CANVAS() == Color::Reset {
            assert!(style.add_modifier.contains(Modifier::REVERSED));
        } else {
            let ratio = theme::contrast_ratio(style.fg.unwrap(), style.bg.unwrap()).unwrap();
            assert!(ratio >= 4.5, "{}: {ratio}", option.id);
            if option.kind != theme::ThemeKind::Contrast {
                assert_eq!(style.bg, Some(theme::BG_SELECTION()), "{}", option.id);
            }
        }
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), &s))
            .unwrap();
        let selected = action_area(&s, Action::Date(s.selected));
        let text = rendered_text(
            terminal.backend().buffer(),
            Rect::new(selected.x, selected.y, selected.width, 1),
        );
        assert!(text.starts_with('▸'), "{}: {text}", option.id);
    }
}

#[test]
fn calendar_high_contrast_selection_distinguishes_surface_and_text() {
    let _restore = RestoreTheme::new();
    theme::set_current_by_id("contrast");
    let palette_fill = theme::BG_SELECTION();
    let style = selection_style();
    let fill = style.bg.unwrap();
    assert!(theme::contrast_ratio(fill, theme::BG_CANVAS()).unwrap() >= 3.0);
    assert!(theme::contrast_ratio(style.fg.unwrap(), fill).unwrap() >= 4.5);
    assert_eq!(theme::BG_SELECTION(), palette_fill);
}

#[tokio::test]
async fn calendar_toolbar_keeps_all_controls_visible_and_separate_on_compact_screens() {
    let _restore = RestoreTheme::new();
    theme::set_current_by_id("contrast");
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    let owner_id = Uuid::from_u128(2);
    s.public.push(PublicCalendar {
        owner_id,
        username: "日本語_calendar_with_a_long_name".into(),
    });
    s.source = CalendarSource::Personal(owner_id);
    s.view = CalendarView::List;
    s.tz = chrono_tz::America::New_York;
    s.selected = "2026-11-02".parse().unwrap();
    s.loading = false;
    for (width, height) in [(120, 40), (80, 24), (44, 22), (48, 16)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let area = Rect::new(2, 1, width - 4, height - 2);
        let mut header_height = 0;
        terminal
            .draw(|frame| {
                header_height = super::toolbar::draw(frame, area, &s);
            })
            .unwrap();
        let header = Rect::new(area.x, area.y, area.width, header_height);
        let hits = s.hits.borrow();
        for action in [
            Action::Source,
            Action::View,
            Action::CycleSource(-1),
            Action::CycleSource(1),
            Action::CycleView(-1),
            Action::CycleView(1),
            Action::Settings,
            Action::New,
            Action::Previous,
            Action::Next,
            Action::Go,
            Action::Today,
        ] {
            let hit = hits
                .iter()
                .find(|hit| hit.action == action)
                .expect("control missing");
            assert_eq!(
                hit.area.intersection(header),
                hit.area,
                "{width}x{height}: {hit:?}"
            );
            assert!(
                !rendered_text(terminal.backend().buffer(), hit.area)
                    .trim()
                    .is_empty(),
                "{hit:?}"
            );
        }
        for (index, hit) in hits.iter().enumerate() {
            for other in &hits[index + 1..] {
                assert_eq!(
                    hit.area.intersection(other.area).area(),
                    0,
                    "{hit:?} overlaps {other:?}"
                );
            }
        }
        let buffer = terminal.backend().buffer();
        let view_value = hits.iter().rfind(|hit| hit.action == Action::View).unwrap();
        let view_text = rendered_text(buffer, view_value.area);
        assert!(
            matches!(view_text.trim(), "Event List" | "List"),
            "{width}x{height}: unexpected view value {view_text:?}"
        );
        let source_value = hits
            .iter()
            .rfind(|hit| hit.action == Action::Source)
            .unwrap();
        assert!(
            rendered_text(buffer, source_value.area)
                .trim()
                .ends_with('…')
        );
        for (action, label) in [
            (Action::Source, "Calendar"),
            (Action::View, "View"),
            (Action::Settings, "Settings"),
            (Action::New, "New event"),
            (Action::Go, "Go to date"),
            (Action::Today, "Today"),
        ] {
            let (x, y) = label_position(buffer, action_bounds(&s, action), label);
            for index in 0..label.len() {
                let cell = &buffer[(x + index as u16, y)];
                assert_eq!(
                    cell.fg,
                    if index == 0 {
                        theme::AMBER()
                    } else {
                        theme::TEXT_BRIGHT()
                    }
                );
                assert!(cell.modifier.contains(Modifier::BOLD));
                assert!(!cell.modifier.contains(Modifier::UNDERLINED));
            }
        }
        let text = rendered_text(buffer, header);
        assert!(
            text.contains("2026") && (text.contains("America/New_York") || text.contains("EST")),
            "{width}x{height}: {text}"
        );
        drop(hits);
        s.invalidate_geometry();
    }
}

#[tokio::test]
async fn calendar_picker_scroll_and_close_follow_the_visible_viewport() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.public = (0..12)
        .map(|index| PublicCalendar {
            owner_id: Uuid::now_v7(),
            username: format!("calendar{index}"),
        })
        .collect();
    s.modal = Some(Modal::Source(0));
    s.picker_reveal.set(false);
    s.picker_scroll.set(4);
    let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    terminal
        .draw(|frame| draw_modal(frame, frame.area(), &s))
        .unwrap();
    assert_eq!(s.picker_scroll.get(), 4);
    assert!(s.max_picker.get() > 0);
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|hit| hit.action == Action::Choice(4))
    );
    assert!(
        !s.hits
            .borrow()
            .iter()
            .any(|hit| hit.action == Action::Choice(0))
    );
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|hit| hit.action == Action::Cancel)
    );
    assert!(
        s.panes
            .borrow()
            .iter()
            .any(|pane| pane.pane == Pane::Picker)
    );

    s.modal = Some(Modal::Source(13));
    s.picker_reveal.set(true);
    terminal
        .draw(|frame| draw_modal(frame, frame.area(), &s))
        .unwrap();
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|hit| hit.action == Action::Choice(13))
    );
}

#[tokio::test]
async fn calendar_agenda_timing_row_and_close_are_mouse_targets() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    let event = event(9, 10);
    s.events = vec![event.clone()];
    s.modal = Some(Modal::Agenda);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| draw_modal(frame, frame.area(), &s))
        .unwrap();
    assert_eq!(action_area(&s, Action::Event(event.id)).height, 2);
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|hit| hit.action == Action::Cancel)
    );
}

#[tokio::test]
async fn calendar_spanning_event_hits_retain_each_visible_segment_date() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    let mut spanning = event(9, 10);
    let start = s.selected - Duration::days(1);
    spanning.timing = EventTiming::AllDay {
        start,
        end_exclusive: s.selected + Duration::days(2),
    };
    s.events = vec![spanning.clone()];
    for view in [CalendarView::Month, CalendarView::Week] {
        s.view = view;
        let mut terminal = Terminal::new(TestBackend::new(140, 45)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), &s))
            .unwrap();
        for day in 0..3 {
            assert!(
                s.hits.borrow().iter().any(|hit| {
                    hit.action == Action::EventAt(spanning.id, start + Duration::days(day))
                }),
                "missing {view:?} segment {day}"
            );
        }
    }
}

#[tokio::test]
async fn calendar_timeline_resize_clamps_hours_and_labels_read_only_slots() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.view = CalendarView::Day;
    s.hour_scroll = 47;
    s.selection = Selection::Slot(1410);
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    let first = s
        .hits
        .borrow()
        .iter()
        .find_map(|hit| match hit.action {
            Action::Slot(_, minute) => Some(minute),
            _ => None,
        })
        .unwrap();
    assert_eq!(first as usize, (48 - s.hour_rows.get()) * 30);
    let selected = action_area(&s, Action::Slot(s.selected, 1410));
    assert!(rendered_text(terminal.backend().buffer(), selected).starts_with("▸ 23:30"));
}

#[tokio::test]
async fn calendar_context_menu_clamps_to_content_and_replaces_underlying_hits() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.context_menu = Some(ContextMenu {
        target: ClickTarget::Date(s.selected),
        anchor: (79, 23),
        selected: 1,
        items: vec![MenuAction::Open, MenuAction::New],
        area: std::cell::Cell::new(Rect::default()),
    });
    let area = Rect::new(1, 1, 78, 22);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, area, &s);
            draw_modal(frame, area, &s);
        })
        .unwrap();
    let menu_area = s.context_menu.as_ref().unwrap().area.get();
    assert_eq!(menu_area.intersection(area), menu_area);
    assert_eq!(s.hits.borrow().len(), 2);
    assert!(
        s.hits
            .borrow()
            .iter()
            .all(|hit| matches!(hit.action, Action::MenuChoice(_)))
    );
    let selected = action_area(&s, Action::MenuChoice(1));
    assert!(rendered_text(terminal.backend().buffer(), selected).starts_with('▸'));
}

#[tokio::test]
async fn calendar_go_date_preview_and_controls_fit_compact_terminals() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2028-01-31".parse().unwrap();
    s.modal = Some(Modal::Go(Box::new(ratatui_textarea::TextArea::from(vec![
        "+1 month".to_string(),
    ]))));
    for (w, h) in [(140, 45), (80, 24), (48, 16)] {
        s.invalidate_geometry();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| draw_modal(frame, frame.area(), &s))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Go to 2028-02-29"), "{w}x{h}: {text}");
        assert!(text.contains("Offsets from 2028-01-31"), "{text}");
        for action in [Action::Save, Action::Cancel] {
            let hit = s
                .hits
                .borrow()
                .iter()
                .find(|hit| hit.action == action)
                .unwrap()
                .area;
            assert_eq!(
                hit.intersection(ratatui::layout::Rect::new(0, 0, w, h)),
                hit
            );
        }
    }
}

#[test]
fn calendar_repeated_dst_hour_keeps_actual_overlaps_in_separate_lanes() {
    let tz = chrono_tz::America::New_York;
    let date = "2026-11-01".parse::<chrono::NaiveDate>().unwrap();
    let make = |start: &str, end: &str, occurrence| {
        let mut e = event(9, 12);
        e.timing = EventTiming::Timed {
            start: local_instant(start.parse().unwrap(), tz, Some(occurrence)).unwrap(),
            end: Some(local_instant(end.parse().unwrap(), tz, Some(Occurrence::Later)).unwrap()),
        };
        e
    };
    let cross_fold = make(
        "2026-11-01T01:45:00",
        "2026-11-01T01:15:00",
        Occurrence::Earlier,
    );
    let later = make(
        "2026-11-01T01:00:00",
        "2026-11-01T01:20:00",
        Occurrence::Later,
    );
    let label = timing_label(&cross_fold, tz);
    assert!(
        label.contains("01:45 EDT") && label.contains("01:15 EST"),
        "{label}"
    );
    let pieces = segments(&[cross_fold, later], date, tz);
    assert_ne!(pieces[0].lane, pieces[1].lane);
}
#[tokio::test]
async fn calendar_all_views_render_clipped_geometry_and_unicode() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    s.events = (0..8).map(|_| event(9, 12)).collect();
    for (w, h) in [(140, 45), (80, 24), (48, 16), (22, 9)] {
        for view in CalendarView::ALL {
            s.view = view;
            s.reset_scroll();
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| draw(frame, frame.area(), &s))
                .unwrap();
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            for hit in s.hits.borrow().iter() {
                assert_eq!(hit.area.intersection(area), hit.area, "{view:?} {hit:?}");
            }
            assert!(!s.hits.borrow().is_empty());
        }
    }
}

#[tokio::test]
async fn calendar_compact_month_keeps_crowded_day_count_intact() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    let mut terminal = Terminal::new(TestBackend::new(48, 16)).unwrap();
    for date in ["2026-10-02", "2026-10-10", "2026-10-31"] {
        s.selected = date.parse().unwrap();
        s.events = (0..21)
            .map(|_| {
                let mut event = event(9, 12);
                event.timing = EventTiming::AllDay {
                    start: s.selected,
                    end_exclusive: s.selected + Duration::days(1),
                };
                event
            })
            .collect();
        terminal
            .draw(|frame| draw(frame, frame.area(), &s))
            .unwrap();
        let cell = s
            .hits
            .borrow()
            .iter()
            .find(|h| h.action == Action::Date(s.selected))
            .unwrap()
            .area;
        let buffer = terminal.backend().buffer();
        let text: String = (cell.x..cell.right())
            .map(|x| buffer[(x, cell.y)].symbol())
            .collect();
        assert!(text.contains(&format!("{}+21", s.selected.day())), "{text}");
    }
}

#[tokio::test]
async fn calendar_editor_controls_remain_visible_and_clipped_after_resize() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    let editor = Editor::from_event(
        &event(9, 12),
        Uuid::nil(),
        CreationTier::Admin,
        chrono_tz::UTC,
    );
    for (w, h) in [(140, 45), (80, 24), (48, 16), (22, 9)] {
        for focus in editor.visible_controls(s.today(), s.tz) {
            let mut editor = editor.clone();
            editor.focus = focus;
            s.modal = Some(Modal::Editor(Box::new(editor)));
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|frame| draw_modal(frame, frame.area(), &s))
                .unwrap();
            let area = ratatui::layout::Rect::new(0, 0, w, h);
            for hit in s.hits.borrow().iter() {
                assert_eq!(hit.area.intersection(area), hit.area);
            }
            assert!(s.hits.borrow().iter().any(|h| h.action == Action::Save));
            assert!(s.hits.borrow().iter().any(|h| h.action == Action::Cancel));
        }
    }
}

#[tokio::test]
async fn calendar_narrow_week_reveals_selected_day_and_every_overlap_lane() {
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.view = CalendarView::Week;
    s.selected = "2026-10-02".parse().unwrap();
    s.events = (0..8).map(|_| event(9, 12)).collect();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|h| h.action == Action::Date(s.selected))
    );
    assert!(s.day_scroll.get() > 0);
    let mut ids: std::collections::HashSet<_> = s
        .hits
        .borrow()
        .iter()
        .filter_map(|h| {
            if let Action::EventAt(id, _) = h.action {
                Some(id)
            } else {
                None
            }
        })
        .collect();
    s.day_scroll.set(s.max_days.get());
    terminal
        .draw(|frame| draw(frame, frame.area(), &s))
        .unwrap();
    ids.extend(s.hits.borrow().iter().filter_map(|h| {
        if let Action::EventAt(id, _) = h.action {
            Some(id)
        } else {
            None
        }
    }));
    assert!(s.events.iter().all(|e| ids.contains(&e.id)));
    let mut compact = Terminal::new(TestBackend::new(48, 16)).unwrap();
    compact.draw(|frame| draw(frame, frame.area(), &s)).unwrap();
    assert!(
        s.hits
            .borrow()
            .iter()
            .any(|h| h.action == Action::Date(s.selected))
    );
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

/// Locate ASCII labels by cells, keeping coordinates correct after wide glyphs.
fn label_position(buffer: &Buffer, area: Rect, text: &str) -> (u16, u16) {
    assert!(text.is_ascii());
    let width = text.len() as u16;
    for y in area.y..area.bottom() {
        for x in area.x..area.right().saturating_sub(width).saturating_add(1) {
            if text
                .chars()
                .enumerate()
                .all(|(i, c)| buffer[(x + i as u16, y)].symbol() == c.to_string())
            {
                return (x, y);
            }
        }
    }
    panic!("Missing {text:?} in {}", rendered_text(buffer, area));
}

fn action_area(s: &CalendarState, action: Action) -> Rect {
    s.hits
        .borrow()
        .iter()
        .find(|hit| hit.action == action)
        .unwrap()
        .area
}

fn action_bounds(s: &CalendarState, action: Action) -> Rect {
    s.hits
        .borrow()
        .iter()
        .filter(|hit| hit.action == action)
        .map(|hit| hit.area)
        .reduce(|left, right| left.union(right))
        .unwrap()
}

#[tokio::test]
async fn calendar_theme_hierarchy_preserves_controls_and_geometry() {
    let _restore = RestoreTheme::new();
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.selected = "2026-10-02".parse().unwrap();
    for id in READABILITY_THEMES {
        theme::set_current_by_id(id);
        for (w, h) in [(140, 45), (80, 24), (48, 16)] {
            s.modal = None;
            s.invalidate_geometry();
            let area = Rect::new(0, 0, w, h);
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|frame| draw(frame, area, &s)).unwrap();
            let buffer = terminal.backend().buffer();
            let view = action_bounds(&s, Action::View);
            let key = &buffer[label_position(buffer, view, "V")];
            let label = &buffer[label_position(buffer, view, "Month")];
            assert_eq!(key.fg, theme::AMBER(), "{id} {w}x{h}");
            assert!(key.modifier.contains(Modifier::BOLD));
            if id == "contrast" {
                assert_ne!(key.fg, label.fg, "{id}: key and label merge");
            }
            assert!(!label.modifier.contains(Modifier::BOLD));

            let source = action_bounds(&s, Action::Source);
            for word in ["C", "Server"] {
                let cell = &buffer[label_position(buffer, source, word)];
                assert_eq!(cell.bg, theme::BG_CANVAS());
                assert!(!cell.modifier.contains(Modifier::REVERSED));
            }
            let grid_corner = buffer
                .content()
                .iter()
                .find(|cell| cell.symbol() == "╭")
                .unwrap();
            assert_eq!(grid_corner.fg, theme::BORDER_DIM(), "{id}");
            for hit in s.hits.borrow().iter() {
                assert_eq!(hit.area.intersection(area), hit.area);
            }

            s.modal = Some(Modal::Go(Box::new(ratatui_textarea::TextArea::from(vec![
                "+1 month".to_string(),
            ]))));
            terminal.draw(|frame| draw_modal(frame, area, &s)).unwrap();
            let buffer = terminal.backend().buffer();
            let title = &buffer[label_position(buffer, area, "Go to date")];
            assert_eq!(title.fg, theme::AMBER(), "{id}");
            if title.fg != Color::Reset {
                assert_ne!(title.fg, title.bg, "{id}: invisible modal title");
            }
            assert!(title.modifier.contains(Modifier::BOLD));
            let corner = buffer
                .content()
                .iter()
                .find(|cell| cell.symbol() == "╭")
                .unwrap();
            assert_eq!(corner.fg, theme::BORDER_ACTIVE(), "{id}");
            let go = action_area(&s, Action::Save);
            let enter = &buffer[label_position(buffer, go, "Enter")];
            assert_eq!(enter.fg, theme::AMBER_DIM(), "{id}");
            assert!(enter.modifier.contains(Modifier::BOLD));
            for action in [Action::Save, Action::Cancel] {
                let rect = action_area(&s, action);
                assert_eq!(rect.intersection(area), rect);
            }
        }
    }
}

#[tokio::test]
async fn calendar_ical_import_controls_and_picker_fit_themes_and_compact_sizes() {
    let _restore = RestoreTheme::new();
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.source = CalendarSource::Personal(s.viewer);
    for id in READABILITY_THEMES {
        theme::set_current_by_id(id);
        for (w, h) in [(120, 40), (80, 24), (44, 22), (48, 16)] {
            let area = Rect::new(0, 0, w, h);
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            s.modal = None;
            terminal.draw(|frame| draw(frame, area, &s)).unwrap();
            assert!(
                !s.hits
                    .borrow()
                    .iter()
                    .any(|hit| hit.action == Action::Import)
            );
            s.modal = Some(Modal::Import(Box::default()));
            terminal.draw(|frame| draw_modal(frame, area, &s)).unwrap();
            assert!(action_area(&s, Action::Save).width > 0);
            assert!(action_area(&s, Action::Cancel).width > 0);
            let Some(Modal::Import(import)) = &mut s.modal else {
                panic!("import");
            };
            import.candidates = Some(super::ical::parse("BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:One\nDTSTART;VALUE=DATE:20261004\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:Two\nDTSTART;VALUE=DATE:20261005\nEND:VEVENT\nEND:VCALENDAR", chrono_tz::UTC).unwrap());
            import.selected = 1;
            terminal.draw(|frame| draw_modal(frame, area, &s)).unwrap();
            assert!(action_area(&s, Action::Choice(1)).width > 0);
            assert!(action_area(&s, Action::Cancel).width > 0);
            for hit in s.hits.borrow().iter() {
                assert_eq!(
                    hit.area.intersection(area),
                    hit.area,
                    "{id} {w}x{h} {hit:?}"
                );
            }
            s.modal = Some(Modal::Details(event(9, 10)));
            terminal.draw(|frame| draw_modal(frame, area, &s)).unwrap();
            assert!(action_area(&s, Action::Copy).width > 0);
            assert!(action_area(&s, Action::Cancel).width > 0);
            let mut draft = EventDraft::from(&event(9, 10));
            draft.description = "First line\nSecond line with a long sentence that extends past a narrow field\nLast line".into();
            let mut editor = Editor::new(
                s.source,
                s.selected,
                EventAccess {
                    edit: true,
                    notifications: true,
                    delegate: false,
                },
            );
            editor.apply_import(&draft, s.tz);
            s.modal = Some(Modal::Editor(Box::new(editor)));
            terminal.draw(|frame| draw_modal(frame, area, &s)).unwrap();
            let import = action_area(&s, Action::Import);
            assert!(import.width > 0, "{id} {w}x{h}");
            assert_eq!(import.intersection(area), import);
            for action in [Action::Save, Action::Cancel] {
                assert!(action_area(&s, action).width > 0);
            }
            let text = rendered_text(terminal.backend().buffer(), area);
            assert!(text.contains("First line"), "{w}x{h}: {text}");
            assert!(text.contains("Second line"), "{w}x{h}: {text}");
        }
    }
}

#[tokio::test]
async fn calendar_upcoming_titles_remain_visible_before_quiet_metadata() {
    let _restore = RestoreTheme::new();
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    let mut notice = event(9, 12);
    let now = Utc::now();
    notice.title = "日本語 café 🗓 Calendar gathering with guest musicians".into();
    notice.timing = EventTiming::Timed {
        start: now + Duration::hours(2),
        end: Some(now + Duration::hours(4)),
    };
    notice.notice_lead_seconds = Some(86400);
    notice.notice_start = now - Duration::hours(1);
    notice.notice_end = now + Duration::hours(4);
    s.notices = vec![notice.clone()];
    for id in READABILITY_THEMES {
        theme::set_current_by_id(id);
        for w in [140, 80, 48] {
            s.invalidate_geometry();
            let area = Rect::new(0, 0, w, 5);
            let mut terminal = Terminal::new(TestBackend::new(w, 5)).unwrap();
            terminal
                .draw(|frame| draw_upcoming_panel(frame, area, &s))
                .unwrap();
            let event_area = action_area(&s, Action::Event(notice.id));
            let buffer = terminal.backend().buffer();
            let text = rendered_text(buffer, event_area);
            // Buffer padding after double-width glyphs is not part of the title.
            assert!(
                text.replace(' ', "").contains("日本語café"),
                "{id} {w}: title hidden by metadata: {text}"
            );
            assert!(text.starts_with("[Server]"), "{text}");
            let source = &buffer[label_position(buffer, event_area, "Server")];
            let title = &buffer[label_position(buffer, event_area, "caf")];
            assert_ne!(title.fg, source.fg, "{id}: title and source merge");
            if title.fg != Color::Reset {
                assert_ne!(title.fg, title.bg, "{id}: invisible title");
            }
            if w == 140 {
                let time = timing_label(&notice, s.tz);
                assert!(text.contains(&time), "{text}");
                let time_cell = &buffer[label_position(buffer, event_area, &time[..10])];
                assert_ne!(title.fg, time_cell.fg, "{id}: title and time merge");
                let separator = (event_area.x..event_area.right())
                    .map(|x| &buffer[(x, event_area.y)])
                    .find(|cell| cell.symbol() == "·")
                    .unwrap();
                assert_eq!(separator.fg, theme::TEXT_FAINT(), "{id}");
            }
            assert_eq!(event_area.intersection(area), event_area);
        }
    }
}

#[tokio::test]
async fn calendar_timed_cards_reserve_selection_for_the_selected_event() {
    let _restore = RestoreTheme::new();
    let mut s = CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    );
    s.view = CalendarView::Day;
    s.selected = "2026-10-02".parse().unwrap();
    let mut first = event(9, 12);
    first.title = "Sapphire".into();
    let mut second = event(10, 11);
    second.title = "Maple".into();
    s.events = vec![first.clone(), second.clone()];
    let area = Rect::new(0, 0, 80, 24);
    for id in READABILITY_THEMES {
        theme::set_current_by_id(id);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        for selected in 0..2 {
            s.event_index = selected;
            s.selection = Selection::Event(s.events[selected].id);
            s.invalidate_geometry();
            terminal.draw(|frame| draw(frame, area, &s)).unwrap();
            let buffer = terminal.backend().buffer();
            for (index, event) in [&first, &second].into_iter().enumerate() {
                let rect = action_area(&s, Action::EventAt(event.id, s.selected));
                assert_eq!(rect.intersection(area), rect);
                let cells = (rect.y..rect.bottom())
                    .flat_map(|y| (rect.x..rect.right()).map(move |x| &buffer[(x, y)]));
                let mut glyphs = 0;
                for cell in cells.filter(|cell| !cell.symbol().trim().is_empty()) {
                    glyphs += 1;
                    assert_ne!(cell.symbol(), "─", "{id}: hour rule crosses event card");
                    if index == selected {
                        if theme::BG_CANVAS() == Color::Reset {
                            assert_eq!(cell.fg, Color::Reset, "{id}: {}", event.title);
                            assert_eq!(cell.bg, Color::Reset, "{id}: {}", event.title);
                            assert!(cell.modifier.contains(Modifier::REVERSED));
                        } else {
                            assert_eq!(
                                cell.bg,
                                selection_style().bg.unwrap(),
                                "{id}: {}",
                                event.title
                            );
                            assert_ne!(cell.fg, cell.bg, "{id}: invisible selected glyph");
                        }
                    } else {
                        assert!(!cell.modifier.contains(Modifier::REVERSED));
                        let background = if theme::BG_CANVAS() == Color::Reset {
                            Color::Reset
                        } else {
                            theme::BG_HIGHLIGHT()
                        };
                        assert_eq!(cell.bg, background, "{id}: {}", event.title);
                        if cell.fg != Color::Reset {
                            assert_ne!(cell.fg, cell.bg, "{id}: invisible event glyph");
                        }
                    }
                }
                assert!(glyphs > 0, "{id}: missing card {}", event.title);
            }
        }
    }
}
