use super::*;
use crate::app::artboard::{
    gallery::svc::GalleryService,
    provenance::ArtboardProvenance,
    svc::{ArtboardSnapshotService, DartboardService, DartboardSnapshot},
};
use crate::app::input::{MouseEvent, MouseModifiers};

fn state() -> State {
    let snapshot = DartboardSnapshot {
        canvas: Canvas::with_size(48, 48),
        ..Default::default()
    };
    State::new(
        DartboardService::disconnected_for_tests(snapshot),
        ArtboardSnapshotService::disabled(),
        GalleryService::disabled(),
        uuid::Uuid::nil(),
        "artist".into(),
        ArtboardProvenance::default().shared(),
    )
}

#[test]
fn avatar_frame_resizes_each_axis_independently_and_stays_on_the_board() {
    let board = Canvas::with_size(48, 48);
    let mut crop = AvatarFrame::new(Pos { x: 47, y: 47 }, &board);
    assert_eq!((crop.width, crop.height), (16, 8));
    assert_eq!(crop.origin, Pos { x: 32, y: 40 });
    crop.resize(100, 0, &board);
    assert_eq!((crop.width, crop.height), (32, 8));
    assert_eq!(crop.origin, Pos { x: 16, y: 40 });
    crop.resize(0, 100, &board);
    assert_eq!((crop.width, crop.height), (32, 32));
    assert_eq!(crop.origin, Pos { x: 16, y: 16 });
    crop.move_by(100, 100, &board);
    assert_eq!(crop.origin, Pos { x: 16, y: 16 });
    crop.resize(-100, 0, &board);
    assert_eq!((crop.width, crop.height), (1, 32));
    crop.resize(0, -100, &board);
    assert_eq!((crop.width, crop.height), (1, 1));
    crop.move_by(-100, -100, &board);
    assert_eq!(crop.origin, Pos { x: 0, y: 0 });
    crop.resize(4, 6, &board);
    assert_eq!((crop.width, crop.height), (5, 7));
    let tiny_board = Canvas::with_size(3, 2);
    let mut crop = AvatarFrame::new(Pos { x: 2, y: 1 }, &tiny_board);
    crop.resize(100, 100, &tiny_board);
    assert_eq!((crop.width, crop.height), (3, 2));
    assert_eq!(crop.origin, Pos { x: 0, y: 0 });
}

#[test]
fn avatar_drag_limits_each_axis_from_its_anchor_in_both_directions() {
    let board = Canvas::with_size(96, 96);
    let anchor = Pos { x: 47, y: 47 };
    let mut crop = AvatarFrame::new(anchor, &board);
    for (target, origin, width, height) in [
        (Pos { x: 95, y: 50 }, anchor, 32, 4),
        (Pos { x: 0, y: 44 }, Pos { x: 16, y: 44 }, 32, 4),
        (Pos { x: 50, y: 95 }, anchor, 4, 32),
        (Pos { x: 44, y: 0 }, Pos { x: 44, y: 16 }, 4, 32),
        (Pos { x: 0, y: 0 }, Pos { x: 16, y: 16 }, 32, 32),
        (anchor, anchor, 1, 1),
    ] {
        crop.drag_to(anchor, target, &board);
        assert_eq!(
            (crop.origin, crop.width, crop.height),
            (origin, width, height)
        );
    }
}

#[test]
fn avatar_shift_arrows_resize_only_the_chosen_axis() {
    let mut state = state();
    begin(&mut state);
    let dimensions = |state: &State| {
        let AvatarFlow::Framing(frame) = &state.gallery().avatar else {
            panic!("framing")
        };
        (frame.width, frame.height)
    };
    for _ in 0..16 {
        handle_event(&mut state, (80, 40), &ParsedInput::ShiftArrow(b'C'));
    }
    for _ in 0..4 {
        handle_event(&mut state, (80, 40), &ParsedInput::ShiftArrow(b'A'));
    }
    assert_eq!(dimensions(&state), (32, 4));
    for _ in 0..28 {
        handle_arrow(&mut state, b'D', true);
        handle_arrow(&mut state, b'B', true);
    }
    assert_eq!(dimensions(&state), (4, 32));
    handle_arrow(&mut state, b'C', false);
    assert_eq!(dimensions(&state), (4, 32));
    let selection = state.selection_view().unwrap();
    assert_eq!(selection.cursor.x - selection.anchor.x + 1, 4);
    assert_eq!(selection.cursor.y - selection.anchor.y + 1, 32);
}

