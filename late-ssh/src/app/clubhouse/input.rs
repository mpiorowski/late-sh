//! Clubhouse input: roguelike walking plus the thin bits that make the room
//! social. Plain arrows/hjkl move your avatar; `i` (or Enter in the open)
//! opens the #lounge composer, and what you send floats over your head as a
//! speech bubble; `w` waves and `x` dances for everyone; `t` at the bar
//! pours a `@bartender ` mention into the composer. Enter next to a landmark
//! prop follows its signpost: the arcade cabinet, the heavy door, and the
//! easel jump to their app pages (2/3/4), the poker table opens the Lobby
//! modal and the pool table opens it on a fresh pool challenge, the jukebox
//! opens the Music Booth, and the dog gets petted where everyone can see it.
//! `n` (or Enter at the back door past the counter) steps outside to
//! Nightcap, the small bar out back (`app/clubhouse/nightcap`).
//! Returns `false` for anything it does not own so global keys (numbers,
//! Tab, `q`, `?`, `v` music chords, ...) keep working. Composing and
//! chat-overlay input never reaches this handler: the shared composer and
//! overlay gates in `app/input.rs` intercept first (`screen_composes_chat`).

use crate::app::common::primitives::Screen;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use crate::app::lobby::daily::games::DailyGame;
use crate::app::state::App;

use late_core::models::presence::Emote;

use super::map::Interactive;
use super::state::{BoardStop, TableStop, TourModal, TourMove, TourStart, TourStep};
use crate::app::presence::svc::now_ms;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // A left click on a patron opens their profile, the same view as
    // `/profile <name>`. Other mouse events (scroll) fall through to the
    // global handlers.
    if let ParsedInput::Mouse(mouse) = event {
        return handle_click(app, mouse);
    }

    // The first-visit tour never reaches this handler: while it runs, the
    // forced gate (`handle_tour_gate`, below, which `app/input.rs` asks
    // first) owns all input, including the homecoming Enter.
    if let Some(byte) = event_byte(event) {
        match byte {
            b'i' | b'I' => {
                if let Some(lounge_id) = app.chat.lounge_room_id() {
                    app.chat.start_composing_in_room(lounge_id);
                }
                return true;
            }
            b'w' | b'W' => {
                app.clubhouse.emote(Emote::Wave, now_ms());
                return true;
            }
            b'x' | b'X' => {
                app.clubhouse.emote(Emote::Dance, now_ms());
                return true;
            }
            b's' | b'S' => {
                app.clubhouse.sit(now_ms());
                return true;
            }
            b'n' | b'N' => {
                app.clubhouse.step_to_back_door(now_ms());
                app.nightcap.enter_screen(now_ms());
                app.set_screen(Screen::Nightcap);
                return true;
            }
            b't' | b'T' if app.clubhouse.nearby() == Some(Interactive::Bartender) => {
                if let Some(lounge_id) = app.chat.lounge_room_id() {
                    app.chat.insert_mention_in_room(lounge_id, "bartender");
                }
                return true;
            }
            b'\r' | b'\n' => {
                match app.clubhouse.nearby() {
                    Some(Interactive::Jukebox) => crate::app::audio::booth::input::open(app),
                    Some(Interactive::Bartender) => {
                        if let Some(lounge_id) = app.chat.lounge_room_id() {
                            app.chat.insert_mention_in_room(lounge_id, "bartender");
                        }
                    }
                    Some(Interactive::Dog) => app.clubhouse.pet_dog(now_ms()),
                    Some(Interactive::BackDoor) => {
                        app.nightcap.enter_screen(now_ms());
                        app.set_screen(Screen::Nightcap);
                    }
                    // The landmark props are signposts: Enter walks through.
                    Some(Interactive::Arcade) => app.set_screen(Screen::Arcade),
                    Some(Interactive::Doors) => app.set_screen(Screen::Games),
                    Some(Interactive::Poker) => {
                        crate::app::input::open_daily_modal_globally(app);
                    }
                    // The same Lobby, but a player standing at the pool table
                    // came to play pool: open it on the challenge picker with
                    // eight-ball already under the cursor.
                    Some(Interactive::Pool) => {
                        crate::app::input::open_daily_modal_globally(app);
                        app.daily.begin_challenge_draft_for(DailyGame::EightBall);
                    }
                    Some(Interactive::Easel) => app.set_screen(Screen::Artboard),
                    _ => {
                        if let Some(lounge_id) = app.chat.lounge_room_id() {
                            app.chat.start_composing_in_room(lounge_id);
                        }
                    }
                }
                return true;
            }
            _ => {}
        }
    }

    handle_walk(app, event)
}

