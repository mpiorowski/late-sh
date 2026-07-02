//! Clubhouse renderer: the tavern viewport (camera-follow over the floor
//! plan, live occupants, animated fire/jukebox/dog/candles, proximity
//! popovers) with the embedded #lounge chat pinned to the bottom of the
//! screen. Dwarf Fortress vibes, single-width glyphs only: walking people
//! are 3-row stick figures (`o` head, `/|\` arms, `Λ` legs; you get an `@`),
//! and a seated user is an `o` perched on their stool.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;
use crate::app::files::terminal_image::TerminalImageFrame;
use late_core::api_types::NowPlaying;

use super::map;
use super::state::State;

const LABEL_MAX: usize = 10;
const FIRE_CHARS: [char; 6] = ['(', ')', '~', '^', '*', '\''];
const EQ_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Phosphor pixels for the arcade cabinet's attract mode.
const SCREEN_CHARS: [char; 4] = ['▀', '▄', '·', ' '];

pub struct ClubhouseView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    pub now_playing: Option<&'a NowPlaying>,
    pub chat: Option<crate::app::chat::ui::EmbeddedRoomChatView<'a>>,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: ClubhouseView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    // Bottom ~40% is the live #lounge; the tavern gets the rest.
    let chat_height = ((u32::from(area.height) * 2 / 5) as u16)
        .max(8)
        .min(area.height.saturating_sub(8));
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(chat_height)]).split(area);

    draw_tavern(frame, layout[0], &view);

    if let Some(chat) = view.chat {
        crate::app::chat::ui::draw_embedded_room_chat(frame, layout[1], chat, terminal_images);
    }
}

fn draw_tavern(frame: &mut Frame, area: Rect, view: &ClubhouseView<'_>) {
    let state = view.state;
    // No widget border: the room's own walls are the frame. The headcount
    // and keybinds live in the app frame title (`app_frame_title` in
    // `render.rs`), so the tavern gets every cell.
    let inner = area;
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let mut cells = styled_base_grid();
    animate(&mut cells, view);
    place_people(&mut cells, view);

    // Camera: follow the player, clamped to the map; center when the
    // viewport is larger than the room.
    let vw = usize::from(inner.width);
    let vh = usize::from(inner.height);
    let map_w = usize::from(map::MAP_W);
    let map_h = usize::from(map::MAP_H);
    let cam_x = camera_origin(usize::from(state.player_x), vw, map_w);
    let cam_y = camera_origin(usize::from(state.player_y), vh, map_h);
    let pad_x = vw.saturating_sub(map_w) / 2;
    let pad_y = vh.saturating_sub(map_h) / 2;

    let mut lines: Vec<Line> = Vec::with_capacity(vh);
    for _ in 0..pad_y {
        lines.push(Line::default());
    }
    for row in cells.iter().skip(cam_y).take(vh.saturating_sub(pad_y)) {
        let mut spans: Vec<Span> = Vec::with_capacity(vw);
        if pad_x > 0 {
            spans.push(Span::raw(" ".repeat(pad_x)));
        }
        for &(ch, style) in row.iter().skip(cam_x).take(vw.saturating_sub(pad_x)) {
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    draw_popover(frame, inner, view);
}

fn camera_origin(player: usize, viewport: usize, map_len: usize) -> usize {
    if viewport >= map_len {
        return 0;
    }
    player.saturating_sub(viewport / 2).min(map_len - viewport)
}

type Cells = Vec<Vec<(char, Style)>>;

fn styled_base_grid() -> Cells {
    map::grid()
        .iter()
        .enumerate()
        .map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(|(x, &ch)| (ch, base_style(ch, x as u16, y as u16)))
                .collect()
        })
        .collect()
}

