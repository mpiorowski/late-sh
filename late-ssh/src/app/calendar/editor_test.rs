use super::*;
use crate::app::calendar::svc::CalendarService;
use late_core::db::{Db, DbConfig};
use ratatui::{Terminal, backend::TestBackend};

fn today() -> NaiveDate {
    "2026-10-03".parse().unwrap()
}

fn editor() -> Editor {
    Editor::new(
        CalendarSource::Personal(Uuid::nil()),
        today(),
        EventAccess {
            edit: true,
            notifications: true,
            delegate: false,
        },
    )
}

fn state() -> CalendarState {
    CalendarState::new(
        CalendarService::new(Db::new(&DbConfig::default()).unwrap()),
        Uuid::nil(),
    )
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

#[test]
fn calendar_editor_focus_order_tracks_visible_controls_and_retains_time_drafts() {
    use EditorControl::{
        AllDay, Cancel, Description, EndDate, Import, LeadTime, Notifications, Save, StartDate,
        StartTime, Title,
    };
    let mut e = editor();
    assert_eq!(
        e.visible_controls(today(), chrono_tz::UTC),
        vec![
            Title,
            Description,
            AllDay,
            StartDate,
            EndDate,
            Notifications,
            Save,
            Cancel,
            Import
        ]
    );
    e.focus(AllDay, today(), chrono_tz::UTC);
    handle_key(&mut e, &ParsedInput::Char(' '), today(), chrono_tz::UTC);
    e.fields[3] = field("14:30");
    e.fields[5] = field("15:45");
    assert!(
        e.visible_controls(today(), chrono_tz::UTC)
            .contains(&StartTime)
    );
    handle_key(&mut e, &ParsedInput::Byte(b' '), today(), chrono_tz::UTC);
    assert!(
        !e.visible_controls(today(), chrono_tz::UTC)
            .contains(&StartTime)
    );
    assert_eq!(e.text(3), "14:30");
    assert_eq!(e.text(5), "15:45");
    e.focus(Notifications, today(), chrono_tz::UTC);
    handle_key(&mut e, &ParsedInput::Byte(b' '), today(), chrono_tz::UTC);
    handle_key(&mut e, &ParsedInput::Byte(b'\t'), today(), chrono_tz::UTC);
    assert_eq!(e.focus, LeadTime);
    e.focus(Notifications, today(), chrono_tz::UTC);
    handle_key(&mut e, &ParsedInput::Byte(b' '), today(), chrono_tz::UTC);
    handle_key(&mut e, &ParsedInput::Byte(b'\t'), today(), chrono_tz::UTC);
    assert_eq!(e.focus, Save);
    e.focus = Title;
    handle_key(&mut e, &ParsedInput::BackTab, today(), chrono_tz::UTC);
    assert_eq!(e.focus, Import);
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(b'\r'), today(), chrono_tz::UTC),
        EditorCommand::Import
    );
}

#[test]
fn calendar_editor_dst_choices_only_appear_for_ambiguous_local_times() {
    let mut e = editor();
    let tz = chrono_tz::America::New_York;
    e.all_day = false;
    e.fields[2] = field("2026-11-01");
    e.fields[3] = field("01:30");
    e.fields[5] = field("01:45");
    let controls = e.visible_controls(today(), tz);
    assert!(controls.contains(&EditorControl::StartOccurrence));
    assert!(controls.contains(&EditorControl::EndOccurrence));
    e.fields[5] = field("02:30");
    assert!(
        !e.visible_controls(today(), tz)
            .contains(&EditorControl::EndOccurrence)
    );
    e.fields[2] = field("2026-03-08");
    e.fields[3] = field("02:30");
    assert!(
        !e.visible_controls(today(), tz)
            .contains(&EditorControl::StartOccurrence)
    );
    e.all_day = true;
    assert!(
        !e.visible_controls(today(), tz)
            .contains(&EditorControl::StartTime)
    );
}

#[test]
fn calendar_editor_discard_confirmation_defaults_to_keep_and_escape_returns_to_editing() {
    let mut e = editor();
    e.fields[0].insert_str("Unfinished appointment");
    e.discard_prompt = true;
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(b'\r'), today(), chrono_tz::UTC),
        EditorCommand::Keep
    );
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(0x1b), today(), chrono_tz::UTC),
        EditorCommand::Keep
    );
    handle_key(&mut e, &ParsedInput::Byte(b'\t'), today(), chrono_tz::UTC);
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Byte(b'\r'), today(), chrono_tz::UTC),
        EditorCommand::Discard
    );
    assert_eq!(e.text(0), "Unfinished appointment");
    assert_eq!(
        handle_key(&mut e, &ParsedInput::Char('d'), today(), chrono_tz::UTC),
        EditorCommand::Discard
    );
}

#[test]
fn calendar_editor_mouse_columns_follow_unicode_graphemes() {
    let text = "a界e\u{301}🙂z";
    for (cell, expected) in [
        (0, 0),
        (1, 1),
        (2, 1),
        (3, 2),
        (4, 4),
        (5, 4),
        (6, 5),
        (20, 6),
    ] {
        assert_eq!(column_at_cell(text, cell), expected, "cell {cell}");
    }
}

