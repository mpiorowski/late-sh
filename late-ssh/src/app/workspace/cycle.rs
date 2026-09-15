//! The backtick workspace cycle: the base page (Home chat, or Zen when you
//! went into the games from Zen) -> each daily board waiting on your move
//! -> each house table you're seated at -> each Arcade daily puzzle you've
//! started but not finished -> each live door game (a recently-detached
//! Lateania world, the running roguelikes, then the two loaded native
//! remakes) -> back to the base. The one key that spans the Lobby game
//! domains, the Arcade dailies, and the door games: inside a running
//! roguelike the same backtick detaches (the game keeps running) and hops
//! onward; inside an active Lateania world it leaves (autosave) and keeps
//! the door on the cycle for a few minutes so hopping back re-joins the
//! character; inside Dark Room or Green Dragon it hops with the door still
//! loaded, and an idle deadline in `App::tick` ends the visit for a player
//! who never comes back.

use uuid::Uuid;

use crate::app::{
    common::primitives::{Banner, Screen},
    lobby::house::tables::HouseTable,
    state::App,
    workspace::arcade::{ArcadeStop, active_daily_stop, open_stop, unfinished_daily_stops},
};

/// One stop on the backtick cycle: the base page, a daily board where it's
/// your move, a house table where you hold a seat, an Arcade daily puzzle
/// with moves on it that isn't solved yet, or a roguelike door game with a
/// live (running, possibly detached) session. Rooms are gone and real-time
/// Arcade games (Lateris, Snake, Traffic, NES) never participate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameWorkspace {
    /// The page the chain comes home to, `App::workspace_base`.
    Base,
    DailyBoard(Uuid),
    HouseTable(HouseTable),
    Arcade(ArcadeStop),
    /// A live door game, identified by its live-game screen. For the
    /// roguelikes (Nethack/Dcss/Brogue) the turn-based child idles on its
    /// host while detached, so hopping in resumes exactly where the player
    /// left off. For Lateania "live" means detached recently: hopping in
    /// re-joins the autosaved character. For Dark Room and Green Dragon it
    /// means the door is still loaded on this session, which it stays until
    /// an explicit leave or the idle deadline.
    Door(Screen),
}

/// Where the hop chain comes home to: the page you went into the games
/// from. Only the two pages that answer backtick can be a base, so the loop
/// always closes; going in from any other page comes home to Home chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceBase {
    Home,
    /// Zen, with the page its `Ctrl+F` hands back. `set_screen` forgets
    /// that page on the way out of Zen, so the base carries it and the
    /// return restores it.
    Zen { back: Option<Screen> },
}

impl WorkspaceBase {
    /// The base for leaving the current screen into the games.
    fn leaving(app: &App) -> Self {
        match app.screen {
            Screen::Zen => WorkspaceBase::Zen {
                back: app.zen_return_screen,
            },
            _ => WorkspaceBase::Home,
        }
    }

    pub(crate) fn screen(self) -> Screen {
        match self {
            WorkspaceBase::Home => Screen::Dashboard,
            WorkspaceBase::Zen { .. } => Screen::Zen,
        }
    }
}

/// The game side of the chain: every screen a stop can be on, the Arcade
/// lobby included (a daily is started there without a screen change).
/// Crossing from a page onto this side is going into the games.
fn is_game_side(screen: Screen) -> bool {
    match screen {
        Screen::DailyMatch
        | Screen::HouseTable
        | Screen::Arcade
        | Screen::Nethack
        | Screen::Dcss
        | Screen::Brogue
        | Screen::Lateania
        | Screen::Darkroom
        | Screen::GreenDragon => true,
        Screen::Dashboard
        | Screen::Games
        | Screen::Rebels
        | Screen::Dopewars
        | Screen::Bashquest
        | Screen::Codekeep
        | Screen::Usurper
        | Screen::Artboard
        | Screen::Profiles
        | Screen::Leaderboard
        | Screen::Clubhouse
        | Screen::Zen
        | Screen::Scratchpad => false,
    }
}

/// `set_screen`'s hook, run before a real screen change while `app.screen`
/// is still the page being left. Going from a page into the games (a hop,
/// a Lobby jump, a hub launch) records the base, except `Ctrl+F` handing
/// Zen back to a game screen it was opened over: that is closing Zen, not
/// going in from it, so the base stays whatever it was. Coming back to Zen
/// from the games restores the page its `Ctrl+F` hands back; a fresh chord
/// has already stamped its own, so the restore only fills an empty one.
pub(crate) fn note_screen_change(app: &mut App, next: Screen) {
    let closing_zen = app.screen == Screen::Zen && app.zen_return_screen == Some(next);
    if is_game_side(next) && !is_game_side(app.screen) && !closing_zen {
        app.workspace_base = WorkspaceBase::leaving(app);
    }
    if next == Screen::Zen
        && is_game_side(app.screen)
        && app.zen_return_screen.is_none()
        && let WorkspaceBase::Zen { back } = app.workspace_base
    {
        app.zen_return_screen = back;
    }
}

