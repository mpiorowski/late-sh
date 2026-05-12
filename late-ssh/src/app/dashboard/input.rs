use crate::app::{
    chat::{self, state::RoomSlot},
    common::{
        cli_install,
        primitives::{Banner, Screen},
    },
    dashboard::ui::{DASHBOARD_DAILY_CYCLE_SECONDS, featured_dashboard_room, wire_current_article},
    state::{
        App, DashboardGameToggleTarget, GAME_SELECTION_MINESWEEPER, GAME_SELECTION_NONOGRAMS,
        GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU,
    },
    vote,
};
use late_core::models::leaderboard::DailyGame;

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    chat::input::handle_arrow(app, key)
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.vote_prefix_armed {
        app.vote_prefix_armed = false;
        if vote::input::handle_vote_suffix(app, byte) {
            return true;
        }
    }

    if app.dashboard_box_prefix_armed {
        app.dashboard_box_prefix_armed = false;
        if let Some(slot) = dashboard_box_slot_for_key(byte) {
            if slot == 1 {
                return launch_current_dashboard_daily(app);
            }
            if slot == 2 {
                return open_current_dashboard_wire_article(app);
            }
            if slot == 3 {
                return open_announcements_room(app);
            }
            return enter_dashboard_room_slot(app, slot);
        }
        // Any non-slot key disarms and continues through normal handling so
        // the second keystroke still does what the user typed.
    }

    if byte == b'`' {
        return enter_last_game_room(app);
    }

    if byte == b'b' && home_selected(app) {
        app.dashboard_box_prefix_armed = true;
        return true;
    }

    if byte == b'B' {
        open_cli_install_modal(app);
        return true;
    }
    if byte == b'P' {
        open_browser_pairing_qr(app);
        return true;
    }

    if vote::input::handle_key(app, byte) {
        return true;
    }

    chat::input::handle_byte(app, byte)
}

fn home_selected(app: &App) -> bool {
    let Some(general) = app.chat.general_room_id() else {
        return false;
    };
    app.chat.selected_room_id == Some(general)
        && !app.chat.feeds_selected
        && !app.chat.news_selected
        && !app.chat.notifications_selected
        && !app.chat.discover_selected
        && !app.chat.showcase_selected
        && !app.chat.work_selected
}

pub(crate) fn open_cli_install_modal(app: &mut App) {
    app.pending_clipboard = Some(cli_install::INSTALL_COMMAND.to_string());
    app.show_web_chat_qr = false;
    app.web_chat_qr_url = None;
    app.show_cli_install_modal = true;
}

pub(crate) fn open_browser_pairing_qr(app: &mut App) {
    app.pending_clipboard = Some(app.connect_url.clone());
    app.web_chat_qr_url = Some(app.connect_url.clone());
    app.show_cli_install_modal = false;
    app.show_web_chat_qr = true;
}

fn enter_dashboard_room_slot(app: &mut App, slot: usize) -> bool {
    if slot != 0 {
        return false;
    }
    let Some(room) =
        featured_dashboard_room(&app.rooms_snapshot, &app.room_game_registry).map(|card| card.room)
    else {
        return false;
    };

    if crate::app::rooms::input::enter_room(app, room) {
        app.set_screen(Screen::Rooms);
        true
    } else {
        false
    }
}

fn enter_last_game_room(app: &mut App) -> bool {
    if app.dashboard_game_toggle_target == Some(DashboardGameToggleTarget::Arcade)
        && app.is_playing_game
    {
        app.set_screen(Screen::Arcade);
        return true;
    }

    let room = app.rooms_active_room.clone().or_else(|| {
        let room_id = app.rooms_last_active_room_id?;
        app.rooms_snapshot
            .rooms
            .iter()
            .find(|room| room.id == room_id)
            .cloned()
    });
    let Some(room) = room else {
        if app.is_playing_game {
            app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
            app.set_screen(Screen::Arcade);
        } else {
            app.banner = Some(Banner::error("No game to return to."));
        }
        return true;
    };

    if crate::app::rooms::input::enter_room(app, room) {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Room);
        app.set_screen(Screen::Rooms);
    }
    true
}