#[tokio::test]
async fn calendar_editor_every_focused_control_is_visible_after_compact_resize() {
    let s = state();
    let mut e = editor();
    e.all_day = false;
    e.notifications = true;
    e.access.delegate = true;
    for (width, height) in [(120, 40), (80, 24), (48, 16), (48, 14), (22, 9)] {
        for control in e.visible_controls(today(), chrono_tz::UTC) {
            e.focus(control, today(), chrono_tz::UTC);
            draw_editor(&e, &s, width, height);
            let geometry = e.geometry.borrow();
            assert!(
                geometry.targets.iter().any(|t| match t.action {
                    TargetAction::Focus(c) => c == control,
                    TargetAction::Command(EditorCommand::Save) => control == EditorControl::Save,
                    TargetAction::Command(EditorCommand::Cancel) =>
                        control == EditorControl::Cancel,
                    TargetAction::Command(EditorCommand::Import) =>
                        control == EditorControl::Import,
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
            if control.field().is_some() {
                drop(geometry);
                let target = value_target(&e, control);
                assert!(target.area.height > 0);
            }
        }
    }
}

#[tokio::test]
async fn calendar_editor_click_places_caret_and_keeps_unicode_intact() {
    let s = state();
    let mut e = editor();
    e.fields[0] = field("abcdef");
    draw_editor(&e, &s, 80, 24);
    let target = value_target(&e, EditorControl::Title);
    click(
        &mut e,
        target.area.x,
        target.area.y,
        today(),
        chrono_tz::UTC,
    );
    handle_key(&mut e, &ParsedInput::Char('X'), today(), chrono_tz::UTC);
    assert_eq!(e.text(0), "Xabcdef");
    e.fields[0] = field("a界e\u{301}🙂z");
    draw_editor(&e, &s, 80, 24);
    let target = value_target(&e, EditorControl::Title);
    click(
        &mut e,
        target.area.x + 2,
        target.area.y,
        today(),
        chrono_tz::UTC,
    );
    handle_key(&mut e, &ParsedInput::Char('X'), today(), chrono_tz::UTC);
    assert_eq!(e.text(0), "aX界e\u{301}🙂z");
}

#[tokio::test]
async fn calendar_editor_mouse_click_accounts_for_horizontal_and_description_viewports() {
    let s = state();
    let mut e = editor();
    e.fields[0] = field("abcdefghijklmnopqrstuvwxyz0123456789");
    draw_editor(&e, &s, 22, 14);
    let target = value_target(&e, EditorControl::Title);
    let TargetAction::Value(_, _, left) = target.action else {
        panic!()
    };
    assert!(left > 0);
    click(
        &mut e,
        target.area.x + 1,
        target.area.y,
        today(),
        chrono_tz::UTC,
    );
    assert_eq!(e.fields[0].cursor().1, left + 1);
    handle_key(&mut e, &ParsedInput::Char('X'), today(), chrono_tz::UTC);
    assert_eq!(e.text(0).chars().nth(left + 1), Some('X'));

    e.fields[1] = field(
        "zero\none\ntwo\nthree abcdefghijklmnopqrstuvwxyz\nfour abcdefghijklmnopqrstuvwxyz\nfive abcdefghijklmnopqrstuvwxyz",
    );
    e.focus(EditorControl::Description, today(), chrono_tz::UTC);
    draw_editor(&e, &s, 22, 16);
    let target = value_target(&e, EditorControl::Description);
    let TargetAction::Value(_, top, left) = target.action else {
        panic!()
    };
    assert_eq!(top, 3);
    assert!(left > 0);
    click(
        &mut e,
        target.area.x,
        target.area.y,
        today(),
        chrono_tz::UTC,
    );
    assert_eq!(e.fields[1].cursor().0, 3);
    assert_eq!(e.fields[1].cursor().1, left);
    handle_key(&mut e, &ParsedInput::Char('X'), today(), chrono_tz::UTC);
    assert_eq!(e.fields[1].lines()[3].chars().nth(left), Some('X'));
}

#[tokio::test]
async fn calendar_editor_wheel_and_scrollbar_reach_hidden_fields_without_moving_focus() {
    let s = state();
    let mut e = editor();
    e.all_day = false;
    e.notifications = true;
    draw_editor(&e, &s, 48, 16);
    let area = e.geometry.borrow().viewport;
    assert!(scroll(&mut e, area.x + 3, area.y + 2, 3));
    draw_editor(&e, &s, 48, 16);
    assert_eq!(e.scroll.get(), 3);
    assert_eq!(e.focus, EditorControl::Title);
    for _ in 0..8 {
        click(
            &mut e,
            area.right() - 1,
            area.bottom() - 1,
            today(),
            chrono_tz::UTC,
        );
        draw_editor(&e, &s, 48, 16);
    }
    assert_eq!(e.scroll.get(), e.geometry.borrow().max_scroll);
    assert!(
        e.geometry
            .borrow()
            .targets
            .iter()
            .any(|t| matches!(t.action, TargetAction::Focus(EditorControl::LeadTime)))
    );
    handle_key(&mut e, &ParsedInput::Byte(b'\t'), today(), chrono_tz::UTC);
    draw_editor(&e, &s, 48, 16);
    assert_eq!(e.focus, EditorControl::Description);
    assert!(value_target(&e, EditorControl::Description).area.height > 0);
}
