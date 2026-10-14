use super::{
    editor::{
        Editor, EditorCommand, EditorControl, Target, TargetAction, click, draw, field, handle_key,
    },
    state::CalendarState,
    svc::CalendarService,
};
use crate::app::input::ParsedInput;
use chrono::{NaiveDate, TimeZone, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::calendar::{CalendarEvent, CalendarSource, EventTiming},
};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use uuid::Uuid;

fn today() -> NaiveDate {
    "2026-10-03".parse().unwrap()
}

fn editor() -> Editor {
    Editor::new(CalendarSource::Board, today(), chrono_tz::UTC)
}

fn state() -> CalendarState {
    CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    )
}

fn board_event() -> CalendarEvent {
    let start = Utc.with_ymd_and_hms(2026, 10, 3, 21, 0, 0).unwrap();
    CalendarEvent {
        id: Uuid::from_u128(7),
        owner_id: None,
        creator_id: Uuid::nil(),
        creator_name: "mat".into(),
        title: "Movie night".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start,
            end: Some(start + chrono::Duration::hours(2)),
        },
        creator_timezone: "UTC".into(),
        starts_at: start,
        ends_at: start + chrono::Duration::hours(2),
        going: 3,
        revision: 2,
    }
}

fn draw_editor(e: &Editor, s: &CalendarState, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| draw(frame, Rect::new(2, 2, width - 4, height - 4), s, e))
        .unwrap();
}

fn value_target(e: &Editor, control: EditorControl) -> Target {
    e.geometry
        .borrow()
        .targets
        .iter()
        .find(|target| matches!(target.action, TargetAction::Value(c, _, _) if c == control))
        .copied()
        .unwrap()
}

/// A new draft offers where it goes; an opened event does not. The time
/// fields appear once all-day is off, and a draft typed into the title
/// fills them in.
#[test]
fn a_new_draft_picks_its_target_and_infers_its_timing_from_the_title() {
    use EditorControl::{
        AllDay, Cancel, Description, EndDate, EndTime, Save, StartDate, StartTime, Target, Title,
    };
    let mut e = editor();
    assert_eq!(e.source, CalendarSource::Board, "the board is the default");
    assert_eq!(
        e.visible_controls(today()),
        vec![
            Title,
            Description,
            Target,
            AllDay,
            StartDate,
            EndDate,
            Save,
            Cancel
        ]
    );
    e.focus(Target, today());
    handle_key(&mut e, &ParsedInput::Byte(b' '), today());
    assert_eq!(e.source, CalendarSource::Personal);
    assert!(e.dirty(), "flipping the target is a change worth keeping");

    e.fields[0].insert_str("Movie night tomorrow at 9pm");
    e.blur_title(today());
    assert_eq!(e.text(0), "Movie night");
    assert_eq!(e.text(2), "2026-10-04");
    assert_eq!(e.text(3), "21:00");
    assert!(!e.all_day);
    assert_eq!(
        e.visible_controls(today()),
        vec![
            Title,
            Description,
            Target,
            AllDay,
            StartDate,
            StartTime,
            EndDate,
            EndTime,
            Save,
            Cancel
        ]
    );
    let draft = e.draft(today()).unwrap();
    assert!(matches!(
        draft.timing,
        EventTiming::Timed { start, end: None } if start == Utc.with_ymd_and_hms(2026, 10, 4, 21, 0, 0).unwrap()
    ));

    let opened = Editor::from_event(&board_event(), Uuid::nil(), false, chrono_tz::UTC);
    assert!(
        !opened.visible_controls(today()).contains(&Target),
        "where a saved event lives is fixed"
    );
    assert_eq!(opened.existing, Some((Uuid::from_u128(7), 2)));
    assert_eq!(opened.text(0), "Movie night");
    assert_eq!(opened.text(3), "21:00");
    assert!(!opened.dirty());
}

#[test]
fn dst_choices_only_appear_for_ambiguous_local_times() {
    let mut e = Editor::new(CalendarSource::Board, today(), chrono_tz::America::New_York);
    e.fields[0] = field("Fall back");
    e.all_day = false;
    e.fields[2] = field("2026-11-01");
    e.fields[3] = field("01:30");
    assert!(
        e.visible_controls(today())
            .contains(&EditorControl::StartOccurrence)
    );
    assert!(e.draft(today()).is_err(), "a repeated hour needs a choice");
    e.focus(EditorControl::StartOccurrence, today());
    handle_key(&mut e, &ParsedInput::Byte(b' '), today());
    assert!(e.draft(today()).is_ok());
    e.fields[3] = field("03:30");
    assert!(
        !e.visible_controls(today())
            .contains(&EditorControl::StartOccurrence)
    );
}

#[test]
fn discard_confirmation_defaults_to_keep_and_escape_returns_to_editing() {
    let mut e = editor();
    e.fields[0].insert_str("Draft");
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(0x1b), today()),
        EditorCommand::Cancel
    );
    e.discard_prompt = true;
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(b'\r'), today()),
        EditorCommand::Keep
    );
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(0x1b), today()),
        EditorCommand::Keep
    );
    handle_key(&mut e, &ParsedInput::Byte(b'\t'), today());
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(b'\r'), today()),
        EditorCommand::Discard
    );
}

#[tokio::test]
async fn every_focused_control_is_visible_after_compact_resize() {
    let s = state();
    let mut e = editor();
    e.all_day = false;
    for (width, height) in [(120, 40), (80, 24), (48, 16), (48, 14), (22, 9)] {
        for control in e.visible_controls(today()) {
            e.focus(control, today());
            draw_editor(&e, &s, width, height);
            let geometry = e.geometry.borrow();
            assert!(
                geometry.targets.iter().any(|t| match t.action {
                    TargetAction::Focus(c) => c == control,
                    TargetAction::Command(EditorCommand::Save) => control == EditorControl::Save,
                    TargetAction::Command(EditorCommand::Cancel) =>
                        control == EditorControl::Cancel,
                    _ => false,
                }),
                "{control:?} missing at {width}x{height}"
            );
            for target in &geometry.targets {
                assert_eq!(
                    target.area.intersection(Rect::new(0, 0, width, height)),
                    target.area
                );
            }
        }
    }
}

#[tokio::test]
async fn click_places_caret_and_keeps_unicode_intact() {
    let s = state();
    let mut e = editor();
    e.fields[0] = field("abcdef");
    draw_editor(&e, &s, 80, 24);
    let target = value_target(&e, EditorControl::Title);
    click(&mut e, target.area.x, target.area.y, today());
    handle_key(&mut e, &ParsedInput::Char('X'), today());
    assert_eq!(e.text(0), "Xabcdef");
    e.fields[0] = field("a界e\u{301}🙂z");
    draw_editor(&e, &s, 80, 24);
    let target = value_target(&e, EditorControl::Title);
    click(&mut e, target.area.x + 2, target.area.y, today());
    handle_key(&mut e, &ParsedInput::Char('X'), today());
    assert_eq!(e.text(0), "aX界e\u{301}🙂z");
}
