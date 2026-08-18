use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

/// Draw the BashQuest page below the top bar: the Launcher when idle, the live
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
        landing::action(">", "Enter", "log in and start the run", theme::SUCCESS()),
    );
    render_landing(frame, area, launch);
}

/// BashQuest landing copy, used by both the standalone screen fallback and the
/// Games hub when BashQuest is selected. No live/resume state: unlike the
/// roguelike doors, bashquest.sh has no detach-and-resume model (it saves
/// continuously instead, see `state.rs`'s teardown notes), so there is only
/// enabled/disabled to show here.
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let action_line = if enabled {
        landing::action(">", "Enter", "log in and start the run", theme::SUCCESS())
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
    lines.extend(bashquest_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Turn your shell into a systems administration training ground ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("hosted on late.sh", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "Type real commands. A mentor named Tasmania reacts to every answer.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        tier_strip(),
        tier_legend(),
        Line::from(""),
        landing::stat("levels", "90 levels across 18 tiers", 8),
        landing::stat(
            "topics",
            "LVM, networking, SAN, kernels, containers, and TUI tooling",
            8,
        ),
        landing::stat("style", "type the command, get graded, keep the streak", 8),
        Line::from(""),
        flavor_headline(),
        flavor_quote(),
        Line::from(""),
        landing::heading("Launch"),
    ]);
    lines.extend(launch);
    lines.extend([
        landing::heading("Once Inside"),
        landing::hint("hint", "a clue for the current challenge, free", 8),
        landing::hint("skip", "pass a challenge (costs 1 life)", 8),
        landing::hint("Ctrl-C", "quit back to the Games hub", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://github.com/hardlygospel/bashquest",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn bashquest_logo() -> Vec<Line<'static>> {
    [
        " ██████╗  █████╗ ███████╗██╗  ██╗ ██████╗ ██╗   ██╗███████╗███████╗████████╗",
        " ██╔══██╗██╔══██╗██╔════╝██║  ██║██╔═══██╗██║   ██║██╔════╝██╔════╝╚══██╔══╝",
        " ██████╔╝███████║███████╗███████║██║   ██║██║   ██║█████╗  ███████╗   ██║   ",
        " ██╔══██╗██╔══██║╚════██║██╔══██║██║▄▄ ██║██║   ██║██╔══╝  ╚════██║   ██║   ",
        " ██████╔╝██║  ██║███████║██║  ██║╚██████╔╝╚██████╔╝███████╗███████║   ██║   ",
        " ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝ ╚══▀▀╝  ╚═════╝ ╚══════╝╚══════╝   ╚═╝   ",
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

/// A strip naming a handful of the tiers, giving a sense of scale at a glance.
fn tier_strip() -> Line<'static> {
    let dim = |s: &'static str| Span::styled(s, Style::default().fg(theme::TEXT_FAINT()));
    let tier = |s: &'static str| {
        Span::styled(
            s,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    };
    Line::from(vec![
        dim("  "),
        tier("Beginner"),
        dim(" -> "),
        tier("Networking"),
        dim(" -> "),
        tier("SAN"),
        dim(" -> "),
        tier("Kernel"),
        dim(" -> "),
        tier("Ricing"),
        dim(" -> "),
        tier("Docker"),
        dim(" -> "),
        tier("TUI"),
    ])
}

fn tier_legend() -> Line<'static> {
    Line::from(Span::styled(
        "  eighteen tiers, each builds on the last, from ls to kernels and containers",
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

/// The pitch in one line: not a toy. A real Bash script teaching real
/// commands, built and owned end to end.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Pure Bash, zero dependencies \u{b7} GPL-3.0 \u{b7} by Tony \"Hardlygospel\" Hosaroygard",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

/// The single line that sells the tension: every wrong answer costs a life.
fn flavor_headline() -> Line<'static> {
    Line::from(Span::styled(
        "  \"Not quite. Even I've typed 'sl' instead of 'ls' more times than I'll admit.\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  finish all 90 levels and graduate: a certificate, your name, your stats, kept for good.",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting bashquest..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}
