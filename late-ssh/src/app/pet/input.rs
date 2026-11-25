//! The pet's gestures: clicks on its tile (the pet, a treat in the pouch,
//! the tile's rows, the floor), the keys and clicks of its care box, and
//! the `/pet` line. Nothing here decides anything about the pet: every
//! gesture becomes a call on `PetState`, which asks the service.

use late_core::models::pet::{PetSpecies, Pick};
use ratatui::layout::Position;

use crate::app::{
    common::primitives::Banner,
    input::{MouseButton, MouseEventKind, ParsedInput},
    pet::state::{Clicked, Toy},
    state::App,
};

/// A left click at (`x`, `y`) on what the pet's box last drew, on Home or
/// in the care box. True when the pet's tile took it. The box only draws
/// for a pet that is out, so a click can only land on one here; the
/// service's claims decide off the row, since another session may have put
/// it away since.
pub(crate) fn handle_click(app: &mut App, x: u16, y: u16) -> bool {
    if !app.pet_out() {
        return false;
    }
    let Some(frame) = app.last_pet_frame.get() else {
        return false;
    };
    let at = Position { x, y };
    let on_pet = app
        .last_pet_rect
        .get()
        .is_some_and(|rect| rect.contains(at));
    if on_pet {
        let clicked = app.pet_state.click(
            std::time::Instant::now(),
            chrono::Utc::now().date_naive(),
            app.show_pet_modal,
        );
        match clicked {
            Clicked::Done => {}
            Clicked::OpenCare => open_care(app),
        }
        return true;
    }
    if frame.treat.is_some_and(|rect| rect.contains(at)) {
        app.pet_state.give();
        return true;
    }
    if frame.rows.iter().flatten().any(|row| row.contains(at)) {
        open_care(app);
        return true;
    }
    if frame.zone.contains(at) {
        app.pet_state.throw(
            Toy::Ball,
            usize::from(x - frame.zone.x),
            usize::from(frame.zone.height.saturating_sub(1)),
        );
        return true;
    }
    false
}

/// A right click at (`x`, `y`): anywhere on the pet's tile it opens the
/// care box, which is how a tile too short for its status row gets there.
pub(crate) fn handle_right_click(app: &mut App, x: u16, y: u16) -> bool {
    let Some(frame) = app.last_pet_frame.get() else {
        return false;
    };
    let at = Position { x, y };
    let on_tile =
        frame.zone.contains(at) || frame.rows.iter().flatten().any(|row| row.contains(at));
    if !on_tile || !app.pet_out() {
        return false;
    }
    open_care(app);
    true
}

/// Open the care box over whatever is up. Only for a pet that is out:
/// every caller has checked `App::pet_out`.
pub(crate) fn open_care(app: &mut App) {
    crate::app::input::open_pet_modal_globally(app);
    app.pet_state.open_care();
}

/// Every key and click while the care box is up.
pub(crate) fn handle_modal_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => close(app),
        ParsedInput::Byte(b'?') | ParsedInput::Char('?') => {
            app.help_modal_state
                .open(crate::app::help_modal::data::HelpTopic::Bonsai);
            app.show_help = true;
        }
        ParsedInput::Byte(b'f' | b'F') | ParsedInput::Char('f' | 'F') => {
            app.pet_state.give();
        }
        // The picks: what comes out of a ready egg, and which of the two
        // drawings a pet with its days grows into. Only one of the two can
        // be waiting, and each asks for nothing when it is not.
        ParsedInput::Byte(b'1') | ParsedInput::Char('1') => {
            app.pet_state.hatch(PetSpecies::Cat);
            app.pet_state.grow(Pick::One);
        }
        ParsedInput::Byte(b'2') | ParsedInput::Char('2') => {
            app.pet_state.hatch(PetSpecies::Dog);
            app.pet_state.grow(Pick::Two);
        }
        ParsedInput::Byte(b'3') | ParsedInput::Char('3') => app.pet_state.hatch(PetSpecies::Bird),
        ParsedInput::Byte(b'b' | b'B') | ParsedInput::Char('b' | 'B') => throw(app, Toy::Ball),
        ParsedInput::Byte(b'm' | b'M') | ParsedInput::Char('m' | 'M') => throw(app, Toy::Mouse),
        ParsedInput::Byte(b't' | b'T') | ParsedInput::Char('t' | 'T') => {
            app.pet_state.teach_next();
        }
        ParsedInput::Byte(b'p' | b'P') | ParsedInput::Char('p' | 'P') => app.pet_state.practise(),
        ParsedInput::Byte(b'r' | b'R') | ParsedInput::Char('r' | 'R') => {
            app.pet_state.perform_next();
        }
        ParsedInput::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down && mouse.button == Some(MouseButton::Left) =>
        {
            // The terminal reports cells from 1; the frame's rects
            // count from 0.
            handle_click(app, mouse.x.saturating_sub(1), mouse.y.saturating_sub(1));
        }
        _ => {}
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    close(app);
}

