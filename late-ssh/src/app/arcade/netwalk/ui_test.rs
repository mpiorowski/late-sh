use super::*;
use crate::app::common::theme;

const fn size(width: u16, height: u16) -> TileSize {
    TileSize { width, height }
}

/// A tile draws its connections out to its edges where the size has room,
/// so neighbours join up; the big size marks the source and the network's
/// ends in the centre, and a locked tile draws heavy.
#[test]
fn tiles_draw_their_connections_at_every_size() {
    let tee = Side::East.bit() | Side::South.bit() | Side::West.bit();
    let north_leaf = Side::North.bit();
    let corner = Side::North.bit() | Side::East.bit();
    let straight = Side::North.bit() | Side::South.bit();

    assert_eq!(
        tile_rows(tee, size(5, 3), false, false),
        ["     ", "──┬──", "  │  "]
    );
    assert_eq!(
        tile_rows(north_leaf, size(5, 3), false, false),
        ["  │  ", "  ▣  ", "     "]
    );
    assert_eq!(
        tile_rows(corner, size(5, 3), true, false),
        ["  │  ", "  ■──", "     "]
    );
    assert_eq!(
        tile_rows(tee, size(5, 3), false, true),
        ["     ", "━━┳━━", "  ┃  "]
    );
    assert_eq!(tile_rows(tee, size(3, 1), false, false), ["─┬─"]);
    assert_eq!(tile_rows(north_leaf, size(3, 1), false, false), [" ╵ "]);
    assert_eq!(tile_rows(corner, size(3, 1), true, false), [" └─"]);
    assert_eq!(tile_rows(straight, size(3, 1), false, true), [" ┃ "]);
    assert_eq!(tile_rows(corner, size(1, 1), false, false), ["└"]);
}

/// The cursor paints its tile on the selection fill, and twelve AMOLED
/// palettes reuse an accent as that fill, so a raw token can vanish under
/// the cursor. Every tile colour must clear the legibility floor there on
/// every palette.
#[test]
fn tiles_under_the_cursor_stay_legible_on_every_palette() {
    for option in theme::OPTIONS {
        theme::set_current_by_id(option.id);
        let fill = theme::BG_SELECTION();
        // The terminal palette's fill has no RGB reading.
        if theme::contrast_ratio(fill, fill).is_none() {
            continue;
        }
        for (name, lit, is_source) in [
            ("lit tile", true, false),
            ("dark tile", false, false),
            ("source", true, true),
        ] {
            let style = tile_style(lit, is_source, true);
            assert_eq!(style.bg, Some(fill));
            let Some(ratio) = theme::contrast_ratio(style.fg.expect("a colour"), fill) else {
                continue;
            };
            assert!(
                ratio >= theme::MIN_GLYPH_CONTRAST,
                "the {name} is unreadable on {}: contrast {ratio:.2} against the cursor",
                option.id
            );
        }
    }
}

/// Every board size takes the largest tile that fits, and a click on any
/// cell of a tile lands on that tile.
#[test]
fn a_click_lands_on_the_tile_the_frame_drew() {
    for (area, dimension, expected) in [
        (Rect::new(0, 0, 120, 40), 7, size(5, 3)),
        (Rect::new(0, 0, 80, 24), 9, size(3, 1)),
        (Rect::new(0, 0, 50, 10), 5, size(3, 1)),
    ] {
        let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
        let (grid, tile) = board_layout(board_area, dimension).expect("the board fits");
        assert_eq!(tile, expected, "{area:?} {dimension}");
        for index in 0..dimension * dimension {
            let cell = tile_area(grid, tile, dimension, index);
            for (x, y) in [(cell.x, cell.y), (cell.right() - 1, cell.bottom() - 1)] {
                assert_eq!(hit_test(area, dimension, x, y), Some(index), "{area:?}");
            }
        }
        assert_eq!(
            hit_test(area, dimension, grid.x.saturating_sub(1), grid.y),
            None
        );
    }
}
