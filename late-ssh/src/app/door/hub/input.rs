//! Key routing for the Games hub: the rail, the selected card's keys and
//! the one way a card launches. `App` only calls in (`handle_event` from
//! the root dispatcher while the hub is the screen, `open_rc_modal` from
//! the door screens' `c` key).
//!
//! Every launch is counted here: `launch_selection` runs the one
//! availability gate (`unavailable`) and then, before the card's own way
//! in, writes a `door_openings` row through `svc::GamesHubService`. A
//! card whose host is switched off shows its banner and writes nothing.

use late_core::models::door_rc::DoorRcGame;

use crate::app::common::primitives::{Banner, Screen};
use crate::app::input::{MouseButton, MouseEventKind, ParsedInput, app_content_area};
use crate::app::state::App;

/// Games hub keys. Up/down (or j/k, h/l) move the selection along the rail
/// (the grouped cards, then the live games, wrapping); Enter launches the
/// selected card; `d` opens the reset prompt for the saved-character doors.
/// Returns `false` for keys it does not own (digit/Tab nav, `q`, `?`) so
/// they fall through to the global handlers.
pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    use super::state::{HubGame, RailEntry, rail_step};

    let roster = HubGame::roster(app.is_runner());
    let selected = app.games_hub_state.selected_game(roster);

    // An open watch owns the page and every event on it. A preview is the
    // rail sitting on a live row: it gets each event first (Enter opens it),
    // and what it leaves falls to the rail keys below.
    {
        use crate::app::door::spectate::{input as spectate_input, state::WatchMode};
        match spectate_input::mode(app) {
            Some(WatchMode::Open) => return spectate_input::handle_open_event(app, event),
            Some(WatchMode::Preview) if spectate_input::handle_preview_event(app, event) => {
                return true;
            }
            Some(WatchMode::Preview) | None => {}
        }
    }

    // The rc config modal is fully modal while open: `x` clears the stored
    // config, paste replaces it (handle_bracketed_paste), Esc closes it
    // (dispatch_escape), and every other key is swallowed.
    if let Some(game) = app.door_rc_modal {
        return match event {
            ParsedInput::Byte(b'x' | b'X') | ParsedInput::Char('x' | 'X') => {
                // Nothing stored means nothing to clear: skip the DB round
                // trip and don't claim success for a no-op.
                if app.door_rcs.remove(&game).is_some() {
                    app.door_rc_service.clear_task(app.user_id, game);
                    app.banner = Some(Banner::success(&format!(
                        "{} cleared. Defaults are back at your next launch.",
                        game.file_label()
                    )));
                }
                true
            }
            ParsedInput::Byte(_) | ParsedInput::Char(_) | ParsedInput::Arrow(_) => true,
            _ => false,
        };
    }

    // Click on a rail row jumps to that card or live game; the hit test
    // mirrors the hub's own layout against the same content area the
    // renderer gets.
    if let ParsedInput::Mouse(mouse) = event
        && matches!(mouse.kind, MouseEventKind::Down)
        && matches!(mouse.button, Some(MouseButton::Left))
    {
        let body = app_content_area(app);
        let live = app.live_games.live_rows();
        if let Some(entry) = crate::app::door::hub::ui::sidebar_hit_test(
            body,
            roster,
            app.games_hub_state.selected(roster),
            live.len(),
            watched_live_row(app, &live),
            mouse.x.saturating_sub(1),
            mouse.y.saturating_sub(1),
        ) {
            app.door_delete_confirm = false;
            select_rail_entry(app, roster, &live, entry);
            return true;
        }
        return false;
    }

    // While the Lateania reset prompt is up it captures confirm/cancel keys.
    if app.door_delete_confirm {
        return match event {
            ParsedInput::Byte(b'y' | b'Y' | b'\r' | b'\n') | ParsedInput::Char('y' | 'Y') => {
                app.door_delete_confirm = false;
                match selected {
                    HubGame::GreenDragon => {
                        app.leave_greendragon();
                        app.greendragon_service.delete_character(app.user_id);
                        app.banner = Some(Banner::success(
                            "Green Dragon character reset. Enter the village to start over.",
                        ));
                    }
                    HubGame::Darkroom => {
                        app.leave_darkroom();
                        app.darkroom_service.delete_game(app.user_id);
                        app.banner = Some(Banner::success(
                            "A Dark Room save reset. The fire is dead again.",
                        ));
                    }
                    // Only the native saved-character doors offer a reset; the
                    // proxied ones own their own saves upstream. Lateania is
                    // multi-slot now and no longer arms this confirm from the
                    // hub at all (see the guard above) - its own landing is
                    // the only place that can reach this arm, and it never
                    // will, but the match still has to be exhaustive.
                    HubGame::NightCity
                    | HubGame::Lateania
                    | HubGame::Realm
                    | HubGame::Minecraft
                    | HubGame::VintageStory
                    | HubGame::Rebels
                    | HubGame::Nethack
                    | HubGame::Dcss
                    | HubGame::Brogue
                    | HubGame::Usurper
                    | HubGame::Dopewars
                    | HubGame::Bashquest
                    | HubGame::Codekeep
                    | HubGame::Zork => {}
                }
                true
            }
            ParsedInput::Byte(b'n' | b'N' | b'd' | b'D')
            | ParsedInput::Char('n' | 'N' | 'd' | 'D') => {
                app.door_delete_confirm = false;
                true
            }
            // Swallow other keys so the prompt is modal, like the old landing.
            ParsedInput::Byte(_) | ParsedInput::Char(_) | ParsedInput::Arrow(_) => true,
            _ => false,
        };
    }

    // The rail keys: Down/Right or j/l step forward, Up/Left or k/h step
    // back, through the cards and then the live games, wrapping at both
    // ends.
    let step = match event {
        ParsedInput::Byte(b'l' | b'j')
        | ParsedInput::Char('l' | 'j')
        | ParsedInput::Arrow(b'C' | b'B') => Some(true),
        ParsedInput::Byte(b'h' | b'k')
        | ParsedInput::Char('h' | 'k')
        | ParsedInput::Arrow(b'D' | b'A') => Some(false),
        _ => None,
    };
    if let Some(forward) = step {
        let live = app.live_games.live_rows();
        let current = match watched_live_row(app, &live) {
            Some(index) => RailEntry::Live(index),
            None => RailEntry::Card(app.games_hub_state.selected(roster)),
        };
        let next = rail_step(roster.len(), live.len(), current, forward);
        select_rail_entry(app, roster, &live, next);
        return true;
    }

    // Everything below belongs to the selected card, and a watch has none.
    if app.spectate_state.is_some() {
        return false;
    }

    match event {
        ParsedInput::Byte(b'\r') => {
            launch_selection(app, selected);
            true
        }
        // Scroll the selected landing: Ctrl+K / Ctrl+Up up, Ctrl+J / Ctrl+Down
        // down. Ctrl+J is a bare LF, which is why Enter above matches CR only
        // (the same CR/LF split the chat composer relies on).
        ParsedInput::Byte(0x0B) | ParsedInput::CtrlArrow(b'A') => {
            app.games_hub_state.scroll_up();
            true
        }
        ParsedInput::Byte(b'\n') | ParsedInput::CtrlArrow(b'B') => {
            app.games_hub_state.scroll_down();
            true
        }
        // Lateania has multiple character slots now, so its own landing (with
        // a slot cursor to say *which* character) is the only safe place to
        // confirm a delete; this hub shortcut still covers the single-save
        // games.
        ParsedInput::Byte(b'd' | b'D') | ParsedInput::Char('d' | 'D')
            if matches!(selected, HubGame::GreenDragon | HubGame::Darkroom) =>
        {
            app.door_delete_confirm = true;
            true
        }
        ParsedInput::Byte(b'c' | b'C') | ParsedInput::Char('c' | 'C')
            if selected.rc_game().is_some() =>
        {
            app.door_rc_modal = selected.rc_game();
            true
        }
        _ => false,
    }
}

