use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
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
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if area.width >= 122 && area.height >= 20 {
            [Constraint::Min(62), Constraint::Length(38)]
        } else {
            [Constraint::Min(0), Constraint::Length(0)]
        })
        .split(area);

    draw_launch_copy(frame, layout[0], state);
    if layout.len() > 1 && layout[1].width > 0 {
        draw_sky_art(frame, layout[1]);
    }
}

fn draw_launch_copy(frame: &mut Frame, area: Rect, state: &State) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let action_line = if state.is_enabled() {
        action_line(">", "Enter", "launch the proxy", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };

    let mut lines = vec![Line::raw("")];
    lines.extend(rebels_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Pirate basketball ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "across a corporate galaxy",
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]),
        Line::from(Span::styled(
            "A remote SSH door game from frittura.org, embedded inside late.sh.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
    ]);
    lines.extend(game_stats());
    lines.extend([
        action_line,
        Line::from(""),
        section("Once Inside"),
        hint_line("Esc", "return to this launcher"),
        hint_line("Ctrl-C", "also leaves the remote session"),
        hint_line("mouse", "forwarded into the remote terminal"),
        Line::from(""),
        Line::from(Span::styled(
            "https://wiki.rebels.frittura.org/index.html",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            "github.com/ricott1/rebels-in-the-sky",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_sky_art(frame: &mut Frame, area: Rect) {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(12),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    frame.render_widget(Paragraph::new(spaceship_ascii()), inner[1]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Starter ships",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            fact_line("Bresci", "fast shuttle"),
            fact_line("Orwell", "sturdy pincher"),
            fact_line("Ibarruri", "double-engine jester"),
        ])
        .wrap(Wrap { trim: false }),
        inner[3],
    );
}

fn rebels_logo() -> Vec<Line<'static>> {
    [
        "██████╗ ███████╗██████╗ ███████╗██╗     ███████╗",
        "██╔══██╗██╔════╝██╔══██╗██╔════╝██║     ██╔════╝",
        "██████╔╝█████╗  ██████╔╝█████╗  ██║     ███████╗",
        "██╔══██╗██╔══╝  ██╔══██╗██╔══╝  ██║     ╚════██║",
        "██║  ██║███████╗██████╔╝███████╗███████╗███████║",
        "╚═╝  ╚═╝╚══════╝╚═════╝ ╚══════╝╚══════╝╚══════╝",
    ]
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
    })
    .collect()
}

fn spaceship_ascii() -> Vec<Line<'static>> {
    [
        "          .        *",
        "    *                  .",
        "              /\\",
        "             /  \\",
        "            /_==_\\",
        "       ____/|_||_|\\____",
        "   ___/  _    ||    _  \\___",
        "  /___  /_\\___||___/_\\  ___\\",
        "      \\____   ||   ____/",
        "           \\__||__/",
        "            /_||_\\",
        "          ==  ||  ==",
    ]
    .into_iter()
    .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::TEXT_DIM()))))
    .collect()
}

fn game_stats() -> Vec<Line<'static>> {
    vec![
        stat_line("remote ssh", "proxied live into this terminal"),
        stat_line("identity", "derived from your late.sh account"),
        stat_line("style", "space travel, roster building, basketball"),
        Line::from(""),
        section("Launch"),
    ]
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

fn stat_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{label:<12}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn action_line(marker: &str, key: &str, label: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(color)),
        Span::styled(
            format!("{key:<8}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT())),
    ])
}

fn hint_line(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{key:<8}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn fact_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<9}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
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
