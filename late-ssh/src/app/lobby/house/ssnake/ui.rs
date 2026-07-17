use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    lobby::house::{
        game_ui::{
            draw_game_frame_with_info_sidebar, draw_game_overlay, info_label_value, info_tagline,
            key_hint, payout_cooldown_label,
        },
        ssnake::{
            levels::{Cell, MAX_HEIGHT, SsnakeLevel},
            state::{MAX_SEATS, Motion, Pos, SsnakeColor, SsnakeOutcome, SsnakePhase, State},
            svc::{SSNAKE_WIN_CHIPS, SSNAKE_WIN_PAYOUT_COOLDOWN, SsnakeSnapshot},
        },
    },
};
use crate::usernames::UsernameLookup;

// ── Layout ─────────────────────────────────────────────────────
// Arenas are up to 63x36 matrix cells. Each terminal row renders two
// matrix rows with the upper-half block, so at 1x the arena pane is at
// most 65 wide (63 + border) and 21 tall (18 + border + status row).
// On panes with room to spare every cell doubles to a 2x2 block (2 cols
// x 1 terminal row) so the arena fills the screen instead of floating
// as a small box; the map itself never changes.

const SIDEBAR_WIDTH: u16 = 28;

// ── Arena palette ──────────────────────────────────────────────
// Very block, always two snakes: green and red. Walls keep the DOS
// brick-brown of the original; warp tunnels hint in dim blue.
const ARENA_BG: Color = Color::Rgb(12, 14, 18);
const WALL: Color = Color::Rgb(146, 92, 46);
const WARP: Color = Color::Rgb(42, 62, 96);
const GREEN_HEAD: Color = Color::Rgb(112, 232, 138);
const GREEN_BODY: Color = Color::Rgb(56, 148, 80);
const RED_HEAD: Color = Color::Rgb(255, 96, 96);
const RED_BODY: Color = Color::Rgb(168, 52, 52);
const BLUE_HEAD: Color = Color::Rgb(96, 196, 255);
const BLUE_BODY: Color = Color::Rgb(40, 110, 176);
const PURPLE_HEAD: Color = Color::Rgb(198, 118, 255);
const PURPLE_BODY: Color = Color::Rgb(122, 62, 170);
const POINT: Color = Color::Rgb(255, 200, 84);
const LIFE_POINT: Color = Color::Rgb(255, 108, 198);
const LIFE_POINT_BLINK: Color = Color::Rgb(255, 214, 240);

pub fn preferred_height(state: &State, area: Rect) -> u16 {
    let arena_rows = state
        .snapshot()
        .level
        .as_ref()
        .map(|level| {
            if zoom_eligible(level, area) {
                level.height as u16
            } else {
                level.height.div_ceil(2) as u16
            }
        })
        .unwrap_or(MAX_HEIGHT.div_ceil(2) as u16);
    // +2 border, +2 breathing room so the board never sits cramped against
    // the pane edges (centering turns the slack into margins).
    (arena_rows + 4).min(area.height.max(1))
}

/// Rows the embedded chat keeps when the zoomed arena claims the rest of
/// the screen (plus the one-row spacer above it).
const ZOOM_CHAT_FLOOR: u16 = 8;

/// Whether the pane should be sized for the 2x zoom: the doubled arena must
/// fit beside the sidebar, and chat below must keep at least its floor.
/// `area` is the full pre-split content area; the draw side re-derives the
/// scale from the rect it actually receives, so a request the layout can't
/// honor degrades back to 1x.
fn zoom_eligible(level: &SsnakeLevel, area: Rect) -> bool {
    area.width >= level.width as u16 * 2 + 2 + SIDEBAR_WIDTH
        && level.height as u16 + 4 + ZOOM_CHAT_FLOOR <= area.height
}