/// The live row this session is watching, as its index among `live`; `None`
/// when it is not watching, or the roster no longer lists the game.
fn watched_live_row(
    app: &App,
    live: &[crate::app::door::spectate::state::LiveRow],
) -> Option<usize> {
    app.spectate_state.as_ref()?.row_in(live)
}

/// Move the hub's rail selection to `entry`: a card ends any watch and
/// becomes the selected card, a live row previews that game.
fn select_rail_entry(
    app: &mut App,
    roster: &[super::state::HubGame],
    live: &[crate::app::door::spectate::state::LiveRow],
    entry: super::state::RailEntry,
) {
    use super::state::RailEntry;
    match entry {
        RailEntry::Card(index) => {
            app.stop_spectating();
            app.games_hub_state.select(roster, index);
        }
        RailEntry::Live(index) => {
            // Re-selecting the row already being watched keeps its stream.
            if watched_live_row(app, live) != Some(index) {
                let row = &live[index];
                app.start_spectating(row.key);
            }
        }
    }
}

/// Jump to the Games hub with `game` selected in the sidebar and its rc config
/// modal open. The modal only lives on the hub; the door screens' `c` key
/// bounces through here so the landing hint works on both surfaces.
pub(crate) fn open_rc_modal(app: &mut App, game: DoorRcGame) {
    use super::state::HubGame;

    let hub_game = match game {
        DoorRcGame::Nethack => HubGame::Nethack,
        DoorRcGame::Dcss => HubGame::Dcss,
    };
    app.games_hub_state
        .select_game(HubGame::roster(app.is_runner()), hub_game);
    app.set_screen(Screen::Games);
    app.door_rc_modal = Some(game);
}