/// Arrow keys and lowercase hjkl move the avatar. Consumes the key even when
/// the step is blocked so walking into a wall doesn't trigger global actions.
fn handle_walk(app: &mut App, event: &ParsedInput) -> bool {
    let (dx, dy) = match event {
        ParsedInput::Arrow(b'A') => (0, -1),
        ParsedInput::Arrow(b'B') => (0, 1),
        ParsedInput::Arrow(b'C') => (1, 0),
        ParsedInput::Arrow(b'D') => (-1, 0),
        ParsedInput::Byte(b'k') | ParsedInput::Char('k') => (0, -1),
        ParsedInput::Byte(b'j') | ParsedInput::Char('j') => (0, 1),
        ParsedInput::Byte(b'l') | ParsedInput::Char('l') => (1, 0),
        ParsedInput::Byte(b'h') | ParsedInput::Char('h') => (-1, 0),
        _ => return false,
    };
    // A consumed movement key also cancels a pending `v` music chord, like
    // any locally-handled key would on the chat screens.
    app.music_prefix_armed = false;
    app.clubhouse.walk(dx, dy, now_ms());
    if app.clubhouse.welcome_pour_due() {
        app.show_clubhouse_bartender_welcome();
    }
    true
}

/// A left-button press over a patron's avatar or name label opens their
/// profile modal. Returns `false` for anything else (misses, scroll, drags,
/// button-ups) so the global mouse handlers keep working.
fn handle_click(app: &mut App, mouse: &MouseEvent) -> bool {
    if mouse.kind != MouseEventKind::Down || mouse.button != Some(MouseButton::Left) {
        return false;
    }
    // SGR mouse cells are 1-indexed; the hit layout is in 0-indexed frame
    // cells (same convention as `handle_mouse_click`).
    let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
        return false;
    };
    let Some((user_id, username)) = app.clubhouse.hit_test(x, y) else {
        return false;
    };
    app.open_profile_modal(user_id, username);
    true
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}

