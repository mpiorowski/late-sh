use super::{chat_tile_title, draw_board_day, draw_tile_chrome, hint_line_fitting, hints_shown};
use crate::app::common::primitives::hint_line;
use crate::app::zen::layout::Chrome;
use crate::app::zen::state::{TileKind, TileTitle};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use unicode_width::UnicodeWidthStr;

#[test]
fn no_title_names_a_key_outside_arrange_mode() {
    for kind in TileKind::ALL {
        let keys = kind.hints();
        assert_eq!(hints_shown(keys, false, false), vec![], "{kind:?}");
        assert_eq!(hints_shown(keys, false, true), vec![], "{kind:?}");
    }
}

#[test]
fn a_key_that_works_only_on_the_focused_tile_shows_only_there() {
    let ascii = TileKind::Ascii.hints();
    assert_eq!(hints_shown(ascii, false, false), vec![]);
    assert_eq!(hints_shown(ascii, false, true), vec![]);
    assert_eq!(hints_shown(ascii, true, false), vec![]);
    assert_eq!(hints_shown(ascii, true, true), vec![("[ ] Enter", "pick")]);

    // Arranging, every tile names its house key, focused or not; a chat
    // tile adds its room cycling ahead of it only while focused.
    assert_eq!(
        hints_shown(TileKind::Bonsai.hints(), true, false),
        vec![("w", "tend")]
    );
    let chat = TileKind::Chat.hints();
    assert_eq!(hints_shown(chat, true, false), vec![("ctrl+/", "room")]);
    assert_eq!(
        hints_shown(chat, true, true),
        vec![("[ ]", "cycle"), ("ctrl+/", "room")]
    );
    // The tank names its click on every tile and takes `a` only focused.
    let tank = TileKind::Tank.hints();
    assert_eq!(hints_shown(tank, true, false), vec![("click", "feed")]);
    assert_eq!(
        hints_shown(tank, true, true),
        vec![("a", "feed"), ("click", "feed")]
    );
    // The rail takes clicks on its rows and no key, so it names none.
    assert_eq!(hints_shown(TileKind::Rooms.hints(), true, true), vec![]);
}

#[test]
fn the_footer_drops_whole_hints_instead_of_cutting_one_in_half() {
    let hints = [("←→", "focus"), ("space", "kind"), ("S", "split")];
    let full = hint_line_fitting(&hints, 200);
    assert_eq!(full.width(), hint_line(&hints).width());
    // Room for the first two hints and part of the third: the third goes.
    let two = hint_line(&hints[..2]).width();
    let narrow = hint_line_fitting(&hints, two + 4);
    assert_eq!(narrow.width(), two);
    assert_eq!(hint_line_fitting(&hints, 3).width(), hint_line(&[]).width());
}

/// A stream room's tile carries the rail's watcher count after the room,
/// and on a narrow tile the room label shortens so the count is never the
/// part the border clips.
#[test]
fn a_stream_chat_tile_title_keeps_the_watcher_count_on_a_narrow_tile() {
    assert_eq!(chat_tile_title("#lounge", None, 40), "chat · #lounge");
    assert_eq!(
        chat_tile_title("#mat-live", Some("[3]"), 40),
        "chat · #mat-live [3]"
    );
    assert_eq!(
        chat_tile_title("#mat-live", Some("[…]"), 40),
        "chat · #mat-live […]"
    );

    // Four cells go to the corners and the title's padding spaces.
    let title = chat_tile_title("#mat-live", Some("[12]"), 20);
    assert_eq!(title, "chat · #ma… [12]");
    assert_eq!(UnicodeWidthStr::width(title.as_str()), 16);
}

/// The board's day reads in amber at the left of its row with the board's
/// key flush right, one column in from each edge, the text giving way to
/// the key when the row is short.
#[test]
fn the_boards_day_banner_is_the_text_left_and_the_key_right() {
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("terminal");
    terminal
        .draw(|frame| draw_board_day(frame, Rect::new(0, 0, 40, 1), "Hackathon"))
        .expect("draw");
    let row: String = (0..40)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect();
    assert_eq!(row, format!(" Hackathon{}7 for the board ", " ".repeat(14)));

    let mut terminal = Terminal::new(TestBackend::new(24, 1)).expect("terminal");
    terminal
        .draw(|frame| draw_board_day(frame, Rect::new(0, 0, 24, 1), "Hackathon and more"))
        .expect("draw");
    let row: String = (0..24)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect();
    assert!(row.ends_with("7 for the board "), "{row:?}");
    assert!(row.starts_with(" Hack"), "{row:?}");
}

/// The top row of a narrow lobby tile wearing `title`, as text.
fn lobby_top_row(title: TileTitle) -> String {
    let mut terminal = Terminal::new(TestBackend::new(23, 4)).expect("terminal");
    terminal
        .draw(|frame| {
            draw_tile_chrome(
                frame,
                Rect::new(0, 0, 23, 4),
                TileKind::Lobby,
                false,
                title,
                "lobby",
                &hints_shown(TileKind::Lobby.hints(), true, false),
                false,
                false,
                &Chrome::Rules { arrange: false },
            );
        })
        .expect("draw");
    (0..23)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect()
}

/// In the right column's width the lobby's name and its key both fit, the rule
/// between them; the rule alone drops every word; hidden draws nothing on
/// the row.
#[test]
fn the_three_title_modes_on_a_narrow_lobby() {
    assert_eq!(lobby_top_row(TileTitle::Named), " ── lobby ─── ^G open  ");
    assert_eq!(
        lobby_top_row(TileTitle::Rule),
        format!(" {} ", "─".repeat(21))
    );
    assert_eq!(lobby_top_row(TileTitle::Hidden), " ".repeat(23));
}
