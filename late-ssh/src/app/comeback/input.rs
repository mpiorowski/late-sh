//! The box takes two keys: Enter does the moment's own thing, Esc keeps it
//! for later. Everything else holds, like the tour's welcome box, so a
//! stray key never takes the pitch off the screen.

use crate::app::common::primitives::Screen;
use crate::app::help_modal::data::HelpTopic;
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::state::{Beat, Moment, Start};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answer {
    Take,
    Later,
    Hold,
}

pub(super) fn answer_for(event: &ParsedInput) -> Answer {
    match event {
        ParsedInput::Byte(b'\r' | b'\n') => Answer::Take,
        ParsedInput::Byte(0x1B) => Answer::Later,
        _ => Answer::Hold,
    }
}

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match answer_for(&event) {
        Answer::Take => take(app),
        Answer::Later => later(app),
        Answer::Hold => {}
    }
}

/// A lone Esc, which the root flushes on its own path: later. Returns
/// whether a box was up to take it.
pub(crate) fn handle_escape(app: &mut App) -> bool {
    match app.comeback.is_open() {
        true => {
            later(app);
            true
        }
        false => false,
    }
}

fn take(app: &mut App) {
    let Some((moment, start)) = app.comeback.close() else {
        return;
    };
    count(moment, start, Beat::Taken);
    match moment {
        Moment::HearIt => match app.paired_client_state().is_some() {
            // What `v` then `r` does: the Stations modal on the current
            // station.
            true => {
                crate::app::audio::input::handle_music_suffix(app, b'r', false);
            }
            false => crate::app::input::open_guide_on_globally(app, HelpTopic::Pair),
        },
        Moment::BigGames => crate::app::input::switch_page_globally(app, Screen::Games),
        // What `/map` does: the People page on its Map shelf.
        Moment::People => crate::app::input::open_usermap_globally(app),
    }
}

fn later(app: &mut App) {
    let Some((moment, start)) = app.comeback.close() else {
        return;
    };
    count(moment, start, Beat::Skipped);
}

/// A login's moment is counted; a `/comeback` rerun is not.
fn count(moment: Moment, start: Start, beat: Beat) {
    match start {
        Start::Login => crate::metrics::record_comeback(moment, beat),
        Start::Command => {}
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
