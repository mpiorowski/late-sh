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
use super::model::View;
use super::state::{Acted, State};
use super::world::Direction;

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
        "The fire is dead and the room is freezing. Light it, see what the light brings in, and build a village around it. Then walk out into the wasteland, and find a way off this rock. Your save grows while you are connected."
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
    if ending_took_key(app) {
        return true;
    }

    let acted = {
        let state = app.darkroom_state.as_mut().unwrap();
        // Esc means different things per view: it aborts a flight, parks a
        // trip, and otherwise leaves the door. The modal keeps the keys while
        // it is up, so Esc there does nothing (its own rows are the way out).
        if byte == 0x1B {
            if state.flight.is_some() {
                state.abort_flight();
                return true;
            }
            if state.event.is_some() {
                return true;
            }
            if state.view == View::World {
                state.park();
                leave(app);
                return true;
            }
            leave(app);
            return true;
        }
        // Walking the wasteland: wasd doubles for the arrows, and nothing
        // else on this screen wants those keys.
        if state.view == View::World && state.event.is_none() {
            let walked = match byte {
                b'w' | b'W' => Some(Direction::North),
                b's' | b'S' => Some(Direction::South),
                b'a' | b'A' => Some(Direction::West),
                b'd' | b'D' => Some(Direction::East),
                _ => None,
            };
            if let Some(direction) = walked {
                state.walk(direction);
                return true;
            }
        }
        if state.flight.is_some() {
            let steered = match byte {
                b'w' | b'W' => Some((0.0, -1.0)),
                b's' | b'S' => Some((0.0, 1.0)),
                b'a' | b'A' => Some((-1.0, 0.0)),
                b'd' | b'D' => Some((1.0, 0.0)),
                _ => None,
            };
            if let Some((dx, dy)) = steered {
                state.steer(dx, dy);
            }
            // Mid-flight the ship rows are still underneath; nothing but
            // steering (and the Esc abort above) may reach them, or Enter
            // could restart the ascent.
            return true;
        }
        match byte {
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

/// The ending owns every key while it is up: the first press skips the
/// reveal, the next one steps out of the door. There is nothing behind it to
/// go back to (the save was deleted the moment the ship got through), so this
/// is the only exit the screen offers. Returns whether it took the key.
fn ending_took_key(app: &mut App) -> bool {
    let done = match app
        .darkroom_state
        .as_mut()
        .and_then(|state| state.ending.as_mut())
    {
        None => return false,
        Some(ending) if ending.done() => true,
        Some(ending) => {
            ending.reveal_all();
            false
        }
    };
    if done {
        leave(app);
    }
    true
}

fn handle_arrow(app: &mut App, key: u8) -> bool {
    if ending_took_key(app) {
        return true;
    }
    let Some(state) = app.darkroom_state.as_mut() else {
        return false;
    };
    if state.flight.is_some() {
        match key {
            b'A' => state.steer(0.0, -1.0),
            b'B' => state.steer(0.0, 1.0),
            b'C' => state.steer(1.0, 0.0),
            b'D' => state.steer(-1.0, 0.0),
            _ => {}
        }
        return true;
    }
    // Out in the wasteland an arrow is a step, and a step is the whole game.
    if state.view == View::World && state.event.is_none() {
        match key {
            b'A' => state.walk(Direction::North),
            b'B' => state.walk(Direction::South),
            b'C' => state.walk(Direction::East),
            b'D' => state.walk(Direction::West),
            _ => {}
        }
        return true;
    }
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
