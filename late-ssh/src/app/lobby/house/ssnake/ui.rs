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
            key_hint,
        },
        ssnake::{
            levels::{Cell, MAX_HEIGHT, SsnakeLevel},
            settings::{
                SSNAKE_BONUS_FOOD_MULTIPLIER, SSNAKE_CLEAR_CHIPS, SSNAKE_CRASH_CHIPS,
                SSNAKE_EDGE_BONUS_CHIPS, SSNAKE_FOOD_CHIPS,
            },
            state::{MAX_SEATS, Motion, Pos, SsnakeColor, SsnakePhase, State},
            svc::{SsnakeChipKind, SsnakePlayerSnapshot, SsnakeSnapshot},
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
/// Usable columns inside the sidebar: `draw_info_sidebar` indents by 2 off
/// its own left border. Every line is fitted to this so none of them wrap,
/// which is what lets `sidebar_height` count rows exactly.
const SIDEBAR_TEXT_WIDTH: usize = SIDEBAR_WIDTH as usize - 2;

/// Truncate to `width` columns, marking the cut so a clipped name does not
/// read as someone's actual handle.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

// ── Arena palette ──────────────────────────────────────────────
// Very block, up to five snakes. Walls keep the DOS brick-brown of the
// original; warp tunnels hint in dim blue.
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
// The fifth snake sits in blue-green, far enough from both the yellow-green
// of seat one and the sky blue of seat three to read at a glance.
const CYAN_HEAD: Color = Color::Rgb(86, 240, 226);
const CYAN_BODY: Color = Color::Rgb(30, 140, 134);
// Food reads on two independent channels so the two bonuses never get
// mistaken for each other: hue says what it pays (gold plain, pink triple),
// and pulsing is reserved for the one that ends the lap. Nothing else on the
// board blinks.
const POINT: Color = Color::Rgb(255, 200, 84);
const BONUS_FOOD: Color = Color::Rgb(255, 108, 198);
const FINAL_FOOD: Color = Color::Rgb(255, 116, 24);
const FINAL_FOOD_BLINK: Color = Color::Rgb(255, 248, 220);

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
    let arena_need = arena_rows + 4;

    // The sidebar is unscrolled, so the only way to guarantee its text is
    // readable is to ask for a pane tall enough to hold it. Chat keeps its
    // floor either way — this table's chat is the least-used part of it, but
    // it must not vanish.
    let need = arena_need.max(sidebar_height(state));
    let ceiling = area
        .height
        .saturating_sub(CHAT_FLOOR)
        .max(arena_need.min(area.height));
    need.min(ceiling).min(area.height.max(1)).max(1)
}

/// Rows the sidebar wants: every line it would render, plus nothing. Lines
/// are built for real rather than counted by hand so the two can never drift
/// — and because every line is truncated to the sidebar width, none of them
/// wrap, which makes the count exact.
fn sidebar_height(state: &State) -> u16 {
    let empty = std::collections::HashMap::new();
    let lookup = UsernameLookup::new(&empty, None);
    info_lines(state, &lookup).len() as u16
}

/// Rows always left to the embedded chat plus its spacer, however much the
/// game would like. Deliberately small: people come to this table to play,
/// and the chat is one keypress from the room's own screen.
const CHAT_FLOOR: u16 = 6;

