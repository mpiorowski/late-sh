//! The backtick workspace cycle: Home -> each daily board waiting on your move
//! -> each house table you're seated at -> each Arcade daily puzzle you've
//! started but not finished -> each live door game (a recently-detached
//! Lateania world, the running roguelikes, then the two loaded native
//! remakes) -> each open watch of someone else's game (on the Games hub, or
//! stepped away from; door then handle order) -> back to Home.
//! The one key that spans the Lobby game
//! domains, the Arcade dailies, and the door games: inside a running
//! roguelike the same backtick detaches (the game keeps running) and hops
//! onward; inside an active Lateania world it leaves (autosave) and keeps
//! the door on the cycle for the shared door window
//! (`door::game::IDLE_WINDOW`) so hopping back re-joins the character; inside Dark Room or Green Dragon it hops with the door still
//! loaded, and an idle deadline in `App::tick` ends the visit for a player
//! who never comes back.

use late_core::models::leaderboard::DailyPuzzle;
use uuid::Uuid;

use crate::app::{
    common::primitives::{Banner, Screen},
    door::spectate::state::LiveGameKey,
    lobby::{daily::state::BoardEntry, house::tables::HouseTable},
    state::App,
    workspace::arcade::{active_daily_stop, open_stop, unfinished_daily_stops},
};

/// One stop on the backtick cycle: Home, a daily board where it's
/// your move, a house table where you hold a seat, an Arcade daily puzzle
/// with moves on it that isn't solved yet, or a roguelike door game with a
/// live (running, possibly detached) session. Rooms are gone and real-time
/// Arcade games (Lateris, Snake, Traffic, NES) never participate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameWorkspace {
    /// Home, where the chain starts and comes home to.
    Base,
    DailyBoard(Uuid),
    HouseTable(HouseTable),
    Arcade(DailyPuzzle),
    /// A live door game, identified by its live-game screen. For the
    /// roguelikes (Nethack/Dcss/Brogue) the turn-based child idles on its
    /// host while detached, so hopping in resumes exactly where the player
    /// left off. For Lateania "live" means detached recently: hopping in
    /// re-joins the autosaved character. For Dark Room and Green Dragon it
    /// means the door is still loaded on this session, which it stays until
    /// an explicit leave or the idle deadline.
    Door(Screen),
    /// An open watch of someone else's live game, drawn on the Games hub's
    /// screen. Leaving the hub steps away from it (`App::away_watches`) with
    /// the stream still up, so hopping in puts it back on the hub; reaching
    /// the hub any other way shows the hub's cards.
    Watch(LiveGameKey),
}

/// Where this screen sits on the cycle, `None` for a screen backtick does
/// nothing on. The frame title's hop hint reads it too
/// ([`title_offers_hop`]), so the hint and the key cannot drift.
fn current_stop(app: &App) -> Option<GameWorkspace> {
    let stop = match app.screen {
        Screen::Dashboard => GameWorkspace::Base,
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
            None => return None,
        },
        // A running roguelike reaches here through the detach path in
        // `App::handle_input` (backtick is otherwise forwarded raw to the
        // game); `set_screen` keeps the running state alive on the hop out.
        Screen::Nethack | Screen::Dcss | Screen::Brogue | Screen::Zork => {
            GameWorkspace::Door(app.screen)
        }
        // An active Lateania world reaches here through the detach action in
        // `lateania::screen::handle_active_lateania_key`, which arms the
        // recency window first; the hop-out screen switch tears the session
        // down (autosave + world leave) rather than keeping it running.
        Screen::Lateania => match app.lateania_state.is_some() {
            true => GameWorkspace::Door(Screen::Lateania),
            false => return None,
        },
        // The two native remakes reach here through the backtick arm in their
        // own `screen::handle_key`. Nothing is armed first: their state
        // survives the hop (it keeps ticking off-screen), so being loaded is
        // itself the proof they are live.
        Screen::Darkroom => match app.darkroom_state.is_some() {
            true => GameWorkspace::Door(Screen::Darkroom),
            false => return None,
        },
        Screen::GreenDragon => match app.greendragon_state.is_some() {
            true => GameWorkspace::Door(Screen::GreenDragon),
            false => return None,
        },
        // The realm board reaches here through the backtick arm in
        // `door::realm::input::handle_key`. Like the remakes, nothing is
        // armed first: the board stays open across the hop (`q`/Esc is what
        // closes it), so an open board is itself the proof it is live, and
        // the hub's door list offers it back (`HubGame::Realm`).
        Screen::Realm => match app.realm.board.is_some() {
            true => GameWorkspace::Door(Screen::Realm),
            false => return None,
        },
        // An open watch reaches here through its own key handler
        // (`door::spectate::input`); the hub proper and a preview never do.
        Screen::Games => match watch_key_on_hub(app) {
            Some(key) => GameWorkspace::Watch(key),
            None => return None,
        },
        _ => return None,
    };
    Some(stop)
}

