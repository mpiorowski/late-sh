//! Clubhouse renderer: the tavern viewport (camera-follow over the floor
//! plan, the live shared crowd, animated fire/jukebox/dog/candles, proximity
//! popovers) with the #lounge composer pinned to the bottom of the screen.
//! There is no chat panel: fresh #lounge messages render as speech bubbles
//! over their author's head, emotes play on avatars, and arrivals slip in at
//! the door. Dwarf Fortress vibes, single-width glyphs only: walking people
//! are 3-row stick figures (`o` head, `/|\` arms, `Λ` legs; you get an `@`),
//! a seated user is an `o` perched on their stool, and the dog is a pocket
//! `(ᴥ)` with a wagging tail that trots wherever the shared lobby says.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;
use uuid::Uuid;

use crate::app::common::theme;
use late_core::api_types::NowPlaying;
use late_core::models::chat_message::ChatMessage;

use super::lobby::{Emote, Placement};
use super::map;
use super::state::{State, Tutorial};

const LABEL_MAX: usize = 10;
const FIRE_CHARS: [char; 6] = ['(', ')', '~', '^', '*', '\''];
const EQ_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Phosphor pixels for the arcade cabinet's attract mode.
const SCREEN_CHARS: [char; 4] = ['▀', '▄', '·', ' '];
/// How long a #lounge message floats over its author's head.
const BUBBLE_MS: i64 = 10_000;
/// Bubble width tiers: short quips stay cozy, longer messages (a bartender
/// answer, a pasted sentence) widen before they truncate.
const BUBBLE_WIDTHS: [usize; 3] = [28, 36, 44];
const BUBBLE_MAX_LINES: usize = 3;