/// Launch the chosen door game from the hub: switch to its screen and start it
/// immediately (no second launcher step). Disabled hosts stay in the hub
/// with a banner instead of bouncing through an empty screen. Every launch
/// that gets past the gate is written down as a door opened
/// (`GamesHubService::record_task`), before the connect: a host that then
/// refuses still had its door opened, a card that only showed the banner
/// did not, and the two server cards launch nothing. The Realm between
/// seasons is the one exception: its "being raised" banner lasts a
/// sweeper tick at most and counts as the door tried.
fn launch_selection(app: &mut App, game: super::state::HubGame) {
    use super::state::HubGame;

    app.door_delete_confirm = false;
    let Some(card) = game.card() else {
        // Played from the game client, not the terminal: the landing says
        // how to connect and there is nothing to launch.
        return;
    };
    if let Some(unavailable) = unavailable(app, game) {
        app.banner = Some(Banner::error(unavailable));
        return;
    }
    app.games_hub_service.record_task(app.user_id, card);
    match game {
        // Only on a runner's roster, so the gate already held when the
        // card was selected; the descent is the one `0` takes.
        HubGame::NightCity => crate::app::deadchannel::city::input::descend(app),
        HubGame::Lateania => {
            // Lands on the character-select landing rather than jumping
            // straight into the world, since which of the account's saved
            // characters to play is no longer a foregone conclusion.
            app.set_screen(Screen::Lateania);
        }
        // The standing world, play or watch alike: the board is the same
        // and joining happens from it (`j`), after seeing the map. Nothing
        // to open between seasons, which lasts a sweeper tick at most.
        HubGame::Realm => crate::app::door::realm::input::open_realm(app),
        HubGame::Minecraft | HubGame::VintageStory => {
            unreachable!("a server card has no door card")
        }
        HubGame::Rebels => {
            app.set_screen(Screen::Rebels);
            if let Some(state) = app.rebels_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Nethack => {
            app.set_screen(Screen::Nethack);
            if let Some(state) = app.nethack_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Dcss => {
            app.set_screen(Screen::Dcss);
            if let Some(state) = app.dcss_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Brogue => {
            app.set_screen(Screen::Brogue);
            if let Some(state) = app.brogue_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Usurper => {
            app.set_screen(Screen::Usurper);
            if let Some(state) = app.usurper_state.as_mut() {
                state.connect();
            }
        }
        HubGame::GreenDragon => {
            app.set_screen(Screen::GreenDragon);
            app.enter_greendragon();
        }
        HubGame::Dopewars => {
            app.set_screen(Screen::Dopewars);
            if let Some(state) = app.dopewars_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Bashquest => {
            app.set_screen(Screen::Bashquest);
            if let Some(state) = app.bashquest_state.as_mut() {
                state.connect();
            }
        }
        HubGame::Darkroom => {
            app.set_screen(Screen::Darkroom);
            app.enter_darkroom();
        }
        HubGame::Zork => crate::app::door::zork::input::launch(app),
        HubGame::Codekeep => {
            app.set_screen(Screen::Codekeep);
            if let Some(state) = app.codekeep_state.as_mut() {
                state.connect();
            }
        }
    }
}

/// The banner a card shows instead of launching while its host is switched
/// off, every switched-off host in one place so the opening is never
/// written for a card that shows one; the native doors and the server
/// cards are never off.
fn unavailable(app: &App, game: super::state::HubGame) -> Option<&'static str> {
    use super::state::HubGame;

    match game {
        HubGame::NightCity
        | HubGame::Lateania
        | HubGame::Realm
        | HubGame::Minecraft
        | HubGame::VintageStory
        | HubGame::GreenDragon
        | HubGame::Darkroom => None,
        HubGame::Zork if !app.zork_enabled => Some("Zork Trilogy is currently unavailable."),
        HubGame::Rebels if !app.rebels_enabled => {
            Some("Rebels in the Sky is currently unavailable.")
        }
        HubGame::Nethack if !app.nethack_enabled => Some("NetHack is currently unavailable."),
        HubGame::Dcss if !app.dcss_enabled => Some("DCSS is currently unavailable."),
        HubGame::Brogue if !app.brogue_enabled => Some("Brogue is currently unavailable."),
        HubGame::Usurper if !app.usurper_enabled => Some("Usurper is currently unavailable."),
        HubGame::Dopewars if !app.dopewars_enabled => Some("dopewars is currently unavailable."),
        HubGame::Bashquest if !app.bashquest_enabled => Some("BashQuest is currently unavailable."),
        HubGame::Codekeep if !app.codekeep_enabled => Some("CodeKeep is currently unavailable."),
        HubGame::Zork
        | HubGame::Rebels
        | HubGame::Nethack
        | HubGame::Dcss
        | HubGame::Brogue
        | HubGame::Usurper
        | HubGame::Dopewars
        | HubGame::Bashquest
        | HubGame::Codekeep => None,
    }
}