/// Rows the embedded chat keeps when the zoomed arena claims the rest of
/// the screen (plus the one-row spacer above it).
const ZOOM_CHAT_FLOOR: u16 = CHAT_FLOOR;

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
        Line::from(format!(
            "this food: {} · crash -{SSNAKE_CRASH_CHIPS}",
            snapshot.food_chips()
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
    let scale =
        if area.width >= level.width as u16 * 2 + 2 && area.height >= level.height as u16 + 2 {
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
        SsnakePhase::Idle => theme::BORDER(),
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(status_line(snapshot))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(ARENA_BG));
    // Payout pops ride the bottom border, out of the playfield: arcade
    // feedback that never covers a cell you are about to steer into.
    if let Some(pops) = chip_pop_line(state) {
        block = block.title_bottom(pops.right_aligned());
    }
    let inner = block.inner(arena);
    frame.render_widget(block, arena);
    frame.render_widget(Paragraph::new(board_lines(snapshot, level, scale)), inner);

    // The countdown goes over the board, not off in the sidebar: it is the
    // one moment the player has nothing to do but look at the new arena.
    if snapshot.is_frozen() {
        let (heading, color) = countdown_overlay(snapshot);
        draw_game_overlay(frame, inner, heading, &level.name, color);
    }
}

/// The post-shuffle countdown, one second a step: 3, 2, 1, then GO on the
/// last second before steering unlocks.
fn countdown_overlay(snapshot: &SsnakeSnapshot) -> (&'static str, Color) {
    match snapshot.freeze_millis_left {
        millis if millis > 3_000 => ("3", theme::AMBER()),
        millis if millis > 2_000 => ("2", theme::AMBER()),
        millis if millis > 1_000 => ("1", theme::AMBER()),
        _ => ("GO", theme::SUCCESS()),
    }
}

/// Fallback splash for the arena pane when no level could be loaded at all
/// (every level asset failed to parse). The arena normally always has a
/// board, even with nobody sitting at it.
fn waiting_lines(state: &State) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    vec![
        Line::raw(""),
        Line::from(Span::styled(
            "S U P E R   S N A K E",
            Style::default().fg(GREEN_HEAD).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!(
                "Up to {} snakes, one endless arena, shared food.",
                snapshot.seat_limit
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "No arena could be loaded.",
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
        colors[point.y as usize * level.width + point.x as usize] = food_color(snapshot);
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
                Motion::Idle if snapshot.tick_count.is_multiple_of(2) => body_color(seat_index),
                _ => head_color(seat_index),
            };
            paint(&mut colors, level, head, color);
        }
    }
    colors
}

/// Which food is on the board, in one glance. Only the lap-ending food
/// pulses — pink sits still, so a blinking cell always means "this one
/// reshuffles the arena" and never "this one pays triple".
fn food_color(snapshot: &SsnakeSnapshot) -> Color {
    if snapshot.is_final_food() {
        return if snapshot.tick_count.is_multiple_of(2) {
            FINAL_FOOD
        } else {
            FINAL_FOOD_BLINK
        };
    }
    if snapshot.bonus_food {
        return BONUS_FOOD;
    }
    POINT
}

fn paint(colors: &mut [Color], level: &SsnakeLevel, pos: Pos, color: Color) {
    let index = pos.y as usize * level.width + pos.x as usize;
    if index < colors.len() {
        colors[index] = color;
    }
}

// ── Info sidebar ───────────────────────────────────────────────

/// The sidebar is a plain unscrolled `Paragraph`, so anything past the pane
/// height is silently clipped. Order is therefore load-bearing: the keys go
/// first (a newcomer needs them before anything else), then who is playing,
/// then the live payout numbers, and only then the rules — which also live
/// in the `?` guide, so they are the safe thing to lose off the bottom.
fn info_lines(state: &State, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let moving = snapshot.moving_snakes;
    let mut lines = control_lines(state);
    lines.push(Line::raw(""));
    lines.push(section_header("Snakes"));

    // Every seat gets its colour row, taken or not: the roster is how a
    // spectator learns which snake would be theirs, and the pane is now
    // sized to fit it.
    for seat in 0..snapshot.seat_limit.min(MAX_SEATS) {
        lines.extend(player_lines(seat, state, usernames));
    }

    lines.push(Line::raw(""));
    lines.push(info_label_value(
        "Arena",
        fit(
            snapshot
                .level
                .as_ref()
                .map(|level| level.name.as_str())
                .unwrap_or("none"),
            SIDEBAR_TEXT_WIDTH - 11,
        ),
        theme::TEXT_BRIGHT(),
    ));
    lines.extend([
        info_label_value("Food left", snapshot.points_left.max(0).to_string(), POINT),
        info_label_value("Pace", snapshot.speed_label.clone(), theme::AMBER()),
        // The multiplier is the whole point of the room, so it gets the live
        // arithmetic spelled out rather than a bare rate.
        info_label_value(
            "Moving now",
            format!("{moving} snake{}", if moving == 1 { "" } else { "s" }),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "This food",
            snapshot.food_chips().to_string(),
            theme::SUCCESS(),
        ),
        info_tagline(&food_breakdown(snapshot)),
        info_label_value(
            "Arena clear",
            format!(
                "{SSNAKE_CLEAR_CHIPS} × {} = {}",
                moving.max(1),
                snapshot.clear_chips()
            ),
            theme::SUCCESS(),
        ),
        info_label_value("Crash", format!("-{SSNAKE_CRASH_CHIPS}"), theme::ERROR()),
    ]);
    if let Some(skip) = skip_status(snapshot) {
        lines.push(skip);
    }
    lines.extend([
        Line::raw(""),
        section_header("House rules"),
        info_tagline("Food pays times the MOVING"),
        info_tagline("snakes — parked ones count"),
        info_tagline("for nobody."),
        info_tagline(&format!(
            "Pink {SSNAKE_BONUS_FOOD_MULTIPLIER}x, +{SSNAKE_EDGE_BONUS_CHIPS} per wall it"
        )),
        info_tagline("touches. Blinking orange food"),
        info_tagline("ends the arena for a bonus."),
        info_tagline(&format!("Crash -{SSNAKE_CRASH_CHIPS}. Standing up while")),
        info_tagline("moving costs the same — no"),
        info_tagline("bailing out of a crash."),
        info_tagline("Your take is pending until"),
        info_tagline("you stand up; that is when"),
        info_tagline("it reaches your balance."),
    ]);
    lines
}

fn control_lines(state: &State) -> Vec<Line<'static>> {
    control_block(state.seat_index().is_some())
}