pub(crate) struct ClubhouseView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    pub now_playing: Option<&'a NowPlaying>,
    /// The #lounge tail, for speech bubbles.
    pub lounge_messages: &'a [ChatMessage],
    /// Staff bot ids so their #lounge lines can bubble over their sprites.
    pub graybeard_user_id: Option<Uuid>,
    /// The shared composer block, pinned under the tavern. `None` only
    /// before the #lounge room id is known.
    pub composer: Option<crate::app::chat::ui::ComposerBlockView<'a>>,
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: ClubhouseView<'_>) {
    let Some(composer) = &view.composer else {
        draw_tavern(frame, area, &view);
        return;
    };
    // The composer footer keeps the compact height the dashboard card uses:
    // one placeholder line while idle, growing with the draft while typing.
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let composer_lines = crate::app::chat::ui::chat_composer_lines_for_height(
        composer.composer,
        composer_text_width,
    )
    .max(crate::app::chat::ui::composer_placeholder_lines(
        composer,
        composer_text_width,
    ));
    let composer_height = (composer_lines.min(4) as u16 + 2).min(area.height.saturating_sub(4));
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);

    draw_tavern(frame, layout[0], &view);
    crate::app::chat::ui::draw_composer_block(frame, layout[1], composer);
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
    let anchors = place_people(&mut cells, view);
    draw_door_events(&mut cells, view);
    draw_bubbles(&mut cells, view, &anchors);

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

    draw_overlays(frame, inner, view);
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
            '╔' | '╗' | '╚' | '╝' | '═' | '║' => {
                Style::default().fg(theme::TEXT_MUTED())
            }
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
        '╭' | '╮' | '╰' | '╯' | '─' | '│' | '┬' | '┴' => {
            Style::default().fg(theme::AMBER_DIM())
        }
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
        let ch = if h.is_multiple_of(7) { '!' } else { '¡' };
        let color = if h.is_multiple_of(3) {
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
        put_if_floor(
            cells,
            jx + 1 + phase,
            jy + 1 + (phase % 2),
            '♪',
            theme::AMBER_GLOW(),
        );
        let phase2 = ((t / 5 + 3) % 6) as u16;
        put_if_floor(
            cells,
            jx + 8 + phase2,
            jy + 2 - (phase2 % 2),
            '♫',
            theme::AMBER(),
        );
    }

    // The arcade cabinet plays its attract mode to an empty room.
    for y in map::ARCADE_SCREEN.y0..=map::ARCADE_SCREEN.y1 {
        for x in map::ARCADE_SCREEN.x0..=map::ARCADE_SCREEN.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t / 4);
            let ch = SCREEN_CHARS[(h % SCREEN_CHARS.len() as u64) as usize];
            let color = if h.is_multiple_of(5) {
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
    if mix(t / 4).is_multiple_of(19) {
        for y in map::NEON_SIGN.y0..=map::NEON_SIGN.y1 {
            for x in map::NEON_SIGN.x0..=map::NEON_SIGN.x1 {
                let ch = map::char_at(x, y);
                if ch != ' ' {
                    set(cells, x, y, ch, Style::default().fg(theme::TEXT_FAINT()));
                }
            }
        }
    }

    // The door sign glows while someone is slipping in.
    if view.state.door_glow() {
        for x in map::DOOR_SIGN.x0..=map::DOOR_SIGN.x1 {
            let ch = map::char_at(x, map::DOOR_SIGN.y0);
            set(
                cells,
                x,
                map::DOOR_SIGN.y0,
                ch,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    // The tutorial's "find the bar" beat pulses the bar sign so the goal
    // reads from across the room.
    if view.state.tutorial == Tutorial::GoToBar {
        let pulse = if (t / 8).is_multiple_of(2) {
            theme::AMBER_GLOW()
        } else {
            theme::AMBER()
        };
        let y = map::BAR_COUNTER.y1;
        for x in map::BAR_COUNTER.x0..=map::BAR_COUNTER.x1 {
            let ch = map::char_at(x, y);
            if ch != '█' && ch != ' ' {
                set(
                    cells,
                    x,
                    y,
                    ch,
                    Style::default().fg(pulse).add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    // The dog: a pocket wanderer, `(ᴥ)` plus a wagging tail, drawn from the
    // shared lobby so every session sees the same trot. Napping slows the
    // tail and drifts a `z`; a fresh pet speeds it up, floats hearts, and
    // credits the petter.
    let dog = view.state.snapshot.dog;
    let (dx, dy) = (dog.x, dog.y);
    let amber = Style::default().fg(theme::AMBER());
    let petted = view.state.snapshot.dog_pet.as_ref();
    set(cells, dx.saturating_sub(1), dy, '(', amber);
    set(cells, dx, dy, 'ᴥ', amber);
    set(cells, dx + 1, dy, ')', amber);
    let wag = if petted.is_some() {
        2
    } else if dog.resting {
        16
    } else {
        5
    };
    let tail = if (t / wag).is_multiple_of(2) {
        '/'
    } else {
        '\\'
    };
    let tail_x = if dog.facing_left {
        dx + 2
    } else {
        dx.saturating_sub(2)
    };
    set(cells, tail_x, dy, tail, amber);
    if let Some((name, _)) = petted {
        let beat = ((t / 4) % 3) as u16;
        put_if_floor(
            cells,
            dx.saturating_sub(1) + beat,
            dy.saturating_sub(1),
            '♥',
            theme::ERROR(),
        );
        put_if_floor(
            cells,
            dx + 2,
            dy.saturating_sub(1) - (beat % 2),
            '♥',
            theme::ERROR(),
        );
        put_label(
            cells,
            dx,
            dy + 1,
            &format!("{} pets the dog", truncate_name(name)),
            Style::default().fg(theme::TEXT_FAINT()),
        );
    } else if dog.resting && (t / 40).is_multiple_of(3) {
        put_if_floor(
            cells,
            dx + 2,
            dy.saturating_sub(1),
            'z',
            theme::TEXT_FAINT(),
        );
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

/// Where speech bubbles anchor for each drawn person: the row just above
/// their name label, keyed by user id.
type BubbleAnchors = HashMap<Uuid, (u16, u16)>;

fn place_people(cells: &mut Cells, view: &ClubhouseView<'_>) -> BubbleAnchors {
    let state = view.state;
    let mut anchors: BubbleAnchors = HashMap::new();

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
    // No bubble anchor for the bartender: his lines render as the pinned
    // banner in the top-left corner (`draw_bartender_banner`), out of the
    // way of patron bubbles at the busy bar.

    if state.graybeard_online {
        let seat = map::GRAYBEARD_SEAT;
        let style = Style::default().fg(theme::TEXT_MUTED());
        set(cells, seat.x, seat_head_y(&seat), 'o', style);
        put_label(cells, seat.x, seat.y + 2, "graybeard", style);
        if let Some(id) = view.graybeard_user_id {
            anchors.insert(id, (seat.x, seat_head_y(&seat).saturating_sub(1)));
        }
    }

    let own_id = state.own_user_id();
    for who in state.snapshot.people.iter().filter(|p| p.user_id != own_id) {
        let style = Style::default().fg(occupant_color(who.user_id));
        let label_style = Style::default().fg(theme::TEXT_DIM());
        let anchor = draw_presence(cells, who.placement, 'o', style, &who.username, label_style);
        anchors.insert(who.user_id, anchor);
        if let Some(emote) = who.emote {
            draw_emote(cells, who.placement, emote, state.anim_tick, style);
        }
    }

    if view.state.snapshot.door_overflow > 0 {
        put_label(
            cells,
            map::DOOR_LABEL.0,
            map::DOOR_LABEL.1,
            &format!("+{} at the door", view.state.snapshot.door_overflow),
            Style::default().fg(theme::AMBER_DIM()),
        );
    }

    // You, last: always on top.
    let own_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let own_label_style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let own_placement = state
        .snapshot
        .find(own_id)
        .map(|p| p.placement)
        .unwrap_or(Placement::Walking(state.player_x, state.player_y));
    let anchor = draw_presence(
        cells,
        own_placement,
        '@',
        own_style,
        view.own_username,
        own_label_style,
    );
    anchors.insert(own_id, anchor);
    if let Some(emote) = state.snapshot.find(own_id).and_then(|p| p.emote) {
        draw_emote(cells, own_placement, emote, state.anim_tick, own_style);
    }

    anchors
}

/// Draw one person at their placement and return their bubble anchor (the
/// row above their name label).
fn draw_presence(
    cells: &mut Cells,
    placement: Placement,
    head: char,
    style: Style,
    username: &str,
    label_style: Style,
) -> (u16, u16) {
    match placement {
        Placement::Seated(i) => {
            let seat = &map::SEATS[i.min(map::SEATS.len() - 1)];
            let head_y = seat_head_y(seat);
            set(cells, seat.x, head_y, head, style);
            let label_y = if seat.label_below {
                seat.y + 2
            } else {
                head_y.saturating_sub(1).max(1)
            };
            put_label(
                cells,
                seat.x,
                label_y,
                &truncate_name(username),
                label_style,
            );
            if seat.label_below {
                (seat.x, head_y.saturating_sub(1))
            } else {
                (seat.x, label_y.saturating_sub(1))
            }
        }
        Placement::Standing(_) | Placement::Door(_) | Placement::Walking(..) => {
            let (x, y) = placement.position();
            draw_figure(cells, x, y, head, style);
            let label_y = y.saturating_sub(3).max(1);
            put_label(cells, x, label_y, &truncate_name(username), label_style);
            (x, label_y.saturating_sub(1))
        }
    }
}

/// Two-frame emote animation on an avatar; walkers get full-body frames,
/// seated patrons get a marker beside the head.
fn draw_emote(cells: &mut Cells, placement: Placement, emote: Emote, tick: u64, style: Style) {
    let frame = (tick / 4).is_multiple_of(2);
    let note = Style::default().fg(theme::AMBER_GLOW());
    match placement {
        Placement::Seated(i) => {
            let seat = &map::SEATS[i.min(map::SEATS.len() - 1)];
            let head_y = seat_head_y(seat);
            match emote {
                Emote::Wave => {
                    let arm = if frame { '/' } else { '\'' };
                    set(cells, seat.x + 1, head_y, arm, style);
                }
                Emote::Dance => {
                    let (lx, rx) = (seat.x.saturating_sub(1), seat.x + 1);
                    let x = if frame { lx } else { rx };
                    set(cells, x, head_y, '♪', note.add_modifier(Modifier::BOLD));
                }
            }
        }
        Placement::Standing(_) | Placement::Door(_) | Placement::Walking(..) => {
            let (x, y) = placement.position();
            if y < 2 {
                return;
            }
            match emote {
                Emote::Wave => {
                    // The right arm swings up and down.
                    let (left, right) = if frame { ('─', '/') } else { ('/', '\\') };
                    set(cells, x.saturating_sub(1), y - 1, left, style);
                    set(cells, x + 1, y - 1, right, style);
                }
                Emote::Dance => {
                    // Arms flail, a note bounces side to side.
                    let (left, right) = if frame { ('\\', '/') } else { ('/', '\\') };
                    set(cells, x.saturating_sub(1), y - 1, left, style);
                    set(cells, x + 1, y - 1, right, style);
                    if y >= 3 {
                        let nx = if frame { x.saturating_sub(2) } else { x + 2 };
                        set(cells, nx, y - 2, '♪', note);
                    }
                }
            }
        }
    }
}

/// `* name slipped in` lines stacked over the welcome mat.
fn draw_door_events(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let base_y = 41u16;
    for (i, event) in view.state.door_events.iter().enumerate().take(4) {
        let verb = if event.arrived {
            "slipped in"
        } else {
            "headed out"
        };
        put_label(
            cells,
            map::SPAWN.0,
            base_y + i as u16,
            &format!("* {} {}", truncate_name(&event.username), verb),
            Style::default().fg(theme::TEXT_FAINT()),
        );
    }
}

/// Fresh #lounge messages float over their author's head.
fn draw_bubbles(cells: &mut Cells, view: &ClubhouseView<'_>, anchors: &BubbleAnchors) {
    for message in fresh_bubble_messages(view.lounge_messages, chrono::Utc::now()) {
        let Some(&(x, bottom_y)) = anchors.get(&message.user_id) else {
            continue;
        };
        let lines = wrap_bubble_fitting(bubble_text(&message.body));
        if lines.is_empty() {
            continue;
        }
        draw_bubble_box(cells, x, bottom_y, &lines);
    }
}

/// The bubble-worthy slice of a room tail: the newest fresh message per
/// author. Room message lists are newest-first (see
/// `ChatState::push_message`), so iterate in natural order and stop at the
/// first stale message.
fn fresh_bubble_messages(
    messages: &[ChatMessage],
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<&ChatMessage> {
    let mut seen_authors: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let mut fresh = Vec::new();
    for message in messages {
        let age_ms = now
            .signed_duration_since(message.created)
            .num_milliseconds();
        if age_ms > BUBBLE_MS {
            break;
        }
        if seen_authors.insert(message.user_id) {
            fresh.push(message);
        }
    }
    fresh
}

/// The bubble body: replies drop their quote line; whitespace collapses.
fn bubble_text(body: &str) -> String {
    let body = match body.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with("> ") && !rest.trim().is_empty() => {
            rest
        }
        _ => body,
    };
    to_single_width(&body.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Fold user-controlled text down to one terminal cell per char so it lands
/// cleanly in the tavern grid, which assumes single-width glyphs everywhere.
/// Wide glyphs (emoji, CJK) and zero-width/combining marks would otherwise
/// desync the row they draw into; each is replaced with a `·` placeholder.
fn to_single_width(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.width() == Some(1) { ch } else { '·' })
        .collect()
}

/// Wrap at the narrowest width tier that fits the whole message; fall back
/// to the widest tier with an ellipsis.
fn wrap_bubble_fitting(text: String) -> Vec<String> {
    for width in BUBBLE_WIDTHS {
        let (lines, truncated) = wrap_bubble(text.clone(), width, BUBBLE_MAX_LINES);
        if !truncated {
            return lines;
        }
    }
    wrap_bubble(
        text,
        BUBBLE_WIDTHS[BUBBLE_WIDTHS.len() - 1],
        BUBBLE_MAX_LINES,
    )
    .0
}

/// Greedy word wrap into at most `max_lines` lines of `width` chars; the
/// last line gets an ellipsis when the text keeps going, and the flag
/// reports whether that happened.
fn wrap_bubble(text: String, width: usize, max_lines: usize) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut truncated = false;
    for word in text.split_whitespace() {
        let word: String = word.chars().take(width).collect();
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
            if lines.len() == max_lines {
                truncated = true;
                break;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    } else if !current.is_empty() {
        truncated = true;
    }
    if truncated && let Some(last) = lines.last_mut() {
        while last.chars().count() >= width {
            last.pop();
        }
        last.push('…');
    }
    (lines, truncated)
}

/// A bordered speech bubble whose bottom row sits at `bottom_y`, centered
/// on `x`. Flips below the anchor when the top wall is too close.
fn draw_bubble_box(cells: &mut Cells, x: u16, bottom_y: u16, lines: &[String]) {
    let text_w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    let box_w = text_w + 4;
    let box_h = lines.len() as u16 + 2;
    let mut top = bottom_y.saturating_sub(box_h - 1);
    if top == 0 {
        top = bottom_y.saturating_add(2).min(map::MAP_H - 1 - box_h);
    }
    let max_left = map::MAP_W.saturating_sub(box_w + 1);
    let left = x.saturating_sub(box_w / 2).clamp(1, max_left.max(1));

    let border = Style::default().fg(theme::TEXT_MUTED());
    let text = Style::default().fg(theme::TEXT_BRIGHT());
    for row in 0..box_h {
        for col in 0..box_w {
            let (cx, cy) = (left + col, top + row);
            let ch = match (row, col) {
                (0, 0) => '╭',
                (0, c) if c == box_w - 1 => '╮',
                (r, 0) if r == box_h - 1 => '╰',
                (r, c) if r == box_h - 1 && c == box_w - 1 => '╯',
                (0, _) => '─',
                (r, _) if r == box_h - 1 => '─',
                (_, 0) => '│',
                (_, c) if c == box_w - 1 => '│',
                _ => ' ',
            };
            let style = if ch == ' ' { text } else { border };
            set(cells, cx, cy, ch, style);
        }
        if row > 0 && row < box_h - 1 {
            let line = &lines[(row - 1) as usize];
            for (i, ch) in line.chars().enumerate() {
                set(cells, left + 2 + i as u16, top + row, ch, text);
            }
        }
    }
}

fn draw_overlays(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    draw_bartender_banner(frame, inner, view);
    if draw_tutorial(frame, inner, view) {
        return;
    }
    draw_popover(frame, inner, view);
}

/// The bartender speaks to the whole room: his #lounge lines pin to the
/// top-left corner of the viewport (camera-independent, so you never miss
/// him from across the tavern) instead of bubbling over his sprite, where
/// patron bubbles at the bar would collide with it. Which line shows, and
/// for how long, is the banner queue's call (`State::update_bartender_banner`):
/// a burst of answers plays one at a time instead of overwriting itself.
fn draw_bartender_banner(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    let Some(message_id) = view.state.bartender_banner_message_id() else {
        return;
    };
    let Some(message) = view.lounge_messages.iter().find(|m| m.id == message_id) else {
        return;
    };
    // Roomy on purpose: his replies are up to three sanitized lines of real
    // directions, and the banner is the only place they render.
    let width_budget = usize::from(inner.width.saturating_sub(6)).min(56);
    let (lines, _) = wrap_bubble(bubble_text(&message.body), width_budget.max(16), 8);
    if lines.is_empty() {
        return;
    }

    let border = Style::default().fg(theme::ERROR());
    let text = Style::default().fg(theme::TEXT_BRIGHT());
    let title = " O the bartender ";
    let width = (lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(inner.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(inner.height);
    let rect = Rect {
        x: inner.x + 1,
        y: inner.y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .map(|l| Line::from(Span::styled(l, text)))
                .collect::<Vec<_>>(),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
}

/// The first-visit walkthrough boxes. Returns true when a tutorial overlay
/// owned the frame (prop popovers wait their turn).
fn draw_tutorial(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) -> bool {
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let border = Style::default().fg(theme::AMBER());

    let (title, lines): (&str, Vec<Line>) = match view.state.tutorial {
        Tutorial::Welcome => (
            " ☾ welcome to the late lounge ☽ ",
            vec![
                Line::from(Span::styled(
                    "you're on the welcome mat, the house is live.",
                    text,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("[arrows/hjkl] ", key),
                    Span::styled("walk around", text),
                ]),
                Line::from(vec![
                    Span::styled("[Ctrl+O] ", key),
                    Span::styled("introduce yourself first", text),
                ]),
                Line::default(),
                Line::from(Span::styled(
                    "the bartender is waving you over, head northwest to the bar.",
                    text,
                )),
                Line::default(),
                Line::from(Span::styled("Esc skips the tour", dim)),
            ],
        ),
        Tutorial::BarLesson => (
            " O the bartender leans in ",
            vec![
                Line::from(vec![
                    Span::styled("[i] ", key),
                    Span::styled("say something, it floats over your head", text),
                ]),
                Line::from(vec![
                    Span::styled("[w] ", key),
                    Span::styled("wave · ", text),
                    Span::styled("[x] ", key),
                    Span::styled("dance · ", text),
                    Span::styled("[t] ", key),
                    Span::styled("talk to the bartender", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("[Ctrl+/] ", key),
                    Span::styled("jump to any room or DM", text),
                ]),
                Line::from(vec![
                    Span::styled("[Ctrl+]] ", key),
                    Span::styled("pick an icon, spice up your words", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("got it", dim),
                ]),
            ],
        ),
        Tutorial::SendOff => (
            " ☾ make yourself at home ☽ ",
            vec![
                Line::from(Span::styled(
                    "the room is a map of the house, walk up and press Enter:",
                    text,
                )),
                Line::default(),
                Line::from(Span::styled(
                    "arcade cabinet (2) · big table (4) · artboard (5)",
                    text,
                )),
                Line::from(Span::styled(
                    "heavy door (3): real NetHack, Green Dragon reborn",
                    text,
                )),
                Line::from(Span::styled(
                    "jukebox picks the music · the dog is a dog",
                    text,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("[Ctrl+G] ", key),
                    Span::styled("the hub, quests · shop · leaderboard", text),
                ]),
                Line::from(vec![
                    Span::styled("[?] ", key),
                    Span::styled("the full guide, any time", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("start the night", dim),
                ]),
            ],
        ),
        Tutorial::GoToBar => {
            // A small nudge, pinned bottom-left, out of the walking path.
            let lines = vec![
                Line::from(Span::styled("find the glowing bar, northwest", text)),
                Line::from(Span::styled("Esc skips the tour", dim)),
            ];
            let width = (34u16).min(inner.width.saturating_sub(2));
            let height = 4u16.min(inner.height);
            let rect = Rect {
                x: inner.x + 1,
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
                        .title(Span::styled(" ↖ the bar ", border)),
                ),
                rect,
            );
            return false;
        }
        _ => return false,
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
        x: inner.x + (inner.width.saturating_sub(width)) / 2,
        y: inner.y + (inner.height.saturating_sub(height)) / 3,
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
    true
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
                    Line::from(Span::styled("[?] full guide, opens on the Pair tab", dim)),
                ],
            )
        }
        map::Interactive::Arcade => (
            " ● arcade cabinet ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("play, the Arcade is page 2", text),
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
                    Span::styled("the door games, page 3", text),
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
                    Span::styled("the game tables, page 4", text),
                ]),
                Line::from(Span::styled(
                    "poker · blackjack · chess · tron, chips on the line",
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
                    Span::styled("the Artboard, page 5", text),
                ]),
                Line::from(Span::styled("one shared canvas, everyone paints", dim)),
            ],
        ),
        map::Interactive::Dog => (
            " ∪ the dog ",
            flavor,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("pet the dog", text),
                ]),
                Line::from(Span::styled(
                    "thumps tail. has never once deployed on a friday.",
                    dim,
                )),
            ],
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
    let name = to_single_width(name);
    if name.chars().count() <= LABEL_MAX {
        return name;
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
    fn single_width_folds_wide_and_zero_width_glyphs() {
        // ASCII and box-drawing art survive untouched.
        assert_eq!(to_single_width("hello ·│─"), "hello ·│─");
        // Emoji (width 2) and combining marks (width 0) become one cell each,
        // so the char count matches the rendered cell count.
        let folded = to_single_width("a🎉b");
        assert_eq!(folded, "a·b");
        assert_eq!(folded.chars().count(), 3);
        // Wide names collapse before truncation, so the length math is honest:
        // 12 double-width chars fold to 12 cells, cut to 9 plus an ellipsis.
        assert_eq!(truncate_name("你你你你你你你你你你你你"), "·········…");
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

    #[test]
    fn bubble_text_drops_reply_quotes_and_flattens_lines() {
        assert_eq!(
            bubble_text("> @alice: earlier\nthanks a lot"),
            "thanks a lot"
        );
        assert_eq!(bubble_text("two\nlines  here"), "two lines here");
    }

    #[test]
    fn wrap_bubble_wraps_and_ellipsizes() {
        let (lines, truncated) = wrap_bubble("hello there".to_string(), 28, 3);
        assert_eq!(lines, vec!["hello there"]);
        assert!(!truncated);

        let long = "one two three four five six seven eight nine ten eleven twelve \
                    thirteen fourteen fifteen sixteen seventeen"
            .to_string();
        let (lines, truncated) = wrap_bubble(long, 12, 3);
        assert_eq!(lines.len(), 3);
        assert!(truncated);
        assert!(lines.iter().all(|l| l.chars().count() <= 12));
        assert!(lines.last().unwrap().ends_with('…'));

        assert!(wrap_bubble("   ".to_string(), 10, 3).0.is_empty());
    }

    #[test]
    fn bubbles_widen_before_they_truncate() {
        // Fits at the cozy tier: stays narrow.
        let lines = wrap_bubble_fitting("a short one".to_string());
        assert_eq!(lines, vec!["a short one"]);

        // Too long for 28x3 but fits wider: widens instead of cutting. This
        // is the bartender-answer case.
        let mid = "the arcade cabinet is page 2, the heavy door is page 3, \
                   the big table is page 4, and the easel is page 5"
            .to_string();
        let lines = wrap_bubble_fitting(mid.clone());
        assert!(lines.len() <= BUBBLE_MAX_LINES);
        assert!(!lines.last().unwrap().ends_with('…'), "widening failed");
        assert_eq!(lines.join(" "), mid);

        // Genuinely huge: widest tier plus ellipsis.
        let huge = "word ".repeat(80);
        let lines = wrap_bubble_fitting(huge);
        assert_eq!(lines.len(), BUBBLE_MAX_LINES);
        assert!(lines.last().unwrap().ends_with('…'));
    }

    #[test]
    fn fresh_bubbles_take_the_newest_message_per_author_from_a_newest_first_tail() {
        let now = chrono::Utc::now();
        let msg = |n: u128, author: u128, secs_ago: i64, body: &str| ChatMessage {
            id: Uuid::from_u128(n),
            created: now - chrono::Duration::seconds(secs_ago),
            updated: now - chrono::Duration::seconds(secs_ago),
            pinned: false,
            reply_to_message_id: None,
            reply_to_user_id: None,
            room_id: Uuid::from_u128(99),
            user_id: Uuid::from_u128(author),
            body: body.to_string(),
        };
        // Newest-first, like ChatState room tails.
        let tail = vec![
            msg(1, 1, 2, "newest from alice"),
            msg(2, 2, 4, "from bob"),
            msg(3, 1, 6, "older from alice"),
            msg(4, 3, 60, "stale from carol"),
            msg(5, 4, 3, "unreachable behind the stale break"),
        ];
        let picked: Vec<&str> = fresh_bubble_messages(&tail, now)
            .iter()
            .map(|m| m.body.as_str())
            .collect();
        assert_eq!(picked, vec!["newest from alice", "from bob"]);
    }

    #[test]
    fn bubble_boxes_stay_inside_the_map() {
        let mut cells: Cells =
            vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
        // Anchored right at the top wall: flips below instead of clipping.
        draw_bubble_box(&mut cells, 5, 1, &["hi".to_string()]);
        let top_row: String = cells[0].iter().map(|(ch, _)| *ch).collect();
        assert!(top_row.trim().is_empty(), "bubble drew over the top wall");
        // Anchored mid-room: the border lands above the anchor.
        draw_bubble_box(&mut cells, 90, 20, &["hello".to_string()]);
        assert_eq!(cells[18][86].0, '╭');
    }
}
