use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{HandleStatus, Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

/// Draw the DCSS page below the top bar: the Launcher when idle, the live
/// embedded vt100 widget once the process is running.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_launcher(frame, area, state),
        Mode::Running => draw_running(frame, area, state),
    }
}

/// The door-screen launcher: the landing with a handle-aware Launch block (the
/// one-time arcade-name claim prompt, then the play action). Constant three
/// lines in every handle state, so the chrome never moves as lookups and
/// claims resolve.
fn draw_launcher(frame: &mut Frame, area: Rect, state: &State) {
    if !state.is_enabled() {
        draw_landing(frame, area, false);
        return;
    }
    let dim = |text: String| Line::from(Span::styled(text, Style::default().fg(theme::TEXT_DIM())));
    let launch = match state.handle_status() {
        HandleStatus::Loading => vec![
            dim("Checking your arcade name...".to_string()),
            Line::from(""),
            Line::from(""),
        ],
        HandleStatus::Missing { error } => {
            let notice = match error {
                Some(msg) => Line::from(Span::styled(msg, Style::default().fg(theme::ERROR()))),
                None => Line::from(Span::styled(
                    "Shown publicly with your games. Cannot be changed later.",
                    Style::default().fg(theme::TEXT_FAINT()),
                )),
            };
            vec![
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme::SUCCESS())),
                    Span::styled("claim your arcade name: ", Style::default().fg(theme::TEXT())),
                    Span::styled(
                        state.entry_input().to_string(),
                        Style::default()
                            .fg(theme::TEXT_BRIGHT())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("_", Style::default().fg(theme::AMBER())),
                ]),
                dim("3-20 characters: letters, digits, underscore. Enter claims and plays."
                    .to_string()),
                notice,
            ]
        }
        HandleStatus::Claiming => vec![
            dim(format!("Claiming {}...", state.entry_input())),
            Line::from(""),
            Line::from(""),
        ],
        HandleStatus::Claimed(name) => vec![
            landing::action(">", "Enter", "descend for the Orb of Zot", theme::SUCCESS()),
            dim(format!("Playing as {name}.")),
            Line::from(""),
        ],
        HandleStatus::Failed => vec![
            Line::from(Span::styled(
                "Couldn't check your arcade name.",
                Style::default().fg(theme::ERROR()),
            )),
            landing::action(">", "Enter", "retry", theme::SUCCESS()),
            Line::from(""),
        ],
    };
    render_landing(frame, area, launch);
}

/// DCSS landing copy with the classic one-line Launch block, used by the Games
/// hub when DCSS is selected (the hub has no per-session door state).
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let action_line = if enabled {
        landing::action(">", "Enter", "descend for the Orb of Zot", theme::SUCCESS())
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
    lines.extend(crawl_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Dungeon Crawl Stone Soup ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("hosted on late.sh", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "Real upstream crawl. Grab three runes, seize the Orb, and get out alive.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        dungeon_strip(),
        dungeon_legend(),
        Line::from(""),
        landing::stat("saves", "kept per player, resume any time", 8),
        landing::stat("runes", "collect 3 of 15, then the Realm of Zot opens", 8),
        landing::stat("style", "tactics over grinding: every fight is a puzzle", 8),
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
        landing::hint("? or F1", "crawl's own in-game help menu", 8),
        landing::hint("S", "save and continue another night", 8),
        landing::hint("Ctrl-Q", "abandon the character for good", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://crawl.develz.org/",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn crawl_logo() -> Vec<Line<'static>> {
    [
        " ██████╗██████╗  █████╗ ██╗    ██╗██╗     ",
        "██╔════╝██╔══██╗██╔══██╗██║    ██║██║     ",
        "██║     ██████╔╝███████║██║ █╗ ██║██║     ",
        "██║     ██╔══██╗██╔══██║██║███╗██║██║     ",
        "╚██████╗██║  ██║██║  ██║╚███╔███╔╝███████╗",
        " ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚══╝╚══╝ ╚══════╝",
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

/// A glyph painted in its crawl-ish color, bold so it reads against the floor.
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
        glyph("g", theme::AMBER()),
        floor("....."),
        glyph("$", theme::BADGE_GOLD()),
        floor("......"),
        glyph("&", theme::ERROR()),
        floor("....."),
        glyph(">", theme::AMBER_GLOW()),
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
        glyph("g", theme::AMBER()),
        word(" a goblin   "),
        glyph("$", theme::BADGE_GOLD()),
        word(" gold   "),
        glyph("&", theme::ERROR()),
        word(" a demon lord   "),
        glyph(">", theme::AMBER_GLOW()),
        word(" stairs down"),
    ])
}

/// The pitch in one line: NetHack's living successor generation. Community-run
/// since 2006, still shipping yearly versions with public tournaments.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Born 2006 from Linley's Crawl \u{b7} yearly releases \u{b7} tournaments still running",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

/// The design philosophy the community repeats; the one-line reason DCSS feels
/// different from the older roguelikes, followed by a concrete taste of it.
fn flavor_headline() -> Line<'static> {
    // Faint italic, matching `flavor_quote` below, so the two read as one flavor
    // block. Bold (not amber) gives it weight without colliding with `section`
    // headings, which own amber-bold.
    Line::from(Span::styled(
        "  \"You have escaped with the Orb!\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  most runs end as a morgue file; the good ones end with that line.",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting crawl..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}
