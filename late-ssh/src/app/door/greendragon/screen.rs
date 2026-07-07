//! Top-level Green Dragon door screen: the [`DoorGame`] implementation plus the
//! launcher/active input handling and landing render. Mirrors the Lateania
//! screen shell — this is a native in-process door, so input mutates the
//! session `State` directly and leaving returns to the Games hub.

use ratatui::{Frame, layout::Rect};

use crate::app::{
    common::primitives::Screen,
    door::game::{DoorGame, DoorGameId},
    files::terminal_image::TerminalImageFrame,
    state::App,
};

use super::state::{Selection, State};

pub const GAME: GreenDragonDoorGame = GreenDragonDoorGame;

pub struct GreenDragonDoorGame;

impl DoorGame for GreenDragonDoorGame {
    type View<'a> = GreenDragonScreenView<'a>;

    fn id(&self) -> DoorGameId {
        DoorGameId::GreenDragon
    }

    fn title(&self) -> &'static str {
        "Green Dragon"
    }

    fn description(&self) -> &'static str {
        "An open-source remake of LORD: hunt the forest, beat the masters, gear up, and slay the Green Dragon. Your character persists."
    }

    fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        view: &GreenDragonScreenView<'_>,
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
        if app.greendragon_state.is_some() {
            leave(app);
            true
        } else {
            false
        }
    }
}

pub struct GreenDragonScreenView<'a> {
    pub delete_confirm: bool,
    pub state: Option<&'a State>,
}

fn draw_screen(frame: &mut Frame, area: Rect, view: &GreenDragonScreenView<'_>) {
    if let Some(state) = view.state {
        super::ui::draw_page(frame, area, state);
    } else {
        super::ui::draw_landing(frame, area, view.delete_confirm);
    }
}

fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.greendragon_state.is_none() {
        // Launcher fallback: Enter starts a game (the hub normally does this).
        if matches!(byte, b'\r' | b'\n') {
            app.enter_greendragon();
            return true;
        }
        return false;
    }

    // Compute the selection in a tight borrow, then act on `app` once it's
    // released (leaving the game re-borrows `app` mutably).
    let selection = {
        let state = app.greendragon_state.as_mut().unwrap();
        match byte {
            0x1B => Some(state.back()),
            b'k' | b'K' | b'w' | b'W' => {
                state.move_cursor(-1);
                None
            }
            b'j' | b'J' | b's' | b'S' => {
                state.move_cursor(1);
                None
            }
            b'\r' | b'\n' | b' ' => Some(state.select()),
            _ => None,
        }
    };

    if selection == Some(Selection::Leave) {
        leave(app);
    }
    true
}

fn handle_arrow(app: &mut App, key: u8) -> bool {
    let Some(state) = app.greendragon_state.as_mut() else {
        return false;
    };
    match key {
        b'A' => state.move_cursor(-1),
        b'B' => state.move_cursor(1),
        _ => {}
    }
    true
}

/// Save the character and return to the Games hub.
fn leave(app: &mut App) {
    if let Some(state) = app.greendragon_state.as_ref() {
        state.save_on_leave();
    }
    app.leave_greendragon();
    app.set_screen(Screen::Games);
}

/// Two-column landing card for the Games hub (delegates to the renderer).
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool) {
    super::ui::draw_landing(frame, area, delete_confirm);
}
