use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
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
    draw_landing(frame, area, state.is_enabled());
}

/// NetHack landing copy, used by both the standalone screen fallback and the
/// Games hub when NetHack is selected.
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
        landing::action(">", "Enter", "descend into the dungeon", theme::SUCCESS())
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
            "Real upstream NetHack. Your save persists; the dead stay down there.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        dungeon_strip(),
        dungeon_legend(),
        Line::from(""),
        landing::stat("saves", "kept per player, resume any time", 8),
        landing::stat("bones", "your deaths haunt other late.sh players", 8),
        landing::stat("style", "explore, fight, ascend with the Amulet", 8),
        Line::from(""),
        flavor_headline(),
        flavor_quote(),
        Line::from(""),
        landing::heading("Rewards"),
        landing::stat(
            "Amulet of Yendor",
            "10,000 chips + NHA badge, once per account",
            18,
        ),
        landing::stat(
            "Ascension",
            "20,000 chips + NHY badge, once per account",
            18,
        ),
        Line::from(Span::styled(
            "  Play again any time, but these chip payouts are lifetime claims.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(""),
        landing::heading("Launch"),
        action_line,
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint("? or F1", "NetHack's own in-game help menu", 8),
        landing::hint("S", "save and continue another night", 8),
        landing::hint("Ctrl-C", "quit back to the Games hub", 8),
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

/// A glyph painted in its NetHack-ish color, bold so it reads against the floor.
fn glyph(ch: &'static str, color: Color) -> Span<'static> {
    Span::styled(ch, Style::default().fg(color).add_modifier(Modifier::BOLD))
}

/// A scrap of colored dungeon: signals at a glance that this is a real ASCII
/// roguelike, not a menu. Floor dots are faint so the live glyphs pop.
fn dungeon_strip() -> Line<'static> {
    let floor = |dots: &'static str| Span::styled(dots, Style::default().fg(theme::TEXT_FAINT()));
    Line::from(vec![
        floor("  ....."),
        glyph("@", theme::TEXT_BRIGHT()),
        floor("...."),
        glyph("d", theme::AMBER()),
        floor("....."),
        glyph("$", theme::BADGE_GOLD()),
        floor("......"),
        glyph("D", theme::ERROR()),
        floor("....."),
        glyph("<", theme::AMBER_GLOW()),
        floor("....."),
    ])
}

/// Decodes the strip above for anyone who has never seen the @ before.
fn dungeon_legend() -> Line<'static> {
    let word = |w: &'static str| Span::styled(w, Style::default().fg(theme::TEXT_DIM()));
    Line::from(vec![
        word("  "),
        glyph("@", theme::TEXT_BRIGHT()),
        word(" you   "),
        glyph("d", theme::AMBER()),
        word(" a foe   "),
        glyph("$", theme::BADGE_GOLD()),
        word(" gold   "),
        glyph("D", theme::ERROR()),
        word(" a dragon   "),
        glyph("<", theme::AMBER_GLOW()),
        word(" stairs up"),
    ])
}

/// The pitch in one line: not abandonware. A nearly-40-year-old game, kept in the
/// Museum of Modern Art, that still ships major releases (5.0.0 landed recently
/// with over 3,000 changes).
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Born 1987 \u{b7} in the MoMA collection \u{b7} still shipping (5.0.0, 3,000+ fixes)",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

/// The community's name for the game's obsessive depth; the single strongest line
/// for selling it, followed by one concrete taste of that detail.
fn flavor_headline() -> Line<'static> {
    // Faint italic, matching `flavor_quote` below, so the two read as one flavor
    // block. Bold (not amber) gives it weight without colliding with `section`
    // headings, which own amber-bold.
    Line::from(Span::styled(
        "  \"The DevTeam thinks of everything\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  dip a potion into itself: \"this is a potion bottle, not a Klein bottle.\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
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
}
