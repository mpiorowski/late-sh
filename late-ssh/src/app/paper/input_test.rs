use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::*;
use crate::app::input::MouseModifiers;
use crate::app::paper::{
    state::{PaperInk, PaperSpan},
    ui,
};

fn paper() -> PaperModal {
    let mut modal = PaperModal::at_the_press();
    modal.lines = (0..100)
        .map(|row| vec![PaperSpan::new(format!("row {row}"), PaperInk::Body)])
        .collect();
    draw(&modal, 80, 24);
    modal
}

fn draw(modal: &PaperModal, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::draw(frame, frame.area(), modal))
        .unwrap();
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        button: Some(MouseButton::Left),
        x: x + 1,
        y: y + 1,
        modifiers: MouseModifiers::default(),
    }
}

#[test]
fn wheel_scrolls_only_inside_the_paper_and_stops_at_both_ends() {
    let modal = paper();
    let body = modal.viewport().body;
    handle_mouse(&modal, mouse(MouseEventKind::ScrollDown, 0, 0));
    assert_eq!(modal.scroll_offset(), 0);
    handle_mouse(&modal, mouse(MouseEventKind::ScrollDown, body.x, body.y));
    assert_eq!(modal.scroll_offset(), 3);
    for _ in 0..100 {
        handle_mouse(&modal, mouse(MouseEventKind::ScrollDown, body.x, body.y));
    }
    assert_eq!(modal.scroll_offset(), 100 - body.height);
    for _ in 0..100 {
        handle_mouse(&modal, mouse(MouseEventKind::ScrollUp, body.x, body.y));
    }
    assert_eq!(modal.scroll_offset(), 0);
}

#[test]
fn scrollbar_pages_drags_and_releases_outside_the_popup() {
    let modal = paper();
    let viewport = modal.viewport();
    let track = viewport.track;
    handle_mouse(
        &modal,
        mouse(MouseEventKind::Down, track.x, track.bottom() - 1),
    );
    assert_eq!(modal.scroll_offset(), viewport.body.height);
    let thumb = modal.thumb();
    handle_mouse(&modal, mouse(MouseEventKind::Down, thumb.x, thumb.y));
    handle_mouse(&modal, mouse(MouseEventKind::Drag, 0, track.bottom() + 3));
    assert_eq!(modal.scroll_offset(), 100 - viewport.body.height);
    handle_mouse(&modal, mouse(MouseEventKind::Drag, 0, 0));
    assert_eq!(modal.scroll_offset(), 0);
    handle_mouse(&modal, mouse(MouseEventKind::Up, 0, 0));
    handle_mouse(&modal, mouse(MouseEventKind::Drag, track.x, track.bottom()));
    assert_eq!(modal.scroll_offset(), 0, "release outside cancels dragging");
}

#[test]
fn only_left_press_on_close_requests_dismissal() {
    let modal = paper();
    let close = modal.viewport().close;
    assert!(!handle_mouse(&modal, mouse(MouseEventKind::Down, 0, 0)));
    assert!(!handle_mouse(
        &modal,
        mouse(MouseEventKind::Up, close.x, close.y)
    ));
    let mut right = mouse(MouseEventKind::Down, close.x, close.y);
    right.button = Some(MouseButton::Right);
    assert!(!handle_mouse(&modal, right));
    assert!(handle_mouse(
        &modal,
        mouse(MouseEventKind::Down, close.x, close.y)
    ));
}

#[test]
fn resize_cancels_drag_and_short_content_has_no_scrollbar() {
    let mut modal = paper();
    let thumb = modal.thumb();
    handle_mouse(&modal, mouse(MouseEventKind::Down, thumb.x, thumb.y));
    modal.invalidate_viewport();
    assert_eq!(modal.viewport().close, Rect::default());
    draw(&modal, 100, 30);
    handle_mouse(&modal, mouse(MouseEventKind::Drag, thumb.x, 29));
    assert_eq!(modal.scroll_offset(), 0);
    modal.scroll(90);
    modal.lines.truncate(2);
    draw(&modal, 100, 30);
    assert_eq!(modal.scroll_offset(), 0);
    assert!(modal.viewport().track.is_empty());
    assert!(modal.thumb().is_empty());
    let loading = PaperModal::at_the_press();
    draw(&loading, 80, 24);
    loading.scroll(3);
    assert_eq!(loading.scroll_offset(), 0);
    assert!(loading.viewport().track.is_empty());
}

#[tokio::test]
async fn paper_mouse_routing_honors_mode_modal_priority_and_loading_cancellation() {
    use crate::{
        app::{common::primitives::Screen, leaderboard::state::Board, paper::svc::PaperTrigger},
        test_helpers::{make_app, new_test_db},
    };
    use late_core::{models::user::InteractionMode, test_utils::create_test_user};

    let db = new_test_db().await;
    let user = create_test_user(&db.db, "paper-mouse-routing").await;
    let mut app = make_app(db.db.clone(), user.id, "paper-mouse-routing");
    app.set_screen(Screen::Leaderboard);
    app.show_help = true;
    app.paper.modal = Some(paper());
    app.resize(80, 24).unwrap();
    let bytes = app.render().unwrap();
    let mut screen = vt100::Parser::new(24, 80, 0);
    screen.process(&bytes);
    let modal = app.paper.modal.as_ref().unwrap();
    let viewport = modal.viewport();
    assert_eq!(
        screen
            .screen()
            .cell(viewport.close.y, viewport.close.x)
            .unwrap()
            .contents(),
        "[",
        "paper draws above the help modal"
    );
    let wheel = format!("\x1b[<65;{};{}M", viewport.body.x + 1, viewport.body.y + 1);
    let close = format!("\x1b[<0;{};{}M", viewport.close.x + 1, viewport.close.y + 1);
    app.interaction_mode = InteractionMode::Keyboard;
    app.handle_input(wheel.as_bytes());
    app.handle_input(close.as_bytes());
    assert_eq!(app.paper.modal.as_ref().unwrap().scroll_offset(), 0);
    app.interaction_mode = InteractionMode::Mouse;
    app.handle_input(wheel.as_bytes());
    assert_eq!(app.paper.modal.as_ref().unwrap().scroll_offset(), 3);
    app.handle_input(b"j");
    assert_eq!(app.paper.modal.as_ref().unwrap().scroll_offset(), 4);
    assert_eq!(app.leaderboard_page.selected_board(), Board::TopChips);
    app.handle_input(close.as_bytes());
    assert!(app.paper.modal.is_none());
    assert!(
        app.show_help,
        "closing the paper leaves the underlying modal open"
    );
    app.paper.modal = Some(PaperModal::at_the_press());
    app.paper.awaiting = Some(PaperTrigger::Command);
    app.render().unwrap();
    app.handle_input(close.as_bytes());
    assert!(app.paper.modal.is_none());
    assert!(app.paper.awaiting.is_none());
}
