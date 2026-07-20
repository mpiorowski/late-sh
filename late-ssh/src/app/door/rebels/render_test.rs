use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use crate::app::door::rebels::render::*;

fn parser(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Parser {
    let mut p = vt100::Parser::new(rows, cols, 0);
    p.process(bytes);
    p
}

#[test]
fn plain_text_lands_in_the_right_cells() {
    let p = parser(2, 5, b"hi");
    let mut buf = Buffer::empty(Rect::new(0, 0, 5, 2));
    blit_screen(&mut buf, Rect::new(0, 0, 5, 2), p.screen());
    assert_eq!(buf[(0, 0)].symbol(), "h");
    assert_eq!(buf[(1, 0)].symbol(), "i");
}

#[test]
fn blit_respects_area_offset() {
    let p = parser(1, 3, b"abc");
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
    let area = Rect::new(2, 1, 3, 1);
    blit_screen(&mut buf, area, p.screen());
    assert_eq!(buf[(2, 1)].symbol(), "a");
    assert_eq!(buf[(4, 1)].symbol(), "c");
    // outside the area is untouched
    assert_eq!(buf[(0, 0)].symbol(), " ");
}

#[test]
fn sgr_red_foreground_maps_through() {
    // ESC[31m sets foreground to indexed red (idx 1).
    let p = parser(1, 1, b"\x1b[31mX");
    let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
    blit_screen(&mut buf, Rect::new(0, 0, 1, 1), p.screen());
    assert_eq!(buf[(0, 0)].fg, Color::Indexed(1));
}

#[test]
fn default_color_maps_to_reset() {
    assert_eq!(to_ratatui_color(vt100::Color::Default), Color::Reset);
}

#[test]
fn visible_cursor_is_drawn_as_a_reversed_block() {
    // Park the cursor at row 0, col 2 (CUP is 1-based) with it shown.
    let p = parser(2, 5, b"\x1b[?25h\x1b[1;3H");
    let mut buf = Buffer::empty(Rect::new(0, 0, 5, 2));
    blit_screen(&mut buf, Rect::new(0, 0, 5, 2), p.screen());
    assert!(buf[(2, 0)].modifier.contains(Modifier::REVERSED));
    // A cell the cursor is not on stays un-reversed.
    assert!(!buf[(0, 0)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn hidden_cursor_draws_no_block() {
    // ESC[?25l hides the cursor; nothing should be reversed.
    let p = parser(2, 5, b"\x1b[?25l\x1b[1;3H");
    let mut buf = Buffer::empty(Rect::new(0, 0, 5, 2));
    blit_screen(&mut buf, Rect::new(0, 0, 5, 2), p.screen());
    assert!(!buf[(2, 0)].modifier.contains(Modifier::REVERSED));
}
