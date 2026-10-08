use late_core::models::user::{AsciiPiece, Scene, SceneStyle, TextPiece};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

use super::draw_piece;
use crate::app::ascii::{
    alpine_dawn, aurora_fjord, donut,
    piece::{Shade, pixel},
};

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
    let buffer = draw(AsciiPiece::Text(TextPiece::Donut), 60, 30);
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
    let buffer = draw(AsciiPiece::Text(TextPiece::Donut), 20, 10);
    let art = donut::frame(0.0);
    // 40 columns into 20 crops 10 from each side; 22 rows into 10 crops 6
    // from the top.
    for row in 0..10u16 {
        let line: String = (10..30).map(|col| art.at(col, 6 + row as usize)).collect();
        assert_eq!(row_text(&buffer, row), line, "cropped row {row}");
    }
}

#[test]
fn a_scene_covers_every_cell_on_its_own_ground_in_either_style() {
    for (scene, [r, g, b]) in [
        (Scene::AuroraFjord, aurora_fjord::GROUND),
        (Scene::AlpineDawn, alpine_dawn::GROUND),
    ] {
        let ground = Color::Rgb(r, g, b);
        for style in SceneStyle::ALL {
            let piece = AsciiPiece::Scene(scene, style);
            for (cols, rows) in [(200, 50), (80, 24), (31, 9)] {
                let buffer = draw(piece, cols, rows);
                let mut lit = 0;
                for row in 0..rows {
                    for col in 0..cols {
                        let cell = &buffer[(col, row)];
                        let at = format!("{piece:?} {cols}x{rows} at {col},{row}");
                        // Dots sit on the ground; a pixel pair carries its
                        // own colours, and only bare ground is a space.
                        match (style, cell.symbol()) {
                            (_, " ") => assert_eq!(cell.bg, ground, "{at}"),
                            (SceneStyle::Pixels, "▀") => lit += 1,
                            (SceneStyle::Dots, glyph) if is_braille(glyph) => {
                                assert_eq!(cell.bg, ground, "{at}");
                                lit += 1;
                            }
                            (_, other) => panic!("{at} drew {other:?}"),
                        }
                    }
                }
                assert!(lit > 0, "{piece:?} {cols}x{rows} drew nothing but ground");
            }
        }
    }
}

fn is_braille(glyph: &str) -> bool {
    let mut chars = glyph.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => ('\u{2800}'..='\u{28ff}').contains(&c),
        _ => false,
    }
}

#[test]
fn a_pixel_is_the_ink_over_the_ground_by_its_brightness() {
    let ground = [10, 20, 30];
    let shade = |level| Shade {
        level,
        rgb: [0.0; 3],
        ink: [1.0, 0.5, 0.0],
    };
    assert_eq!(pixel(shade(0.0), ground), ground);
    assert_eq!(pixel(shade(1.0), ground), [255, 128, 0]);
    assert_eq!(pixel(shade(0.5), ground), [133, 74, 15]);
}

#[test]
fn the_aurora_at_its_native_aspect_is_its_own_pixels_two_to_a_cell() {
    // 200x50 terminal cells are the scene's 200x100 square cells exactly:
    // the upper scene row is the cell's ink, the lower its background.
    let buffer = draw(
        AsciiPiece::Scene(Scene::AuroraFjord, SceneStyle::Pixels),
        200,
        50,
    );
    let scene = aurora_fjord::frame(0.0);
    let rgb = |[r, g, b]: [u8; 3]| Color::Rgb(r, g, b);
    let ground = rgb(aurora_fjord::GROUND);
    for (col, row) in [(10usize, 3usize), (150, 25), (147, 28), (100, 45)] {
        let top = rgb(pixel(scene.cells[(2 * row) * 200 + col], scene.ground));
        let bottom = rgb(pixel(scene.cells[(2 * row + 1) * 200 + col], scene.ground));
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