pub(crate) fn dashboard_box_slot_for_key(byte: u8) -> Option<usize> {
    match byte {
        b'1'..=b'4' => Some((byte - b'1') as usize),
        _ => None,
    }
}

fn launch_current_dashboard_daily(app: &mut App) -> bool {
    let Some(game) = current_dashboard_daily_game(app) else {
        app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
        app.is_playing_game = false;
        app.set_screen(Screen::Arcade);
        app.banner = Some(Banner::success("All dailies complete."));
        return true;
    };

    match game {
        DailyGame::Sudoku => {
            app.sudoku_state.show_daily();
            app.game_selection = GAME_SELECTION_SUDOKU;
        }
        DailyGame::Nonogram => {
            if !app.nonogram_state.has_puzzles() {
                app.banner = Some(Banner::error("No nonogram packs loaded."));
                return true;
            }
            app.nonogram_state.show_daily();
            app.game_selection = GAME_SELECTION_NONOGRAMS;
        }
        DailyGame::Solitaire => {
            app.solitaire_state.show_daily();
            app.game_selection = GAME_SELECTION_SOLITAIRE;
        }
        DailyGame::Minesweeper => {
            app.minesweeper_state.show_daily();
            app.game_selection = GAME_SELECTION_MINESWEEPER;
        }
    }

    app.dashboard_game_toggle_target = Some(DashboardGameToggleTarget::Arcade);
    app.is_playing_game = true;
    app.set_screen(Screen::Arcade);
    true
}

fn current_dashboard_daily_game(app: &App) -> Option<DailyGame> {
    let completion = app.leaderboard.user_daily_statuses.get(&app.user_id);
    let unfinished: Vec<DailyGame> = [
        DailyGame::Sudoku,
        DailyGame::Nonogram,
        DailyGame::Solitaire,
        DailyGame::Minesweeper,
    ]
    .into_iter()
    .filter(|game| !completion.is_some_and(|status| status.completed(*game)))
    .collect();

    if unfinished.is_empty() {
        return None;
    }

    let idx = (dashboard_cycle_secs() / DASHBOARD_DAILY_CYCLE_SECONDS) as usize % unfinished.len();
    unfinished.get(idx).copied()
}

fn open_current_dashboard_wire_article(app: &mut App) -> bool {
    let articles = app.chat.news.all_articles();
    let Some(item) = wire_current_article(articles, dashboard_cycle_secs()) else {
        app.banner = Some(Banner::error("no headline to open"));
        return true;
    };
    let article_id = item.article.id;

    app.chat.close_overlay();
    app.set_screen(Screen::Chat);
    app.chat.select_news();
    app.chat.news.select_article_by_id(article_id);
    true
}

fn open_announcements_room(app: &mut App) -> bool {
    let Some(room_id) = app
        .chat
        .rooms
        .iter()
        .find(|(room, _)| room.slug.as_deref() == Some("announcements"))
        .map(|(room, _)| room.id)
    else {
        app.chat.request_list();
        app.banner = Some(Banner::error("#announcements not loaded yet."));
        return true;
    };

    app.chat.close_overlay();
    app.chat.reset_composer();
    app.chat.select_room_slot(RoomSlot::Room(room_id));
    app.set_screen(Screen::Chat);
    app.sync_visible_chat_room();
    app.chat.request_list();
    true
}

fn dashboard_cycle_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::dashboard_box_slot_for_key;

    #[test]
    fn dashboard_box_slot_accepts_announcements_chord() {
        assert_eq!(dashboard_box_slot_for_key(b'1'), Some(0));
        assert_eq!(dashboard_box_slot_for_key(b'2'), Some(1));
        assert_eq!(dashboard_box_slot_for_key(b'3'), Some(2));
        assert_eq!(dashboard_box_slot_for_key(b'4'), Some(3));
        assert_eq!(dashboard_box_slot_for_key(b'5'), None);
    }
}
