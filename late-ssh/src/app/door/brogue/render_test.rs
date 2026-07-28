use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{clear_canvas_black, grid_rect, reset_door_area};

fn buf_with_bg(area: Rect, bg: Color) -> Buffer {
    let mut buf = Buffer::empty(area);
    buf.set_style(area, Style::default().bg(bg));
    buf
}

#[test]
fn keys_out_truecolor_black_background() {
    let area = Rect::new(0, 0, 4, 2);
    let mut buf = buf_with_bg(area, Color::Rgb(0, 0, 0));
    clear_canvas_black(&mut buf, area);
    assert_eq!(buf[(0, 0)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(3, 1)].style().bg, Some(Color::Reset));
}

#[test]
fn keys_out_color_cube_black_background() {
    let area = Rect::new(0, 0, 2, 1);
    let mut buf = buf_with_bg(area, Color::Indexed(16));
    clear_canvas_black(&mut buf, area);
    assert_eq!(buf[(1, 0)].style().bg, Some(Color::Reset));
}

/// ncurses clears the screen at startup with SGR 40 (ANSI black) active, and
/// vt100 erases with current attributes, so unrepainted cells carry
/// `Indexed(0)`. Terminal themes often tint palette slot 0 blue-gray; keyed
/// out, those cells show the terminal default like the rest of the canvas.
#[test]
fn keys_out_ansi_black_background() {
    let area = Rect::new(0, 0, 2, 1);
    let mut buf = buf_with_bg(area, Color::Indexed(0));
    clear_canvas_black(&mut buf, area);
    assert_eq!(buf[(1, 0)].style().bg, Some(Color::Reset));
}

#[test]
fn keeps_non_black_backgrounds_and_foregrounds() {
    let area = Rect::new(0, 0, 3, 1);
    let mut buf = buf_with_bg(area, Color::Rgb(0, 0, 0));
    buf[(1, 0)].set_style(
        Style::default()
            .fg(Color::Rgb(255, 200, 0))
            .bg(Color::Rgb(120, 20, 20)),
    );
    clear_canvas_black(&mut buf, area);
    assert_eq!(buf[(1, 0)].style().bg, Some(Color::Rgb(120, 20, 20)));
    assert_eq!(buf[(1, 0)].style().fg, Some(Color::Rgb(255, 200, 0)));
    assert_eq!(buf[(0, 0)].style().bg, Some(Color::Reset));
}

#[test]
fn only_touches_cells_inside_the_area() {
    let full = Rect::new(0, 0, 4, 1);
    let mut buf = buf_with_bg(full, Color::Rgb(0, 0, 0));
    clear_canvas_black(&mut buf, Rect::new(0, 0, 2, 1));
    assert_eq!(buf[(1, 0)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(2, 0)].style().bg, Some(Color::Rgb(0, 0, 0)));
}

#[test]
fn door_area_resets_to_the_shared_canvas() {
    // Whatever the frame left in the door area, the clear puts every cell back
    // to `Reset`, which the app's OSC 11 push resolves to the theme background.
    let area = Rect::new(0, 0, 6, 3);
    let mut buf = buf_with_bg(area, Color::Rgb(70, 75, 95));
    reset_door_area(&mut buf, area);
    assert_eq!(buf[(0, 0)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(5, 2)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(3, 1)].symbol(), " ");
}

/// Brogue's grid is a fixed 100x34, so a roomy viewport leaves slack. It goes
/// to the right and bottom edges: the game starts in the viewport's top-left
/// corner like every other door.
#[test]
fn grid_pins_top_left_inside_a_larger_viewport() {
    let parser = vt100::Parser::new(4, 10, 0);
    let area = Rect::new(2, 1, 20, 10);
    let grid = grid_rect(area, parser.screen());
    assert_eq!(grid, Rect::new(2, 1, 10, 4));
}

#[test]
fn grid_clamps_to_a_smaller_viewport() {
    let parser = vt100::Parser::new(4, 10, 0);
    let area = Rect::new(0, 0, 6, 2);
    let grid = grid_rect(area, parser.screen());
    assert_eq!(grid, Rect::new(0, 0, 6, 2));
}
