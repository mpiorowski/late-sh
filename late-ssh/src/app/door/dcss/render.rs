use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;
use crate::app::door::spectate::proxy::LiveGame;
use crate::app::door::spectate::ui::{duration_label, minutes_since};

/// How many live games the hub landing lists by name before summing the rest.
const WATCH_LIST_MAX: usize = 5;

/// Draw the DCSS page below the top bar: the Launcher when idle, the live
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
        draw_landing(frame, area, false, false, &[], false, 0);
        return;
    }
    let launch = landing::handle_launch_block(
        state.handle_status(),
        state.entry_input(),
        landing::action(">", "Enter", "descend for the Orb of Zot", theme::SUCCESS()),
    );
    render_landing(frame, area, launch, Vec::new(), 0);
}

/// DCSS landing copy with the classic one-line Launch block, used by the Games
/// hub when DCSS is selected (the hub has no per-session door state), plus
/// who is playing right now for the hub's `s` watch key and the player's own
/// `t` switch for seeing watcher chat under their game.
pub fn draw_landing(
    frame: &mut Frame,
    area: Rect,
    enabled: bool,
    live: bool,
    roster: &[LiveGame],
    show_watch_chat: bool,
    scroll: u16,
) -> u16 {
    let action_line = if live {
        landing::action(
            ">",
            "Enter",
            "resume your game in progress",
            theme::SUCCESS(),
        )
    } else if enabled {
        landing::action(">", "Enter", "descend for the Orb of Zot", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };
    let watch = if enabled {
        let mut lines = vec![landing::hint(
            "t",
            match show_watch_chat {
                true => "watcher chat under your game: shown",
                false => "watcher chat under your game: hidden",
            },
            8,
        )];
        lines.extend(watch_block(roster));
        lines
    } else {
        Vec::new()
    };
    render_landing(frame, area, vec![action_line], watch, scroll)
}

/// The Watch Live section: who is in the dungeon right now, and the key.
fn watch_block(roster: &[LiveGame]) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(""), landing::heading("Watch Live")];
    if roster.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Nobody is in the dungeon right now.",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
        return lines;
    }
    for game in roster.iter().take(WATCH_LIST_MAX) {
        let mut detail = format!("{} in", duration_label(minutes_since(game.started_unix)));
        if game.watchers > 0 {
            detail.push_str(&format!(" \u{b7} {} watching", game.watchers));
        }
        lines.push(landing::stat(&game.playname, &detail, 22));
    }
    if roster.len() > WATCH_LIST_MAX {
        lines.push(Line::from(Span::styled(
            format!("  and {} more", roster.len() - WATCH_LIST_MAX),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    lines.push(landing::action(
        ">",
        "s",
        "watch over their shoulder (\u{2190}/\u{2192} switch, Esc back)",
        theme::AMBER_GLOW(),
    ));
    lines
}

/// The landing body around a caller-supplied Launch block and the optional
/// Watch Live section beneath it.
fn render_landing(
    frame: &mut Frame,
    area: Rect,
    launch: Vec<Line<'static>>,
    watch: Vec<Line<'static>>,
    scroll: u16,
) -> u16 {
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
                "The best roguelike in active development ",
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
        landing::stat(
            "builds",
            "dozens of species and backgrounds, no two runs alike",
            8,
        ),
        landing::stat("style", "tactics over grinding: every fight is a puzzle", 8),
        Line::from(""),
        flavor_headline(),
        flavor_quote(),
        Line::from(""),
        landing::heading("Rewards"),
        landing::stat(
            "Orb of Zot",
            "20,000 chips, and the DCO badge the first time",
            14,
        ),
        landing::stat(
            "Escape",
            "40,000 chips, and the DCW badge the first time",
            14,
        ),
        Line::from(Span::styled(
            "  Each pays again 30 days after the last time it paid. One run, one payout.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(""),
        landing::heading("Launch"),
    ]);
    lines.extend(launch);
    lines.push(landing::hint("c", "customize your init.txt (paste box)", 8));
    lines.extend(watch);
    lines.extend([
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint("? or F1", "crawl's own in-game help menu", 8),
        landing::hint("S", "save and continue another night", 8),
        landing::hint("`", "step out to chat; the game keeps running", 8),
        landing::hint("Ctrl-Q", "abandon the character for good", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://crawl.develz.org/",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    crate::app::door::landing::render_scrolled(
        frame,
        inner,
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        scroll,
    )
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

/// The pitch in one line: NetHack's living successor generation, and today's
/// most-played traditional roguelike (the public online servers' logfiles are
/// the receipts). Community-run since 2006, tournament every release.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Born 2006 from Linley's Crawl \u{b7} most-played roguelike online \u{b7} yearly tournaments",
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