// ── Entry point ────────────────────────────────────────────────
// The arena gets the whole pane: no separate status row or in-arena
// clutter. Status text lives in the arena border title; everything else
// (players, level, controls) lives in the info sidebar when it fits.

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    if area.height < 8 || area.width < 30 {
        draw_compact(frame, area, state);
        return;
    }

    let arena_width = state
        .snapshot()
        .level
        .as_ref()
        .map(|level| level.width as u16 + 2)
        .unwrap_or(40);
    let show_sidebar = area.width >= arena_width + SIDEBAR_WIDTH;
    let info = info_lines(state, usernames);
    let content = draw_game_frame_with_info_sidebar(frame, area, "Super Snake", info, show_sidebar);

    if show_sidebar {
        draw_arena(frame, content, state);
    } else {
        let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(content);
        draw_arena(frame, rows[0], state);
        frame.render_widget(
            Paragraph::new(key_line(state)).alignment(Alignment::Center),
            rows[1],
        );
    }
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let seated = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
    let level_name = snapshot
        .level
        .as_ref()
        .map(|level| level.name.clone())
        .unwrap_or_else(|| "no arena".to_string());
    let lines = vec![
        Line::from(Span::styled(
            status_text(snapshot),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(format!(
            "{seated}/{} seated · {} · {}",
            snapshot.seat_limit, level_name, snapshot.speed_label
        ))
        .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

// ── Arena ──────────────────────────────────────────────────────

fn draw_arena(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let Some(level) = snapshot.level.as_ref() else {
        frame.render_widget(
            Paragraph::new(waiting_lines(state)).alignment(Alignment::Center),
            area,
        );
        return;
    };

    if area.width < level.width as u16 + 2 || area.height < level.height.div_ceil(2) as u16 + 2 {
        frame.render_widget(
            Paragraph::new("Arena needs more room.").alignment(Alignment::Center),
            area,
        );
        return;
    }

    // Largest cell scale the pane fits; preferred_height only requests the
    // taller pane when the zoom is worth it, so this usually lands on the
    // scale the layout was sized for.
    let scale = if area.width >= level.width as u16 * 2 + 2
        && area.height >= level.height as u16 + 2
    {
        2
    } else {
        1
    };
    let outer_w = (level.width * scale) as u16 + 2;
    let outer_h = (level.height * scale).div_ceil(2) as u16 + 2;

    let arena = Rect {
        x: area.x + (area.width - outer_w) / 2,
        y: area.y + (area.height - outer_h) / 2,
        width: outer_w,
        height: outer_h,
    };

    let border_color = match snapshot.phase {
        SsnakePhase::Running => theme::AMBER(),
        SsnakePhase::Finished => theme::SUCCESS(),
        SsnakePhase::Waiting => theme::BORDER(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(status_line(snapshot))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(ARENA_BG));
    let inner = block.inner(arena);
    frame.render_widget(block, arena);
    frame.render_widget(Paragraph::new(board_lines(snapshot, level, scale)), inner);

    if snapshot.phase == SsnakePhase::Finished {
        let (heading, subtitle, color) = outcome_overlay(snapshot);
        draw_game_overlay(frame, inner, heading, &subtitle, color);
    }
}

fn waiting_lines(state: &State) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let seated = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
    vec![
        Line::raw(""),
        Line::from(Span::styled(
            "S U P E R   S N A K E",
            Style::default().fg(GREEN_HEAD).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!(
                "Up to {} snakes, one arena, shared food.",
                snapshot.seat_limit
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            format!("{} · {} pace", snapshot.arena_choice, snapshot.speed_label),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            if seated >= 2 {
                "Press n to start."
            } else {
                "Press s to take a seat."
            },
            Style::default().fg(theme::AMBER()),
        )),
    ]
}

/// Render two virtual rows per terminal line with the upper-half block:
/// foreground paints the top cell, background the bottom cell. Every cell,
/// food included, is a plain colored half block; text glyphs are full
/// terminal-cell height and misalign with this grid. `scale` stretches each
/// arena cell to a scale x scale block of virtual cells (2 = the zoom).
fn board_lines(snapshot: &SsnakeSnapshot, level: &SsnakeLevel, scale: usize) -> Vec<Line<'static>> {
    let colors = cell_colors(snapshot, level);
    let virtual_w = level.width * scale;
    let virtual_h = level.height * scale;
    let color_at = |x: usize, y: usize| {
        if y < virtual_h {
            colors[y / scale * level.width + x / scale]
        } else {
            ARENA_BG
        }
    };
    let rows = virtual_h.div_ceil(2);
    let mut lines = Vec::with_capacity(rows);
    for row in 0..rows {
        let top_y = row * 2;
        let bottom_y = top_y + 1;
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(level.width);
        let mut run_start = 0usize;
        for x in 0..=virtual_w {
            let same_run = x < virtual_w
                && x > run_start
                && color_at(x, top_y) == color_at(run_start, top_y)
                && color_at(x, bottom_y) == color_at(run_start, bottom_y);
            if x == run_start || same_run {
                continue;
            }
            spans.push(Span::styled(
                "▀".repeat(x - run_start),
                Style::default()
                    .fg(color_at(run_start, top_y))
                    .bg(color_at(run_start, bottom_y)),
            ));
            run_start = x;
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn cell_colors(snapshot: &SsnakeSnapshot, level: &SsnakeLevel) -> Vec<Color> {
    let mut colors = vec![ARENA_BG; level.width * level.height];
    for y in 0..level.height {
        for x in 0..level.width {
            colors[y * level.width + x] = match level.cell(x, y) {
                Cell::Empty => ARENA_BG,
                Cell::Wall => WALL,
                Cell::Warp => WARP,
            };
        }
    }
    if let Some(point) = snapshot.point {
        let blink = snapshot.tick_count % 2 == 0;
        colors[point.y as usize * level.width + point.x as usize] = if snapshot.life_point {
            if blink { LIFE_POINT } else { LIFE_POINT_BLINK }
        } else {
            POINT
        };
    }
    for seat_index in 0..MAX_SEATS {
        let player = &snapshot.players[seat_index];
        for segment in player.body.iter().skip(1) {
            paint(&mut colors, level, *segment, body_color(seat_index));
        }
    }
    // Heads last so they stay visible over walls and the other body while a
    // fresh collision plays out its death shrink.
    for seat_index in 0..MAX_SEATS {
        let player = &snapshot.players[seat_index];
        if let Some(head) = player.body.first().copied() {
            let color = match player.motion {
                Motion::Dying => body_color(seat_index),
                Motion::Idle if snapshot.tick_count % 2 == 0 => body_color(seat_index),
                _ => head_color(seat_index),
            };
            paint(&mut colors, level, head, color);
        }
    }
    colors
}

fn paint(colors: &mut [Color], level: &SsnakeLevel, pos: Pos, color: Color) {
    let index = pos.y as usize * level.width + pos.x as usize;
    if index < colors.len() {
        colors[index] = color;
    }
}

// ── Info sidebar ───────────────────────────────────────────────

fn info_lines(state: &State, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let mut lines = vec![
        info_tagline("The 90s DOS classic,"),
        info_tagline("now head to head."),
        Line::raw(""),
        section_header("Snakes"),
    ];
    for seat in 0..snapshot.seat_limit.min(MAX_SEATS) {
        lines.extend(player_lines(seat, state, usernames));
    }
    lines.push(Line::raw(""));
    let arena_name = match (snapshot.phase, snapshot.level.as_ref()) {
        (SsnakePhase::Running, Some(level)) => level.name.clone(),
        _ => snapshot.arena_choice.clone(),
    };
    lines.push(info_label_value("Arena", arena_name, theme::TEXT_BRIGHT()));
    if snapshot.phase == SsnakePhase::Running {
        lines.push(info_label_value(
            "Food left",
            snapshot.points_left.max(0).to_string(),
            POINT,
        ));
    }
    lines.extend([
        info_label_value("Pace", snapshot.speed_label.clone(), theme::AMBER()),
        info_label_value("Prize", SSNAKE_WIN_CHIPS.to_string(), theme::SUCCESS()),
        info_label_value(
            "Cooldown",
            payout_cooldown_label(SSNAKE_WIN_PAYOUT_COOLDOWN),
            theme::TEXT_DIM(),
        ),
        info_label_value("State", state_label(snapshot), theme::SUCCESS()),
        Line::raw(""),
        section_header("How it plays"),
        info_tagline("Eat food, grow, don't crash."),
        info_tagline("Pink food grants a life."),
        info_tagline("Crash: lose a life, respawn."),
        info_tagline("Last food ends the match;"),
        info_tagline("highest score wins."),
        Line::raw(""),
        section_header("Controls"),
    ]);
    if state.seat_index().is_some() {
        if snapshot.phase == SsnakePhase::Running {
            lines.push(key_hint("arrows/wasd", "steer"));
        } else {
            lines.push(key_hint("arrows/[ ]", "choose arena"));
            lines.push(key_hint("n", "start match"));
        }
        lines.extend([key_hint("l", "leave seat"), key_hint("q", "leave room")]);
    } else {
        lines.extend([
            key_hint("s/space", "take a seat"),
            key_hint("q", "leave room"),
        ]);
    }
    lines
}

/// Two lines per seated player during a match; one line while waiting.
fn player_lines(seat: usize, state: &State, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let color = SsnakeColor::for_seat(seat);
    let user = snapshot.seats[seat];
    let is_self = user.is_some_and(|uid| state.is_self(uid));
    let name = match user {
        Some(uid) => usernames
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| "snake".to_string()),
        None => "open".to_string(),
    };
    let player = &snapshot.players[seat];

    // Row 1: marker + color label + name.
    let name_line = Line::from(vec![
        Span::styled(
            if is_self { "> " } else { "  " },
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(
            format!("{:<7}", color.label()),
            Style::default()
                .fg(head_color_of(color))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(name, player_name_style(user, is_self)),
    ]);

    // Row 2 (active match only): score left, hearts right — own clean row so
    // name length never competes with the hearts.
    if snapshot.phase != SsnakePhase::Waiting && user.is_some() {
        let stats_line = if player.eliminated {
            Line::from(Span::styled(
                "  eliminated".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ))
        } else {
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                format!("  {} pts  ", player.score),
                Style::default().fg(theme::TEXT_DIM()),
            )];
            spans.extend(heart_spans(player.lives, snapshot));
            Line::from(spans)
        };
        vec![name_line, stats_line]
    } else {
        vec![name_line]
    }
}

/// Lives as hearts, Traffic-style: filled red hearts for remaining lives,
/// hollow dim hearts for lives lost off the level's starting count.
fn heart_spans(lives: i32, snapshot: &SsnakeSnapshot) -> Vec<Span<'static>> {
    const MAX_HEART_GLYPHS: i32 = 8;
    let lives = lives.max(0);
    let max_lives = snapshot
        .level
        .as_ref()
        .map(|level| level.lives)
        .unwrap_or(lives);
    let lost = max_lives.saturating_sub(lives).max(0);

    if lives + lost > MAX_HEART_GLYPHS {
        return vec![Span::styled(
            format!("♥ x{lives}"),
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )];
    }
    vec![
        Span::styled(
            "♥ ".repeat(lives as usize),
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "♡ ".repeat(lost as usize),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]
}

fn player_name_style(user: Option<Uuid>, is_self: bool) -> Style {
    if is_self {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if user.is_some() {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    }
}

fn section_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

// ── Status / keys / overlay ────────────────────────────────────

/// Rendered as the arena border title so the board itself stays clean.
fn status_line(snapshot: &SsnakeSnapshot) -> Line<'static> {
    let color = match snapshot.phase {
        SsnakePhase::Running => theme::AMBER(),
        SsnakePhase::Finished => theme::SUCCESS(),
        SsnakePhase::Waiting => theme::TEXT_DIM(),
    };
    Line::from(Span::styled(
        format!(" {} ", status_text(snapshot)),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

fn status_text(snapshot: &SsnakeSnapshot) -> String {
    match snapshot.outcome {
        Some(SsnakeOutcome::Winner { seat_index }) => {
            format!("{} wins", SsnakeColor::for_seat(seat_index).label())
        }
        Some(SsnakeOutcome::Draw) => "Draw".to_string(),
        // The sidebar already names the arena; keep the title to the one
        // number that matters mid-match.
        None if snapshot.phase == SsnakePhase::Running => {
            format!("{} food left", snapshot.points_left.max(0))
        }
        None => snapshot.status_message.clone(),
    }
}

fn state_label(snapshot: &SsnakeSnapshot) -> String {
    match snapshot.outcome {
        Some(SsnakeOutcome::Winner { seat_index }) => {
            format!("{} won", SsnakeColor::for_seat(seat_index).label())
        }
        Some(SsnakeOutcome::Draw) => "draw".to_string(),
        None => match snapshot.phase {
            SsnakePhase::Running => "running".to_string(),
            SsnakePhase::Waiting => "waiting".to_string(),
            SsnakePhase::Finished => "finished".to_string(),
        },
    }
}

fn outcome_overlay(snapshot: &SsnakeSnapshot) -> (&'static str, String, Color) {
    match snapshot.outcome {
        Some(SsnakeOutcome::Winner { seat_index }) => (
            "Winner",
            format!(
                "{} wins · press n",
                SsnakeColor::for_seat(seat_index).label()
            ),
            theme::SUCCESS(),
        ),
        Some(SsnakeOutcome::Draw) => (
            "Draw",
            "dead even · press n".to_string(),
            theme::TEXT_MUTED(),
        ),
        None => (
            "Match over",
            "press n to play again".to_string(),
            theme::AMBER(),
        ),
    }
}

fn key_line(state: &State) -> Line<'static> {
    let seated = state.seat_index().is_some();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };

    let mut spans = Vec::new();
    if seated {
        hint(&mut spans, "arrows/wasd", "steer");
        hint(&mut spans, "n", "start");
        hint(&mut spans, "l", "leave seat");
    } else {
        hint(&mut spans, "s/space", "take a seat");
    }
    hint(&mut spans, "q", "leave room");

    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

// ── Seat colours ───────────────────────────────────────────────

fn head_color(seat: usize) -> Color {
    head_color_of(SsnakeColor::for_seat(seat))
}

fn head_color_of(color: SsnakeColor) -> Color {
    match color {
        SsnakeColor::Green => GREEN_HEAD,
        SsnakeColor::Red => RED_HEAD,
        SsnakeColor::Blue => BLUE_HEAD,
        SsnakeColor::Purple => PURPLE_HEAD,
    }
}

fn body_color(seat: usize) -> Color {
    match SsnakeColor::for_seat(seat) {
        SsnakeColor::Green => GREEN_BODY,
        SsnakeColor::Red => RED_BODY,
        SsnakeColor::Blue => BLUE_BODY,
        SsnakeColor::Purple => PURPLE_BODY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::lobby::house::ssnake::levels::open_test_arena;
    use crate::app::lobby::house::ssnake::svc::SsnakePlayerSnapshot;
    use std::sync::Arc;

    fn empty_player() -> SsnakePlayerSnapshot {
        SsnakePlayerSnapshot {
            body: Vec::new(),
            motion: Motion::Idle,
            lives: 0,
            score: 0,
            eliminated: false,
            in_round: false,
        }
    }

    fn snapshot_with_level(level: SsnakeLevel) -> SsnakeSnapshot {
        SsnakeSnapshot {
            room_id: Uuid::nil(),
            seats: [None; MAX_SEATS],
            seat_limit: 2,
            level: Some(Arc::new(level)),
            arena_choice: "random arena".to_string(),
            players: [
                SsnakePlayerSnapshot {
                    body: vec![Pos { x: 2, y: 2 }, Pos { x: 3, y: 2 }],
                    motion: Motion::Moving(crate::app::lobby::house::ssnake::state::Direction::Left),
                    lives: 3,
                    score: 0,
                    eliminated: false,
                    in_round: true,
                },
                SsnakePlayerSnapshot {
                    body: vec![Pos { x: 5, y: 5 }],
                    motion: Motion::Idle,
                    lives: 3,
                    score: 0,
                    eliminated: false,
                    in_round: true,
                },
                empty_player(),
                empty_player(),
            ],
            point: Some(Pos { x: 7, y: 7 }),
            life_point: false,
            points_left: 5,
            phase: SsnakePhase::Running,
            outcome: None,
            status_message: "test".to_string(),
            speed_label: "classic".to_string(),
            tick_count: 1,
        }
    }

    #[test]
    fn board_lines_cover_full_arena_width() {
        let level = open_test_arena(30, 21);
        let snapshot = snapshot_with_level(level.clone());
        let lines = board_lines(&snapshot, &level, 1);
        assert_eq!(lines.len(), 11, "21 rows pack into 11 half-block lines");
        for line in &lines {
            let width: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert_eq!(width, level.width);
        }
    }

    #[test]
    fn zoomed_board_doubles_every_cell() {
        let level = open_test_arena(30, 21);
        let snapshot = snapshot_with_level(level.clone());
        let lines = board_lines(&snapshot, &level, 2);
        assert_eq!(lines.len(), 21, "42 virtual rows pack into 21 lines");
        for line in &lines {
            let width: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert_eq!(width, level.width * 2);
        }
        // Each terminal line covers exactly one arena row at 2x, so the
        // half-block fg and bg agree everywhere; the green head at (2, 2)
        // spans virtual columns 4-5 on line 2.
        let line = &lines[2];
        let mut x = 0usize;
        let mut found = false;
        for span in &line.spans {
            let len = span.content.chars().count();
            if x <= 4 && 4 < x + len {
                assert_eq!(span.style.fg, Some(GREEN_HEAD));
                assert_eq!(span.style.bg, Some(GREEN_HEAD));
                found = true;
            }
            x += len;
        }
        assert!(found, "head span missing on the zoomed line");
    }

    #[test]
    fn zoom_asks_for_taller_pane_only_when_it_fits() {
        let level = open_test_arena(30, 21);
        let wide = Rect::new(0, 0, 120, 50);
        let narrow = Rect::new(0, 0, 80, 50);
        let short = Rect::new(0, 0, 120, 30);
        assert!(zoom_eligible(&level, wide));
        assert!(!zoom_eligible(&level, narrow), "2x + sidebar needs 90 cols");
        assert!(!zoom_eligible(&level, short), "chat must keep its floor");
    }

    #[test]
    fn hearts_show_remaining_and_lost_lives() {
        // Test arena starts with 3 lives; 2 left = 2 filled + 1 hollow.
        let snapshot = snapshot_with_level(open_test_arena(30, 20));
        let spans = heart_spans(2, &snapshot);
        assert_eq!(spans[0].content, "♥ ♥ ");
        assert_eq!(spans[1].content, "♡ ");

        // Extra lives from life points never render negative hollow hearts.
        let spans = heart_spans(5, &snapshot);
        assert_eq!(spans[0].content, "♥ ♥ ♥ ♥ ♥ ");
        assert_eq!(spans[1].content, "");

        // Absurd totals collapse to a count.
        let spans = heart_spans(12, &snapshot);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "♥ x12");
    }

    #[test]
    fn cell_colors_layer_snakes_over_floor() {
        let level = open_test_arena(30, 20);
        let snapshot = snapshot_with_level(level.clone());
        let colors = cell_colors(&snapshot, &level);
        assert_eq!(colors[2 * level.width + 2], GREEN_HEAD);
        assert_eq!(colors[2 * level.width + 3], GREEN_BODY);
        assert_eq!(colors[7 * level.width + 7], POINT);
        assert_eq!(colors[0], WALL);
        assert_eq!(colors[level.width + 1], ARENA_BG);
    }
}