#[test]
fn avatar_preview_is_frozen_and_cancel_does_not_change_the_board() {
    let mut state = state();
    state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, '@');
    let before = state.snapshot.canvas.clone();
    begin(&mut state);
    assert!(state.gallery().captures_typing());
    assert!(state.gallery_claims_input());
    assert!(
        !handle_event(&mut state, (80, 40), &ParsedInput::Byte(b'\r')),
        "plain keys must reach the byte handler"
    );
    for _ in 0..12 {
        handle_arrow(&mut state, b'D', true);
    }
    for _ in 0..5 {
        handle_arrow(&mut state, b'A', true);
    }
    handle_key(&mut state, b'\r');
    let AvatarFlow::Confirm {
        avatar: Some(avatar),
        ..
    } = &state.gallery().avatar
    else {
        panic!("preview")
    };
    assert_eq!((avatar.width(), avatar.height()), (4, 3));
    assert_eq!(
        avatar.canvas().glyph_at(Pos { x: 0, y: 0 }).unwrap().ch,
        '@'
    );
    state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, 'X');
    let AvatarFlow::Confirm {
        avatar: Some(avatar),
        ..
    } = &state.gallery().avatar
    else {
        panic!("preview")
    };
    assert_eq!(
        avatar.canvas().glyph_at(Pos { x: 0, y: 0 }).unwrap().ch,
        '@'
    );
    state.snapshot.canvas = before.clone();
    handle_key(&mut state, 0x1b);
    assert!(matches!(state.gallery().avatar, AvatarFlow::Framing(_)));
    handle_key(&mut state, 0x1b);
    assert!(!state.gallery().avatar.is_active());
    assert!(state.selection_view().is_none());
    assert_eq!(state.snapshot.canvas, before);
}

#[test]
fn avatar_mouse_drag_uses_board_coordinates_without_an_aspect_ratio_lock() {
    let mut state = state();
    begin(&mut state);
    let mouse = |kind, x, y| {
        ParsedInput::Mouse(MouseEvent {
            kind,
            button: Some(MouseButton::Left),
            x,
            y,
            modifiers: MouseModifiers::default(),
        })
    };
    // The board starts at terminal (1,1), SGR (2,2).
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Down, 5, 6));
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Drag, 12, 9));
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Up, 12, 9));
    let AvatarFlow::Framing(frame) = &state.gallery().avatar else {
        panic!("framing")
    };
    assert_eq!(frame.origin, Pos { x: 3, y: 4 });
    assert_eq!((frame.width, frame.height), (8, 4));
    let selection = state.selection_view().unwrap();
    assert_eq!(selection.cursor.x - selection.anchor.x + 1, 8);
    assert_eq!(selection.cursor.y - selection.anchor.y + 1, 4);
    // A mostly vertical drag keeps its narrow width.
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Down, 5, 6));
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Drag, 8, 12));
    handle_event(&mut state, (80, 40), &mouse(MouseEventKind::Up, 8, 12));
    let selection = state.selection_view().unwrap();
    assert_eq!(selection.cursor.x - selection.anchor.x + 1, 4);
    assert_eq!(selection.cursor.y - selection.anchor.y + 1, 7);
}

#[test]
fn empty_frames_stay_in_picker_and_remove_requires_confirmation() {
    let mut state = state();
    begin(&mut state);
    handle_key(&mut state, b'\r');
    assert!(matches!(state.gallery().avatar, AvatarFlow::Framing(_)));
    assert!(state.gallery().notice().unwrap().contains("empty"));
    handle_key(&mut state, b'x');
    assert!(matches!(
        state.gallery().avatar,
        AvatarFlow::Confirm {
            avatar: None,
            saving: false,
            ..
        }
    ));
    handle_key(&mut state, 0x1b);
    assert!(matches!(state.gallery().avatar, AvatarFlow::Framing(_)));
}

#[test]
fn failed_avatar_save_keeps_the_frozen_preview_for_retry() {
    let mut state = state();
    state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, '@');
    begin(&mut state);
    handle_key(&mut state, b'\r');
    handle_key(&mut state, b'\r');
    assert!(matches!(
        state.gallery().avatar,
        AvatarFlow::Confirm { saving: true, .. }
    ));
    state.tick(); // Disabled storage reports an error through the normal result channel.
    let AvatarFlow::Confirm {
        avatar: Some(avatar),
        saving: false,
        ..
    } = &state.gallery().avatar
    else {
        panic!("retryable preview")
    };
    assert_eq!(
        avatar.canvas().glyph_at(Pos { x: 0, y: 0 }).unwrap().ch,
        '@'
    );
    assert!(state.gallery().notice().unwrap().contains("unavailable"));
}

#[test]
fn a_large_avatar_preview_scrolls_to_its_last_row_on_a_short_terminal() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = state();
    state.snapshot.canvas.put_glyph(Pos { x: 0, y: 0 }, '@');
    state.snapshot.canvas.put_glyph(Pos { x: 0, y: 31 }, 'Z');
    begin(&mut state);
    for _ in 0..24 {
        handle_key(&mut state, b'+');
    }
    handle_key(&mut state, b'\r');
    let mut terminal = Terminal::new(TestBackend::new(80, 14)).unwrap();
    terminal
        .draw(|frame| draw(frame, Rect::new(0, 0, 80, 14), &state))
        .unwrap();
    assert!(
        !terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol() == "Z")
    );
    for _ in 0..32 {
        handle_arrow(&mut state, b'B', false);
    }
    terminal
        .draw(|frame| draw(frame, Rect::new(0, 0, 80, 14), &state))
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol() == "Z")
    );
    assert!(matches!(
        state.gallery().avatar,
        AvatarFlow::Confirm { saving: false, .. }
    ));
}
