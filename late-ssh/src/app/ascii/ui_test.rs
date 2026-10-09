use late_core::models::user::{AsciiPiece, Scene, SceneStyle};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

use super::draw_piece;
use crate::app::ascii::{
    alpine_dawn, aurora_fjord, earthrise, misty_forest,
    piece::{Shade, cadence, pixel},
};

fn draw(piece: AsciiPiece, cols: u16, rows: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("test terminal");
    terminal
        .draw(|frame| draw_piece(frame, Rect::new(0, 0, cols, rows), piece, 0))
        .expect("draw");
    terminal.backend().buffer().clone()
}

#[test]
fn a_scene_covers_every_cell_on_its_own_ground_in_either_style() {
    for (scene, [r, g, b]) in [
        (Scene::Earthrise, earthrise::GROUND),
        (Scene::MistyForest, misty_forest::GROUND),
        (Scene::AuroraFjord, aurora_fjord::GROUND),
        (Scene::AlpineDawn, alpine_dawn::GROUND),
    ] {
        let ground = Color::Rgb(r, g, b);
        for style in SceneStyle::ALL {
            let piece = AsciiPiece {
                scene: scene,
                style: style,
            };
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
        AsciiPiece {
            scene: Scene::AuroraFjord,
            style: SceneStyle::Pixels,
        },
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

/// A slow piece earns its place as a screensaver that can default to on by
/// moving a few cells a second: on a full 200x50 terminal, in dots, a
/// frame changes a bounded handful of cells for every second of wall time
/// it stands for, so an away session under it ships about a kilobyte a
/// second rather than a repaint. The budget is the contract; each slow
/// piece's cadence (`piece::cadence`: the forest's `SLOW_RATE` at a frame
/// a second, the Earth's `EARTH_RATE` at a frame every `EARTH_FRAME_MS`)
/// and its own pace within it (`misty_forest::BEAM_PACE`) are tuned to it.
#[test]
fn a_slow_piece_moves_a_few_cells_a_second() {
    const BUDGET_PER_SECOND: usize = 100;
    for scene in [Scene::Earthrise, Scene::MistyForest] {
        let piece = AsciiPiece {
            scene,
            style: SceneStyle::Dots,
        };
        let frame_ms = cadence(piece).frame_ms();
        let draw_at = |clock_ms: u64| {
            let mut terminal = Terminal::new(TestBackend::new(200, 50)).expect("test terminal");
            terminal
                .draw(|frame| draw_piece(frame, Rect::new(0, 0, 200, 50), piece, clock_ms))
                .expect("draw");
            terminal.backend().buffer().clone()
        };
        let mut worst = 0;
        for edge in [0u64, 7, 30, 61, 240, 1200] {
            let before = draw_at(edge * frame_ms);
            let after = draw_at((edge + 1) * frame_ms);
            let changed = before
                .content()
                .iter()
                .zip(after.content().iter())
                .filter(|(a, b)| a != b)
                .count();
            let per_second = changed * 1000 / frame_ms as usize;
            worst = worst.max(per_second);
            assert!(
                per_second <= BUDGET_PER_SECOND,
                "{}: frame {edge} to {}: {changed} cells changed, {per_second} a second, over the {BUDGET_PER_SECOND} budget",
                piece.label(),
                edge + 1
            );
            assert!(changed > 0, "{}: frame {edge}: it froze", piece.label());
        }
        eprintln!("{}: at most {worst} cells change per second", piece.label());
    }
}
