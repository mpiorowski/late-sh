use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{clear_canvas_black, clear_letterbox, grid_rect};

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
fn letterbox_resets_the_page_background() {
    // The root paints the page over BG_CANVAS; after the clear, every cell in
    // the door area must be back to the terminal-default canvas.
    let area = Rect::new(0, 0, 6, 3);
    let mut buf = buf_with_bg(area, Color::Rgb(70, 75, 95));
    clear_letterbox(&mut buf, area);
    assert_eq!(buf[(0, 0)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(5, 2)].style().bg, Some(Color::Reset));
    assert_eq!(buf[(3, 1)].symbol(), " ");
}

#[test]
fn grid_centers_inside_a_larger_viewport() {
    let parser = vt100::Parser::new(4, 10, 0);
    let area = Rect::new(2, 1, 20, 10);
    let grid = grid_rect(area, parser.screen());
    assert_eq!(grid, Rect::new(2 + 5, 1 + 3, 10, 4));
}

#[test]
fn grid_pins_top_left_when_viewport_is_smaller() {
    let parser = vt100::Parser::new(4, 10, 0);
    let area = Rect::new(0, 0, 6, 2);
    let grid = grid_rect(area, parser.screen());
    assert_eq!(grid, Rect::new(0, 0, 6, 2));
}