/// Backtick: hop the base page -> each match waiting on your move (nearest
/// deadline first) -> each house table you're seated at (roster order) ->
/// each unfinished Arcade daily (lobby order) -> each live door game (hub
/// sidebar order) -> back to the base.
pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    let current = match app.screen {
        // Pressed on a base page: that page is the base for this chain.
        Screen::Dashboard | Screen::Zen => {
            app.workspace_base = WorkspaceBase::leaving(app);
            GameWorkspace::Base
        }
        Screen::DailyMatch => match app.daily.board.as_ref() {
            Some(board) => GameWorkspace::DailyBoard(board.match_id),
            None => GameWorkspace::Base,
        },
        Screen::HouseTable => match app.house.open {
            Some(table) => GameWorkspace::HouseTable(table),
            None => GameWorkspace::Base,
        },
        Screen::Arcade => match app
            .is_playing_game
            .then(|| active_daily_stop(app))
            .flatten()
        {
            Some(stop) => GameWorkspace::Arcade(stop),
            None => return false,
        },
        // A running roguelike reaches here through the detach path in
        // `App::handle_input` (backtick is otherwise forwarded raw to the
        // game); `set_screen` keeps the running state alive on the hop out.
        Screen::Nethack | Screen::Dcss | Screen::Brogue => GameWorkspace::Door(app.screen),
        // An active Lateania world reaches here through the detach action in
        // `lateania::screen::handle_active_lateania_key`, which arms the
        // recency window first; the hop-out screen switch tears the session
        // down (autosave + world leave) rather than keeping it running.
        Screen::Lateania => match app.lateania_state.is_some() {
            true => GameWorkspace::Door(Screen::Lateania),
            false => return false,
        },
        // The two native remakes reach here through the backtick arm in their
        // own `screen::handle_key`. Nothing is armed first: their state
        // survives the hop (it keeps ticking off-screen), so being loaded is
        // itself the proof they are live.
        Screen::Darkroom => match app.darkroom_state.is_some() {
            true => GameWorkspace::Door(Screen::Darkroom),
            false => return false,
        },
        Screen::GreenDragon => match app.greendragon_state.is_some() {
            true => GameWorkspace::Door(Screen::GreenDragon),
            false => return false,
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
    let door_stops = crate::app::door::hub::state::live_doors(app);
    let base = app.workspace_base.screen();
    // Preserve where the first stop in the hop chain was opened from so
    // `q`/`Esc` still returns there after any number of backtick hops. Arcade
    // and door stops don't record an origin, so a chain passing through one
    // returns to the base.
    let return_screen = match app.screen {
        Screen::DailyMatch => app
            .daily
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(base),
        Screen::HouseTable => app.house.return_screen,
        _ => base,
    };
    let next = next_workspace(
        &my_turn_ids,
        &seated_tables,
        &arcade_stops,
        &door_stops,
        current,
    );
    // Hopping out of an active Arcade puzzle closes the view (the board
    // itself is already saved move-by-move), mirroring how a kept seat
    // outlives a closed table view.
    if app.screen == Screen::Arcade && !matches!(next, GameWorkspace::Arcade(_)) {
        app.is_playing_game = false;
    }
    match next {
        GameWorkspace::Base => {
            match app.screen {
                Screen::Dashboard | Screen::Zen => {
                    app.banner = Some(Banner::error("No games waiting on you."));
                }
                // Wrap back to the base, no modal: this is the other half of
                // the toggle, not a lobby visit.
                Screen::HouseTable => {
                    crate::app::lobby::house::input::leave_table(app, base);
                }
                // A roguelike door detaches on a plain screen switch; nothing
                // to close. Lateania's screen switch runs its own teardown
                // (autosave + world leave) inside `set_screen`.
                Screen::Arcade
                | Screen::Nethack
                | Screen::Dcss
                | Screen::Brogue
                | Screen::Lateania
                | Screen::Darkroom
                | Screen::GreenDragon => {
                    app.set_screen(base);
                }
                _ => {
                    crate::app::lobby::daily::board_input::leave_board(app, base);
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
        GameWorkspace::Door(screen) => {
            // For the roguelikes the running door state is still on the App
            // (kept by `set_screen`'s detach rule), so switching screens is
            // the whole resume: the vt100 parser holds the live frame and the
            // next draw re-sizes the remote PTY if the viewport changed.
            app.set_screen(screen);
            // Lateania has no detached session: the hop-out saved and removed
            // the character, so hopping in re-joins the remembered slot
            // directly, skipping the character-select landing.
            if screen == Screen::Lateania {
                app.enter_lateania();
            }
            true
        }
    }
}

/// The stop after `current` in `[base, boards..., tables..., arcade...,
/// doors...]`. A current stop missing from the list (the turn just passed,
/// the seat was lost, the puzzle got solved, the dungeon run ended) restarts
/// from the front so the hop chain keeps draining the queue instead of
/// bailing home early.
fn next_workspace(
    my_turn_ids: &[Uuid],
    seated_tables: &[HouseTable],
    arcade_stops: &[ArcadeStop],
    door_stops: &[Screen],
    current: GameWorkspace,
) -> GameWorkspace {
    let stops: Vec<GameWorkspace> = my_turn_ids
        .iter()
        .copied()
        .map(GameWorkspace::DailyBoard)
        .chain(seated_tables.iter().copied().map(GameWorkspace::HouseTable))
        .chain(arcade_stops.iter().copied().map(GameWorkspace::Arcade))
        .chain(door_stops.iter().copied().map(GameWorkspace::Door))
        .collect();
    let next = match current {
        GameWorkspace::Base => stops.first(),
        current => match stops.iter().position(|stop| *stop == current) {
            Some(index) => stops.get(index + 1),
            None => stops.first(),
        },
    };
    next.copied().unwrap_or(GameWorkspace::Base)
}

#[cfg(test)]
#[path = "cycle_test.rs"]
mod cycle_test;