/// The skip tally, or why the key will not do anything right now. `None`
/// while the table is empty and there is nobody to reach consensus with.
fn skip_status(snapshot: &SsnakeSnapshot) -> Option<Line<'static>> {
    let (cast, seated) = snapshot.skip_tally();
    if seated == 0 {
        return None;
    }
    if snapshot.skip_cooldown_millis_left > 0 {
        return Some(info_label_value(
            "Skip vote",
            format!("in {}s", snapshot.skip_cooldown_secs_left()),
            theme::TEXT_DIM(),
        ));
    }
    Some(info_label_value(
        "Skip vote",
        format!("{cast}/{seated}"),
        if cast > 0 {
            theme::AMBER()
        } else {
            theme::TEXT_DIM()
        },
    ))
}

/// The whole keybinding guide, and deliberately the same four rows whether
/// or not you are seated: a spectator deciding whether to join needs to see
/// what they would be able to do, not just how to sit.
fn control_block(seated: bool) -> Vec<Line<'static>> {
    // Dim the row that is not available right now rather than hiding it, so
    // the block never changes shape under the reader.
    let hint = |key: &str, desc: &str, available: bool| {
        if available {
            key_hint(key, desc)
        } else {
            Line::from(Span::styled(
                format!("{key:<12}{desc}"),
                Style::default().fg(theme::TEXT_FAINT()),
            ))
        }
    };
    vec![
        section_header("Controls"),
        hint("s / space", "sit down", !seated),
        hint("arrows/wasd", "steer", seated),
        hint("v", "vote skip", seated),
        hint("l", "stand up", seated),
        key_hint("q", "leave table"),
        info_tagline("Your snake waits until you"),
        info_tagline("steer it. No U-turns."),
    ]
}

/// Where the number above came from, in one 28-column line: the base, the
/// wall-edge bonus this particular food happens to carry, the pink multiple,
/// and the moving-snake count. Terms that are not in play are left out so
/// the common case reads as short as it is.
fn food_breakdown(snapshot: &SsnakeSnapshot) -> String {
    let mut parts = vec![format!("  {SSNAKE_FOOD_CHIPS}")];
    if snapshot.food_wall_edges > 0 {
        parts.push(format!(
            "+{} edge",
            SSNAKE_EDGE_BONUS_CHIPS * snapshot.food_wall_edges
        ));
    }
    if snapshot.bonus_food {
        parts.push(format!("×{SSNAKE_BONUS_FOOD_MULTIPLIER} pink"));
    }
    if snapshot.moving_snakes > 1 {
        parts.push(format!("×{} moving", snapshot.moving_snakes));
    }
    parts.join(" ")
}

/// Two lines per seated player: who they are, then what the arena has paid
/// them since they sat down and whether they are actually slithering.
fn player_lines(seat: usize, state: &State, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let color = SsnakeColor::for_seat(seat);
    let user = snapshot.seats[seat];
    let is_self = user.is_some_and(|uid| state.is_self(uid));
    // Row 1 spends 2 cols on the self marker and 7 on the colour, so a long
    // handle has to be cut: letting it wrap would silently add a row and
    // push the bottom of the sidebar off the pane.
    let name = fit(
        &match user {
            Some(uid) => usernames
                .get(&uid)
                .cloned()
                .unwrap_or_else(|| "snake".to_string()),
            None => "open".to_string(),
        },
        SIDEBAR_TEXT_WIDTH - 9,
    );
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

    // Row 2 (seated only): session earnings and whether this snake is
    // counting toward everyone's multiplier.
    if user.is_some() {
        vec![name_line, earnings_line(player, snapshot.skip_votes[seat])]
    } else {
        vec![name_line]
    }
}