fn base_style(ch: char, x: u16, y: u16) -> Style {
    let dim = Style::default().fg(theme::TEXT_DIM());
    // The sign over the door.
    if y == 0 && !matches!(ch, '═' | '╔' | '╗') {
        return match ch {
            '☾' | '☽' => Style::default().fg(theme::AMBER_GLOW()),
            '╡' | '╞' => dim,
            _ => Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        };
    }
    // The back-bar shelf: every bottle body gets its own liquor glint.
    if map::BACK_BAR.contains(x, y) {
        return match ch {
            '█' => Style::default().fg(hashed_color(x, y, BOTTLE_PALETTE)),
            _ => Style::default().fg(theme::TEXT_MUTED()),
        };
    }
    // The neon house sign burns over the north wall.
    if map::NEON_SIGN.contains(x, y) {
        return match ch {
            '╭' | '╮' | '╰' | '╯' | '─' | '│' => Style::default().fg(theme::ERROR()),
            _ => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        };
    }
    // Moonlight in the windows.
    if map::WINDOWS.iter().any(|w| w.contains(x, y)) {
        return match ch {
            '☾' => Style::default().fg(theme::AMBER_GLOW()),
            '·' | '*' => Style::default().fg(theme::TEXT_MUTED()),
            _ => dim,
        };
    }
    // The dog is the same warm amber as the rest of the hearth corner.
    if map::DOG_ZONE.contains(x, y) {
        return Style::default().fg(theme::AMBER());
    }
    // Interactive props wear red frames so they read as "walk up to me";
    // their names sit amber-bold in the art with the page digit glowing.
    let signpost_text = |ch: char| {
        if ch.is_ascii_digit() {
            Some(
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
        } else if ch.is_ascii_alphabetic() || ch == '·' {
            Some(
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            None
        }
    };
    if map::JUKEBOX.contains(x, y) {
        return match ch {
            '♪' => Style::default().fg(theme::AMBER_GLOW()),
            '[' | ']' | '·' | '▞' | '▚' | '○' => Style::default().fg(theme::TEXT_MUTED()),
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::ARCADE_SCREEN.contains(x, y) {
        return Style::default().fg(theme::SUCCESS());
    }
    if map::ARCADE.contains(x, y) {
        return match ch {
            '●' => Style::default().fg(theme::ERROR()),
            '┃' => Style::default().fg(theme::TEXT_BRIGHT()),
            '╭' | '╮' | '╰' | '╯' | '─' | '│' => dim,
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::DOORS.contains(x, y) {
        if x == map::DOORS.x0 || x == map::DOORS.x1 || matches!(ch, '╭' | '╮' | '╰' | '╯' | '─')
        {
            return Style::default().fg(theme::ERROR());
        }
        return match ch {
            '○' => Style::default().fg(theme::AMBER_GLOW()),
            '║' => Style::default().fg(theme::AMBER()),
            '│' | '▒' => Style::default().fg(theme::AMBER_DIM()),
            _ => signpost_text(ch).unwrap_or(dim),
        };
    }
    if map::POKER_TABLE.contains(x, y) {
        return match ch {
            '▒' => Style::default().fg(theme::SUCCESS()),
            '♥' | '♦' => Style::default().fg(theme::ERROR()),
            '♠' | '♣' => Style::default().fg(theme::TEXT_BRIGHT()),
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::EASEL.contains(x, y) {
        // The title row is the ARTBOARD·5 signpost; the rest of the canvas
        // is paint splatter.
        if y == map::EASEL.y0 + 1
            && let Some(style) = signpost_text(ch)
        {
            return style;
        }
        return match ch {
            '·' | '~' | '°' | '*' => Style::default().fg(hashed_color(x, y, PAINT_PALETTE)),
            '╱' | '╲' => Style::default().fg(theme::TEXT_MUTED()),
            _ => Style::default().fg(theme::ERROR()),
        };
    }
    if map::BOOKSHELF.contains(x, y) {
        if x == map::BOOKSHELF.x0
            || x == map::BOOKSHELF.x1
            || matches!(ch, '╔' | '╗' | '╚' | '╝' | '╠' | '╣' | '═')
        {
            return Style::default().fg(theme::AMBER_DIM());
        }
        return Style::default().fg(hashed_color(x, y, BOOK_PALETTE));
    }
    if map::FIREPLACE.contains(x, y) {
        return match ch {
            '¡' => Style::default().fg(theme::AMBER_GLOW()),
            '▒' => Style::default().fg(theme::AMBER_DIM()),
            '█' | '▓' | '▄' | '▀' => Style::default().fg(theme::TEXT_MUTED()),
            '╔' | '╗' | '╚' | '╝' | '═' | '║' => Style::default().fg(theme::TEXT_MUTED()),
            _ => Style::default().fg(theme::AMBER()),
        };
    }
    match ch {
        '║' | '═' | '╔' | '╗' | '╚' | '╝' | '╡' | '╞' => dim,
        '▔' | '▄' | '▀' => Style::default().fg(theme::AMBER_DIM()),
        '█' => Style::default().fg(theme::TEXT_MUTED()),
        '╥' => Style::default().fg(theme::AMBER()),
        '≡' | '·' => Style::default().fg(theme::AMBER_DIM()),
        '¡' | '!' => Style::default().fg(theme::AMBER_GLOW()),
        '╭' | '╮' | '╰' | '╯' | '─' | '│' | '┬' | '┴' => Style::default().fg(theme::AMBER_DIM()),
        '▒' => Style::default().fg(theme::TEXT_FAINT()),
        '(' | ')' | '_' => dim,
        '▐' => Style::default().fg(theme::TEXT_MUTED()),
        '░' => Style::default().fg(theme::TEXT_FAINT()),
        '♣' => Style::default().fg(theme::SUCCESS()),
        '$' => Style::default().fg(theme::SUCCESS()),
        '[' | ']' => dim,
        _ if ch.is_ascii_alphabetic() => Style::default().fg(theme::AMBER_DIM()),
        _ => Style::default().fg(theme::TEXT_MUTED()),
    }
}

const BOTTLE_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::AMBER,
    theme::SUCCESS,
    theme::ERROR,
    theme::CHAT_AUTHOR,
    theme::TEXT_MUTED,
];
const PAINT_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::CHAT_AUTHOR,
    theme::SUCCESS,
    theme::AMBER,
    theme::MENTION,
    theme::ERROR,
];
const BOOK_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::CHAT_AUTHOR,
    theme::SUCCESS,
    theme::AMBER,
    theme::MENTION,
    theme::TEXT_MUTED,
];

/// A stable per-cell pick from a small palette, so the bottle shelf and the
/// easel's paint read as a colorful jumble without flickering per frame.
fn hashed_color(
    x: u16,
    y: u16,
    palette: [fn() -> ratatui::style::Color; 5],
) -> ratatui::style::Color {
    let h = mix(u64::from(x) * 31 + u64::from(y) * 131);
    palette[(h % palette.len() as u64) as usize]()
}

fn animate(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let t = view.state.anim_tick;

    // Fire: flicker glyph and color per cell.
    for y in map::FIRE_CELLS.y0..=map::FIRE_CELLS.y1 {
        for x in map::FIRE_CELLS.x0..=map::FIRE_CELLS.x1 {
            let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 3);
            let ch = FIRE_CHARS[(h % FIRE_CHARS.len() as u64) as usize];
            let color = match h / 7 % 3 {
                0 => theme::ERROR(),
                1 => theme::AMBER_GLOW(),
                _ => theme::AMBER(),
            };
            set(cells, x, y, ch, Style::default().fg(color));
        }
    }

    // Candle flames breathe on the tables and the mantle.
    for &(x, y) in map::CANDLES.iter() {
        let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 6);
        let ch = if h % 7 == 0 { '!' } else { '¡' };
        let color = if h % 3 == 0 {
            theme::AMBER()
        } else {
            theme::AMBER_GLOW()
        };
        set(cells, x, y, ch, Style::default().fg(color));
    }

    // Jukebox equalizer: dances while something is playing, sleeps flat when
    // the stream is quiet.
    for x in map::JUKEBOX_EQ.x0..=map::JUKEBOX_EQ.x1 {
        let y = map::JUKEBOX_EQ.y0;
        if view.now_playing.is_some() {
            let h = mix(u64::from(x) * 97 + t / 2);
            let ch = EQ_CHARS[(h % EQ_CHARS.len() as u64) as usize];
            set(cells, x, y, ch, Style::default().fg(theme::AMBER_GLOW()));
        } else {
            set(cells, x, y, '▁', Style::default().fg(theme::TEXT_FAINT()));
        }
    }

    // Notes drift out of the jukebox, across the floor below it.
    if view.now_playing.is_some() {
        let (jx, jy) = (map::JUKEBOX.x0, map::JUKEBOX.y1);
        let phase = ((t / 5) % 6) as u16;
        put_if_floor(cells, jx + 1 + phase, jy + 1 + (phase % 2), '♪', theme::AMBER_GLOW());
        let phase2 = ((t / 5 + 3) % 6) as u16;
        put_if_floor(cells, jx + 8 + phase2, jy + 2 - (phase2 % 2), '♫', theme::AMBER());
    }

    // The arcade cabinet plays its attract mode to an empty room.
    for y in map::ARCADE_SCREEN.y0..=map::ARCADE_SCREEN.y1 {
        for x in map::ARCADE_SCREEN.x0..=map::ARCADE_SCREEN.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t / 4);
            let ch = SCREEN_CHARS[(h % SCREEN_CHARS.len() as u64) as usize];
            let color = if h % 5 == 0 {
                theme::TEXT_BRIGHT()
            } else {
                theme::SUCCESS()
            };
            set(cells, x, y, ch, Style::default().fg(color));
        }
    }

    // Stars twinkle in the window panes (the moon holds still).
    for window in map::WINDOWS.iter() {
        for y in window.y0..=window.y1 {
            for x in window.x0..=window.x1 {
                if !matches!(map::char_at(x, y), '·' | '*') {
                    continue;
                }
                let h = mix(u64::from(x) * 53 + u64::from(y) * 97 + t / 10);
                let (ch, color) = match h % 5 {
                    0 => ('*', theme::TEXT_BRIGHT()),
                    1 => (' ', theme::TEXT_FAINT()),
                    _ => ('·', theme::TEXT_MUTED()),
                };
                set(cells, x, y, ch, Style::default().fg(color));
            }
        }
    }

    // The neon sign shorts out for a frame now and then.
    if mix(t / 4) % 19 == 0 {
        for y in map::NEON_SIGN.y0..=map::NEON_SIGN.y1 {
            for x in map::NEON_SIGN.x0..=map::NEON_SIGN.x1 {
                let ch = map::char_at(x, y);
                if ch != ' ' {
                    set(cells, x, y, ch, Style::default().fg(theme::TEXT_FAINT()));
                }
            }
        }
    }

    // The dog: slow blinks, a wagging tail, the occasional dream.
    let (dx, dy) = map::DOG;
    let amber = Style::default().fg(theme::AMBER());
    if (t / 45) % 7 == 0 {
        set(cells, dx + 2, dy + 1, '-', amber);
        set(cells, dx + 4, dy + 1, '-', amber);
    }
    let tail = if (t / 8) % 2 == 0 { ')' } else { '/' };
    set(cells, dx + 7, dy, tail, amber);
    if (t / 40) % 3 == 0 {
        put_if_floor(cells, dx + 8, dy.saturating_sub(1), 'z', theme::TEXT_FAINT());
    }
}

/// A 3-row stick figure standing on `(x, y)` (the feet cell). Degrades near
/// the top wall: torso needs one row of headroom, the head needs two.
fn draw_figure(cells: &mut Cells, x: u16, y: u16, head: char, style: Style) {
    set(cells, x, y, 'Λ', style);
    if y >= 2 {
        set(cells, x.saturating_sub(1), y - 1, '/', style);
        set(cells, x, y - 1, '|', style);
        set(cells, x + 1, y - 1, '\\', style);
    }
    if y >= 3 {
        set(cells, x, y - 2, head, style);
    }
}

/// Where an occupant's head goes for a seat: perched above a stool, sunk
/// into an armchair.
fn seat_head_y(seat: &map::Seat) -> u16 {
    match seat.kind {
        map::SeatKind::Stool => seat.y - 1,
        map::SeatKind::Armchair => seat.y,
    }
}

fn place_people(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let state = view.state;

    // Staff first, so patrons' labels can never erase the bartender.
    let bartender_style = if state.bartender_online {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let (bx, by) = map::BARTENDER;
    set(cells, bx, by, 'O', bartender_style);
    set(cells, bx - 1, by + 1, '/', bartender_style);
    set(cells, bx, by + 1, '|', bartender_style);
    set(cells, bx + 1, by + 1, '\\', bartender_style);
    put_label(cells, bx, by - 1, "bartender", bartender_style);

    if state.graybeard_online {
        let seat = map::GRAYBEARD_SEAT;
        let style = Style::default().fg(theme::TEXT_MUTED());
        set(cells, seat.x, seat_head_y(&seat), 'o', style);
        put_label(cells, seat.x, seat.y + 2, "graybeard", style);
    }

    for (seat, who) in state.seated() {
        let style = Style::default().fg(occupant_color(who.user_id));
        let head_y = seat_head_y(seat);
        set(cells, seat.x, head_y, 'o', style);
        let label_y = if seat.label_below {
            seat.y + 2
        } else {
            head_y.saturating_sub(1).max(1)
        };
        put_label(
            cells,
            seat.x,
            label_y,
            &truncate_name(&who.username),
            Style::default().fg(theme::TEXT_DIM()),
        );
    }

    for ((x, y), who) in state.standing() {
        let style = Style::default().fg(occupant_color(who.user_id));
        draw_figure(cells, x, y, 'o', style);
        put_label(
            cells,
            x,
            y.saturating_sub(3).max(1),
            &truncate_name(&who.username),
            Style::default().fg(theme::TEXT_DIM()),
        );
    }

    let door_count = state.door_count();
    if door_count > 0 {
        put_label(
            cells,
            map::DOOR_LABEL.0,
            map::DOOR_LABEL.1,
            &format!("+{door_count} at the door"),
            Style::default().fg(theme::AMBER_DIM()),
        );
    }

    // You, last: always on top.
    draw_figure(
        cells,
        state.player_x,
        state.player_y,
        '@',
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    );
    put_label(
        cells,
        state.player_x,
        state.player_y.saturating_sub(3).max(1),
        &truncate_name(view.own_username),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    );
}

fn draw_popover(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    let Some(prop) = view.state.nearby() else {
        return;
    };

    let interactive = Style::default().fg(theme::ERROR());
    let flavor = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);

    let (title, border, lines): (&str, Style, Vec<Line>) = match prop {
        map::Interactive::Bartender => (
            " O the bartender ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[t] ", key),
                    Span::styled("talk to the bartender", text),
                ]),
                Line::from(Span::styled(
                    "ask about the house: rooms, music, games",
                    dim,
                )),
            ],
        ),
        map::Interactive::Jukebox => {
            let now = view
                .now_playing
                .map(|np| format!("♪ {}", np.track))
                .unwrap_or_else(|| "the jukebox hums to itself".to_string());
            (
                " ♫ jukebox ",
                interactive,
                vec![
                    Line::from(Span::styled(now, Style::default().fg(theme::AMBER_GLOW()))),
                    Line::from(Span::styled("v v music booth · v x cycle source", text)),
                    Line::from(Span::styled("v s skip vote · v 1-4 pick a station", text)),
                    Line::from(Span::styled("m mute · +/- volume · Enter opens booth", dim)),
                ],
            )
        }
        map::Interactive::Arcade => (
            " ● arcade cabinet ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("play — the Arcade is page 2", text),
                ]),
                Line::from(Span::styled("daily puzzles, high scores, chips", dim)),
            ],
        ),
        map::Interactive::Doors => (
            " ○ the heavy door ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the door games — page 3", text),
                ]),
                Line::from(Span::styled(
                    "Lateania · NetHack · Green Dragon · dopewars · Rebels",
                    dim,
                )),
            ],
        ),
        map::Interactive::Poker => (
            " ♠ the big table ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the game tables — page 4", text),
                ]),
                Line::from(Span::styled(
                    "poker · blackjack · chess · tron — chips on the line",
                    dim,
                )),
            ],
        ),
        map::Interactive::Easel => (
            " ° the easel ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the Artboard — page 5", text),
                ]),
                Line::from(Span::styled("one shared canvas, everyone paints", dim)),
            ],
        ),
        map::Interactive::Dog => (
            " ∪ the dog ",
            flavor,
            vec![Line::from(Span::styled(
                "thumps tail. has never once deployed on a friday.",
                text,
            ))],
        ),
        map::Interactive::Fireplace => (
            " )( fireplace ",
            flavor,
            vec![Line::from(Span::styled(
                "the fire crackles. someone kept your seat warm.",
                text,
            ))],
        ),
    };

    let width = (lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(inner.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(inner.height.saturating_sub(1));
    let rect = Rect {
        x: inner.x + inner.width.saturating_sub(width + 1),
        y: inner.y + inner.height.saturating_sub(height),
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
}

fn set(cells: &mut Cells, x: u16, y: u16, ch: char, style: Style) {
    if x < map::MAP_W && y < map::MAP_H {
        cells[usize::from(y)][usize::from(x)] = (ch, style);
    }
}

/// Draw only over bare floor so scenery never gets stomped by an effect.
fn put_if_floor(cells: &mut Cells, x: u16, y: u16, ch: char, color: ratatui::style::Color) {
    if x < map::MAP_W && y < map::MAP_H && matches!(map::char_at(x, y), ' ' | '░') {
        cells[usize::from(y)][usize::from(x)] = (ch, Style::default().fg(color));
    }
}

/// Write a name centered on `x_center`, clamped inside the walls.
fn put_label(cells: &mut Cells, x_center: u16, y: u16, label: &str, style: Style) {
    if y == 0 || y >= map::MAP_H - 1 {
        return;
    }
    let len = label.chars().count() as u16;
    let max_start = map::MAP_W.saturating_sub(len + 1);
    let start = x_center.saturating_sub(len / 2).clamp(1, max_start.max(1));
    for (i, ch) in label.chars().enumerate() {
        set(cells, start + i as u16, y, ch, style);
    }
}

pub(crate) fn truncate_name(name: &str) -> String {
    if name.chars().count() <= LABEL_MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(LABEL_MAX - 1).collect();
    out.push('…');
    out
}

fn occupant_color(user_id: uuid::Uuid) -> ratatui::style::Color {
    let palette = [
        theme::CHAT_AUTHOR(),
        theme::SUCCESS(),
        theme::AMBER(),
        theme::MENTION(),
        theme::TEXT_BRIGHT(),
    ];
    let h = mix(user_id.as_u128() as u64);
    palette[(h % palette.len() as u64) as usize]
}

fn mix(mut v: u64) -> u64 {
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_name_keeps_short_names_and_cuts_long_ones() {
        assert_eq!(truncate_name("alice"), "alice");
        assert_eq!(truncate_name("exactly-10"), "exactly-10");
        assert_eq!(truncate_name("much-too-long-name"), "much-too-…");
    }

    #[test]
    fn camera_centers_small_maps_and_clamps_large_ones() {
        // Viewport wider than the map: origin pinned to 0 (padding centers).
        assert_eq!(camera_origin(10, 300, 200), 0);
        // Player near the left edge: no negative origin.
        assert_eq!(camera_origin(2, 40, 200), 0);
        // Player mid-map: centered on the player.
        assert_eq!(camera_origin(100, 40, 200), 80);
        // Player near the right edge: clamped to the map end.
        assert_eq!(camera_origin(199, 40, 200), 160);
    }

    #[test]
    fn labels_clamp_inside_the_walls() {
        let mut cells: Cells =
            vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
        put_label(&mut cells, 1, 5, "longishname", Style::default());
        assert_eq!(cells[5][1].0, 'l');
        put_label(
            &mut cells,
            map::MAP_W - 2,
            6,
            "longishname",
            Style::default(),
        );
        let end: String = cells[6].iter().map(|(ch, _)| *ch).collect();
        assert!(end.trim_end().ends_with("longishname"));
    }
}
