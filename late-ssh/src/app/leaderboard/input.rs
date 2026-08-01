use crate::app::state::App;

pub(crate) fn handle_key(app: &mut App, byte: u8) {
    match byte {
        b'j' => app.leaderboard_page.select_next(),
        b'k' => app.leaderboard_page.select_previous(),
        _ => {}
    }
}

/// Arrow keys mirror j/k. Returns whether the key was consumed.
pub(crate) fn handle_arrow(app: &mut App, key: u8) -> bool {
    match key {
        b'B' => {
            app.leaderboard_page.select_next();
            true
        }
        b'A' => {
            app.leaderboard_page.select_previous();
            true
        }
        _ => false,
    }
}
