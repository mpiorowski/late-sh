//! Top-level Dark Room door screen: the [`DoorGame`] implementation plus the
//! launcher/active input handling and landing render. Mirrors the Green Dragon
//! screen shell — this is a native in-process door, so input mutates the
//! session `State` directly and leaving returns to the Games hub.

use ratatui::{Frame, layout::Rect};

use crate::app::{
    common::primitives::Screen,
    door::game::{DoorGame, DoorGameId},
    files::terminal_image::TerminalImageFrame,
    state::App,
};

use super::data;
use super::state::{Acted, State};

pub const GAME: DarkroomDoorGame = DarkroomDoorGame;

pub struct DarkroomDoorGame;

impl DoorGame for DarkroomDoorGame {
    type View<'a> = DarkroomScreenView<'a>;

    fn id(&self) -> DoorGameId {
        DoorGameId::Darkroom
    }

    fn title(&self) -> &'static str {
        data::TITLE
    }

    fn description(&self) -> &'static str {
        "The fire is dead and the room is freezing. Light it, see what the light brings in, and build a village around it. Your save grows while you are connected."
    }

    fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        view: &DarkroomScreenView<'_>,
        _terminal_images: &mut TerminalImageFrame,
    ) {
        draw_screen(frame, area, view);
    }

    fn handle_key(&self, app: &mut App, byte: u8) -> bool {
        handle_key(app, byte)
    }

    fn handle_arrow(&self, app: &mut App, key: u8) -> bool {
        handle_arrow(app, key)
    }

    fn leave_active(&self, app: &mut App) -> bool {
        if app.darkroom_state.is_some() {
            leave(app);
            true
        } else {
            false
        }
    }
}

pub struct DarkroomScreenView<'a> {
    pub delete_confirm: bool,
    pub state: Option<&'a State>,
}

fn draw_screen(frame: &mut Frame, area: Rect, view: &DarkroomScreenView<'_>) {
    match view.state {
        Some(state) => super::ui::draw_page(frame, area, state),
        None => super::ui::draw_landing(frame, area, view.delete_confirm),
    }
}

fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.darkroom_state.is_none() {
        // Launcher fallback: Enter starts a game (the hub normally does this).
        if matches!(byte, b'\r' | b'\n') {
            app.enter_darkroom();
            return true;
        }
        return false;
    }

    // Compute the outcome in a tight borrow, then act on `app` once it's
    // released (leaving the game re-borrows `app` mutably).
    let acted = {
        let state = app.darkroom_state.as_mut().unwrap();
        match byte {
            0x1B => Acted::Leave,
            b'k' | b'K' | b'w' | b'W' => {
                state.move_cursor(-1);
                Acted::Stay
            }
            b'j' | b'J' | b's' | b'S' => {
                state.move_cursor(1);
                Acted::Stay
            }
            b'\t' => {
                state.toggle_view();
                Acted::Stay
            }
            // Worker rows: +/- move one villager, </> move ten (upstream's
            // up/dn and upMany/dnMany buttons).
            b'+' | b'=' => {
                state.assign_selected(1);
                Acted::Stay
            }
            b'-' | b'_' => {
                state.unassign_selected(1);
                Acted::Stay
            }
            b'>' | b'.' => {
                state.assign_selected(10);
                Acted::Stay
            }
            b'<' | b',' => {
                state.unassign_selected(10);
                Acted::Stay
            }
            b'\r' | b'\n' | b' ' => state.select(),
            _ => Acted::Stay,
        }
    };

    if acted == Acted::Leave {
        leave(app);
    }
    true
}

fn handle_arrow(app: &mut App, key: u8) -> bool {
    let Some(state) = app.darkroom_state.as_mut() else {
        return false;
    };
    match key {
        b'A' => state.move_cursor(-1),
        b'B' => state.move_cursor(1),
        b'D' => state.unassign_selected(1),
        b'C' => state.assign_selected(1),
        _ => {}
    }
    true
}

/// Settle the clock one last time, save, and return to the Games hub.
fn leave(app: &mut App) {
    if let Some(state) = app.darkroom_state.as_mut() {
        state.save_on_leave();
    }
    app.leave_darkroom();
    app.set_screen(Screen::Games);
}

/// Two-column landing card for the Games hub (delegates to the renderer).
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool) {
    super::ui::draw_landing(frame, area, delete_confirm);
}
