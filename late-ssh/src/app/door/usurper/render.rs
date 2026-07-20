use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

/// Draw the Usurper page below the top bar: the Launcher when idle, the live
/// embedded vt100 widget once the process is running.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_launcher(frame, area, state),
        Mode::Running => draw_running(frame, area, state),
    }
}

/// The door-screen launcher: the landing with a handle-aware Launch block (the
/// one-time arcade-name claim prompt, then the play action; see
/// `landing::handle_launch_block`).
fn draw_launcher(frame: &mut Frame, area: Rect, state: &State) {
    if !state.is_enabled() {
        draw_landing(frame, area, false);
        return;
    }
    let launch = landing::handle_launch_block(
        state.handle_status(),
        state.entry_input(),
        landing::action(">", "Enter", "enter the realm", theme::SUCCESS()),
    );
    render_landing(frame, area, launch);
}

/// Usurper landing copy with the classic one-line Launch block, used by the
/// Games hub when Usurper is selected (the hub has no per-session door state).
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let action_line = if enabled {
        landing::action(">", "Enter", "enter the realm", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };
    render_landing(frame, area, vec![action_line]);
}

/// The landing body around a caller-supplied Launch block.
fn render_landing(frame: &mut Frame, area: Rect, launch: Vec<Line<'static>>) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let mut lines = vec![Line::raw("")];
    lines.extend(usurper_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "The legendary LORD-era BBS door ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("hosted on late.sh", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "Real upstream Usurper. Fight, scheme, and drink your way to the throne.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        landing::stat("world", "one shared realm; every player walks the same town", 8),
        landing::stat("daily", "turns refresh each day; a session is a quick visit", 8),
        landing::stat("gangs", "form teams, brawl rivals, hold the town together", 8),
        landing::stat("throne", "dethrone the king, or reach the darkest dungeon", 8),
        Line::from(""),
        flavor_headline(),
        flavor_quote(),
        Line::from(""),
        landing::heading("Launch"),
    ]);
    lines.extend(launch);
    lines.extend([
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint("menus", "every screen lists its own keys; letters choose", 8),
        landing::hint("Q", "quit to the hub from the main menus", 8),
        landing::hint("size", "a classic 80x25 screen; keep the window roomy", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://www.usurper.info/",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn usurper_logo() -> Vec<Line<'static>> {
    [
        "██╗   ██╗███████╗██╗   ██╗██████╗ ██████╗ ███████╗██████╗ ",
        "██║   ██║██╔════╝██║   ██║██╔══██╗██╔══██╗██╔════╝██╔══██╗",
        "██║   ██║███████╗██║   ██║██████╔╝██████╔╝█████╗  ██████╔╝",
        "██║   ██║╚════██║██║   ██║██╔══██╗██╔═══╝ ██╔══╝  ██╔══██╗",
        "╚██████╔╝███████║╚██████╔╝██║  ██║██║     ███████╗██║  ██║",
        " ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚══════╝╚═╝  ╚═╝",
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

/// The pitch in one line: the door LORD players graduated to, still alive
/// thanks to the author GPL'ing it and the community porting it forward.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Born 1993 on the BBS scene \u{b7} kicking fantasy: gangs, gods and steroids \u{b7} GPL since 2007",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

fn flavor_headline() -> Line<'static> {
    // Faint italic, matching `flavor_quote` below, so the two read as one
    // flavor block. Bold (not amber) gives it weight without colliding with
    // `heading`, which owns amber-bold.
    Line::from(Span::styled(
        "  \"Be prepared for violent and bizarre nonstop action.\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  the original box copy undersold it; the steroids alone end careers.",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting usurper..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}
