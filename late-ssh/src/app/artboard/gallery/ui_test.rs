use dartboard_core::{Canvas, Pos, RgbColor};

use super::{PaintRun, piece_paint_lines, place_label};

#[test]
fn paint_lines_keep_each_glyphs_colour_and_trim_the_margin() {
    let red = RgbColor::new(255, 0, 0);
    let blue = RgbColor::new(0, 0, 255);
    let mut canvas = Canvas::with_size(8, 3);
    // Row 0: two red, a gap, one blue, then blanks to the edge.
    canvas.set_colored(Pos { x: 0, y: 0 }, '#', red);
    canvas.set_colored(Pos { x: 1, y: 0 }, '#', red);
    canvas.set_colored(Pos { x: 3, y: 0 }, '@', blue);
    // Row 1: an unpainted glyph after a wide one; the continuation cell is
    // not a glyph of its own.
    canvas.set(Pos { x: 0, y: 1 }, '日');
    canvas.set(Pos { x: 2, y: 1 }, 'x');
    // Row 2: empty.

    let run = |text: &str, fg: Option<RgbColor>| PaintRun {
        text: text.to_string(),
        fg,
    };
    assert_eq!(
        piece_paint_lines(&canvas, 8, 3),
        vec![
            vec![run("##", Some(red)), run(" ", None), run("@", Some(blue))],
            vec![run("日x", None)],
            vec![],
        ]
    );
}

#[test]
fn podium_places_read_as_ordinals() {
    assert_eq!(place_label(1), "1st");
    assert_eq!(place_label(2), "2nd");
    assert_eq!(place_label(3), "3rd");
    assert_eq!(place_label(4), "4th");
}