/// What the seat has run up since sitting down, plus the multiplier status.
/// The word is "pending" on purpose: none of it is in the player's balance
/// until they stand up, and a counter that read "chips" next to a number they
/// cannot spend yet would be a lie. `parked` is the one that matters
/// socially: it is why a full table can still be paying the lone rate.
fn earnings_line(player: &SsnakePlayerSnapshot, voted_skip: bool) -> Line<'static> {
    let (state_text, state_color) = match player.motion {
        Motion::Moving(_) => ("moving", theme::SUCCESS()),
        Motion::Dying => ("crashed", theme::ERROR()),
        Motion::Idle => ("parked", theme::TEXT_DIM()),
    };
    let chips_color = if player.chips < 0 {
        theme::ERROR()
    } else {
        theme::TEXT_DIM()
    };
    let mut spans = vec![
        Span::styled(
            format!("  {:+} pending  ", player.chips),
            Style::default().fg(chips_color),
        ),
        Span::styled(state_text.to_string(), Style::default().fg(state_color)),
    ];
    // Who is already waiting on the rest of the table to agree.
    if voted_skip {
        spans.push(Span::styled(
            " skip".to_string(),
            Style::default().fg(theme::AMBER()),
        ));
    }
    Line::from(spans)
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
    let color = if snapshot.is_frozen() {
        theme::SUCCESS()
    } else {
        match snapshot.phase {
            SsnakePhase::Running => theme::AMBER(),
            SsnakePhase::Idle => theme::TEXT_DIM(),
        }
    };
    Line::from(Span::styled(
        format!(" {} ", status_text(snapshot)),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

/// The arena border title. While the board is turning it carries the two
/// live numbers a player needs — food remaining and what the one on the
/// board pays; during the post-shuffle hold it counts down instead, and
/// idle it falls back to the last thing that happened.
fn status_text(snapshot: &SsnakeSnapshot) -> String {
    // The count itself is on the board; the title just says why nothing moves.
    if snapshot.is_frozen() {
        return "New arena — get ready".to_string();
    }
    if snapshot.phase == SsnakePhase::Running {
        if snapshot.is_final_food() {
            return format!(
                "LAST food · {} chips + {} clear",
                snapshot.food_chips(),
                snapshot.clear_chips()
            );
        }
        // "each" would be a lie: every food is priced on its own walls, and
        // the pink roll, so the title quotes the one actually on the board.
        return format!(
            "{} food left · this one: {} chips",
            snapshot.points_left.max(0),
            snapshot.food_chips()
        );
    }
    snapshot.status_message.clone()
}

/// The live payout pops, oldest first, as one right-aligned border strip.
/// `None` while nothing has landed recently, which is most frames.
fn chip_pop_line(state: &State) -> Option<Line<'static>> {
    let pops = state.chip_pops();
    if pops.is_empty() {
        return None;
    }
    let mut spans = vec![Span::raw(" ")];
    for pop in pops {
        let color = if pop.delta < 0 {
            theme::ERROR()
        } else {
            theme::SUCCESS()
        };
        spans.push(Span::styled(
            format!("{:+}{} ", pop.delta, chip_pop_suffix(pop.kind)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    Some(Line::from(spans))
}

/// What the pop calls itself. Plain food is the common case and stays bare,
/// so the eye only stops on the ones worth stopping for.
fn chip_pop_suffix(kind: SsnakeChipKind) -> &'static str {
    match kind {
        SsnakeChipKind::Food => "",
        SsnakeChipKind::BonusFood => " pink",
        SsnakeChipKind::ArenaClear => " cleared",
        SsnakeChipKind::Crash => " crash",
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

    // One row, so unlike the sidebar block this one only carries the keys
    // that do something right now.
    let mut spans = Vec::new();
    if seated {
        hint(&mut spans, "arrows/wasd", "steer");
        hint(&mut spans, "l", "stand up");
    } else {
        hint(&mut spans, "s/space", "sit down");
    }
    hint(&mut spans, "q", "leave table");

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
        SsnakeColor::Cyan => CYAN_HEAD,
    }
}

fn body_color(seat: usize) -> Color {
    match SsnakeColor::for_seat(seat) {
        SsnakeColor::Green => GREEN_BODY,
        SsnakeColor::Red => RED_BODY,
        SsnakeColor::Blue => BLUE_BODY,
        SsnakeColor::Purple => PURPLE_BODY,
        SsnakeColor::Cyan => CYAN_BODY,
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
