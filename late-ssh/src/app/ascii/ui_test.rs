use late_core::models::user::AsciiPiece;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

use super::draw_piece;
use crate::app::ascii::{aurora_fjord, donut};

fn draw(piece: AsciiPiece, cols: u16, rows: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("test terminal");
    terminal
        .draw(|frame| draw_piece(frame, Rect::new(0, 0, cols, rows), piece, 0))
        .expect("draw");
    terminal.backend().buffer().clone()
}

fn row_text(buffer: &Buffer, row: u16) -> String {
    (0..buffer.area.width)
        .map(|col| buffer[(col, row)].symbol().to_string())
        .collect()
}

#[test]
fn text_art_sits_centred_in_a_larger_area() {
    let buffer = draw(AsciiPiece::Donut, 60, 30);
    let art = donut::frame(0.0);
    // 60 - 40 leaves 10 columns either side; 30 - 22 leaves 4 rows above.
    for row in 0..art.rows {
        let line: String = (0..art.cols).map(|col| art.at(col, row)).collect();
        let drawn = row_text(&buffer, 4 + row as u16);
        assert_eq!(&drawn[10..50], line, "donut row {row}");
        assert_eq!(drawn[..10].trim(), "");
    }
}

#[test]
fn text_art_larger_than_the_area_keeps_its_middle() {
    let buffer = draw(AsciiPiece::Donut, 20, 10);
    let art = donut::frame(0.0);
    // 40 columns into 20 crops 10 from each side; 22 rows into 10 crops 6
    // from the top.
    for row in 0..10u16 {
        let line: String = (10..30).map(|col| art.at(col, 6 + row as usize)).collect();
        assert_eq!(row_text(&buffer, row), line, "cropped row {row}");
    }
}

#[test]
fn the_aurora_covers_every_cell_on_its_own_ground() {
    let [r, g, b] = aurora_fjord::GROUND;
    let ground = Color::Rgb(r, g, b);
    for (cols, rows) in [(200, 50), (80, 24), (31, 9)] {
        let buffer = draw(AsciiPiece::AuroraFjord, cols, rows);
        let mut lit = 0;
        for row in 0..rows {
            for col in 0..cols {
                let cell = &buffer[(col, row)];
                match cell.symbol() {
                    " " => assert_eq!(cell.bg, ground, "{cols}x{rows} at {col},{row}"),
                    "▀" => lit += 1,
                    other => panic!("{cols}x{rows} at {col},{row} drew {other:?}"),
                }
            }
        }
        assert!(lit > 0, "{cols}x{rows} drew nothing but ground");
    }
}

#[test]
fn the_aurora_at_its_native_aspect_is_its_own_pixels_two_to_a_cell() {
    // 200x50 terminal cells are the scene's 200x100 square cells exactly:
    // the upper scene row is the cell's ink, the lower its background, each
    // the halftone pixel dithered where it sits in the scene.
    let buffer = draw(AsciiPiece::AuroraFjord, 200, 50);
    let scene = aurora_fjord::frame(0.0);
    let rgb = |[r, g, b]: [u8; 3]| Color::Rgb(r, g, b);
    let ground = rgb(aurora_fjord::GROUND);
    for (col, row) in [(10usize, 3usize), (150, 25), (147, 28), (100, 45)] {
        let top = rgb(aurora_fjord::pixel(
            scene.cells[(2 * row) * 200 + col],
            col,
            2 * row,
        ));
        let bottom = rgb(aurora_fjord::pixel(
            scene.cells[(2 * row + 1) * 200 + col],
            col,
            2 * row + 1,
        ));
        let cell = &buffer[(col as u16, row as u16)];
        match top == ground && bottom == ground {
            true => assert_eq!(cell.symbol(), " ", "at {col},{row}"),
            false => {
                assert_eq!(cell.symbol(), "▀", "at {col},{row}");
                assert_eq!(cell.fg, top, "at {col},{row}");
            }
        }
        assert_eq!(cell.bg, bottom, "at {col},{row}");
    }
}

#[test]
fn an_empty_area_draws_nothing() {
    let mut terminal = Terminal::new(TestBackend::new(10, 4)).expect("test terminal");
    terminal
        .draw(|frame| {
            for piece in AsciiPiece::ALL {
                draw_piece(frame, Rect::new(2, 1, 0, 3), piece, 0);
                draw_piece(frame, Rect::new(2, 1, 5, 0), piece, 0);
            }
        })
        .expect("draw");
    assert_eq!(
        *terminal.backend().buffer(),
        Buffer::empty(Rect::new(0, 0, 10, 4))
    );
}
