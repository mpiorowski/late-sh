//! The backtick workspace cycle: Home chat -> each daily board waiting on
//! your move -> each house table you're seated at -> each Arcade daily
//! puzzle you've started but not finished -> back to Home chat. The one key
//! that spans the Lobby game domains and the Arcade dailies.

use uuid::Uuid;

use crate::app::{
    arcade::workspace::{ArcadeStop, active_daily_stop, open_stop, unfinished_daily_stops},
    common::primitives::{Banner, Screen},
    lobby::house::tables::HouseTable,
    state::App,
};

/// One stop on the backtick cycle: Home chat, a daily board where it's your
/// move, a house table where you hold a seat, or an Arcade daily puzzle with
/// moves on it that isn't solved yet. Rooms are gone and real-time Arcade
/// games (Lateris, Snake, Traffic, NES) never participate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameWorkspace {
    Dashboard,
    DailyBoard(Uuid),
    HouseTable(HouseTable),
    Arcade(ArcadeStop),
}

/// Backtick: hop Home chat -> each match waiting on your move (nearest
/// deadline first) -> each house table you're seated at (roster order) ->
/// each unfinished Arcade daily (lobby order) -> back to Home chat.
pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    let current = match app.screen {
        Screen::Dashboard => GameWorkspace::Dashboard,
        Screen::DailyMatch => match app.daily.board.as_ref() {
            Some(board) => GameWorkspace::DailyBoard(board.match_id),
            None => GameWorkspace::Dashboard,
        },
        Screen::HouseTable => match app.house.open {
            Some(table) => GameWorkspace::HouseTable(table),
            None => GameWorkspace::Dashboard,
        },
        Screen::Arcade => match app
            .is_playing_game
            .then(|| active_daily_stop(app))
            .flatten()
        {
            Some(stop) => GameWorkspace::Arcade(stop),
            None => return false,
        },
        _ => return false,
    };
    let my_turn_ids: Vec<Uuid> = app
        .daily
        .my_turn_matches()
        .iter()
        .map(|item| item.id)
        .collect();
    let seated_tables = app.house.my_seated_tables();
    let arcade_stops = unfinished_daily_stops(app);
    // Preserve where the first stop in the hop chain was opened from so
    // `q`/`Esc` still returns there after any number of backtick hops.
    // Arcade stops don't record an origin (Esc there always returns to the
    // Arcade lobby), so a chain passing through one resumes with Arcade as
    // the return screen.
    let return_screen = match app.screen {
        Screen::DailyMatch => app
            .daily
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(Screen::Dashboard),
        Screen::HouseTable => app.house.return_screen,
        Screen::Arcade => Screen::Arcade,
        _ => Screen::Dashboard,
    };
    let next = next_workspace(&my_turn_ids, &seated_tables, &arcade_stops, current);
    // Hopping out of an active Arcade puzzle closes the view (the board
    // itself is already saved move-by-move), mirroring how a kept seat
    // outlives a closed table view.
    if app.screen == Screen::Arcade && !matches!(next, GameWorkspace::Arcade(_)) {
        app.is_playing_game = false;
    }
    match next {
        GameWorkspace::Dashboard => {
            match app.screen {
                Screen::Dashboard => {
                    app.banner = Some(Banner::error("No games waiting on you."));
                }
                // Wrap back to Home chat, no modal: this is the chat half of
                // the toggle, not a lobby visit.
                Screen::HouseTable => {
                    crate::app::lobby::house::input::leave_table(app, Screen::Dashboard);
                }
                Screen::Arcade => {
                    app.set_screen(Screen::Dashboard);
                }
                _ => {
                    crate::app::lobby::daily::board_input::leave_board(app, Screen::Dashboard);
                }
            }
            true
        }
        GameWorkspace::DailyBoard(match_id) => {
            let Some(item) = app
                .daily
                .my_turn_matches()
                .into_iter()
                .find(|item| item.id == match_id)
                .cloned()
            else {
                return true;
            };
            app.daily.open_board(&item, return_screen);
            app.set_screen(Screen::DailyMatch);
            true
        }
        GameWorkspace::HouseTable(table) => {
            if app.house.enter(table, return_screen, app.chip_balance) {
                app.set_screen(Screen::HouseTable);
            }
            true
        }
        GameWorkspace::Arcade(stop) => {
            open_stop(app, stop);
            app.set_screen(Screen::Arcade);
            true
        }
    }
}

/// The stop after `current` in `[Home, boards..., tables..., arcade...]`. A
/// current stop missing from the list (the turn just passed, the seat was
/// lost, the puzzle got solved) restarts from the front so the hop chain
/// keeps draining the queue instead of bailing home early.
fn next_workspace(
    my_turn_ids: &[Uuid],
    seated_tables: &[HouseTable],
    arcade_stops: &[ArcadeStop],
    current: GameWorkspace,
) -> GameWorkspace {
    let stops: Vec<GameWorkspace> = my_turn_ids
        .iter()
        .copied()
        .map(GameWorkspace::DailyBoard)
        .chain(seated_tables.iter().copied().map(GameWorkspace::HouseTable))
        .chain(arcade_stops.iter().copied().map(GameWorkspace::Arcade))
        .collect();
    let next = match current {
        GameWorkspace::Dashboard => stops.first(),
        current => match stops.iter().position(|stop| *stop == current) {
            Some(index) => stops.get(index + 1),
            None => stops.first(),
        },
    };
    next.copied().unwrap_or(GameWorkspace::Dashboard)
}

#[cfg(test)]
#[path = "workspace_test.rs"]
mod workspace_test;
