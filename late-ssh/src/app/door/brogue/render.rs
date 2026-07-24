use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

/// Draw the Brogue page below the top bar: the Launcher when idle, the live
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
        landing::action(
            ">",
            "Enter",
            "descend into the Dungeons of Doom",
            theme::SUCCESS(),
        ),
    );
    render_landing(frame, area, launch);
}

/// Brogue landing copy with the classic one-line Launch block, used by the
/// Games hub when Brogue is selected (the hub has no per-session door state).
pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let action_line = if enabled {
        landing::action(
            ">",
            "Enter",
            "descend into the Dungeons of Doom",
            theme::SUCCESS(),
        )
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
    lines.extend(brogue_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "The most beautiful roguelike a terminal can draw ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("hosted on late.sh", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "Real upstream Brogue CE. Descend 26 floors, take the Amulet of Yendor, climb back out.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        legend_credentials(),
        Line::from(""),
        dungeon_strip(),
        dungeon_legend(),
        Line::from(""),
        landing::stat("saves", "kept per player, resume any time", 8),
        landing::stat("goal", "steal the Amulet of Yendor from depth 26, then get out alive", 8),
        landing::stat(
            "style",
            "no grinding, no classes: your build is whatever you find and dare to use",
            8,
        ),
        landing::stat("runs", "short and deadly: a good one fits in an evening", 8),
        landing::stat("screen", "roomiest at 100x34; smaller terminals crop the map", 8),
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
        landing::hint("? or F1", "brogue's own commands and help", 8),
        landing::hint("S", "save and continue another night", 8),
        landing::hint("Q", "abandon the run for good", 8),
        Line::from(""),
        Line::from(Span::styled(
            "https://github.com/tmewett/BrogueCE",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn brogue_logo() -> Vec<Line<'static>> {
    [
        "██████╗ ██████╗  ██████╗  ██████╗ ██╗   ██╗███████╗",
        "██╔══██╗██╔══██╗██╔═══██╗██╔════╝ ██║   ██║██╔════╝",
        "██████╔╝██████╔╝██║   ██║██║  ███╗██║   ██║█████╗  ",
        "██╔══██╗██╔══██╗██║   ██║██║   ██║██║   ██║██╔══╝  ",
        "██████╔╝██║  ██║╚██████╔╝╚██████╔╝╚██████╔╝███████╗",
        "╚═════╝ ╚═╝  ╚═╝ ╚═════╝  ╚═════╝  ╚═════╝ ╚══════╝",
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

/// A glyph painted in a brogue-ish color, bold so it reads against the floor.
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
        glyph("k", theme::AMBER()),
        floor("....."),
        glyph("$", theme::BADGE_GOLD()),
        floor("......"),
        glyph("D", theme::ERROR()),
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
        glyph("k", theme::AMBER()),
        word(" a kobold   "),
        glyph("$", theme::BADGE_GOLD()),
        word(" gold   "),
        glyph("D", theme::ERROR()),
        word(" a dragon   "),
        glyph(">", theme::AMBER_GLOW()),
        word(" stairs down"),
    ])
}

/// The pitch in one line: Brian Walker's 2009 design landmark, kept alive and
/// polished by the Community Edition since 2020.
fn legend_credentials() -> Line<'static> {
    Line::from(Span::styled(
        "Brian Walker's 2009 masterpiece \u{b7} kept alive by the Community Edition \u{b7} pure painted ASCII",
        Style::default().fg(theme::AMBER_DIM()),
    ))
}

/// The high-score line every run chases; the one-line reason Brogue reads
/// different from the older roguelikes, followed by a concrete taste of it.
fn flavor_headline() -> Line<'static> {
    // Faint italic, matching `flavor_quote` below, so the two read as one flavor
    // block. Bold (not amber) gives it weight without colliding with `section`
    // headings, which own amber-bold.
    Line::from(Span::styled(
        "  \"Escaped the Dungeons of Doom!\"",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    ))
}

fn flavor_quote() -> Line<'static> {
    Line::from(Span::styled(
        "  most runs end as an epitaph on the high-score list; the good ones end with that line.",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy().filter(|p| p.is_running()) else {
        frame.render_widget(Paragraph::new("Starting brogue..."), area);
        return;
    };
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| {
        let grid = grid_rect(area, screen);
        clear_letterbox(buf, area);
        blit_screen(buf, grid, screen);
        clear_canvas_black(buf, grid);
    });
}

/// Center brogue's fixed grid inside the viewport. The parser tracks the
/// game's own geometry (brogue emits `ESC[8;34;100t` at startup, which the
/// vt100 crate honors), so the screen is usually exactly 100x34 while the
/// viewport is larger. A viewport smaller than the grid pins to the top-left
/// so the sidebar and message line stay visible.
fn grid_rect(area: Rect, screen: &vt100::Screen) -> Rect {
    let (rows, cols) = screen.size();
    let w = cols.min(area.width);
    let h = rows.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// The app root paints every page over `BG_CANVAS`, and `blit_screen` never
/// touches cells outside the game grid, so without this the fixed 100x34 game
/// floats in a page-colored frame. Reset the whole door area first: the
/// letterbox then shares one canvas (`Reset`, the terminal default) with the
/// keyed-out game interior below, matching how nethack and dcss render.
fn clear_letterbox(buf: &mut ratatui::buffer::Buffer, area: Rect) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
            }
        }
    }
}

/// Turn brogue's canvas black transparent so the late.sh theme background shows
/// through, matching how nethack and dcss default-background cells render.
/// Those games emit the terminal-default color for empty space; brogue never
/// does (its curses build paints every cell an explicit color, black included),
/// so the equivalent is keying out its black after the blit. Rgb(0,0,0) is the
/// host's 24-bit output (COLORTERM=truecolor); Indexed(16), the color-cube
/// black, covers a host still on the 256-color coercion path.
fn clear_canvas_black(buf: &mut ratatui::buffer::Buffer, area: Rect) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            let bg = cell.style().bg;
            if bg == Some(Color::Rgb(0, 0, 0)) || bg == Some(Color::Indexed(16)) {
                cell.set_style(cell.style().bg(Color::Reset));
            }
        }
    }
}

#[cfg(test)]
#[path = "render_test.rs"]
mod render_test;
