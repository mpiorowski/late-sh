use super::*;

fn player_text(handle: &str, username: Option<&str>) -> String {
    player_spans(handle, username)
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// The watch header names the person and, after them, the arcade handle the
/// game itself shows (crawl's `Wormsong the Trooper`), so the two tie up.
#[test]
fn the_header_names_the_username_then_the_handle() {
    assert_eq!(player_text("Wormsong", Some("mat")), "mat (Wormsong)");
    assert_eq!(player_text("Mat", Some("mat")), "mat");
    assert_eq!(player_text("Wormsong", None), "Wormsong");
}

#[test]
fn a_smaller_screen_is_drawn_whole() {
    assert_eq!(fit_axis(80, 100, 40), AxisFit { src: 0, len: 80 });
}

#[test]
fn a_larger_screen_follows_the_cursor() {
    assert_eq!(fit_axis(120, 80, 60), AxisFit { src: 20, len: 80 });
}

#[test]
fn the_crop_window_stops_at_the_screens_edges() {
    assert_eq!(fit_axis(120, 80, 5).src, 0);
    assert_eq!(fit_axis(120, 80, 119).src, 40);
}

/// A game that hides its cursor (Brogue) leaves it after the last cell it
/// repainted, so a crop that followed it would jump every frame: the crop
/// pins to the top-left instead. A shown cursor (crawl, NetHack on the `@`)
/// is followed.
#[test]
fn a_hidden_cursor_anchors_the_crop_top_left() {
    let mut parser = vt100::Parser::new(34, 100, 0);
    parser.process(b"\x1b[11;51H@");
    assert_eq!(crop_anchor(parser.screen()), (10, 51));
    parser.process(b"\x1b[?25l");
    assert_eq!(crop_anchor(parser.screen()), (0, 0));
}

#[test]
fn duration_label_reads_minutes_then_hours() {
    assert_eq!(duration_label(0), "0m");
    assert_eq!(duration_label(59), "59m");
    assert_eq!(duration_label(185), "3h 05m");
}

#[test]
fn the_chat_docks_right_when_wide_then_below_when_tall() {
    let dcss = SpectateGame::Dcss;
    // Wide enough for crawl's 80 columns, the rule and the chat column.
    assert_eq!(chat_dock(Rect::new(0, 0, 121, 30), dcss), ChatDock::Right);
    // Not wide enough, but 24 rows of game fit above the rule and the strip.
    assert_eq!(chat_dock(Rect::new(0, 0, 120, 34), dcss), ChatDock::Below);
    // Neither: the watched screen keeps the whole view.
    assert_eq!(chat_dock(Rect::new(0, 0, 120, 33), dcss), ChatDock::Hidden);
    assert_eq!(chat_dock(Rect::new(0, 0, 80, 24), dcss), ChatDock::Hidden);
}

/// Brogue's grid is 100x34, so its chat needs that much more room before it
/// docks; a view that would dock beside crawl leaves Brogue whole.
#[test]
fn a_brogue_watch_docks_only_around_its_whole_grid() {
    let brogue = SpectateGame::Brogue;
    assert_eq!(
        chat_dock(Rect::new(0, 0, 121, 30), brogue),
        ChatDock::Hidden
    );
    assert_eq!(chat_dock(Rect::new(0, 0, 141, 30), brogue), ChatDock::Right);
    assert_eq!(
        chat_dock(Rect::new(0, 0, 140, 43), brogue),
        ChatDock::Hidden
    );
    assert_eq!(chat_dock(Rect::new(0, 0, 140, 44), brogue), ChatDock::Below);
}

#[test]
fn a_docked_chat_leaves_crawl_its_minimum_screen() {
    let (screen, chat) = dock_areas(Rect::new(0, 1, 121, 29), ChatDock::Right);
    assert_eq!(screen, Rect::new(0, 1, 80, 29));
    assert_eq!(
        chat,
        Some((Rect::new(80, 1, 1, 29), Rect::new(81, 1, 40, 29)))
    );

    let (screen, chat) = dock_areas(Rect::new(0, 1, 100, 33), ChatDock::Below);
    assert_eq!(screen, Rect::new(0, 1, 100, 24));
    assert_eq!(
        chat,
        Some((Rect::new(0, 25, 100, 1), Rect::new(0, 26, 100, 8)))
    );

    let body = Rect::new(0, 1, 80, 23);
    assert_eq!(dock_areas(body, ChatDock::Hidden), (body, None));
}

/// A chat docked on the right runs the full height of the view, so its rule
/// meets the frame's top border; the header sits over the screen alone.
#[test]
fn a_right_dock_runs_from_the_top_of_the_view() {
    let layout = watch_layout(Rect::new(19, 1, 130, 40), ChatDock::Right);
    assert_eq!(
        layout,
        WatchLayout {
            header: Rect::new(19, 1, 89, 1),
            screen: Rect::new(19, 2, 89, 39),
            chat: Some((Rect::new(108, 1, 1, 40), Rect::new(109, 1, 40, 40))),
        }
    );
}

#[test]
fn a_dock_below_keeps_the_header_the_full_width() {
    let layout = watch_layout(Rect::new(0, 0, 100, 34), ChatDock::Below);
    assert_eq!(
        layout,
        WatchLayout {
            header: Rect::new(0, 0, 100, 1),
            screen: Rect::new(0, 1, 100, 24),
            chat: Some((Rect::new(0, 25, 100, 1), Rect::new(0, 26, 100, 8))),
        }
    );
}

/// The player's open chat is Realm's column, a third of the screen within
/// 34..=64 columns. Beside the game where crawl keeps its 80 columns, else
/// over the game's right side with the game keeping the whole area.
#[test]
fn a_players_chat_is_beside_the_game_only_where_crawl_keeps_its_screen() {
    assert_eq!(
        own_game_split(Rect::new(1, 1, 150, 30), SpectateGame::Dcss),
        (
            Rect::new(1, 1, 99, 30),
            OwnChat {
                rule: Rect::new(100, 1, 1, 30),
                pane: Rect::new(101, 1, 50, 30),
            }
        )
    );
    assert_eq!(
        own_game_split(Rect::new(1, 1, 240, 30), SpectateGame::Dcss)
            .1
            .pane,
        Rect::new(177, 1, 64, 30),
        "never wider than 64"
    );
    let narrow = Rect::new(1, 1, 100, 24);
    assert_eq!(
        own_game_split(narrow, SpectateGame::Dcss),
        (
            narrow,
            OwnChat {
                rule: Rect::new(66, 1, 1, 24),
                pane: Rect::new(67, 1, 34, 24),
            }
        ),
        "over the game, never narrower than 34"
    );
}

/// A Brogue player keeps the whole 100x34 grid under an open chat: where
/// the column does not fit beside it, it is drawn over the grid.
#[test]
fn a_brogue_players_chat_never_costs_the_grid() {
    let roomy_for_crawl = Rect::new(1, 1, 121, 34);
    assert_eq!(
        own_game_split(roomy_for_crawl, SpectateGame::Brogue).0,
        roomy_for_crawl
    );
    assert_eq!(
        own_game_split(Rect::new(1, 1, 150, 34), SpectateGame::Brogue),
        (
            Rect::new(1, 1, 150, 34),
            OwnChat {
                rule: Rect::new(100, 1, 1, 34),
                pane: Rect::new(101, 1, 50, 34),
            }
        ),
        "99 columns beside it would cut the grid"
    );
    assert_eq!(
        own_game_split(Rect::new(1, 1, 160, 34), SpectateGame::Brogue).0,
        Rect::new(1, 1, 106, 34)
    );
}

#[test]
fn the_rule_tees_into_the_composer_borders() {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(3, 6)).expect("test terminal");
    terminal
        .draw(|frame| {
            let rule = Rect::new(0, 0, 1, 6);
            draw_rule(frame, rule);
            join_rule_to_composer(frame, rule, Rect::new(1, 2, 2, 3));
        })
        .expect("draw");
    let column: String = (0..6)
        .map(|y| terminal.backend().buffer()[(0, y)].symbol().to_string())
        .collect();
    assert_eq!(column, "\u{2502}\u{2502}\u{251c}\u{2502}\u{251c}\u{2502}");
}

