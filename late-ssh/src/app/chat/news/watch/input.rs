//! Keys while the watch modal is up. The player relays pause, seek (10 s,
//! a minute with Shift), restart and mute to the paired CLI and toggles
//! its size; the setup box only copies and closes. Everything else, mouse
//! included, is swallowed, so nothing reaches the page under the video.

use ratatui::layout::Position;

use super::state::WatchModal;
use super::svc;
use crate::app::common::primitives::Banner;
use crate::app::input::{
    MouseButton, MouseEvent, MouseEventKind, ParsedInput, sanitize_paste_markers,
};
use crate::app::state::App;
use crate::paired_clients::VideoControl;

pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    let Some(modal) = app.chat.watch() else {
        return;
    };
    let close = match modal {
        WatchModal::Player(player) => player.drawn().close,
        WatchModal::Setup(setup) => setup.close(),
    };
    let player = match modal {
        WatchModal::Player(_) => true,
        WatchModal::Setup(_) => false,
    };
    match event {
        ParsedInput::Mouse(mouse) => {
            if app.interaction_mode.mouse_enabled() && clicks_close(*mouse, close) {
                app.chat.close_watch();
            }
        }
        ParsedInput::Byte(0x1B) => app.chat.close_watch(),
        ParsedInput::Byte(b'c') | ParsedInput::Char('c') => copy_link(app),
        ParsedInput::Byte(b' ') | ParsedInput::Char(' ') if player => {
            svc::send_control(app, VideoControl::TogglePause);
        }
        ParsedInput::Arrow(b'D') | ParsedInput::Byte(b'h') | ParsedInput::Char('h') if player => {
            svc::send_control(app, VideoControl::SeekBack);
        }
        ParsedInput::Arrow(b'C') | ParsedInput::Byte(b'l') | ParsedInput::Char('l') if player => {
            svc::send_control(app, VideoControl::SeekForward);
        }
        ParsedInput::ShiftArrow(b'D') | ParsedInput::Byte(b'H') | ParsedInput::Char('H')
            if player =>
        {
            svc::send_control(app, VideoControl::SeekBackFar);
        }
        ParsedInput::ShiftArrow(b'C') | ParsedInput::Byte(b'L') | ParsedInput::Char('L')
            if player =>
        {
            svc::send_control(app, VideoControl::SeekForwardFar);
        }
        ParsedInput::Byte(b'0') | ParsedInput::Char('0') | ParsedInput::Home if player => {
            svc::send_control(app, VideoControl::Restart);
        }
        ParsedInput::Byte(b'm') | ParsedInput::Char('m') if player => {
            svc::send_control(app, VideoControl::ToggleMute);
        }
        ParsedInput::Byte(b'f') | ParsedInput::Char('f') => {
            if let Some(player) = app.chat.news.player_mut() {
                player.toggle_size();
            }
        }
        _ => {}
    }
}

fn copy_link(app: &mut App) {
    let url = match app.chat.watch() {
        Some(WatchModal::Player(player)) => player.url().to_owned(),
        Some(WatchModal::Setup(setup)) => setup.url().to_owned(),
        None => return,
    };
    app.pending_clipboard = Some(sanitize_paste_markers(&url).trim().to_owned());
    app.banner = Some(Banner::success("Link copied!"));
}

/// A left click on `[x]`. Mouse coordinates are 1-based.
fn clicks_close(mouse: MouseEvent, close: ratatui::layout::Rect) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    close.contains(Position::new(x, y))
}
