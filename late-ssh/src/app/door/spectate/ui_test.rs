use super::*;

#[test]
fn a_smaller_screen_is_centered() {
    assert_eq!(
        fit_axis(80, 100, 40),
        AxisFit {
            src: 0,
            dst: 10,
            len: 80
        }
    );
}

#[test]
fn a_larger_screen_follows_the_cursor() {
    assert_eq!(
        fit_axis(120, 80, 60),
        AxisFit {
            src: 20,
            dst: 0,
            len: 80
        }
    );
}

#[test]
fn the_crop_window_stops_at_the_screens_edges() {
    assert_eq!(fit_axis(120, 80, 5).src, 0);
    assert_eq!(fit_axis(120, 80, 119).src, 40);
}

#[test]
fn duration_label_reads_minutes_then_hours() {
    assert_eq!(duration_label(0), "0m");
    assert_eq!(duration_label(59), "59m");
    assert_eq!(duration_label(185), "3h 05m");
}

#[test]
fn the_chat_docks_right_when_wide_then_below_when_tall() {
    // Wide enough for crawl's 80 columns, the rule and the chat column.
    assert_eq!(chat_dock(Rect::new(0, 0, 121, 30)), ChatDock::Right);
    // Not wide enough, but 24 rows of game fit above the rule and the strip.
    assert_eq!(chat_dock(Rect::new(0, 0, 120, 34)), ChatDock::Below);
    // Neither: the watched screen keeps the whole view.
    assert_eq!(chat_dock(Rect::new(0, 0, 120, 33)), ChatDock::Hidden);
    assert_eq!(chat_dock(Rect::new(0, 0, 80, 24)), ChatDock::Hidden);
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

/// The player's own game: a pane on the right when wide, one row underneath
/// when only tall, nothing when neither leaves crawl its 80x24.
#[test]
fn a_players_chat_never_costs_crawl_its_minimum_screen() {
    assert_eq!(
        own_game_split(Rect::new(1, 1, 121, 30)),
        (
            Rect::new(1, 1, 80, 30),
            OwnChat::Pane {
                rule: Rect::new(81, 1, 1, 30),
                pane: Rect::new(82, 1, 40, 30),
            }
        )
    );
    assert_eq!(
        own_game_split(Rect::new(1, 1, 100, 25)),
        (
            Rect::new(1, 1, 100, 24),
            OwnChat::Line(Rect::new(1, 25, 100, 1))
        )
    );
    let small = Rect::new(1, 1, 100, 24);
    assert_eq!(own_game_split(small), (small, OwnChat::Hidden));
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