/// One row of a 40-column (or wider) buffer, as plain text.
fn drawn_row(width: u16, draw: impl FnOnce(&mut Frame, Rect)) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 1))
        .expect("test terminal");
    terminal
        .draw(|frame| draw(frame, Rect::new(0, 0, width, 1)))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..width)
        .map(|x| buffer[(x, 0)].symbol().to_string())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The watch chat's header: on Zork how a suggested command works from each
/// side, on the player's side how to get back to the game, a watcher's on
/// the other games the count alone.
#[test]
fn the_zork_chat_header_says_how_to_suggest() {
    let header = |watchers: usize, game: SpectateGame, side: ChatSide| {
        drawn_row(DOCK_RIGHT_WIDTH, |frame, area| {
            draw_chat_header(frame, area, watchers, game, side)
        })
    };

    assert_eq!(
        header(2, SpectateGame::Zork, ChatSide::Watcher),
        " 2 watching \u{b7} type >look to suggest"
    );
    assert_eq!(
        header(2, SpectateGame::Nethack, ChatSide::Watcher),
        " 2 watching"
    );
    assert_eq!(
        header(100, SpectateGame::Nethack, ChatSide::Player),
        " 100 watching \u{b7} Esc to the game",
        "whole in the player's narrowest column"
    );
    assert_eq!(
        drawn_row(64, |frame, area| draw_chat_header(
            frame,
            area,
            100,
            SpectateGame::Zork,
            ChatSide::Player
        )),
        " 100 watching \u{b7} F3 takes a >suggestion \u{b7} Esc to the game",
        "whole in the widest column"
    );
}