/// The forced first-visit tour's gate, which `app/input.rs` asks first
/// (above the reserved chords, below the quit confirm): while a stop is up
/// (`State::tutorial_forced_step`), Enter moves it on and quitting is the
/// way out. At the board stop the board's own keys reach the Arcade;
/// everything else, mouse, chords and pastes included, dies here so no
/// modal, composer, or game can hijack a newcomer mid-route. Returns true
/// when the event was consumed.
pub fn handle_tour_gate(app: &mut App, event: &ParsedInput) -> bool {
    let Some(step) = app.clubhouse.tutorial_forced_step() else {
        return false;
    };
    let byte = match event {
        ParsedInput::Byte(byte) => *byte,
        ParsedInput::Char(ch) if ch.is_ascii() => *ch as u8,
        // The board takes the arrows; nowhere else does.
        ParsedInput::Arrow(key) => {
            match step {
                TourStep::Board => {
                    crate::app::arcade::input::lent_board_arrow(app, *key);
                }
                TourStep::Enter | TourStep::Table | TourStep::Fight => {}
            }
            return true;
        }
        // Mouse, pastes, chords: swallowed while the tour runs.
        _ => return true,
    };
    match (step, byte) {
        // The Artboard stop lands behind its content disclaimer: `s` shows
        // the art for this visit only, Enter skips ahead without it. Neither
        // touches the account tweak; the standalone dialog never runs here.
        (TourStep::Enter, b's' | b'S') if app.artboard_disclaimer_visible() => {
            app.artboard_content_accepted = true;
            app.sync_dartboard_connection();
        }
        (TourStep::Enter, b'\r' | b'\n') => tour_advance(app),
        // The table's one shot has to be played: Enter or Space strikes the
        // break, and only then does Enter move on. A terminal the table does
        // not fit has no shot to see, so Enter walks on from there.
        (TourStep::Table, b'\r' | b'\n' | b' ') => {
            let table = super::ui::table_stop(app.content_area(), app.daily.practice_played());
            match (table, byte) {
                (TableStop::Racked, _) => app.daily.practice_break(),
                (TableStop::TooSmall | TableStop::Played, b' ') => {}
                (TableStop::TooSmall | TableStop::Played, _) => tour_advance(app),
            }
        }
        // The fight is the same: every press is the next blow until it is won.
        (TourStep::Fight, b'\r' | b'\n') if app.clubhouse.tour_fight.won() => tour_advance(app),
        (TourStep::Fight, b'\r' | b'\n' | b' ') => app.clubhouse.tour_fight.strike(),
        // The board has to be solved: until then Enter is one of its keys
        // (it turns the tile), and only a solved board lets it move on. A
        // terminal the board does not fit has nothing to solve, so Enter
        // walks on from there.
        (TourStep::Board, b'\r' | b'\n') => {
            match board_stop_now(app) {
                BoardStop::Open => {
                    crate::app::arcade::input::lent_board_key(app, byte);
                }
                // The skip, confirmed: the board goes unsolved, and that is
                // counted on a first visit the way the stops are.
                BoardStop::SkipAsked => {
                    match app.clubhouse.tour_start() {
                        TourStart::FirstVisit => crate::metrics::record_tour_board_skip(),
                        TourStart::Rerun => {}
                    }
                    tour_advance(app);
                }
                BoardStop::TooSmall | BoardStop::Solved { .. } => tour_advance(app),
            }
        }
        // `s` on the open board asks before it lets the board go.
        (TourStep::Board, b's' | b'S') => match board_stop_now(app) {
            BoardStop::Open => app.clubhouse.board_skip_asked = true,
            BoardStop::SkipAsked | BoardStop::TooSmall | BoardStop::Solved { .. } => {}
        },
        // The way out is always open.
        (TourStep::Enter | TourStep::Table | TourStep::Fight | TourStep::Board, b'q' | b'Q') => {
            crate::app::input::trigger_global_quit(app)
        }
        // While the stop asks, the board does not move.
        (TourStep::Board, _) => match board_stop_now(app) {
            BoardStop::SkipAsked => {}
            BoardStop::Open | BoardStop::TooSmall | BoardStop::Solved { .. } => {
                crate::app::arcade::input::lent_board_key(app, byte);
            }
        },
        (TourStep::Enter | TourStep::Table | TourStep::Fight, _) => {}
    }
    sync_tour_modal(app);
    true
}

/// Where the board stop stands this frame.
fn board_stop_now(app: &App) -> BoardStop {
    super::ui::board_stop(
        app.content_area(),
        app.netwalk_state.is_solved(),
        app.clubhouse.board_skip_asked,
        chrono::Utc::now(),
    )
}

/// `/onboard`: the first-visit tour from the top, for anyone who asks.
/// Through `leave_board` so a board left behind is closed properly.
pub fn start_tour(app: &mut App) {
    crate::app::lobby::daily::board_input::leave_board(app, Screen::Clubhouse);
    app.clubhouse.begin_tutorial(now_ms(), TourStart::Rerun);
}