/// Whether the frame title names the hop: this screen answers backtick and
/// says nothing of it yet. A live roguelike or Zork says `` ` step out `` in
/// its own title; their landing pages keep the byte for themselves.
pub(crate) fn title_offers_hop(app: &App) -> bool {
    match app.screen {
        Screen::Nethack | Screen::Dcss | Screen::Brogue | Screen::Zork => false,
        _ => current_stop(app).is_some(),
    }
}

/// Backtick: hop Home -> each match waiting on your move (nearest deadline
/// first) -> each house table you're seated at (roster order) -> each
/// unfinished Arcade daily (lobby order) -> each live door game (hub
/// sidebar order) -> back to Home.
pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    let Some(current) = current_stop(app) else {
        return false;
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
    let watches = watch_keys(app);
    let base = Screen::Dashboard;
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
        &watches,
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
                Screen::Dashboard => {
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
                | Screen::GreenDragon
                | Screen::Zork
                | Screen::Games => {
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
            // A hop keeps the entry of the board it left, like the return
            // screen; a chain that starts anywhere else closes to the Lobby.
            let entry = match (app.screen, app.daily.board.as_ref()) {
                (Screen::DailyMatch, Some(board)) => board.entry,
                _ => BoardEntry::Lobby,
            };
            app.daily.open_board(&item, return_screen, entry);
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
            if screen == Screen::Zork
                && let Some(state) = app.zork_state.as_mut()
            {
                state.resume_live();
            }
            // Lateania has no detached session: the hop-out saved and removed
            // the character, so hopping in re-joins the remembered slot
            // directly, skipping the character-select landing.
            if screen == Screen::Lateania {
                app.enter_lateania();
            }
            true
        }
        // Back onto a watch stepped away from, drawn over the hub.
        GameWorkspace::Watch(key) => {
            app.resume_watch(key);
            app.set_screen(Screen::Games);
            true
        }
    }
}

/// The open watch the Games hub draws, if any. Stepped away from, a watch is
/// still a stop on the cycle (`App::away_watches`), but the hub shows its
/// cards.
fn watch_key_on_hub(app: &App) -> Option<LiveGameKey> {
    app.spectate_state
        .as_ref()
        .filter(|state| state.is_open())
        .map(|state| state.key())
}

/// Every open watch, the hub's and the ones stepped away from, in key order
/// (door, then handle), so the order holds however often each is resumed.
fn watch_keys(app: &App) -> Vec<LiveGameKey> {
    let mut keys: Vec<LiveGameKey> = app
        .away_watches
        .iter()
        .map(|state| state.key())
        .chain(watch_key_on_hub(app))
        .collect();
    keys.sort();
    keys
}

/// The stop after `current` in `[base, boards..., tables..., arcade...,
/// doors..., watches...]`. A current stop missing from the list (the turn just passed,
/// the seat was lost, the puzzle got solved, the dungeon run ended) restarts
/// from the front so the hop chain keeps draining the queue instead of
/// bailing home early.
fn next_workspace(
    my_turn_ids: &[Uuid],
    seated_tables: &[HouseTable],
    arcade_stops: &[DailyPuzzle],
    door_stops: &[Screen],
    watches: &[LiveGameKey],
    current: GameWorkspace,
) -> GameWorkspace {
    let stops: Vec<GameWorkspace> = my_turn_ids
        .iter()
        .copied()
        .map(GameWorkspace::DailyBoard)
        .chain(seated_tables.iter().copied().map(GameWorkspace::HouseTable))
        .chain(arcade_stops.iter().copied().map(GameWorkspace::Arcade))
        .chain(door_stops.iter().copied().map(GameWorkspace::Door))
        .chain(watches.iter().copied().map(GameWorkspace::Watch))
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
