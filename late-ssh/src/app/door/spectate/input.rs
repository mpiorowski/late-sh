// Watch-view keys, routed here by the Games hub while this session spectates.
// Left/right (or h/l) switch to the previous/next live game; Esc leaves
// (`dispatch_escape`). Everything else returns `false` so the global keys
// (numbers, Tab, `q`, ...) keep working; leaving the hub ends the watch.

use super::state::{SpectateGame, step_target};
use crate::app::common::primitives::Banner;
use crate::app::input::ParsedInput;
use crate::app::state::App;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(state) = app.spectate_state.as_ref() else {
        return false;
    };
    let forward = match event {
        ParsedInput::Arrow(b'C') | ParsedInput::Byte(b'l') | ParsedInput::Char('l') => true,
        ParsedInput::Arrow(b'D') | ParsedInput::Byte(b'h') | ParsedInput::Char('h') => false,
        _ => return false,
    };
    let game = state.game();
    let roster = app.live_games.roster(game);
    if let Some(next) = step_target(&roster, state.playname(), forward) {
        app.start_spectating(game, next.to_string());
    }
    true
}

/// The hub's `s` key: start watching `game`'s longest-running live game.
pub fn watch_first(app: &mut App, game: SpectateGame) {
    let roster = app.live_games.roster(game);
    match roster.first() {
        Some(first) => app.start_spectating(game, first.playname.clone()),
        None => {
            app.banner = Some(Banner::error(&format!(
                "Nobody is playing {} right now.",
                game.label()
            )));
        }
    }
}