/// Enter at a tour stop: walk the newcomer to wherever the next one lives.
fn tour_advance(app: &mut App) {
    match app.clubhouse.tutorial_advance() {
        TourMove::Stay => {}
        // Through `leave_board` so a practice table left behind is dropped.
        TourMove::Page(screen) => crate::app::lobby::daily::board_input::leave_board(app, screen),
        // The People page, on its map whatever section a rerun left it on.
        TourMove::Map => {
            crate::app::lobby::daily::board_input::leave_board(app, Screen::Profiles);
            crate::app::directory::input::switch_shelf(
                app,
                crate::app::directory::state::Shelf::Map,
            );
        }
        // `]`: the next section of the People page.
        TourMove::NextSection => {
            let next = app.directory_state.shelf().next();
            crate::app::directory::input::switch_shelf(app, next);
        }
        // A pool table nobody else sees, for one break.
        TourMove::Table => {
            let username = app.username.clone();
            app.daily.open_practice_table(Screen::Arcade, &username);
            app.set_screen(Screen::DailyMatch);
        }
        // Today's board, opened on the Arcade the way its lobby card opens
        // it (the practice table left behind is dropped on the way): a real
        // daily, a real win.
        TourMove::Board => {
            crate::app::lobby::daily::board_input::leave_board(app, Screen::Arcade);
            crate::app::arcade::input::open_lent_board(app);
        }
        // Home with its stock second layout up, the street: a device that
        // deleted it gets the missing-name banner under the stop.
        TourMove::Layouts => {
            crate::app::lobby::daily::board_input::leave_board(app, Screen::Dashboard);
            crate::app::zen::input::run_layout_command(
                app,
                crate::app::zen::presets::LayoutCommand::Load(
                    crate::app::zen::presets::STOCK_STREET_NAME.to_string(),
                ),
            );
        }
        // Home's first layout back before the tavern, so the newcomer's
        // page is as the tour found it.
        TourMove::Homecoming => {
            crate::app::zen::input::load_home_layout(app);
            crate::app::lobby::daily::board_input::leave_board(app, Screen::Clubhouse);
        }
        TourMove::Finished => app.persist_clubhouse_tutorial_done(),
        // The won board is put away; the week's box draws over the lobby.
        TourMove::Week => crate::app::arcade::input::close_lent_board(app),
        // The session lands in the chat, the place the route pitched first
        // and where everyone is.
        TourMove::FinishedAtChat => {
            app.set_screen(Screen::Dashboard);
            app.persist_clubhouse_tutorial_done();
        }
    }
    // The stop just reached, on a first visit's tour only (its welcome box
    // is counted where the tour began, `App::enter_clubhouse`): a rerun is
    // a regular walking the route again, not a newcomer leaving it.
    match app.clubhouse.tour_start() {
        TourStart::FirstVisit => crate::metrics::record_tour_stop(app.clubhouse.tutorial),
        TourStart::Rerun => {}
    }
}

/// Hold open the real modal the current stop pitches, and close it once the
/// tour has moved past.
fn sync_tour_modal(app: &mut App) {
    let modal = app.clubhouse.tour_modal();
    match (
        modal == TourModal::Stations,
        app.stations_modal_state.is_open(),
    ) {
        (true, false) => app.stations_modal_state.open(app.selected_radio_station),
        (false, true) => app.stations_modal_state.close(),
        (true, true) | (false, false) => {}
    }
    app.show_lobby_modal = modal == TourModal::Lobby;
}

/// A lone Esc reaches `app/input.rs::dispatch_escape` without passing the
/// gate; while a stop is up the tour takes it: the one thing it does is
/// take back the board stop's skip question. `true` when it was taken.
pub fn handle_tour_escape(app: &mut App) -> bool {
    if app.clubhouse.tutorial_forced_step().is_none() {
        return false;
    }
    app.clubhouse.board_skip_asked = false;
    true
}
