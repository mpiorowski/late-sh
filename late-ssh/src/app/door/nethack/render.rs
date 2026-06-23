use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen;

/// Draw the nethack page below the top bar: the Launcher when idle, the live
/// embedded vt100 widget once the process is running.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_launcher(frame, area, state),
        Mode::Running => draw_running(frame, area, state),
    }
}

fn draw_launcher(frame: &mut Frame, area: Rect, state: &State) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let action_line = if state.is_enabled() {
        action_line(">", "Enter", "descend into the dungeon", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };

    let mut lines = vec![Line::raw("")];
    lines.extend(nethack_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "The classic dungeon roguelike ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("hosted on late.sh", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "Real upstream NetHack, running locally with your own saved game.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        stat_line("saves", "kept per player, resume any time"),
        stat_line("bones", "your deaths haunt other late.sh players"),
        stat_line("style", "explore, fight, ascend with the Amulet"),
        Line::from(""),
        section("Launch"),
        action_line,
        Line::from(""),
        section("Once Inside"),
        hint_line("F1", "pop up the late.sh key cheat sheet"),
        hint_line("hjkl", "move (or use the arrow keys)"),
        hint_line("?", "NetHack's own in-game help menu"),
        hint_line("S", "save and continue another night"),
        hint_line("Ctrl-C", "quit back to this launcher"),
        Line::from(""),
        Line::from(Span::styled(
            "https://www.nethack.org/",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn nethack_logo() -> Vec<Line<'static>> {
    [
        "███╗   ██╗███████╗████████╗██╗  ██╗ █████╗  ██████╗██╗  ██╗",
        "████╗  ██║██╔════╝╚══██╔══╝██║  ██║██╔══██╗██╔════╝██║ ██╔╝",
        "██╔██╗ ██║█████╗     ██║   ███████║███████║██║     █████╔╝ ",
        "██║╚██╗██║██╔══╝     ██║   ██╔══██║██╔══██║██║     ██╔═██╗ ",
        "██║ ╚████║███████╗   ██║   ██║  ██║██║  ██║╚██████╗██║  ██╗",
        "╚═╝  ╚═══╝╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝",
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
            format!("{label:<8}"),
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

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting nethack..."), area);
        return;
    };
    {
        let buf = frame.buffer_mut();
        proxy.with_screen(|screen| blit_screen(buf, area, screen));
    }
    if state.help_open() {
        draw_cheatsheet(frame, area);
    }
}

/// Beginner keybinding overlay, toggled with F1. NetHack's own `?` help is a
/// menu maze; this is the at-a-glance card a first-timer actually needs.
fn draw_cheatsheet(frame: &mut Frame, area: Rect) {
    let rows: &[(&str, &str)] = &[
        ("hjkl", "move  (left down up right)"),
        ("yubn", "move diagonally"),
        ("HJKL", "run in a direction"),
        (".", "wait a turn   s  search here"),
        ("i  ,  d", "inventory · pick up · drop"),
        ("<  >", "go up / down stairs"),
        ("o  c", "open / close a door"),
        ("e q r", "eat · drink · read"),
        ("w  W", "wield weapon · wear armor"),
        ("z Z t f", "zap · cast · throw · fire"),
        (":  ;  /", "look here · far-look · what is"),
        ("#", "extended commands (e.g. #pray)"),
        ("?", "NetHack's own help menu"),
        ("S", "save & continue another night"),
        ("Ctrl-C", "quit (forfeit this game)"),
    ];

    let mut lines: Vec<Line> = Vec::with_capacity(rows.len() + 2);
    lines.push(Line::from(Span::styled(
        "Goal: descend, grab the Amulet, ascend.",
        Style::default().fg(theme::AMBER_DIM()),
    )));
    lines.push(Line::raw(""));
    for (keys, desc) in rows {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {keys:<9}"),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled((*desc).to_string(), Style::default().fg(theme::TEXT())),
        ]));
    }

    let width = 46u16.min(area.width.saturating_sub(2));
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = centered_rect(area, width, height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " NetHack keys — F1 to close ",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::BG_CANVAS()));

    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}
