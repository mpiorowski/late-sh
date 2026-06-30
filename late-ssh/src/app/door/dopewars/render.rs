use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

/// Draw the dopewars page below the top bar: the Launcher when idle, the live
/// embedded vt100 widget once the process is running.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_launcher(frame, area, state),
        Mode::Running => draw_running(frame, area, state),
    }
}

fn draw_launcher(frame: &mut Frame, area: Rect, state: &State) {
    draw_landing(frame, area, state.is_enabled());
}

/// dopewars landing copy, used by both the standalone screen fallback and the
/// Games hub when dopewars is selected.
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let action_line = if enabled {
        landing::action(">", "Enter", "hit the streets", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };

    let mut lines = vec![Line::raw("")];
    lines.extend(dopewars_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Buy low, sell high ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "before the month runs out",
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]),
        Line::from(Span::styled(
            "The classic street-market trading game. 31 days to turn a stake into a fortune.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        market_strip(),
        market_legend(),
        Line::from(""),
        landing::stat("turns", "31 days, then the high-score board", 8),
        landing::stat("watch", "the loan shark, the cops, your trenchcoat", 8),
        landing::stat("style", "pure economy: read the market, time the jet", 8),
        Line::from(""),
        flavor_headline(),
        flavor_quote(),
        Line::from(""),
        landing::heading("Launch"),
        action_line,
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint("letters", "pick the highlighted action on each prompt", 8),
        landing::hint("J", "jet to the next location", 8),
        landing::hint("Ctrl-C", "quit back to the Games hub", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://dopewars.sourceforge.io/",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn dopewars_logo() -> Vec<Line<'static>> {
    [
        "██████╗  ██████╗ ██████╗ ███████╗██╗    ██╗ █████╗ ██████╗ ███████╗",
        "██╔══██╗██╔═══██╗██╔══██╗██╔════╝██║    ██║██╔══██╗██╔══██╗██╔════╝",
        "██║  ██║██║   ██║██████╔╝█████╗  ██║ █╗ ██║███████║██████╔╝███████╗",
        "██║  ██║██║   ██║██╔═══╝ ██╔══╝  ██║███╗██║██╔══██║██╔══██╗╚════██║",
        "██████╔╝╚██████╔╝██║     ███████╗╚███╔███╔╝██║  ██║██║  ██║███████║",
        "╚═════╝  ╚═════╝ ╚═╝     ╚══════╝ ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝",
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

/// A glyph painted in a market color, bold so it reads against the dim row.
fn glyph(ch: &'static str, color: Color) -> Span<'static> {
    Span::styled(ch, Style::default().fg(color).add_modifier(Modifier::BOLD))
}

/// A scrap of price ticker: signals at a glance that this is an economy game,
/// the prices swinging wildly is the whole hook.
fn market_strip() -> Line<'static> {
    let dim = |s: &'static str| Span::styled(s, Style::default().fg(theme::TEXT_FAINT()));
    Line::from(vec![
        dim("  "),
        glyph("$1,420", theme::SUCCESS()),
        dim(" \u{25b2}   "),
        glyph("$310", theme::ERROR()),
        dim(" \u{25bc}   "),
        glyph("$58", theme::ERROR()),
        dim(" \u{25bc}   "),
        glyph("$2,900", theme::SUCCESS()),
        dim(" \u{25b2}"),
    ])
}

/// Decodes the ticker: prices crash and spike between cities, and the spread is
/// where the money is.
fn market_legend() -> Line<'static> {
    let word = |w: &'static str| Span::styled(w, Style::default().fg(theme::TEXT_DIM()));
    Line::from(vec![
        word("  prices swing every move \u{b7} "),
        glyph("\u{25b2}", theme::SUCCESS()),
        word(" sell into a spike   "),
        glyph("\u{25bc}", theme::ERROR()),
        word(" buy the crash"),
    ])
}

/// The pitch in one line: not abandonware. A 1984 type-in BASIC game ("Drug
/// Wars") that Ben Webb turned into a real, still-packaged open-source program.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "From a 1984 type-in classic \u{b7} open source since 1998 \u{b7} still in every distro",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

/// The single line that sells the tension: the clock and the debt never stop.
fn flavor_headline() -> Line<'static> {
    Line::from(Span::styled(
        "  \"The loan shark wants his money. The month is half gone.\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  one good arbitrage between two cities and you're set; one bust and you start over.",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting dopewars..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}