/// What a `/pet` line asks for. The pet's settings live on the line, not
/// in the care box: the box is for playing with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// `/pet cat`, `/pet dog`, `/pet bird`: what a hatched pet is.
    Species(PetSpecies),
    /// `/pet away`: put it away. It keeps everything and shows nowhere.
    Away,
    /// `/pet out`: bring it back.
    Out,
    /// `/pet reset`: says what a reset takes, and how to confirm it.
    ResetAsk,
    /// `/pet reset yes`: start over as an egg.
    Reset,
}

pub(crate) const ORDER_USAGE: &str =
    "Usage: /pet cat, /pet dog, /pet bird, /pet away, /pet out, or /pet reset";

/// Parse the words after `/pet`. `None` for anything that is not an order.
pub(crate) fn parse_order(words: &str) -> Option<Order> {
    let words = words.to_ascii_lowercase();
    let mut words = words.split_whitespace();
    let order = match (words.next()?, words.next()) {
        ("reset", None) => Order::ResetAsk,
        ("reset", Some("yes")) => Order::Reset,
        ("away", None) => Order::Away,
        ("out", None) => Order::Out,
        (species, None) => Order::Species(PetSpecies::parse(species)?),
        (_, Some(_)) => return None,
    };
    match words.next() {
        Some(_) => None,
        None => Some(order),
    }
}

/// Carry out a `/pet` order. What the row decides (the species, the reset)
/// answers with its own banner on a later tick.
pub(crate) fn handle_order(app: &mut App, order: Order) -> Option<Banner> {
    const AWAY: &str = "Your pet is put away: /pet out brings it back";
    match (order, app.pet_out()) {
        (Order::Out, true) => Some(Banner::info("Your pet is already out")),
        (Order::Out, false) => {
            app.pet_state.set_enabled(true);
            Some(Banner::success("Your pet is back out"))
        }
        (Order::Away, true) => {
            app.pet_state.set_enabled(false);
            app.show_pet_modal = false;
            Some(Banner::success(
                "Put your pet away: it keeps everything, and /pet out brings it back",
            ))
        }
        (Order::Away | Order::Species(_) | Order::ResetAsk | Order::Reset, false) => {
            Some(Banner::info(AWAY))
        }
        (Order::Species(species), true) => match app.pet_state.animal() {
            Some(_) => {
                app.pet_state.change_species(species);
                None
            }
            None => Some(Banner::info(
                "It is still an egg: you pick what hatches when it is ready",
            )),
        },
        (Order::ResetAsk, true) => Some(Banner::info(
            "A reset turns your pet back into an egg: its days, its stage and its tricks go, its name stays. /pet reset yes does it",
        )),
        (Order::Reset, true) => {
            app.pet_state.start_over();
            None
        }
    }
}

fn close(app: &mut App) {
    app.show_pet_modal = false;
}

/// Throw `toy` into the pen from a key: at a column the wall clock picks,
/// so it does not always land in the same place.
fn throw(app: &mut App, toy: Toy) {
    let Some(frame) = app.last_pet_frame.get() else {
        return;
    };
    let width = usize::from(frame.zone.width.max(1));
    let column = (app.pet_state.animation_ticks() * 7) % width;
    app.pet_state.throw(
        toy,
        column,
        usize::from(frame.zone.height.saturating_sub(1)),
    );
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
