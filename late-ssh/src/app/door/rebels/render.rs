use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::common::theme;

use super::state::{Mode, State};

/// Draw the rebels page below the top bar: the Launcher when idle, the live
/// embedded vt100 widget once connected.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_launcher(frame, area, state),
        Mode::Running => draw_running(frame, area, state),
    }
}

fn draw_launcher(frame: &mut Frame, area: Rect, state: &State) {
    // Frameless, themed splash in the late.sh house style (cf. Lateania):
    // a single AMBER_GLOW bold header line, matching Lateania's header.
    let header = Line::from(Span::styled(
        "REBELS IN THE SKY  |  pirate basketball across the galaxy",
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    ));

    let action_line = if state.is_enabled() {
        Line::from(Span::styled(
            "Press Enter to launch",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };

    let lines = vec![
        header,
        Line::from(""),
        Line::from(Span::styled(
            "The year is 2101 and corporations rule the world. Join a pirate crew,",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(Span::styled(
            "plunder the galaxy, and survive the only way left: by playing basketball.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(Span::styled(
            "Build your crew, wander the stars, and challenge any team you can find.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(""),
        action_line,
        Line::from(""),
        Line::from(vec![
            Span::styled("Exit the game with ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled(" (then confirm) or ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Ctrl-C", Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled(" to come back here.", Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "github.com/ricott1/rebels-in-the-sky",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Connecting to rebels..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}

/// Map a vt100 color to a ratatui color. Default -> Reset so the host theme
/// shows through; indexed/RGB pass through faithfully.
pub fn to_ratatui_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Blit a vt100 screen into `area` of `buf`. The screen must already be sized to
/// `area.width x area.height` (the proxy resizes the parser on layout changes).
pub fn blit_screen(buf: &mut Buffer, area: Rect, screen: &vt100::Screen) {
    for row in 0..area.height {
        for col in 0..area.width {
            let Some(src) = screen.cell(row, col) else {
                continue;
            };
            let x = area.x + col;
            let y = area.y + row;
            let Some(dst) = buf.cell_mut((x, y)) else {
                continue;
            };

            let contents = src.contents();
            if contents.is_empty() {
                dst.set_symbol(" ");
            } else {
                dst.set_symbol(contents);
            }

            let mut modifier = Modifier::empty();
            if src.bold() {
                modifier |= Modifier::BOLD;
            }
            if src.italic() {
                modifier |= Modifier::ITALIC;
            }
            if src.underline() {
                modifier |= Modifier::UNDERLINED;
            }
            if src.inverse() {
                modifier |= Modifier::REVERSED;
            }
            dst.set_style(
                Style::default()
                    .fg(to_ratatui_color(src.fgcolor()))
                    .bg(to_ratatui_color(src.bgcolor()))
                    .add_modifier(modifier),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
